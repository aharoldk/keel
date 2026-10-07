//! Variable interpolation.
//!
//! Templates use `{{name}}` (whitespace tolerated). Lookup order, lowest to
//! highest precedence: global (reserved) → workspace → environment →
//! collection → folders → request → session. Layers are held in a
//! [`ScopeStack`]; later scopes win. Names starting with `$` are builtins:
//! `$timestamp`, `$uuid`, `$randomInt`.
//!
//! A name listed in the active environment's `secrets` resolves through the
//! [`SecretSource`] at send time (typically the OS keychain) and falls back to
//! the default value committed in the environment file. Secret values are
//! reported separately in [`Resolved::secrets`] so callers can keep them out
//! of history, logs and exports.
//!
//! Previous-response tags use `#{...}`: `#{body.path}`, `#{header.Name}`,
//! `#{status}`, `#{body}`. They resolve against the in-memory previous
//! response before `{{name}}` runs, so a request can chain off the one sent
//! before it without scripts or environment picking.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use regex::Regex;

/// One precedence layer of variables.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scope {
    /// Plain name → value entries.
    pub values: BTreeMap<String, String>,
    /// Names that must be resolved from the [`SecretSource`].
    pub secret_names: BTreeSet<String>,
}

impl Scope {
    /// Creates an empty scope.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a scope from a variable map.
    pub fn from_map(map: &BTreeMap<String, String>) -> Self {
        Scope {
            values: map.clone(),
            secret_names: BTreeSet::new(),
        }
    }

    /// Creates a scope from `(name, value)` pairs; later pairs win.
    pub fn from_pairs<K, V>(pairs: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        Scope {
            values: pairs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            secret_names: BTreeSet::new(),
        }
    }

    /// Inserts a plain value.
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into(), value.into());
    }

    /// Declares a name as secret-backed.
    pub fn declare_secret(&mut self, name: impl Into<String>) {
        self.secret_names.insert(name.into());
    }
}

/// Ordered precedence layers, lowest first.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ScopeStack {
    /// Lowest precedence first.
    pub scopes: Vec<Scope>,
}

impl ScopeStack {
    /// Appends a layer; later layers win.
    pub fn push(&mut self, scope: Scope) {
        self.scopes.push(scope);
    }

    fn lookup(&self, name: &str) -> Option<&Scope> {
        self.scopes
            .iter()
            .rev()
            .find(|s| s.values.contains_key(name) || s.secret_names.contains(name))
    }
}

/// Result of resolving one template string.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Resolved {
    /// The template with every resolvable `{{name}}` substituted.
    pub value: String,
    /// Every variable name that was interpolated (builtins included).
    pub used: Vec<String>,
    /// Names that had no value anywhere (left as-is in the output).
    pub missing: Vec<String>,
    /// Names resolved from the [`SecretSource`].
    pub secrets: Vec<String>,
}

/// Substitutes `#{status}`, `#{body}`, `#{body.path}` and `#{header.Name}`
/// from the previous response. Unresolvable tags (no previous response, a
/// missing path, a non-JSON body with a path) are left exactly as written,
/// matching the `{{missing}}` behaviour.
pub fn apply_prev_refs(template: &str, prev: Option<&crate::model::PrevResponse>) -> String {
    apply_prev_refs_report(template, prev, &mut Vec::new())
}

/// Like [`apply_prev_refs`], but tag names that could not be resolved are
/// appended to `missing` (without duplicates) so they can be surfaced as
/// `missing_variables` in the result.
pub fn apply_prev_refs_report(
    template: &str,
    prev: Option<&crate::model::PrevResponse>,
    missing: &mut Vec<String>,
) -> String {
    if !template.contains("#{") {
        return template.to_string();
    }
    let re = Regex::new(r"#\{([^{}#]+)\}").expect("static regex");
    let mut out = String::with_capacity(template.len());
    let mut last = 0usize;
    for caps in re.captures_iter(template) {
        let m = caps.get(0).expect("group 0");
        let spec = caps.get(1).expect("group 1").as_str().trim();
        out.push_str(&template[last..m.start()]);
        last = m.end();
        let value = prev.and_then(|p| prev_value(p, spec));
        match value {
            Some(value) => out.push_str(&value),
            None => {
                if !missing.iter().any(|n| n == spec) {
                    missing.push(spec.to_string());
                }
                out.push_str(m.as_str());
            }
        }
    }
    out.push_str(&template[last..]);
    out
}

fn prev_value(prev: &crate::model::PrevResponse, spec: &str) -> Option<String> {
    if spec == "status" {
        return prev.status.map(|s| s.to_string());
    }
    if spec == "body" {
        return prev.body.clone();
    }
    if let Some(path) = spec.strip_prefix("body.") {
        let json = prev.json.as_ref()?;
        let value = crate::engine::script::json_path(json, path)?;
        return Some(json_scalar_string(value));
    }
    if let Some(name) = spec.strip_prefix("header.") {
        return prev
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone());
    }
    None
}

