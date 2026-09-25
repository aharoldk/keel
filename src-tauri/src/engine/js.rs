//! Keel scripting: a real JavaScript runtime (boa_engine) for pre-request,
//! post-response and test scripts.
//!
//! A single native function `__keel(op, payloadJson) -> resultJson` is the
//! only host boundary; a JS shim (evaluated before user code) builds the
//! documented globals (`keel`, `req`, `res`, `test`, `expect`, `console`, `pm`,
//! plus the v0-DSL compatibility functions) on top of it. This keeps the FFI
//! surface tiny, testable and versionable.
//!
//! Security model: scripts are local files the user (or their git history)
//! authored. They can read plain variables but **never** secret values —
//! `keel.getVar` on a secret name returns null; secrets only enter HTTP
//! headers/URLs via the interpolation pipeline in `engine::send`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{json, Value};

use crate::model::{HttpMethod, TestResultDto};

#[derive(Debug, Clone)]
pub struct ReqState {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ResState {
    pub status: Option<i64>,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub time_ms: f64,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Pre,
    Post,
}

/// Shared mutable state behind `__keel`.
pub struct Host {
    pub phase: Phase,
    /// Merged plain-variable snapshot (transient wins). Secrets excluded.
    pub scope: BTreeMap<String, String>,
    /// Collection-layer variables for `keel.getCollectionVar`.
    pub collection_vars: BTreeMap<String, String>,
    /// Folder-layer merged variables for `keel.getFolderVar`.
    pub folder_vars: BTreeMap<String, String>,
    pub env_name: String,
    pub transient: BTreeMap<String, String>,
    /// `keel.setEnvVar` writes. Persisted as the active environment's current
    /// values (`.keel/env-values.yaml`) by the send caller. `None` means delete.
    pub env_updates: BTreeMap<String, Option<String>>,
    pub req: ReqState,
    pub res: Option<ResState>,
    pub logs: Vec<String>,
    pub tests: Vec<TestResultDto>,
    pub next_request: Option<Option<String>>,
    pub skip_request: bool,
    pub stop_execution: bool,
    /// `res.setBody(...)` in post phase.
    pub body_override: Option<String>,
}

impl Host {
    pub fn new_pre(
        scope: BTreeMap<String, String>,
        collection_vars: BTreeMap<String, String>,
        folder_vars: BTreeMap<String, String>,
        env_name: String,
        transient: BTreeMap<String, String>,
        req: ReqState,
    ) -> Self {
        Self {
            phase: Phase::Pre,
            scope,
            collection_vars,
            folder_vars,
            env_name,
            transient,
            env_updates: BTreeMap::new(),
            req,
            res: None,
            logs: Vec::new(),
            tests: Vec::new(),
            next_request: None,
            skip_request: false,
            stop_execution: false,
            body_override: None,
        }
    }

    pub fn into_transient(self) -> BTreeMap<String, String> {
        self.transient
    }

    /// Empty placeholder used when a host must be taken out of an `Rc`.
    pub fn blank() -> Self {
        Self {
            phase: Phase::Pre,
            scope: BTreeMap::new(),
            collection_vars: BTreeMap::new(),
            folder_vars: BTreeMap::new(),
            env_name: String::new(),
            transient: BTreeMap::new(),
            env_updates: BTreeMap::new(),
            req: ReqState {
                method: HttpMethod::GET,
                url: String::new(),
                headers: Vec::new(),
                body: None,
            },
            res: None,
            logs: Vec::new(),
            tests: Vec::new(),
            next_request: None,
            skip_request: false,
            stop_execution: false,
            body_override: None,
        }
    }

    pub fn new_post(
        scope: BTreeMap<String, String>,
        collection_vars: BTreeMap<String, String>,
        folder_vars: BTreeMap<String, String>,
        env_name: String,
        transient: BTreeMap<String, String>,
        res: ResState,
    ) -> Self {
        let mut h = Self::blank();
        h.phase = Phase::Post;
        h.scope = scope;
        h.collection_vars = collection_vars;
        h.folder_vars = folder_vars;
        h.env_name = env_name;
        h.transient = transient;
        h.res = Some(res);
        h
    }
}

/// Outcome of running one script source.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub logs: Vec<String>,
    pub tests: Vec<TestResultDto>,
    pub next_request: Option<Option<String>>,
    pub skip_request: bool,
    pub stop_execution: bool,
}

