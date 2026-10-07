//! Send orchestration: variable resolution → pre-request scripts (JS) →
//! auth (bearer/basic/apikey/digest/oauth2) → cookie jar → HTTP exchange →
//! post-response scripts (JS) → assertions → history record.
//!
//! Precedence (low → high): workspace → environment → collection → folders
//! → request → transient (session vars set by scripts). Folder/collection
//! inheritance is folded into the incoming `doc` by callers via
//! `runner::merge_send_doc`, except the auth-chain source which arrives as
//! part of that merge as well. This module stays tauri-free.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::auth::digest;
use crate::auth::oauth2::{self, Oauth2Cache};
use crate::cookies::CookieJar;
use crate::engine::http::{execute, url_with_query, HttpOptions, Part, PreparedBody, PreparedRequest};
use crate::engine::js::{self, Host, ReqState, ResState};
use crate::engine::tests::run_tests;
use crate::engine::variables::{Interpolator, Resolved, Scope, ScopeStack, SecretSource};
use crate::model::{
    AuthType, BodyType, CollectionDoc, EnvDoc, HeaderDto, HistoryRecord, PrevResponse, RequestDoc,
    ResponseCtx, SendResult, TestResultDto, TimelineEventDto, WorkspaceDoc,
};
use crate::path_params;

pub struct SendInput<'a> {
    pub doc: &'a RequestDoc,
    pub env_label: Option<String>,
    pub env: Option<&'a EnvDoc>,
    pub collection: Option<&'a CollectionDoc>,
    pub workspace_doc: Option<&'a WorkspaceDoc>,
    pub transient: &'a mut BTreeMap<String, String>,
    /// The last completed response, for `#{body…}` / `#{header…}` / `#{status}`
    /// tags. `None` leaves the tags as written.
    pub prev: Option<&'a PrevResponse>,
    pub http: HttpOptions,
    pub secret_source: &'a dyn SecretSource,
    pub request_path: Option<String>,
    /// Raw collection-scope variables for `keel.getCollectionVar`.
    pub collection_vars: BTreeMap<String, String>,
    /// Raw folder-chain variables for `keel.getFolderVar`.
    pub folder_vars: BTreeMap<String, String>,
    /// Current (local) variable values for the active environment; they win
    /// over the committed default values.
    pub env_values: BTreeMap<String, String>,
    /// Environment file name (`local.yaml`). Required to persist
    /// `keel.setEnvVar` into that environment's current values.
    pub env_file: Option<String>,
    /// Workspace root holding `.keel/env-values.yaml`.
    pub workspace_root: Option<std::path::PathBuf>,
    /// In-memory OAuth2 token cache (shared across sends).
    pub oauth_cache: &'a Oauth2Cache,
    /// Opens a URL in the system browser (authorization-code flow).
    pub open_browser: &'a (dyn Fn(&str) -> Result<(), String> + Send + Sync),
    /// Shared cookie jar; `None` disables cookie features entirely.
    pub cookie_jar: Option<&'a Mutex<CookieJar>>,
    pub send_cookies: bool,
    pub store_cookies: bool,
    /// One data-file row. Wins over request variables; scripts still win.
    /// Keys that name an environment secret are dropped by the caller.
    pub iteration_vars: BTreeMap<String, String>,
}

#[derive(Debug)]
pub struct SendOutput {
    pub result: SendResult,
    pub history: Option<HistoryRecord>,
    /// The response this send captured for the *next* request's `#{…}` tags.
    /// `None` when no HTTP response was received (the previous one stays).
    pub captured: Option<PrevResponse>,
}

struct ResolveStats {
    used: Vec<String>,
    missing: Vec<String>,
    secrets_used: Vec<String>,
}

impl Default for ResolveStats {
    fn default() -> Self {
        Self {
            used: Vec::new(),
            missing: Vec::new(),
            secrets_used: Vec::new(),
        }
    }
}

impl ResolveStats {
    fn absorb(&mut self, r: &Resolved) {
        for name in &r.used {
            if !self.used.contains(name) {
                self.used.push(name.clone());
            }
        }
        for name in &r.missing {
            if !self.missing.contains(name) {
                self.missing.push(name.clone());
            }
        }
        for name in &r.secrets {
            if !self.secrets_used.contains(name) {
                self.secrets_used.push(name.clone());
            }
        }
    }
}

fn now_ts() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn enabled_rows(rows: Option<&Vec<crate::model::KV>>) -> Vec<crate::model::KV> {
    rows.cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|kv| kv.enabled)
        .collect()
}