fn json_scalar_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                n.to_string()
            }
        }
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Suggestion paths for the editor's `#` dropdown: `status`, `body`, every
/// JSON leaf under `body.*`, and each `header.Name`. Values are never
/// included — a secret token in a previous response must not leak into the UI.
pub fn prev_ref_paths(prev: &crate::model::PrevResponse) -> Vec<String> {
    let mut out = vec!["status".to_string(), "body".to_string()];
    if let Some(json) = &prev.json {
        collect_json_paths(json, "body", &mut out, 0);
    }
    let mut seen = BTreeSet::new();
    for (name, _) in &prev.headers {
        let lower = name.to_ascii_lowercase();
        if seen.insert(lower) {
            out.push(format!("header.{name}"));
        }
    }
    out
}

fn collect_json_paths(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>, depth: usize) {
    if depth > 6 || out.len() >= 80 {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if k.contains('.') || k.contains('{') || k.contains('}') || k.contains('#') {
                    continue;
                }
                let path = format!("{prefix}.{k}");
                match v {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        collect_json_paths(v, &path, out, depth + 1);
                    }
                    _ => out.push(path),
                }
            }
        }
        serde_json::Value::Array(items) => {
            for (i, v) in items.iter().take(8).enumerate() {
                let path = format!("{prefix}.{i}");
                match v {
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                        collect_json_paths(v, &path, out, depth + 1);
                    }
                    _ => out.push(path),
                }
            }
        }
        _ => {}
    }
}

/// Backend that provides current secret values, e.g. the OS keychain.
pub trait SecretSource: Send + Sync {
    /// Returns the current value for `name`, or an error when unset.
    fn get(&self, name: &str) -> Result<String, String>;
}

/// In-memory [`SecretSource`], useful for tests and headless tooling.
#[derive(Debug, Clone, Default)]
pub struct MapSecrets(pub BTreeMap<String, String>);

impl MapSecrets {
    /// Creates a source from `(name, value)` pairs.
    pub fn from_pairs<K, V>(pairs: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        MapSecrets(
            pairs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

impl From<BTreeMap<String, String>> for MapSecrets {
    fn from(map: BTreeMap<String, String>) -> Self {
        MapSecrets(map)
    }
}

impl SecretSource for MapSecrets {
    fn get(&self, name: &str) -> Result<String, String> {
        self.0
            .get(name)
            .cloned()
            .ok_or_else(|| format!("secret {name} not set"))
    }
}

/// Resolves `{{name}}` occurrences in template strings.
pub struct Interpolator<'a> {
    /// Precedence layers, lowest first.
    pub scopes: &'a ScopeStack,
    /// Backend for secret-backed names.
    pub secrets: &'a dyn SecretSource,
}

impl<'a> Interpolator<'a> {
    /// Creates an interpolator over the given scopes and secret source.
    pub fn new(scopes: &'a ScopeStack, secrets: &'a dyn SecretSource) -> Self {
        Self { scopes, secrets }
    }

    /// Resolves every `{{name}}` occurrence in `template`.
    pub fn resolve(&self, template: &str) -> Resolved {
        let mut out = String::with_capacity(template.len());
        let mut resolved = Resolved::default();
        let re = variable_re();
        let mut last = 0usize;
        for caps in re.captures_iter(template) {
            let m = caps.get(0).expect("group 0");
            let name = caps.get(1).expect("group 1").as_str().trim();
            out.push_str(&template[last..m.start()]);
            last = m.end();

            if let Some(builtin) = builtin_value(name) {
                out.push_str(&builtin);
                resolved.used.push(name.to_string());
                continue;
            }
            if let Some(scope) = self.scopes.lookup(name) {
                if scope.secret_names.contains(name) {
                    // Current value (OS keychain) wins; fall back to the
                    // default value declared alongside the secret.
                    match self.secrets.get(name) {
                        Ok(secret) => {
                            out.push_str(&secret);
                            resolved.secrets.push(name.to_string());
                            resolved.used.push(name.to_string());
                            continue;
                        }
                        Err(_) => match scope.values.get(name) {
                            Some(default) => {
                                out.push_str(default);
                                resolved.used.push(name.to_string());
                                continue;
                            }
                            None => {
                                resolved.missing.push(name.to_string());
                                out.push_str(m.as_str());
                                continue;
                            }
                        },
                    }
                }
                out.push_str(
                    scope
                        .values
                        .get(name)
                        .map(String::as_str)
                        .unwrap_or_default(),
                );
                resolved.used.push(name.to_string());
                continue;
            }
            resolved.missing.push(name.to_string());
            out.push_str(m.as_str());
        }
        out.push_str(&template[last..]);
        resolved.value = out;
        resolved
    }
}

fn variable_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").expect("static regex"))
}

fn builtin_value(name: &str) -> Option<String> {
    match name {
        "$timestamp" => Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis().to_string())
                .unwrap_or_else(|_| "0".to_string()),
        ),
        "$uuid" => Some(uuid::Uuid::new_v4().to_string()),
        "$randomInt" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
            Some((nanos % 1_000_000).to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prev_with(body: &str) -> crate::model::PrevResponse {
        crate::model::PrevResponse {
            status: Some(200),
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("X-Trace".to_string(), "t1".to_string()),
            ],
            body: Some(body.to_string()),
            json: serde_json::from_str(body).ok(),
        }
    }

