//! Keel script DSL v0.
//!
//! One call per line (or `;`-separated). Available everywhere:
//!   set("name", value)      — store a session variable
//!   get("name")             — read a session variable (as an argument)
//!   log(arg, ...)           — collect a log line
//! Post-response only:
//!   json("dot.path")        — value from the response JSON
//!   status()                — response status code
//!   header("Name")          — response header value
//!   time()                  — response time in ms
//!   size()                  — response size in bytes
//!
//! Values: string, number, boolean literals, or nested calls.
//! A full scripting runtime (JS) is planned; this DSL covers the chaining
//! basics without pulling in an interpreter.

use serde_json::Value;

use crate::model::ResponseCtx;

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("line {0}: {1}")]
    At(usize, String),
}

#[derive(Debug, Clone)]
pub enum ScriptValue {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

impl ScriptValue {
    pub fn to_storage_string(&self) -> String {
        match self {
            ScriptValue::Str(s) => s.clone(),
            ScriptValue::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 9e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            ScriptValue::Bool(b) => b.to_string(),
            ScriptValue::Null => String::new(),
        }
    }

}

#[derive(Debug, Default, Clone)]
pub struct ScriptOutput {
    pub logs: Vec<String>,
    pub error: Option<String>,
}

/// Call AST: `name(args)`.
#[derive(Debug, Clone)]
struct Call {
    name: String,
    args: Vec<Expr>,
}

#[derive(Debug, Clone)]
enum Expr {
    Lit(ScriptValue),
    Call(Call),
}

struct Parser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ') | Some(b'\t')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected `{}`", b as char))
        }
    }

    fn parse_ident(&mut self) -> Result<String, String> {
        self.skip_ws();
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_') {
            self.pos += 1;
        }
        if start == self.pos {
            return Err("expected an identifier".into());
        }
        Ok(self.src[start..self.pos].to_string())
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err("unterminated string".into()),
                Some(b'"') => return Ok(out),
                Some(b'\\') => match self.bump() {
                    Some(b'n') => out.push('\n'),
                    Some(b't') => out.push('\t'),
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(other) => return Err(format!("unknown escape \\{}", other as char)),
                    None => return Err("unterminated escape".into()),
                },
                Some(_) => {
                    // Multi-byte safe: re-read the full char from the source.
                    let start = self.pos - 1;
                    let ch = self.src[start..].chars().next().expect("utf8");
                    out.push(ch);
                    self.pos = start + ch.len_utf8();
                }
            }
        }
    }

    fn parse_arg(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'"') => Ok(Expr::Lit(ScriptValue::Str(self.parse_string()?))),
            Some(c) if c == b'-' || c.is_ascii_digit() => {
                let start = self.pos;
                while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == b'.' || c == b'-')
                {
                    self.pos += 1;
                }
                let text = &self.src[start..self.pos];
                text.parse::<f64>()
                    .map(|n| Expr::Lit(ScriptValue::Num(n)))
                    .map_err(|_| format!("invalid number `{text}`"))
            }
            Some(b't') if self.src[self.pos..].starts_with("true") => {
                self.pos += 4;
                Ok(Expr::Lit(ScriptValue::Bool(true)))
            }
            Some(b'f') if self.src[self.pos..].starts_with("false") => {
                self.pos += 5;
                Ok(Expr::Lit(ScriptValue::Bool(false)))
            }
            Some(b'n') if self.src[self.pos..].starts_with("null") => {
                self.pos += 4;
                Ok(Expr::Lit(ScriptValue::Null))
            }
            Some(c) if c.is_ascii_alphanumeric() || c == b'_' => {
                let name = self.parse_ident()?;
                self.skip_ws();
                self.expect(b'(')?;
                let mut args = Vec::new();
                self.skip_ws();
                if self.peek() != Some(b')') {
                    loop {
                        args.push(self.parse_arg()?);
                        self.skip_ws();
                        match self.peek() {
                            Some(b',') => {
                                self.pos += 1;
                            }
                            Some(b')') => break,
                            _ => return Err("expected `,` or `)`".into()),
                        }
                    }
                }
                self.expect(b')')?;
                Ok(Expr::Call(Call { name, args }))
            }
            other => Err(format!("unexpected {:?}", other.map(|b| b as char))),
        }
    }

    fn parse_call_line(&mut self) -> Result<Call, String> {
        let expr = self.parse_arg()?;
        match expr {
            Expr::Call(call) => Ok(call),
            Expr::Lit(_) => Err("expected a call like `set(\"x\", 1)`".into()),
        }
    }
}