fn to_storage(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

impl Host {
    fn dispatch(&mut self, op: &str, payload: &Value) -> Result<Value, String> {
        let pre = self.phase == Phase::Pre;
        let post = self.phase == Phase::Post;
        match op {
            // ---- variables ----
            "getVar" => {
                let name = str_field(payload, "name")?;
                Ok(json!({ "value": self.scope.get(&name).cloned() }))
            }
            "hasVar" => {
                let name = str_field(payload, "name")?;
                Ok(json!({ "value": self.scope.contains_key(&name) }))
            }
            "getAllVars" => Ok(json!({ "value": self.transient })),
            "setVar" => {
                let name = str_field(payload, "name")?;
                let value = payload.get("value").cloned().unwrap_or(Value::Null);
                self.scope.insert(name.clone(), to_storage(&value));
                self.transient.insert(name, to_storage(&value));
                Ok(json!({}))
            }
            "setEnvVar" => {
                let name = str_field(payload, "name")?;
                if name.trim().is_empty() {
                    return Err("setEnvVar: variable name is empty".into());
                }
                let value = payload.get("value").cloned().unwrap_or(Value::Null);
                let stored = to_storage(&value);
                // Empty clears the current value so the committed default wins.
                let current = if stored.is_empty() { None } else { Some(stored.clone()) };
                self.scope.insert(name.clone(), stored.clone());
                self.transient.insert(name.clone(), stored);
                self.env_updates.insert(name, current);
                Ok(json!({}))
            }
            "deleteVar" => {
                let name = str_field(payload, "name")?;
                self.scope.remove(&name);
                self.transient.remove(&name);
                Ok(json!({}))
            }
            "deleteEnvVar" => {
                let name = str_field(payload, "name")?;
                if name.trim().is_empty() {
                    return Err("deleteEnvVar: variable name is empty".into());
                }
                self.scope.remove(&name);
                self.transient.remove(&name);
                self.env_updates.insert(name, None);
                Ok(json!({}))
            }
            "getCollectionVar" => {
                let name = str_field(payload, "name")?;
                Ok(json!({ "value": self.collection_vars.get(&name).cloned() }))
            }
            "getFolderVar" => {
                let name = str_field(payload, "name")?;
                Ok(json!({ "value": self.folder_vars.get(&name).cloned() }))
            }
            "getEnvName" => Ok(json!({ "value": self.env_name })),
            "interpolate" => {
                // Best-effort: {{name}} from the plain snapshot (no secrets).
                let raw = payload.get("value").cloned().unwrap_or(Value::Null);
                let text = match &raw {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let out = simple_interpolate(&text, &self.scope);
                Ok(json!({ "value": out }))
            }
            // ---- request (pre) ----
            "reqGet" => {
                if !pre {
                    return Err("req is only available in pre-request scripts".into());
                }
                let field = str_field(payload, "field")?;
                match field.as_str() {
                    "url" => Ok(json!({ "value": self.req.url })),
                    "method" => Ok(json!({ "value": self.req.method.as_str() })),
                    "headers" => Ok(json!({
                        "value": self.req.headers.iter().map(|(k, v)| json!({"name": k, "value": v})).collect::<Vec<_>>()
                    })),
                    "body" => Ok(json!({ "value": self.req.body.clone() })),
                    other => Err(format!("reqGet: unknown field `{other}`")),
                }
            }
            "reqGetHeader" => {
                if !pre {
                    return Err("req is only available in pre-request scripts".into());
                }
                let name = str_field(payload, "name")?;
                Ok(json!({
                    "value": self.req.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(&name)).map(|(_, v)| v.clone())
                }))
            }
            "reqSet" => {
                if !pre {
                    return Err("req is only available in pre-request scripts".into());
                }
                let field = str_field(payload, "field")?;
                let value = payload.get("value").cloned().unwrap_or(Value::Null);
                match field.as_str() {
                    "url" => self.req.url = to_storage(&value),
                    "method" => {
                        self.req.method = parse_method(&to_storage(&value))?;
                    }
                    "body" => self.req.body = Some(to_storage(&value)),
                    "headers" => {
                        let mut headers = Vec::new();
                        if let Value::Array(items) = &value {
                            for item in items {
                                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                let val = item.get("value").and_then(|v| v.as_str()).unwrap_or("");
                                if !name.is_empty() {
                                    headers.push((name.to_string(), val.to_string()));
                                }
                            }
                        }
                        self.req.headers = headers;
                    }
                    other => return Err(format!("reqSet: unknown field `{other}`")),
                }
                Ok(json!({}))
            }
            "reqSetHeader" => {
                if !pre {
                    return Err("req is only available in pre-request scripts".into());
                }
                let name = str_field(payload, "name")?;
                let tmp = payload.get("value").cloned().unwrap_or(Value::Null);
                let value = to_storage(&tmp);
                if let Some(existing) = self
                    .req
                    .headers
                    .iter_mut()
                    .find(|(k, _)| k.eq_ignore_ascii_case(&name))
                {
                    existing.1 = value;
                } else {
                    self.req.headers.push((name, value));
                }
                Ok(json!({}))
            }
            "reqRemoveHeader" => {
                if !pre {
                    return Err("req is only available in pre-request scripts".into());
                }
                let name = str_field(payload, "name")?;
                self.req.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(&name));
                Ok(json!({}))
            }
            // ---- response (post) ----
            "resGet" => {
                if !post {
                    return Err("res is only available in post-response scripts".into());
                }
                let Some(res) = &self.res else {
                    return Ok(json!({ "value": null }));
                };
                let field = str_field(payload, "field")?;
                match field.as_str() {
                    "status" => Ok(json!({ "value": res.status })),
                    "statusText" => Ok(json!({ "value": res.status_text })),
                    "headers" => Ok(json!({
                        "value": res.headers.iter().map(|(k, v)| json!({"name": k, "value": v})).collect::<Vec<_>>()
                    })),
                    "body" => Ok(json!({ "value": res.body.clone() })),
                    "time" => Ok(json!({ "value": res.time_ms })),
                    "size" => Ok(json!({ "value": res.size })),
                    other => Err(format!("resGet: unknown field `{other}`")),
                }
            }
            "resGetHeader" => {
                if !post {
                    return Err("res is only available in post-response scripts".into());
                }
                let name = str_field(payload, "name")?;
                Ok(json!({
                    "value": self.res.as_ref().and_then(|r| r.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(&name)).map(|(_, v)| v.clone()))
                }))
            }
            "resSetBody" => {
                if !post {
                    return Err("res is only available in post-response scripts".into());
                }
                let tmp = payload.get("value").cloned().unwrap_or(Value::Null);
                let value = to_storage(&tmp);
                self.body_override = Some(value);
                Ok(json!({}))
            }
            "jsonPath" => {
                let path = str_field(payload, "path")?;
                let value = self
                    .res
                    .as_ref()
                    .and_then(|r| r.body.as_ref())
                    .and_then(|b| serde_json::from_str::<Value>(b).ok())
                    .and_then(|root| crate::engine::script::json_path(&root, &path).cloned());
                Ok(json!({ "value": value }))
            }
            // ---- flow / reporting ----
            "log" => {
                let msg = payload.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
                self.logs.push(msg);
                Ok(json!({}))
            }
            "reportTest" => {
                let name = str_field(payload, "name")?;
                let pass = payload.get("pass").and_then(|v| v.as_bool()).unwrap_or(false);
                let message = payload.get("message").and_then(|v| v.as_str()).map(String::from);
                self.tests.push(TestResultDto {
                    expect: name,
                    matcher: Some("script".into()),
                    expected: Value::Bool(true),
                    actual: Value::Bool(pass),
                    passed: pass,
                    message,
                });
                Ok(json!({}))
            }
            "setNextRequest" => {
                let value = payload.get("name").cloned().unwrap_or(Value::Null);
                let (name, structured) = match value {
                    Value::String(s) => (s.clone(), Some(s)),
                    Value::Null => ("null".to_string(), None),
                    _ => return Err("setNextRequest expects a name or null".into()),
                };
                // Mirror into the transient store so the collection runner
                // (which removes these keys per item) sees the directive.
                self.scope
                    .insert(crate::runner::KEY_NEXT_REQUEST.to_string(), name.clone());
                self.transient
                    .insert(crate::runner::KEY_NEXT_REQUEST.to_string(), name);
                self.next_request = Some(structured);
                Ok(json!({}))
            }
            "skipRequest" => {
                self.skip_request = true;
                self.scope
                    .insert(crate::runner::KEY_SKIP_REQUEST.to_string(), "1".into());
                self.transient
                    .insert(crate::runner::KEY_SKIP_REQUEST.to_string(), "1".into());
                Ok(json!({}))
            }
            "stopExecution" => {
                self.stop_execution = true;
                self.scope
                    .insert(crate::runner::KEY_NEXT_REQUEST.to_string(), "null".into());
                self.transient
                    .insert(crate::runner::KEY_NEXT_REQUEST.to_string(), "null".into());
                Ok(json!({}))
            }
            "sleep" => {
                let ms = payload.get("ms").and_then(|v| v.as_f64()).unwrap_or(0.0).clamp(0.0, 5000.0);
                std::thread::sleep(std::time::Duration::from_millis(ms as u64));
                Ok(json!({}))
            }
            other => Err(format!("unknown op `{other}`")),
        }
    }
}