    #[test]
    fn prev_body_path_and_scalars() {
        let prev = prev_with(r#"{"accessToken":"tok-1","id":7,"ok":true}"#);
        let t = apply_prev_refs("Bearer #{body.accessToken}", Some(&prev));
        assert_eq!(t, "Bearer tok-1");
        assert_eq!(apply_prev_refs("#{body.id}", Some(&prev)), "7");
        assert_eq!(apply_prev_refs("#{body.ok}", Some(&prev)), "true");
        assert_eq!(apply_prev_refs("#{body}", Some(&prev)), r#"{"accessToken":"tok-1","id":7,"ok":true}"#);
    }

    #[test]
    fn prev_nested_path_with_arrays() {
        let prev = prev_with(r#"{"data":{"items":[{"id":7}]}}"#);
        assert_eq!(apply_prev_refs("#{body.data.items.0.id}", Some(&prev)), "7");
        assert_eq!(apply_prev_refs("#{body.data.missing}", Some(&prev)), "#{body.data.missing}");
    }

    #[test]
    fn prev_headers_and_status() {
        let prev = prev_with("{}");
        assert_eq!(apply_prev_refs("#{header.Content-Type}", Some(&prev)), "application/json");
        assert_eq!(apply_prev_refs("#{header.x-trace}", Some(&prev)), "t1");
        assert_eq!(apply_prev_refs("#{header.Nope}", Some(&prev)), "#{header.Nope}");
        assert_eq!(apply_prev_refs("#{status}", Some(&prev)), "200");
    }

    #[test]
    fn prev_missing_prev_response_leaves_tags() {
        let t = apply_prev_refs("#{body.accessToken}/x", None);
        assert_eq!(t, "#{body.accessToken}/x");
    }

    #[test]
    fn prev_non_json_body_with_path_left_as_is() {
        let prev = crate::model::PrevResponse {
            status: Some(500),
            headers: vec![],
            body: Some("not json".to_string()),
            json: None,
        };
        assert_eq!(apply_prev_refs("#{body.x}", Some(&prev)), "#{body.x}");
        assert_eq!(apply_prev_refs("#{body}", Some(&prev)), "not json");
    }

    #[test]
    fn prev_ref_paths_lists_leaf_paths_and_headers() {
        let prev = prev_with(r#"{"data":{"user":"ada","items":[{"id":1}]}}"#);
        let paths = prev_ref_paths(&prev);
        assert!(paths.contains(&"status".to_string()));
        assert!(paths.contains(&"body".to_string()));
        assert!(paths.contains(&"body.data.user".to_string()));
        assert!(paths.contains(&"body.data.items.0.id".to_string()));
        assert!(paths.contains(&"header.content-type".to_string()));
        assert!(!paths.iter().any(|p| p.contains("ada")));
    }

    #[test]
    fn prev_ref_report_collects_unresolved_without_duplicates() {
        // No previous response: every tag is reported, output unchanged.
        let mut missing = Vec::new();
        let out = apply_prev_refs_report("#{body.a} #{body.a} #{status}", None, &mut missing);
        assert_eq!(out, "#{body.a} #{body.a} #{status}");
        assert_eq!(missing, vec!["body.a".to_string(), "status".to_string()]);

        // With a response: only the failing path is reported.
        let prev = prev_with(r#"{"ok":1}"#);
        let mut missing = Vec::new();
        let out = apply_prev_refs_report("#{body.ok} #{body.nope}", Some(&prev), &mut missing);
        assert_eq!(out, "1 #{body.nope}");
        assert_eq!(missing, vec!["body.nope".to_string()]);
    }

    fn stack(maps: &[&[(&str, &str)]]) -> ScopeStack {
        let mut stack = ScopeStack::default();
        for entries in maps {
            stack.push(Scope::from_pairs(entries.iter().copied()));
        }
        stack
    }

    #[test]
    fn precedence_request_wins() {
        let s = stack(&[&[("a", "workspace")], &[("a", "env")], &[("a", "request")]]);
        let secrets = MapSecrets::default();
        let r = Interpolator::new(&s, &secrets).resolve("{{a}}");
        assert_eq!(r.value, "request");
        assert!(r.missing.is_empty());
    }

    #[test]
    fn falls_back_to_lower_scope() {
        let s = stack(&[&[("base", "http://w"), ("x", "1")], &[("y", "2")]]);
        let secrets = MapSecrets::default();
        let r = Interpolator::new(&s, &secrets).resolve("{{base}}/{{x}}/{{y}}");
        assert_eq!(r.value, "http://w/1/2");
        assert_eq!(r.used.len(), 3);
    }

    #[test]
    fn missing_vars_are_reported_and_kept() {
        let s = stack(&[&[("a", "1")]]);
        let secrets = MapSecrets::default();
        let r = Interpolator::new(&s, &secrets).resolve("{{a}}-{{nope}}");
        assert_eq!(r.value, "1-{{nope}}");
        assert_eq!(r.missing, vec!["nope".to_string()]);
    }

    #[test]
    fn whitespace_tolerant() {
        let s = stack(&[&[("id", "7")]]);
        let secrets = MapSecrets::default();
        let r = Interpolator::new(&s, &secrets).resolve("u/{{ id }}/x");
        assert_eq!(r.value, "u/7/x");
    }

    #[test]
    fn builtins_resolve() {
        let s = stack(&[]);
        let secrets = MapSecrets::default();
        let r = Interpolator::new(&s, &secrets).resolve("{{$timestamp}} {{$uuid}} {{$randomInt}}");
        assert!(r.value.starts_with("1"));
        assert!(r.value.contains('-'));
    }

    #[test]
    fn secrets_go_through_source() {
        let mut scope = Scope::default();
        scope.insert("token", "ignored");
        scope.declare_secret("token");
        let mut s = ScopeStack::default();
        s.push(scope);
        let secrets = MapSecrets::from_pairs([("token", "from-keychain")]);
        let r = Interpolator::new(&s, &secrets).resolve("Bearer {{token}}");
        assert_eq!(r.value, "Bearer from-keychain");
        assert_eq!(r.secrets, vec!["token".to_string()]);
    }

    #[test]
    fn pure_secret_without_value_entry() {
        // Name declared only under `secrets:` (no entry in `variables:`).
        let mut scope = Scope::default();
        scope.declare_secret("apiKey");
        let mut s = ScopeStack::default();
        s.push(scope);
        let secrets = MapSecrets::from_pairs([("apiKey", "s3cr3t")]);
        let r = Interpolator::new(&s, &secrets).resolve("{{apiKey}}");
        assert_eq!(r.value, "s3cr3t");
        assert_eq!(r.secrets, vec!["apiKey".to_string()]);
    }

    #[test]
    fn secret_missing_value() {
        let mut scope = Scope::default();
        scope.declare_secret("tok");
        let mut s = ScopeStack::default();
        s.push(scope);
        let secrets = MapSecrets::default();
        let r = Interpolator::new(&s, &secrets).resolve("{{tok}}");
        assert_eq!(r.value, "{{tok}}");
        assert_eq!(r.missing, vec!["tok".to_string()]);
    }

    #[test]
    fn secret_falls_back_to_default_when_keychain_empty() {
        let mut scope = Scope::default();
        scope.declare_secret("apiToken");
        scope.insert("apiToken", "default-token");
        let mut s = ScopeStack::default();
        s.push(scope);
        let secrets = MapSecrets::default();
        let r = Interpolator::new(&s, &secrets).resolve("Bearer {{apiToken}}");
        assert_eq!(r.value, "Bearer default-token");
        assert_eq!(r.used, vec!["apiToken".to_string()]);
        assert!(r.secrets.is_empty());
        assert!(r.missing.is_empty());
    }

    #[test]
    fn secret_current_value_wins_over_default() {
        let mut scope = Scope::default();
        scope.declare_secret("apiToken");
        scope.insert("apiToken", "default-token");
        let mut s = ScopeStack::default();
        s.push(scope);
        let secrets = MapSecrets::from_pairs([("apiToken", "current-token")]);
        let r = Interpolator::new(&s, &secrets).resolve("Bearer {{apiToken}}");
        assert_eq!(r.value, "Bearer current-token");
        assert_eq!(r.secrets, vec!["apiToken".to_string()]);
    }

    #[test]
    fn empty_value_in_higher_scope_wins() {
        let s = stack(&[&[("a", "low")], &[("a", "")]]);
        let secrets = MapSecrets::default();
        let r = Interpolator::new(&s, &secrets).resolve("[{{a}}]");
        assert_eq!(r.value, "[]");
        assert!(r.missing.is_empty());
    }
}