fn eval_call(
    call: &Call,
    ctx: Option<&ResponseCtx>,
    transient: &mut std::collections::BTreeMap<String, String>,
    out: &mut ScriptOutput,
) -> Result<ScriptValue, String> {
    match call.name.as_str() {
        "set" => {
            let name = arg_str(&call.args, 0, "set(name, value)")?;
            let value = eval_arg(
                call.args
                    .get(1)
                    .ok_or_else(|| "set(name, value) needs a value".to_string())?,
                ctx,
                transient,
                out,
            )?;
            transient.insert(name, value.to_storage_string());
            Ok(ScriptValue::Null)
        }
        "get" => {
            let name = arg_str(&call.args, 0, "get(name)")?;
            Ok(transient
                .get(&name)
                .cloned()
                .map(ScriptValue::Str)
                .unwrap_or(ScriptValue::Null))
        }
        "log" => {
            let mut parts = Vec::new();
            for arg in &call.args {
                parts.push(eval_arg(arg, ctx, transient, out)?.to_storage_string());
            }
            out.logs.push(parts.join(" "));
            Ok(ScriptValue::Null)
        }
        "json" => {
            let ctx = ctx.ok_or_else(|| "json() is only available in post-response".to_string())?;
            let path = arg_str(&call.args, 0, "json(path)")?;
            let Some(json) = &ctx.json else {
                return Ok(ScriptValue::Null);
            };
            Ok(json_path(json, &path)
                .cloned()
                .map(|v| script_from_json(&v))
                .unwrap_or(ScriptValue::Null))
        }
        "status" => {
            let ctx = ctx.ok_or_else(|| "status() is only available in post-response".to_string())?;
            Ok(ctx
                .status
                .map(|s| ScriptValue::Num(s as f64))
                .unwrap_or(ScriptValue::Null))
        }
        "header" => {
            let ctx = ctx.ok_or_else(|| "header() is only available in post-response".to_string())?;
            let name = arg_str(&call.args, 0, "header(name)")?;
            Ok(ctx
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&name))
                .map(|(_, v)| ScriptValue::Str(v.clone()))
                .unwrap_or(ScriptValue::Null))
        }
        "time" => {
            let ctx = ctx.ok_or_else(|| "time() is only available in post-response".to_string())?;
            Ok(ScriptValue::Num(ctx.time_ms))
        }
        "size" => {
            let ctx = ctx.ok_or_else(|| "size() is only available in post-response".to_string())?;
            Ok(ScriptValue::Num(ctx.size as f64))
        }
        other => Err(format!("unknown function `{other}`")),
    }
}

fn eval_arg(
    arg: &Expr,
    ctx: Option<&ResponseCtx>,
    transient: &mut std::collections::BTreeMap<String, String>,
    out: &mut ScriptOutput,
) -> Result<ScriptValue, String> {
    match arg {
        Expr::Lit(v) => Ok(v.clone()),
        Expr::Call(call) => eval_call(call, ctx, transient, out),
    }
}

fn arg_str(args: &[Expr], idx: usize, usage: &str) -> Result<String, String> {
    match args.get(idx) {
        Some(Expr::Lit(ScriptValue::Str(s))) => Ok(s.clone()),
        _ => Err(format!("argument {idx} must be a string: {usage}")),
    }
}