/// Builds the scope stack. `transient` is the highest layer.
fn make_scopes<'a>(
    workspace_doc: Option<&'a WorkspaceDoc>,
    env: Option<&'a EnvDoc>,
    collection: Option<&'a CollectionDoc>,
    folder_vars: &'a BTreeMap<String, String>,
    doc: &'a RequestDoc,
    iteration_vars: &'a BTreeMap<String, String>,
    transient: &'a BTreeMap<String, String>,
    env_values: &'a BTreeMap<String, String>,
) -> ScopeStack {
    let mut scopes = ScopeStack::default();
    if let Some(ws) = workspace_doc {
        scopes.push(Scope::from_map(ws.variables.as_ref().unwrap_or(&BTreeMap::new())));
    }
    if let Some(env) = env {
        // Default values are committed in the env file; current values are
        // local overrides and win when present (for non-secret variables).
        let mut secret_entries: Vec<(String, String)> = Vec::new();
        for (name, default) in env.secrets.iter().flatten() {
            let name = name.trim().to_string();
            if !name.is_empty() {
                secret_entries.push((name, default.clone()));
            }
        }
        let mut values = env.variables.clone().unwrap_or_default();
        for (k, v) in env_values {
            if !secret_entries.iter().any(|(n, _)| n == k) {
                values.insert(k.clone(), v.clone());
            }
        }
        let mut scope = Scope::from_map(&values);
        for (name, default) in secret_entries {
            scope.secret_names.insert(name.clone());
            // The secret's default value is the fallback used when the
            // keychain holds no current value.
            scope.values.entry(name).or_insert(default);
        }
        scopes.push(scope);
    }
    if let Some(collection) = collection {
        scopes.push(Scope::from_map(
            collection.variables.as_ref().unwrap_or(&BTreeMap::new()),
        ));
    }
    if !folder_vars.is_empty() {
        scopes.push(Scope::from_map(folder_vars));
    }
    scopes.push(Scope::from_map(
        doc.variables.as_ref().unwrap_or(&BTreeMap::new()),
    ));
    if !iteration_vars.is_empty() {
        scopes.push(Scope::from_map(iteration_vars));
    }
    let mut session = Scope::default();
    for (k, v) in transient {
        session.values.insert(k.clone(), v.clone());
    }
    scopes.push(session);
    scopes
}