fn str_field(v: &Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing string field `{key}`"))
}

fn parse_method(m: &str) -> Result<HttpMethod, String> {
    match m.to_ascii_uppercase().as_str() {
        "GET" => Ok(HttpMethod::GET),
        "POST" => Ok(HttpMethod::POST),
        "PUT" => Ok(HttpMethod::PUT),
        "PATCH" => Ok(HttpMethod::PATCH),
        "DELETE" => Ok(HttpMethod::DELETE),
        "HEAD" => Ok(HttpMethod::HEAD),
        "OPTIONS" => Ok(HttpMethod::OPTIONS),
        "TRACE" => Ok(HttpMethod::TRACE),
        other => Err(format!("unsupported method `{other}`")),
    }
}

fn simple_interpolate(text: &str, scope: &BTreeMap<String, String>) -> String {
    let re = regex::Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").expect("static regex");
    re.replace_all(text, |caps: &regex::Captures| {
        let name = caps.get(1).expect("g1").as_str().trim();
        scope.get(name).cloned().unwrap_or_else(|| caps.get(0).expect("g0").as_str().to_string())
    })
    .to_string()
}

/// The JavaScript shim defining all script-facing globals. Evaluated before
/// user code in every run.
const SHIM: &str = include_str!("js_shim.js");

/// Runs one script source against the shared host state. Errors are returned
/// as `Err(message)`; host-side mutations already applied remain visible.
pub fn run(source: &str, cell: &Rc<RefCell<Host>>) -> Result<Outcome, String> {
    use boa_engine::property::Attribute;
    use boa_engine::{js_string, Context, JsError, JsNativeError, JsResult, JsValue, NativeFunction, Source};

    let mut ctx = Context::default();
    {
        let cell2 = cell.clone();
        // The closure captures only non-GC Rust data (Rc<RefCell<Host>>), so
// nothing inside needs to be traced by the garbage collector.
let f = unsafe { NativeFunction::from_closure(move |_this, args, ctx| -> JsResult<JsValue> {
            let mut js_str = |v: &JsValue| -> Result<String, JsError> {
                let s = v.to_string(ctx)?;
                Ok(s.to_std_string_escaped())
            };
            let op = js_str(args.get(0).ok_or_else(|| {
                JsError::from_native(JsNativeError::typ().with_message("__keel(op, payload)"))
            })?)?;
            let payload_raw = args.get(1).cloned().unwrap_or_else(JsValue::undefined);
            let payload_str = if payload_raw.is_undefined() || payload_raw.is_null() {
                "{}".to_string()
            } else {
                js_str(&payload_raw)?
            };
            let payload: Value = serde_json::from_str(&payload_str).unwrap_or_else(|_| json!({}));
            let result = match cell2.borrow_mut().dispatch(&op, &payload) {
                Ok(v) => v,
                Err(e) => json!({ "error": e }),
            };
            let out = serde_json::to_string(&result).unwrap_or_else(|_| "{}".into());
            Ok(JsValue::from(js_string!(out)))
        }) };
        let fn_value = JsValue::from(f.to_js_function(ctx.realm()));
        let _ = ctx.register_global_property(js_string!("__keel"), fn_value, Attribute::all());
    }

    let program = format!("{SHIM}\n;(() => {{\n{source}\n}})();");
    if let Err(err) = ctx.eval(Source::from_bytes(&program)) {
        return Err(err.to_string());
    }

    let host = cell.borrow();
    Ok(Outcome {
        logs: host.logs.clone(),
        tests: host.tests.clone(),
        next_request: host.next_request.clone(),
        skip_request: host.skip_request,
        stop_execution: host.stop_execution,
    })
}

