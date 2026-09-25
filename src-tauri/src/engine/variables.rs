//! Variable interpolation.
//!
//! Templates use `{{name}}` (whitespace tolerated). Lookup order, lowest to
//! highest precedence: global (reserved) → workspace → environment →
//! collection → request → transient (script `set()` for the session).
//! Names starting with `$` are builtins: `$timestamp`, `$uuid`, `$randomInt`.
//! A name listed in the active environment's `secrets` resolves from the OS
//! keychain at send time and is never echoed back to the UI.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub values: BTreeMap<String, String>,
    /// Names that must be resolved from the OS keychain.
    pub secret_names: BTreeSet<String>,
}

impl Scope {
    pub fn from_map(map: &BTreeMap<String, String>) -> Self {
        Scope {
            values: map.clone(),
            secret_names: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ScopeStack {
    /// Lowest precedence first.
    pub scopes: Vec<Scope>,
}

impl ScopeStack {
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

#[derive(Debug, Default, Clone)]
pub struct Resolved {
    pub value: String,
    /// Every variable name that was interpolated.
    pub used: Vec<String>,
    /// Names that had no value anywhere (left as-is in the output).
    pub missing: Vec<String>,
    /// Names resolved from the OS keychain.
    pub secrets: Vec<String>,
}

pub trait SecretSource: Send + Sync {
    fn get(&self, name: &str) -> Result<String, String>;
}

/// Resolves `{{name}}` occurrences in a template string.
pub struct Interpolator<'a> {
    pub scopes: &'a ScopeStack,
    pub secrets: &'a dyn SecretSource,
}

impl<'a> Interpolator<'a> {
    pub fn resolve(&self, template: &str) -> Resolved {
        let mut out = String::with_capacity(template.len());
        let mut resolved = Resolved::default();
        let re = Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").expect("static regex");
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
pub(crate) struct MapSecrets(pub BTreeMap<String, String>);

#[cfg(test)]
impl SecretSource for MapSecrets {
    fn get(&self, name: &str) -> Result<String, String> {
        self.0
            .get(name)
            .cloned()
            .ok_or_else(|| format!("secret {name} not set"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(maps: &[&[(&str, &str)]]) -> ScopeStack {
        let mut stack = ScopeStack::default();
        for entries in maps {
            let mut scope = Scope::default();
            for (k, v) in *entries {
                scope.values.insert(k.to_string(), v.to_string());
            }
            stack.push(scope);
        }
        stack
    }

    #[test]
    fn precedence_request_wins() {
        let s = stack(&[&[("a", "workspace")], &[("a", "env")], &[("a", "request")]]);
        let secrets = MapSecrets(BTreeMap::new());
        let r = Interpolator {
            scopes: &s,
            secrets: &secrets,
        }
        .resolve("{{a}}");
        assert_eq!(r.value, "request");
        assert!(r.missing.is_empty());
    }

    #[test]
    fn falls_back_to_lower_scope() {
        let s = stack(&[&[("base", "http://w"), ("x", "1")], &[("y", "2")]]);
        let secrets = MapSecrets(BTreeMap::new());
        let r = Interpolator {
            scopes: &s,
            secrets: &secrets,
        }
        .resolve("{{base}}/{{x}}/{{y}}");
        assert_eq!(r.value, "http://w/1/2");
        assert_eq!(r.used.len(), 3);
    }

    #[test]
    fn missing_vars_are_reported_and_kept() {
        let s = stack(&[&[("a", "1")]]);
        let secrets = MapSecrets(BTreeMap::new());
        let r = Interpolator {
            scopes: &s,
            secrets: &secrets,
        }
        .resolve("{{a}}-{{nope}}");
        assert_eq!(r.value, "1-{{nope}}");
        assert_eq!(r.missing, vec!["nope".to_string()]);
    }

    #[test]
    fn whitespace_tolerant() {
        let s = stack(&[&[("id", "7")]]);
        let secrets = MapSecrets(BTreeMap::new());
        let r = Interpolator {
            scopes: &s,
            secrets: &secrets,
        }
        .resolve("u/{{ id }}/x");
        assert_eq!(r.value, "u/7/x");
    }

    #[test]
    fn builtins_resolve() {
        let s = stack(&[]);
        let secrets = MapSecrets(BTreeMap::new());
        let r = Interpolator {
            scopes: &s,
            secrets: &secrets,
        }
        .resolve("{{$timestamp}} {{$uuid}} {{$randomInt}}");
        assert!(r.value.starts_with("1"));
        assert!(r.value.contains('-'));
    }

    #[test]
    fn secrets_go_through_source() {
        let mut scope = Scope::default();
        scope.values.insert("token".to_string(), "ignored".into());
        scope.secret_names.insert("token".to_string());
        let mut stack = ScopeStack::default();
        stack.push(scope);
        let mut secrets = BTreeMap::new();
        secrets.insert("token".to_string(), "from-keychain".to_string());
        let src = MapSecrets(secrets);
        let r = Interpolator {
            scopes: &stack,
            secrets: &src,
        }
        .resolve("Bearer {{token}}");
        assert_eq!(r.value, "Bearer from-keychain");
        assert_eq!(r.secrets, vec!["token".to_string()]);
    }

    #[test]
    fn pure_secret_without_value_entry() {
        // Name declared only under `secrets:` (no entry in `variables:`).
        let mut scope = Scope::default();
        scope.secret_names.insert("apiKey".to_string());
        let mut stack = ScopeStack::default();
        stack.push(scope);
        let mut secrets = BTreeMap::new();
        secrets.insert("apiKey".to_string(), "s3cr3t".to_string());
        let src = MapSecrets(secrets);
        let r = Interpolator {
            scopes: &stack,
            secrets: &src,
        }
        .resolve("{{apiKey}}");
        assert_eq!(r.value, "s3cr3t");
        assert_eq!(r.secrets, vec!["apiKey".to_string()]);
    }

    #[test]
    fn secret_missing_value() {
        let mut scope = Scope::default();
        scope.secret_names.insert("tok".to_string());
        let mut stack = ScopeStack::default();
        stack.push(scope);
        let src = MapSecrets(BTreeMap::new());
        let r = Interpolator {
            scopes: &stack,
            secrets: &src,
        }
        .resolve("{{tok}}");
        assert_eq!(r.value, "{{tok}}");
        assert_eq!(r.missing, vec!["tok".to_string()]);
    }

    #[test]
    fn secret_falls_back_to_default_when_keychain_empty() {
        let mut scope = Scope::default();
        scope.secret_names.insert("apiToken".to_string());
        scope.values.insert("apiToken".to_string(), "default-token".into());
        let mut stack = ScopeStack::default();
        stack.push(scope);
        let src = MapSecrets(BTreeMap::new());
        let r = Interpolator {
            scopes: &stack,
            secrets: &src,
        }
        .resolve("Bearer {{apiToken}}");
        assert_eq!(r.value, "Bearer default-token");
        assert_eq!(r.used, vec!["apiToken".to_string()]);
        assert!(r.secrets.is_empty());
        assert!(r.missing.is_empty());
    }

    #[test]
    fn secret_current_value_wins_over_default() {
        let mut scope = Scope::default();
        scope.secret_names.insert("apiToken".to_string());
        scope.values.insert("apiToken".to_string(), "default-token".into());
        let mut stack = ScopeStack::default();
        stack.push(scope);
        let mut keychain = BTreeMap::new();
        keychain.insert("apiToken".to_string(), "current-token".to_string());
        let src = MapSecrets(keychain);
        let r = Interpolator {
            scopes: &stack,
            secrets: &src,
        }
        .resolve("Bearer {{apiToken}}");
        assert_eq!(r.value, "Bearer current-token");
        assert_eq!(r.secrets, vec!["apiToken".to_string()]);
    }
}