/// Navigates a JSON value by a dotted path (`data.items.0.id`).
pub fn json_path<'v>(root: &'v Value, path: &str) -> Option<&'v Value> {
    let mut cur = root;
    for seg in path.split('.') {
        match cur {
            Value::Object(map) => cur = map.get(seg)?,
            Value::Array(items) => {
                let idx: usize = seg.parse().ok()?;
                cur = items.get(idx)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

pub fn script_from_json(value: &Value) -> ScriptValue {
    match value {
        Value::String(s) => ScriptValue::Str(s.clone()),
        Value::Number(n) => ScriptValue::Num(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => ScriptValue::Bool(*b),
        Value::Null => ScriptValue::Null,
        other => ScriptValue::Str(other.to_string()),
    }
}

/// Executes a script. `transient` is read by `get()` and mutated by `set()`.
pub fn run(
    source: &str,
    transient: &mut std::collections::BTreeMap<String, String>,
    ctx: Option<&ResponseCtx>,
) -> Result<ScriptOutput, String> {
    let mut out = ScriptOutput::default();
    for (i, line) in source.lines().enumerate() {
        for stmt in line.split(';') {
            let stmt = stmt.trim();
            if stmt.is_empty() || stmt.starts_with('#') {
                continue;
            }
            let mut parser = Parser::new(stmt);
            let call = parser.parse_call_line().map_err(|e| ScriptError::At(i + 1, e).to_string())?;
            eval_call(&call, ctx, transient, &mut out)
                .map_err(|e| ScriptError::At(i + 1, e).to_string())?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn set_and_get_roundtrip() {
        let src = r#"set("userId", 42)
set("name", "ada")
"#;
        let out = run(src, &mut map(&[]), None).expect("ok");
        let mut transient = map(&[("userId", "42"), ("name", "ada")]);
        run(r#"set("id", get("userId"))"#, &mut transient, None).expect("ok");
        assert!(transient.contains_key("name"));
        assert_eq!(transient.get("id").map(String::as_str), Some("42"));
        let _ = out;
    }

    #[test]
    fn set_writes_into_transient() {
        let src = r#"set("token", json("access_token")); log("got token")"#;
        let ctx = ResponseCtx {
            status: Some(200),
            time_ms: 12.0,
            size: 10,
            headers: vec![("content-type".into(), "application/json".into())],
            cookies: vec![],
            body: Some(r#"{"access_token":"abc"}"#.into()),
            json: Some(serde_json::json!({"access_token": "abc"})),
        };
        let mut t = map(&[]);
        let out = run(src, &mut t, Some(&ctx)).expect("ok");
        assert_eq!(t.get("token").map(String::as_str), Some("abc"));
        assert_eq!(out.logs, vec!["got token"]);
    }

    #[test]
    fn post_response_calls() {
        let ctx = ResponseCtx {
            status: Some(201),
            time_ms: 5.0,
            size: 3,
            headers: vec![("x-trace".into(), "t1".into())],
            cookies: vec![],
            body: None,
            json: None,
        };
        let mut t = map(&[]);
        run(
            r#"set("code", status()); set("trace", header("X-Trace")); set("ms", time())"#,
            &mut t,
            Some(&ctx),
        )
        .expect("ok");
        assert_eq!(t.get("code").map(String::as_str), Some("201"));
        assert_eq!(t.get("trace").map(String::as_str), Some("t1"));
        assert_eq!(t.get("ms").map(String::as_str), Some("5"));
    }

    #[test]
    fn json_path_navigation() {
        let v = serde_json::json!({"data": {"items": [{"id": 7}]}});
        assert_eq!(json_path(&v, "data.items.0.id"), Some(&Value::Number(7.into())));
        assert_eq!(json_path(&v, "data.missing"), None);
    }

    #[test]
    fn unknown_function_is_error() {
        let err = run("frobnicate(1)", &mut map(&[]), None).unwrap_err();
        assert!(err.contains("line 1"));
    }

    #[test]
    fn comments_and_empty_lines_ok() {
        run("# a comment\n\n; ;\nset(\"a\", 1)\n", &mut map(&[]), None).expect("ok");
    }
}