// shim alias to satisfy the earlier import block removal
#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn pre_host() -> Host {
        Host::new_pre(
            vars(&[("baseUrl", "http://x"), ("plain", "1")]),
            vars(&[("col", "c")]),
            vars(&[("fold", "f")]),
            "dev".into(),
            BTreeMap::new(),
            ReqState {
                method: HttpMethod::GET,
                url: "http://x/a".into(),
                headers: vec![("accept".into(), "application/json".into())],
                body: None,
            },
        )
    }

    fn run_str(src: &str, cell: &Rc<RefCell<Host>>) {
        run(src, cell).expect("script should succeed");
    }

    fn post_cell() -> Rc<RefCell<Host>> {
        let mut h = pre_host();
        h.phase = Phase::Post;
        h.res = Some(ResState {
            status: Some(201),
            status_text: "Created".into(),
            headers: vec![("content-type".into(), "application/json".into())],
            body: Some(r#"{"id": 7, "token": "t"}"#.into()),
            time_ms: 12.0,
            size: 24,
        });
        Rc::new(RefCell::new(h))
    }

    #[test]
    fn set_env_var_records_current_value() {
        let cell = post_cell();
        run_str(r#"keel.setEnvVar("FLOW", "flow-1"); keel.deleteEnvVar("old");"#, &cell);
        let h = cell.borrow();
        assert_eq!(h.env_updates.get("FLOW").and_then(|v| v.as_deref()), Some("flow-1"));
        assert_eq!(h.env_updates.get("old").map(|v| v.is_none()), Some(true));
        assert_eq!(h.transient.get("FLOW").map(String::as_str), Some("flow-1"));
    }

    #[test]
    fn set_and_get_vars_across_boundary() {
        let cell = Rc::new(RefCell::new(pre_host()));
        run_str(r#"keel.setVar("token", "abc"); keel.setVar("count", 42);"#, &cell);
        let h = cell.borrow();
        assert_eq!(h.transient.get("token").map(String::as_str), Some("abc"));
        assert_eq!(h.transient.get("count").map(String::as_str), Some("42"));
    }

    #[test]
    fn req_mutations() {
        let cell = Rc::new(RefCell::new(pre_host()));
        run_str(
            r#"req.setUrl(req.getUrl() + "/b");
               req.setMethod("POST");
               req.setHeader("X-Trace", "t1");
               req.removeHeader("accept");
               req.setBody('{"a":1}');
               test("inline test passes", () => { expect(req.getUrl()).toEndWith("/b"); });"#,
            &cell,
        );
        let h = cell.borrow();
        assert_eq!(h.req.url, "http://x/a/b");
        assert_eq!(h.req.method, HttpMethod::POST);
        assert!(h.req.headers.iter().any(|(k, _)| k == "X-Trace"));
        assert!(!h.req.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("accept")));
        assert_eq!(h.req.body.as_deref(), Some(r#"{"a":1}"#));
        assert_eq!(h.tests.len(), 1);
        assert!(h.tests[0].passed, "{:?}", h.tests[0]);
    }

    #[test]
    fn test_failures_captured() {
        let cell = Rc::new(RefCell::new(pre_host()));
        run_str(r#"test("failing", () => { expect(1).toBe(2); });"#, &cell);
        let h = cell.borrow();
        assert_eq!(h.tests.len(), 1);
        assert!(!h.tests[0].passed);
        assert!(h.tests[0].message.as_deref().unwrap_or("").contains("expected"));
    }

    #[test]
    fn matcher_bisect() {
        let cases = [
            "expect(\"a\").toBe(\"a\")",
            "expect({x:[1,2]}).toEqual({x:[1,2]})",
            "expect(\"hello\").toContain(\"ell\")",
            "expect(\"hello\").toMatch(/^h/)",
            "expect(5).toBeGreaterThan(4)",
            "expect(5).toBeLessThan(6)",
            "expect(\"x\").toBeTruthy()",
            "expect(0).toBeFalsy()",
            "expect(null).toBeNull()",
            "expect(undefined).toBeUndefined()",
            "expect([1,2]).toHaveLength(2)",
            "expect(\"s\").toBeTypeOf(\"string\")",
            "expect(\"abc\").toStartWith(\"a\")",
            "expect(\"abc\").toEndWith(\"c\")",
            "expect({id:1}).to.have.property(\"id\")",
            "expect(\"abc\").to.have.lengthOf(3)",
            "expect([1]).to.include(1)",
            "expect(1).not.toBe(2)",
            "expect(\"a\").not.toContain(\"b\")",
        ];
        let mut bad = Vec::new();
        for c in cases {
            let src = format!(r#"test("t", () => {{ {c}; }});"#);
            let cell = Rc::new(RefCell::new(pre_host()));
            let _ = run(&src, &cell);
            let h = cell.borrow();
            if h.tests.iter().any(|t| !t.passed) {
                bad.push((c.to_string(), h.tests[0].message.clone()));
            }
        }
        assert!(bad.is_empty(), "failing: {bad:?}");
    }

    #[test]
    fn expect_matchers_and_not() {
        let cell = Rc::new(RefCell::new(pre_host()));
        run_str(
            r#"
            test("m", () => {
              expect("a").toBe("a");
              expect({x:[1,2]}).toEqual({x:[1,2]});
              expect("hello").toContain("ell");
              expect("hello").toMatch(/^h/);
              expect(5).toBeGreaterThan(4);
              expect(5).toBeLessThan(6);
              expect("x").toBeTruthy();
              expect(0).toBeFalsy();
              expect(null).toBeNull();
              expect(undefined).toBeUndefined();
              expect([1,2]).toHaveLength(2);
              expect("s").toBeTypeOf("string");
              expect("abc").toStartWith("a");
              expect("abc").toEndWith("c");
              expect({id:1}).to.have.property("id");
              expect("abc").to.have.lengthOf(3);
              expect([1]).to.include(1);
              expect(1).not.toBe(2);
              expect("a").not.toContain("b");
            });
            "#,
            &cell,
        );
        let h = cell.borrow();
        assert_eq!(h.tests.len(), 1);
        assert!(h.tests[0].passed, "{:?}", h.tests[0]);
    }

    #[test]
    fn dsl_compat_globals() {
        let cell = Rc::new(RefCell::new(pre_host()));
        run_str(r#"set("trace", "42"); log("from dsl");"#, &cell);
        let h = cell.borrow();
        assert_eq!(h.transient.get("trace").map(String::as_str), Some("42"));
        assert!(h.logs.iter().any(|l| l == "from dsl"));
    }

    #[test]
    fn post_phase_response_reads() {
        let cell = post_cell();
        run_str(
            r#"
            set("id", json("id"));
            set("code", status());
            set("ct", header("Content-Type"));
            test("status", () => expect(res.getStatus()).toBe(201));
            if (get("id") != 7) throw new Error("id wrong: " + get("id"));
            "#,
            &cell,
        );
        let h = cell.borrow();
        assert_eq!(h.transient.get("id").map(String::as_str), Some("7"));
        assert_eq!(h.transient.get("code").map(String::as_str), Some("201"));
        assert!(h.tests.iter().all(|t| t.passed));
    }

    #[test]
    fn set_body_override() {
        let cell = post_cell();
        run_str(r#"res.setBody('{"ok": true}')"#, &cell);
        let h = cell.borrow();
        assert_eq!(h.body_override.as_deref(), Some(r#"{"ok": true}"#));
    }

    #[test]
    fn flow_controls() {
        let cell = Rc::new(RefCell::new(pre_host()));
        run_str(r#"keel.skipRequest(); keel.setNextRequest(null);"#, &cell);
        let h = cell.borrow();
        assert!(h.skip_request);
        assert_eq!(h.next_request, Some(None));
    }

    #[test]
    fn script_error_propagates() {
        let cell = Rc::new(RefCell::new(pre_host()));
        let err = run(r#"throw new Error("boom")"#, &cell).unwrap_err();
        assert!(err.contains("boom"), "{err}");
    }

    #[test]
    fn syntax_error_propagates() {
        let cell = Rc::new(RefCell::new(pre_host()));
        let err = run(r#"const = ;"#, &cell).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn scope_accessors_and_interpolate() {
        let cell = Rc::new(RefCell::new(pre_host()));
        run_str(
            r#"
            const u = keel.interpolate("{{baseUrl}}/y");
            if (u !== "http://x/y") throw new Error(u);
            if (keel.getCollectionVar("col") !== "c") throw new Error("col");
            if (keel.getFolderVar("fold") !== "f") throw new Error("fold");
            if (keel.getEnvName() !== "dev") throw new Error("envname");
            if (keel.getVar("baseUrl") !== "http://x") throw new Error("baseurl");
            "#,
            &cell,
        );
    }

    #[test]
    fn postman_pm_scripts() {
        let pre = Rc::new(RefCell::new(pre_host()));
        run_str(
            r#"
            pm.request.headers.upsert({ key: "X-Trace", value: "pm" });
            pm.environment.set("token", "abc");
            pm.collectionVariables.set("col", "c2");
            if (pm.environment.get("token") !== "abc") throw new Error("env");
            if (pm.collectionVariables.get("col") !== "c2") throw new Error("col set");
            if (pm.variables.replaceIn("{{baseUrl}}/y") !== "http://x/y") throw new Error("interp");
            pm.execution.skipRequest();
            "#,
            &pre,
        );
        {
            let h = pre.borrow();
            assert!(h.req.headers.iter().any(|(k, v)| k == "X-Trace" && v == "pm"));
            assert_eq!(h.transient.get("token").map(String::as_str), Some("abc"));
            assert_eq!(h.transient.get("col").map(String::as_str), Some("c2"));
            assert!(h.skip_request);
        }

        let post = post_cell();
        run_str(
            r#"
            pm.test("ok", function () {
              pm.expect(pm.response.code).to.eql(201);
              pm.response.to.have.status(201);
              pm.response.to.be.ok;
              pm.response.to.be.json;
              pm.expect(pm.response.json().token).to.eql("t");
              pm.response.to.have.jsonBody("id", 7);
              pm.response.to.have.header("content-type");
            });
            pm.environment.set("saved", pm.response.json().token);
            "#,
            &post,
        );
        let h = post.borrow();
        assert!(h.tests.iter().all(|t| t.passed), "{:?}", h.tests);
        assert_eq!(h.transient.get("saved").map(String::as_str), Some("t"));
    }

    #[test]
    fn secrets_are_not_readable() {
        // Secret names never appear in the scope snapshot the JS host sees.
        let cell = Rc::new(RefCell::new(pre_host()));
        run_str(r#"if (keel.getVar("apiToken") !== null) throw new Error("leak");"#, &cell);
    }
}