fn plain_snapshot(scopes: &ScopeStack) -> BTreeMap<String, String> {
    // JS-visible variables: everything except secret-backed values.
    let mut out = BTreeMap::new();
    for scope in &scopes.scopes {
        for (k, v) in &scope.values {
            if !scope.secret_names.contains(k) {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// Writes `keel.setEnvVar` / `keel.deleteEnvVar` into the active environment's
/// current values. Secret names are skipped — those stay in the keychain.
/// An empty update deletes the current value so the committed default is used.
fn persist_env_updates(
    root: Option<&std::path::Path>,
    file_name: Option<&str>,
    env: Option<&EnvDoc>,
    updates: &BTreeMap<String, Option<String>>,
    env_values: &mut BTreeMap<String, String>,
) {
    if updates.is_empty() {
        return;
    }
    let secrets = env.and_then(|e| e.secrets.as_ref());
    for (name, value) in updates {
        if secrets.is_some_and(|s| s.contains_key(name)) {
            continue;
        }
        match value {
            Some(v) => {
                env_values.insert(name.clone(), v.clone());
            }
            None => {
                env_values.remove(name);
            }
        }
        let Some(root) = root else { continue };
        let Some(file_name) = file_name else { continue };
        let result = match value {
            Some(v) => crate::workspace::env_value_set(root, file_name, name, v),
            None => crate::workspace::env_value_delete(root, file_name, name),
        };
        if let Err(e) = result {
            eprintln!("keel: failed to persist env var `{name}`: {e}");
        }
    }
}

fn script_sources(doc: &RequestDoc, which: WhichScript) -> Vec<String> {
    let Some(scripts) = &doc.scripts else {
        return Vec::new();
    };
    let pick = match which {
        WhichScript::Pre => &scripts.pre_request,
        WhichScript::Post => &scripts.post_response,
    };
    pick.iter().filter(|s| !s.trim().is_empty()).cloned().collect()
}

enum WhichScript {
    Pre,
    Post,
}

pub async fn send(input: SendInput<'_>) -> SendOutput {
    let mut doc = input.doc.clone();
    // History must only ever contain the UNRESOLVED template URL (secrets
    // and session vars never leak into .keel/history.jsonl).
    let template_url = input.doc.request.url.clone();
    let env_label = input.env_label.clone();
    let env = input.env;
    let collection = input.collection;
    let workspace_doc = input.workspace_doc;
    let http = input.http;
    let secret_source = input.secret_source;
    let request_path = input.request_path.clone();
    let collection_vars = input.collection_vars.clone();
    let folder_vars = input.folder_vars.clone();
    let oauth_cache = input.oauth_cache;
    let open_browser = input.open_browser;
    let cookie_jar = input.cookie_jar;
    let send_cookies = input.send_cookies;
    let store_cookies = input.store_cookies;

    let mut stats = ResolveStats::default();
    let mut logs: Vec<String> = Vec::new();
    let mut script_error: Option<String> = None;
    let mut js_tests: Vec<TestResultDto> = Vec::new();
    let mut timeline: Vec<TimelineEventDto> = Vec::new();
    let mut auth_used: Option<String> = None;

    let mut env_values = input.env_values.clone();
    let iteration_vars = input.iteration_vars.clone();
    let prev = input.prev;
    let mut scopes = make_scopes(
        workspace_doc,
        env,
        collection,
        &folder_vars,
        &doc,
        &iteration_vars,
        input.transient,
        &env_values,
    );

    let resolve_template = |scopes: &ScopeStack, template: &str, stats: &mut ResolveStats| -> String {
        // Previous-response tags first (unresolved ones are reported as
        // missing), then `{{vars}}` on the result.
        let staged =
            crate::engine::variables::apply_prev_refs_report(template, prev, &mut stats.missing);
        let interp = Interpolator {
            scopes,
            secrets: secret_source,
        };
        let r = interp.resolve(&staged);
        stats.absorb(&r);
        r.value
    };

    // ---------- pre-request scripts ----------
    let pre_req_state = ReqState {
        method: doc.request.method,
        url: resolve_template(&scopes, &doc.request.url, &mut stats),
        headers: enabled_rows(doc.request.headers.as_ref())
            .into_iter()
            .map(|kv| (kv.name, resolve_template(&scopes, &kv.value, &mut stats)))
            .collect(),
        body: match body_preview(&doc) {
            Ok(s) => s,
            Err(e) => {
                return finish_error(
                    &doc,
                    &template_url, &env_label, &request_path, &e,
                    logs, None, stats, timeline,
                )
            }
        },
    };
    {
        let pre_cell = std::rc::Rc::new(std::cell::RefCell::new(Host::new_pre(
            plain_snapshot(&scopes),
            collection_vars.clone(),
            folder_vars.clone(),
            env_label.clone().unwrap_or_default(),
            std::mem::take(input.transient),
            pre_req_state,
        )));
        {
            let mut aborted: Option<String> = None;
            for src in script_sources(&doc, WhichScript::Pre) {
                match js::run(&src, &pre_cell) {
                    Ok(outcome) => {
                        if outcome.skip_request || outcome.stop_execution {
                            break;
                        }
                    }
                    Err(e) => {
                        aborted = Some(e);
                        break;
                    }
                }
            }
            if let Some(e) = aborted {
                let h = pre_cell.borrow();
                // The transient map was moved into the script host; restore it
                // (including any vars set before the failing script) so the
                // session state survives a broken pre-request script.
                *input.transient = h.transient.clone();
                persist_env_updates(
                    input.workspace_root.as_deref(),
                    input.env_file.as_deref(),
                    env,
                    &h.env_updates,
                    &mut env_values,
                );
                return finish_error(
                    &doc,
                    &template_url,
                    &env_label,
                    &request_path,
                    &format!("pre-request script error: {e}"),
                    h.logs.clone(),
                    Some(e),
                    stats,
                    timeline,
                );
            }
        }
        {
            let h = pre_cell.borrow();
            *input.transient = h.transient.clone();
            persist_env_updates(
                input.workspace_root.as_deref(),
                input.env_file.as_deref(),
                env,
                &h.env_updates,
                &mut env_values,
            );
            logs = h.logs.clone();
            js_tests.extend(h.tests.iter().cloned());
            doc.request.method = h.req.method;
            let script_headers: Vec<(String, String)> = h.req.headers.clone();
            let script_url = h.req.url.clone();
            let script_body = h.req.body.clone();
            drop(h);
            // Rebuild scopes so script-set vars apply to the request itself.
            scopes = make_scopes(
                workspace_doc,
                env,
                collection,
                &folder_vars,
                &doc,
                &iteration_vars,
                input.transient,
                &env_values,
            );
            let final_url = resolve_template(&scopes, &script_url, &mut stats);
            let final_headers: Vec<crate::model::KV> = script_headers
                .into_iter()
                .filter(|(k, _)| !k.trim().is_empty())
                .map(|(name, tmpl)| crate::model::KV {
                    name,
                    value: resolve_template(&scopes, &tmpl, &mut stats),
                    enabled: true,
                    kind: None,
                })
                .collect();
            doc.request.url = final_url;
            doc.request.headers = Some(final_headers);
            if let Some(body) = script_body {
                doc.request.body = Some(crate::model::Body {
                    body_type: detect_text_body_type(&doc.request.body),
                    content: Some(body),
                    ..Default::default()
                });
            }
        }

    }

    // ---------- URL: query + path params ----------
    let mut query_pairs: Vec<(String, String)> = enabled_rows(doc.request.params.as_ref())
        .into_iter()
        .filter(|kv| !kv.name.trim().is_empty())
        .map(|kv| (kv.name, resolve_template(&scopes, &kv.value, &mut stats)))
        .collect();

    let mut headers: Vec<(String, String)> = enabled_rows(doc.request.headers.as_ref())
        .into_iter()
        .filter(|kv| !kv.name.trim().is_empty())
        .map(|kv| (kv.name, kv.value)) // resolved above
        .collect();

    let base_url = doc.request.url.clone();
    let auth: Option<crate::model::Auth> = {
        let a = doc.auth.clone().filter(|a| a.auth_type != AuthType::None);
        a.map(|a| resolve_auth(&a, &scopes, &mut stats, secret_source_ref(secret_source), prev))
    };

    if let Some(auth) = &auth {
        auth_used = Some(label_auth(auth.auth_type));
        match auth.auth_type {
            AuthType::Bearer => {
                let token = auth.token.clone().unwrap_or_default();
                push_header(&mut headers, "Authorization", &format!("Bearer {token}"));
            }
            AuthType::Basic => {
                use base64::Engine as _;
                let user = auth.username.clone().unwrap_or_default();
                let pass = auth.password.clone().unwrap_or_default();
                let enc = base64::engine::general_purpose::STANDARD
                    .encode(format!("{user}:{pass}"));
                push_header(&mut headers, "Authorization", &format!("Basic {enc}"));
            }
            AuthType::Apikey => {
                let key = auth.key.clone().unwrap_or_default();
                let value = auth.value.clone().unwrap_or_default();
                match auth.location.as_deref().unwrap_or("header") {
                    "query" => query_pairs.push((key, value)),
                    _ => push_header(&mut headers, &key, &value),
                }
            }
            AuthType::Oauth2 => {
                if let Some(cfg) = oauth2::Oauth2Config::from_auth(auth) {
                    timeline.push(TimelineEventDto {
                        ts: now_ts(),
                        phase: "auth".into(),
                        message: format!("oauth2: {}", cfg.grant_type),
                    });
                    match oauth2::get_token(&cfg, oauth_cache, open_browser).await {
                        Ok(tok) => {
                            oauth2::apply_token(&cfg, &tok, &mut headers, &mut query_pairs);
                            timeline.push(TimelineEventDto {
                                ts: now_ts(),
                                phase: "auth".into(),
                                message: "oauth2: token acquired".into(),
                            });
                        }
                        Err(e) => {
                            return finish_error(
                                &doc,
                                &template_url,
                                &env_label,
                                &request_path,
                                &format!("OAuth2 error: {e}"),
                                logs,
                                script_error,
                                stats,
                                timeline,
                            )
                        }
                    }
                }
            }
            AuthType::Digest | AuthType::None => {}
        }
    }

    // Path params.
    let path_param_rows: Vec<(String, String)> = enabled_rows(doc.request.path_params.as_ref())
        .into_iter()
        .map(|kv| (kv.name, resolve_template(&scopes, &kv.value, &mut stats)))
        .collect();

    let mut final_url = match url_with_query(&base_url, &query_pairs) {
        Ok(u) => u,
        Err(e) => {
            return finish_error(&doc, &template_url, &env_label, &request_path, &e, logs, script_error, stats, timeline)
        }
    };
    final_url = match path_params::apply(&final_url, &path_param_rows) {
        Ok(u) => u,
        Err(e) => {
            return finish_error(&doc, &template_url, &env_label, &request_path, &e, logs, script_error, stats, timeline)
        }
    };

    let prepared_body = match build_body(&doc, |t| resolve_template(&scopes, t, &mut stats)) {
        Ok(b) => b,
        Err(e) => {
            return finish_error(&doc, &template_url, &env_label, &request_path, &e, logs, script_error, stats, timeline)
        }
    };

    // Cookies (send).
    if let (true, Some(jar)) = (send_cookies, cookie_jar) {
        if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("cookie")) {
            if let Some(header) = jar.lock().map(|j| j.header_for(&final_url)).unwrap_or(None) {
                headers.push(("Cookie".into(), header));
            }
        }
    }

    let prepared = PreparedRequest {
        method: doc.request.method,
        url: final_url.clone(),
        headers: headers.clone(),
        body: prepared_body,
    };

    timeline.push(TimelineEventDto {
        ts: now_ts(),
        phase: "prepared".into(),
        message: format!("{} {}", doc.request.method.as_str(), redact_url_template(&doc)),
    });

    // ---------- exchange (+ digest retry) ----------
    let digest_cfg = auth.as_ref().filter(|a| a.auth_type == AuthType::Digest).cloned();
    let exchange = match execute(&prepared, &http).await {
        Ok(ex) => {
            let www = header_value(&ex.headers, "www-authenticate");
            match (&digest_cfg, &ex) {
            (Some(cfg), ex) if digest::needs_retry(ex.status.unwrap_or(0) as u16, www.as_deref()) => {
                timeline.push(TimelineEventDto {
                    ts: now_ts(),
                    phase: "auth".into(),
                    message: "digest: 401 challenge, retrying".into(),
                });
                let challenge_raw = header_value(&ex.headers, "www-authenticate").unwrap_or_default();
                let Some(challenge) = digest::parse_challenge(&challenge_raw) else {
                    return finish_error(
                        &doc,
                        &template_url, &env_label, &request_path,
                        "Digest challenge not understood", logs, script_error, stats, timeline,
                    );
                };
                let user = resolve_template(&scopes, cfg.username.as_deref().unwrap_or(""), &mut stats);
                let pass = resolve_template(&scopes, cfg.password.as_deref().unwrap_or(""), &mut stats);
                let parsed = match reqwest::Url::parse(&final_url) {
                    Ok(p) => p,
                    Err(e) => {
                        return finish_error(
                            &doc,
                            &template_url, &env_label, &request_path,
                            &format!("invalid URL: {e}"), logs, script_error, stats, timeline,
                        )
                    }
                };
                let uri = format!("{}{}", parsed.path(), parsed.query().map(|q| format!("?{q}")).unwrap_or_default());
                let header = digest::build_header(
                    &challenge,
                    doc.request.method.as_str(),
                    &uri,
                    &user,
                    &pass,
                );
                let mut retry_headers = prepared.headers.clone();
                push_header(&mut retry_headers, "Authorization", &header);
                let mut retry = prepared.clone();
                retry.headers = retry_headers;
                match execute(&retry, &http).await {
                    Ok(ex) => ex,
                    Err(e) => {
                        return finish_error(
                            &doc,
                            &template_url, &env_label, &request_path, &e, logs, script_error, stats, timeline,
                        )
                    }
                }
            }
            _ => ex,
            }
        }
        Err(e) => {
            return finish_error(&doc, &template_url, &env_label, &request_path, &e, logs, script_error, stats, timeline)
        }
    };

    timeline.push(TimelineEventDto {
        ts: now_ts(),
        phase: "response".into(),
        message: format!(
            "{} in {:.0} ms · {} bytes",
            exchange.status.map(|s| s.to_string()).unwrap_or_else(|| "no response".into()),
            exchange.time_ms,
            exchange.size_bytes
        ),
    });

    // Cookies (store).
    if let (true, Some(jar)) = (store_cookies, cookie_jar) {
        if let Ok(mut jar) = jar.lock() {
            jar.absorb(&final_url, &exchange.set_cookie_raw);
        }
    }

    // ---------- response context + post-response scripts ----------
    let mut ctx_body: Option<String> = exchange.body_text.clone();
    let mut res_state = ResState {
        status: exchange.status,
        status_text: exchange.status_text.clone(),
        headers: exchange.headers.clone(),
        body: ctx_body.clone(),
        time_ms: exchange.time_ms,
        size: exchange.size_bytes,
    };

    {
        let post_cell = std::rc::Rc::new(std::cell::RefCell::new(Host::new_post(
            plain_snapshot(&scopes),
            collection_vars,
            folder_vars,
            env_label.clone().unwrap_or_default(),
            std::mem::take(input.transient),
            res_state.clone(),
        )));
        let mut err: Option<String> = None;
        for src in script_sources(&doc, WhichScript::Post) {
            if let Err(e) = js::run(&src, &post_cell) {
                err = Some(e);
                break;
            }
        }
        let h = post_cell.borrow();
        *input.transient = h.transient.clone();
        persist_env_updates(
            input.workspace_root.as_deref(),
            input.env_file.as_deref(),
            env,
            &h.env_updates,
            &mut env_values,
        );
        logs.extend(h.logs.iter().cloned());
        js_tests.extend(h.tests.iter().cloned());
        if let Some(body) = &h.body_override {
            ctx_body = Some(body.clone());
            res_state.body = ctx_body.clone();
        }
        drop(h);
        if let Some(e) = err {
            script_error = Some(e);
        }
    }

    let ctx = ResponseCtx {
        status: exchange.status,
        time_ms: exchange.time_ms,
        size: exchange.size_bytes,
        headers: exchange.headers.clone(),
        cookies: exchange.cookies.clone(),
        body: ctx_body.clone(),
        json: ctx_body
            .as_ref()
            .and_then(|t| serde_json::from_str(t).ok()),
    };

    // ---------- tests ----------
    let mut test_results: Vec<TestResultDto> =
        run_tests(doc.tests.as_deref().unwrap_or(&[]), &ctx)
            .into_iter()
            .map(|r| TestResultDto {
                expect: r.expect,
                matcher: r.matcher,
                expected: r.expected,
                actual: r.actual,
                passed: r.passed,
                message: r.message,
            })
            .collect();
    test_results.extend(js_tests);

    let status = exchange.status;
    SendOutput {
        history: Some(HistoryRecord {
            ts: now_ts(),
            method: doc.request.method.as_str().to_string(),
            url: template_url,
            status,
            ok: status.map(|s| s < 400).unwrap_or(false),
            time_ms: exchange.time_ms,
            env: env_label.clone(),
            request_path,
        }),
        captured: Some(PrevResponse {
            status: exchange.status,
            headers: exchange.headers.clone(),
            body: ctx_body.clone(),
            json: ctx_body.as_ref().and_then(|t| serde_json::from_str(t).ok()),
        }),
        result: SendResult {
            request_id: uuid::Uuid::new_v4().to_string(),
            status: exchange.status,
            status_text: exchange.status_text,
            ok: status.map(|s| s < 400).unwrap_or(false),
            time_ms: exchange.time_ms,
            size_bytes: exchange.size_bytes,
            headers: exchange
                .headers
                .into_iter()
                .map(|(name, value)| HeaderDto { name, value })
                .collect(),
            cookies: exchange
                .cookies
                .into_iter()
                .map(|(name, value)| HeaderDto { name, value })
                .collect(),
            content_type: exchange.content_type,
            body_text: ctx_body,
            body_base64: exchange.body_base64,
            truncated: exchange.truncated,
            error: None,
            variables_used: stats.used,
            missing_variables: stats.missing,
            secrets_used: stats.secrets_used,
            test_results,
            script_logs: logs,
            script_error,
            timeline,
            auth_used,
        },
    }
}

fn resolve_auth(
    auth: &crate::model::Auth,
    scopes: &ScopeStack,
    stats: &mut ResolveStats,
    secret: &dyn SecretSource,
    prev: Option<&PrevResponse>,
) -> crate::model::Auth {
    let mut r = |v: &Option<String>| -> Option<String> {
        v.as_ref().map(|t| {
            // `#{…}` tags first (unresolved ones are reported as missing),
            // then `{{vars}}`.
            let staged = crate::engine::variables::apply_prev_refs_report(
                t,
                prev,
                &mut stats.missing,
            );
            let interp = Interpolator { scopes, secrets: secret };
            let res = interp.resolve(&staged);
            stats.absorb(&res);
            res.value
        })
    };
    crate::model::Auth {
        auth_type: auth.auth_type,
        token: r(&auth.token),
        username: r(&auth.username),
        password: r(&auth.password),
        key: r(&auth.key),
        value: r(&auth.value),
        location: auth.location.clone(),
        grant_type: auth.grant_type,
        access_token_url: r(&auth.access_token_url),
        refresh_token_url: r(&auth.refresh_token_url),
        authorization_url: r(&auth.authorization_url),
        callback_url: r(&auth.callback_url),
        client_id: r(&auth.client_id),
        client_secret: r(&auth.client_secret),
        scope: r(&auth.scope),
        state: r(&auth.state),
        pkce: auth.pkce,
        credentials_placement: auth.credentials_placement.clone(),
        token_placement: auth.token_placement.clone(),
        token_header_prefix: auth.token_header_prefix.clone(),
        token_query_key: auth.token_query_key.clone(),
    }
}

fn secret_source_ref<'a>(s: &'a dyn SecretSource) -> &'a dyn SecretSource {
    s
}

fn label_auth(t: AuthType) -> String {
    match t {
        AuthType::Bearer => "bearer",
        AuthType::Basic => "basic",
        AuthType::Apikey => "apikey",
        AuthType::Digest => "digest",
        AuthType::Oauth2 => "oauth2",
        AuthType::None => "none",
    }
    .to_string()
}

fn push_header(headers: &mut Vec<(String, String)>, name: &str, value: &str) {
    headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
    headers.push((name.to_string(), value.to_string()));
}

fn header_value(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

fn redact_url_template(doc: &RequestDoc) -> String {
    doc.request.url.clone()
}

fn detect_text_body_type(existing: &Option<crate::model::Body>) -> BodyType {
    existing
        .as_ref()
        .map(|b| match b.body_type {
            BodyType::Json => BodyType::Json,
            BodyType::Xml => BodyType::Xml,
            _ => BodyType::Text,
        })
        .unwrap_or(BodyType::Text)
}

/// Resolve the body to text for JS `req.getBody()` in pre scripts.
fn body_preview(doc: &RequestDoc) -> Result<Option<String>, String> {
    let Some(body) = &doc.request.body else {
        return Ok(None);
    };
    Ok(match body.body_type {
        BodyType::None => None,
        BodyType::Json | BodyType::Text | BodyType::Xml => body.content.clone(),
        BodyType::FormUrlencoded | BodyType::Multipart => Some(
            body.items
                .iter()
                .flatten()
                .filter(|kv| kv.enabled)
                .map(|kv| format!("{}={}", kv.name, kv.value))
                .collect::<Vec<_>>()
                .join("&"),
        ),
        BodyType::Graphql => body.query.clone(),
        BodyType::Binary => Some(format!("<binary: {}>", body.path.clone().unwrap_or_default())),
    })
}

fn build_body(
    doc: &RequestDoc,
    mut resolve_str: impl FnMut(&str) -> String,
) -> Result<PreparedBody, String> {
    let Some(body) = &doc.request.body else {
        return Ok(PreparedBody::None);
    };
    Ok(match body.body_type {
        BodyType::None => PreparedBody::None,
        BodyType::Json => {
            let raw = body.content.as_deref().unwrap_or("");
            if raw.trim().is_empty() {
                return Err("body type is JSON but the content is empty".into());
            }
            let value = substitute_json_tags(raw, &mut resolve_str)
                .map_err(|e| format!("body is not valid JSON: {e}"))?;
            let bytes = serde_json::to_vec(&value).map_err(|e| format!("json error: {e}"))?;
            PreparedBody::Raw {
                content_type: Some("application/json"),
                bytes,
            }
        }
        BodyType::Text => PreparedBody::Raw {
            content_type: Some("text/plain"),
            bytes: resolve_str(body.content.as_deref().unwrap_or("")).into_bytes(),
        },
        BodyType::Xml => PreparedBody::Raw {
            content_type: Some("application/xml"),
            bytes: resolve_str(body.content.as_deref().unwrap_or("")).into_bytes(),
        },
        BodyType::Graphql => {
            let query = resolve_str(body.query.as_deref().unwrap_or(""));
            if query.trim().is_empty() {
                return Err("GraphQL body has an empty query".into());
            }
            let variables = body
                .variables
                .as_deref()
                .filter(|v| !v.trim().is_empty())
                .map(|v| {
                    substitute_json_tags(v, &mut resolve_str)
                        .map_err(|e| format!("GraphQL variables are not valid JSON: {e}"))
                })
                .transpose()?;
            let payload = serde_json::json!({ "query": query, "variables": variables });
            PreparedBody::Raw {
                content_type: Some("application/json"),
                bytes: serde_json::to_vec(&payload).expect("json"),
            }
        }
        BodyType::FormUrlencoded => PreparedBody::Form(
            enabled_rows(body.items.as_ref())
                .into_iter()
                .filter(|kv| !kv.name.trim().is_empty())
                .map(|kv| (kv.name, resolve_str(&kv.value)))
                .collect(),
        ),
        BodyType::Multipart => PreparedBody::Multipart(
            enabled_rows(body.items.as_ref())
                .into_iter()
                .filter(|kv| !kv.name.trim().is_empty())
                .map(|kv| Part {
                    name: kv.name,
                    value: resolve_str(&kv.value),
                    is_file: kv.kind.as_deref() == Some("file"),
                })
                .collect(),
        ),
        BodyType::Binary => {
            let path = body.path.as_deref().unwrap_or("").trim().to_string();
            if path.is_empty() {
                return Err("binary body: no file path set".into());
            }
            let bytes = std::fs::read(&path).map_err(|e| format!("cannot read `{path}`: {e}"))?;
            PreparedBody::Raw {
                content_type: Some("application/octet-stream"),
                bytes,
            }
        }
    })
}

/// Parses JSON text after substituting `#{…}` / `{{…}}` tags in a JSON-aware
/// way: inside a string the resolved value is escaped, and a tag outside a
/// string is wrapped in quotes (tags always produce strings). That keeps
/// bodies like `"password": #{body.publicKey}` — or a resolved value
/// containing `"` — parseable. Tags that do not resolve are kept as literal
/// text (they are reported as missing by the resolver).
fn substitute_json_tags(
    raw: &str,
    resolve: &mut impl FnMut(&str) -> String,
) -> Result<serde_json::Value, String> {
    let re = regex::Regex::new(r"#\{[^{}#]+\}|\{\{[^{}]+\}\}").expect("static regex");
    let mut out = String::with_capacity(raw.len());
    let mut last = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for caps in re.captures_iter(raw) {
        let m = caps.get(0).expect("group 0");
        let before = &raw[last..m.start()];
        out.push_str(before);
        track_json_state(before, &mut in_string, &mut escaped);
        let tag = m.as_str();
        let resolved = resolve(tag);
        let emitted = if resolved == tag {
            // Unresolved: keep the literal tag, wrapped in quotes when it
            // sits outside a string so the body still parses.
            if in_string {
                tag.to_string()
            } else {
                serde_json::to_string(tag).expect("string serializes")
            }
        } else if in_string {
            json_string_body(&resolved)
        } else {
            serde_json::to_string(&resolved).expect("string serializes")
        };
        out.push_str(&emitted);
        track_json_state(&emitted, &mut in_string, &mut escaped);
        last = m.end();
    }
    out.push_str(&raw[last..]);
    serde_json::from_str(&out).map_err(|e| e.to_string())
}

/// Tracks JSON string state over `text`: `in_string` toggles on unescaped
/// quotes, `escaped` carries a backslash escape across chunks.
fn track_json_state(text: &str, in_string: &mut bool, escaped: &mut bool) {
    for ch in text.chars() {
        if *escaped {
            *escaped = false;
            continue;
        }
        match ch {
            '\\' if *in_string => *escaped = true,
            '"' => *in_string = !*in_string,
            _ => {}
        }
    }
}

/// Escapes `value` for interpolation inside an existing JSON string.
fn json_string_body(value: &str) -> String {
    let quoted = serde_json::to_string(value).expect("string serializes");
    quoted[1..quoted.len() - 1].to_string()
}

fn finish_error(
    doc: &RequestDoc,
    template_url: &str,
    env_label: &Option<String>,
    request_path: &Option<String>,
    error: &str,
    script_logs: Vec<String>,
    script_error: Option<String>,
    stats: ResolveStats,
    mut timeline: Vec<TimelineEventDto>,
) -> SendOutput {
    timeline.push(TimelineEventDto {
        ts: now_ts(),
        phase: "error".into(),
        message: error.to_string(),
    });
    let history = HistoryRecord {
        ts: now_ts(),
        method: doc.request.method.as_str().to_string(),
        url: template_url.to_string(),
        status: None,
        ok: false,
        time_ms: 0.0,
        env: env_label.clone(),
        request_path: request_path.clone(),
    };
    SendOutput {
        captured: None,
        result: SendResult {
            request_id: uuid::Uuid::new_v4().to_string(),
            status: None,
            status_text: String::new(),
            ok: false,
            time_ms: 0.0,
            size_bytes: 0,
            headers: vec![],
            cookies: vec![],
            content_type: None,
            body_text: None,
            body_base64: None,
            truncated: false,
            error: Some(error.to_string()),
            variables_used: stats.used,
            missing_variables: stats.missing,
            secrets_used: stats.secrets_used,
            test_results: vec![],
            script_logs,
            script_error,
            timeline,
            auth_used: None,
        },
        history: Some(history),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RequestBlock, Scripts, SCHEMA_VERSION};

    struct FakeSecrets;
    impl SecretSource for FakeSecrets {
        fn get(&self, _name: &str) -> Result<String, String> {
            Ok("fake-secret".into())
        }
    }

    fn env_doc(variables: &[(&str, &str)], secrets: &[(&str, &str)]) -> EnvDoc {
        EnvDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: "local".into(),
            description: None,
            variables: Some(
                variables
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ),
            secrets: Some(
                secrets
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ),
        }
    }

    #[test]
    fn current_value_wins_over_default() {
        let env = env_doc(&[("baseUrl", "http://default"), ("other", "keep")], &[]);
        let doc = doc_with_pre_script("");
        let transient = BTreeMap::new();
        let env_values = BTreeMap::from([("baseUrl".to_string(), "http://current".to_string())]);
        let scopes = make_scopes(
            None,
            Some(&env),
            None,
            &BTreeMap::new(),
            &doc,
            &BTreeMap::new(),
            &transient,
            &env_values,
        );
        let r = Interpolator { scopes: &scopes, secrets: &FakeSecrets }
            .resolve("{{baseUrl}}|{{other}}");
        assert_eq!(r.value, "http://current|keep");
    }

    #[test]
    fn default_used_when_no_current_value() {
        let env = env_doc(&[("baseUrl", "http://default")], &[]);
        let doc = doc_with_pre_script("");
        let transient = BTreeMap::new();
        let scopes = make_scopes(
            None,
            Some(&env),
            None,
            &BTreeMap::new(),
            &doc,
            &BTreeMap::new(),
            &transient,
            &BTreeMap::new(),
        );
        let r = Interpolator { scopes: &scopes, secrets: &FakeSecrets }.resolve("{{baseUrl}}");
        assert_eq!(r.value, "http://default");
    }

    #[test]
    fn current_value_does_not_override_secret_names() {
        // A variable override must not hijack a secret's default/current split.
        let env = env_doc(&[], &[("apiToken", "default-token")]);
        let doc = doc_with_pre_script("");
        let transient = BTreeMap::new();
        let env_values = BTreeMap::from([("apiToken".to_string(), "sneaky".to_string())]);
        let scopes = make_scopes(
            None,
            Some(&env),
            None,
            &BTreeMap::new(),
            &doc,
            &BTreeMap::new(),
            &transient,
            &env_values,
        );
        let scope = scopes.scopes.first().expect("env scope");
        assert!(scope.secret_names.contains("apiToken"));
        // Keychain (FakeSecrets) still wins; the override never landed.
        let r = Interpolator { scopes: &scopes, secrets: &FakeSecrets }.resolve("{{apiToken}}");
        assert_eq!(r.value, "fake-secret");
    }

    fn doc_with_pre_script(script: &str) -> RequestDoc {
        RequestDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: "t".into(),
            kind: "request".into(),
            description: None,
            protocol: None,
            graphql: None,
            websocket: None,
            grpc: None,
            request: RequestBlock {
                // Closed port: the request must never get this far anyway.
                url: "http://127.0.0.1:1/x".into(),
                ..RequestBlock::default()
            },
            auth: None,
            variables: None,
            scripts: Some(Scripts {
                pre_request: Some(script.into()),
                post_response: None,
            }),
            tests: None,
        }
    }

    #[tokio::test]
    async fn set_env_var_persists_current_value() {
        let dir = tempfile::TempDir::new().expect("temp");
        let env = env_doc(&[("FLOW", "")], &[]);
        let doc = doc_with_pre_script(r#"keel.setEnvVar("FLOW", "flow-1");"#);
        let oauth_cache = Oauth2Cache::default();
        let noop_browser = |_: &str| -> Result<(), String> { Ok(()) };
        let mut transient = BTreeMap::new();

        let out = send(SendInput {
            doc: &doc,
            env_label: Some("local".into()),
            env: Some(&env),
            collection: None,
            workspace_doc: None,
            transient: &mut transient,
            prev: None,
            http: HttpOptions::default(),
            secret_source: &FakeSecrets,
            request_path: None,
            collection_vars: Default::default(),
            folder_vars: Default::default(),
            env_values: Default::default(),
            env_file: Some("local.yaml".into()),
            workspace_root: Some(dir.path().to_path_buf()),
            oauth_cache: &oauth_cache,
            open_browser: &noop_browser,
            cookie_jar: None,
            send_cookies: false,
            store_cookies: false,
            iteration_vars: Default::default(),
        })
        .await;

        let _ = out;
        let stored = crate::workspace::env_values_read(dir.path(), "local.yaml");
        assert_eq!(stored.get("FLOW").map(String::as_str), Some("flow-1"));
    }

    #[tokio::test]
    async fn pre_script_error_preserves_transient() {
        let doc = doc_with_pre_script(r#"set("kept", "yes"); throw new Error("boom")"#);
        let oauth_cache = Oauth2Cache::default();
        let noop_browser = |_: &str| -> Result<(), String> { Ok(()) };
        let mut transient = BTreeMap::from([("token".to_string(), "tok-1".to_string())]);

        let out = send(SendInput {
            doc: &doc,
            env_label: None,
            env: None,
            collection: None,
            workspace_doc: None,
            transient: &mut transient,
            prev: None,
            http: HttpOptions::default(),
            secret_source: &FakeSecrets,
            request_path: None,
            collection_vars: Default::default(),
            folder_vars: Default::default(),
            env_values: Default::default(),
            env_file: None,
            workspace_root: None,
            oauth_cache: &oauth_cache,
            open_browser: &noop_browser,
            cookie_jar: None,
            send_cookies: false,
            store_cookies: false,
            iteration_vars: Default::default(),
        })
        .await;

        let err = out.result.error.expect("error");
        assert!(err.contains("pre-request script error"), "{err}");
        // Pre-existing session vars survive...
        assert_eq!(transient.get("token").map(String::as_str), Some("tok-1"));
        // ...and so do vars set by the script before it failed.
        assert_eq!(transient.get("kept").map(String::as_str), Some("yes"));
    }

    #[tokio::test]
    async fn pre_script_syntax_error_preserves_transient() {
        let doc = doc_with_pre_script("this is not (valid js");
        let oauth_cache = Oauth2Cache::default();
        let noop_browser = |_: &str| -> Result<(), String> { Ok(()) };
        let mut transient = BTreeMap::from([("token".to_string(), "tok-1".to_string())]);

        let out = send(SendInput {
            doc: &doc,
            env_label: None,
            env: None,
            collection: None,
            workspace_doc: None,
            transient: &mut transient,
            prev: None,
            http: HttpOptions::default(),
            secret_source: &FakeSecrets,
            request_path: None,
            collection_vars: Default::default(),
            folder_vars: Default::default(),
            env_values: Default::default(),
            env_file: None,
            workspace_root: None,
            oauth_cache: &oauth_cache,
            open_browser: &noop_browser,
            cookie_jar: None,
            send_cookies: false,
            store_cookies: false,
            iteration_vars: Default::default(),
        })
        .await;

        assert!(out.result.error.is_some());
        assert_eq!(transient.get("token").map(String::as_str), Some("tok-1"));
    }
}

#[cfg(test)]
mod json_body_tests {
    use super::*;

    fn resolver(map: &[(&str, &str)]) -> impl FnMut(&str) -> String {
        let map: Vec<(String, String)> = map
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |tag: &str| {
            map.iter()
                .find(|(k, _)| k == tag)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| tag.to_string())
        }
    }

    #[test]
    fn bare_tag_outside_string_is_quoted() {
        let raw = "{\n  \"password\": #{body.publicKey},\n  \"username\": \"CUST-1\"\n}";
        let mut resolve = resolver(&[("#{body.publicKey}", "pk-1")]);
        let v = substitute_json_tags(raw, &mut resolve).expect("parses");
        assert_eq!(v["password"], "pk-1");
        assert_eq!(v["username"], "CUST-1");
    }

    #[test]
    fn resolved_value_with_quotes_and_newline_is_escaped() {
        let raw = r##"{"password": "#{body.pk}"}"##;
        let mut resolve = resolver(&[("#{body.pk}", "a\"b\nc")]);
        let v = substitute_json_tags(raw, &mut resolve).expect("parses");
        assert_eq!(v["password"], "a\"b\nc");
    }

    #[test]
    fn unresolved_bare_tag_stays_literal() {
        let raw = r#"{"password": #{body.pk}}"#;
        let mut resolve = resolver(&[]);
        let v = substitute_json_tags(raw, &mut resolve).expect("parses");
        assert_eq!(v["password"], "#{body.pk}");
    }

    #[test]
    fn var_inside_string_is_escaped() {
        let raw = r##"{"tok": "{{token}}"}"##;
        let mut resolve = resolver(&[("{{token}}", "x\"y")]);
        let v = substitute_json_tags(raw, &mut resolve).expect("parses");
        assert_eq!(v["tok"], "x\"y");
    }

    #[test]
    fn genuinely_broken_json_still_errors() {
        let raw = r#"{"a": }"#;
        let mut resolve = resolver(&[]);
        let err = substitute_json_tags(raw, &mut resolve).expect_err("errors");
        assert!(err.contains("expected"), "{err}");
    }
}
