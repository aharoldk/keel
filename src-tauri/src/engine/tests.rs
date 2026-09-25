//! Assertion engine. Expressions resolve over a `ResponseCtx`; each
//! assertion carries a single matcher (see docs/IPC_CONTRACT.md).

use serde::Serialize;
use serde_json::{Number, Value};

use crate::engine::script::json_path;
use crate::model::{ResponseCtx, TestAssertion};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub expect: String,
    pub matcher: Option<String>,
    pub expected: Value,
    pub actual: Value,
    pub passed: bool,
    pub message: Option<String>,
}

fn value_to_actual(v: Option<Value>) -> Value {
    v.unwrap_or(Value::Null)
}

/// Resolves an expression against the response context.
pub fn resolve_expression(expr: &str, ctx: &ResponseCtx) -> Result<Value, String> {
    let expr = expr.trim();
    let (head, rest) = expr
        .split_once('.')
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((expr, None));
    match head {
        "response" => {
            let rest = rest.ok_or_else(|| "expression must start with `response.`".to_string())?;
            let (prop, tail) = rest
                .split_once('.')
                .map(|(a, b)| (a, Some(b)))
                .unwrap_or((rest, None));
            match prop {
                "status" => Ok(ctx.status.map(|s| Value::Number(Number::from(s))).unwrap_or(Value::Null)),
                "time" => Ok(json_num(ctx.time_ms)),
                "size" => Ok(json_num(ctx.size as f64)),
                "body" => Ok(Value::String(ctx.body.clone().unwrap_or_default())),
                "headers" => {
                    let name = tail.ok_or_else(|| "expected `response.headers.<name>`".to_string())?;
                    Ok(ctx
                        .headers
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(name))
                        .map(|(_, v)| Value::String(v.clone()))
                        .unwrap_or(Value::Null))
                }
                "cookies" => {
                    let name = tail.ok_or_else(|| "expected `response.cookies.<name>`".to_string())?;
                    Ok(ctx
                        .cookies
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(name))
                        .map(|(_, v)| Value::String(v.clone()))
                        .unwrap_or(Value::Null))
                }
                "json" => match tail {
                    None => Ok(ctx.json.clone().unwrap_or(Value::Null)),
                    Some(path) => {
                        let Some(json) = &ctx.json else {
                            return Ok(Value::Null);
                        };
                        Ok(json_path(json, path).cloned().unwrap_or(Value::Null))
                    }
                },
                other => Err(format!("unknown response property `{other}`")),
            }
        }
        other => Err(format!("expressions must start with `response`, got `{other}`")),
    }
}

fn json_num(n: f64) -> Value {
    serde_json::Number::from_f64(n)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn numeric(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn stringify(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn json_equal(a: &Value, b: &Value) -> bool {
    // Numeric equality tolerates int/float distinctions.
    if let (Some(x), Some(y)) = (numeric(a), numeric(b)) {
        return (x - y).abs() < f64::EPSILON.max((x.abs() + y.abs()) * 1e-9);
    }
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            ma.len() == mb.len()
                && ma
                    .iter()
                    .all(|(k, va)| mb.get(k).map(|vb| json_equal(va, vb)).unwrap_or(false))
        }
        (Value::Array(aa), Value::Array(ab)) => {
            aa.len() == ab.len() && aa.iter().zip(ab).all(|(x, y)| json_equal(x, y))
        }
        _ => a == b,
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn length(v: &Value) -> Option<usize> {
    match v {
        Value::String(s) => Some(s.chars().count()),
        Value::Array(a) => Some(a.len()),
        Value::Object(o) => Some(o.len()),
        _ => None,
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn compare(matcher: &str, expected: &Value, actual: &Value) -> Result<bool, String> {
    Ok(match matcher {
        "toBe" | "toEqual" => json_equal(expected, actual),
        "toNotBe" => !json_equal(expected, actual),
        "toContain" => match actual {
            Value::String(s) => s.contains(&stringify(expected)),
            Value::Array(items) => items.iter().any(|i| json_equal(i, expected)),
            Value::Object(map) => map.contains_key(&stringify(expected)),
            _ => false,
        },
        "toMatch" => {
            let pattern = expected
                .as_str()
                .ok_or_else(|| "`toMatch` expects a string pattern".to_string())?;
            let re = regex::Regex::new(pattern)
                .map_err(|e| format!("invalid regex: {e}"))?;
            re.is_match(&stringify(actual))
        }
        "toBeType" => {
            let want = expected
                .as_str()
                .ok_or_else(|| "`toBeType` expects a type name string".to_string())?;
            let got = type_name(actual);
            match want {
                "array" => got == "array",
                "object" => got == "object",
                _ => want == got,
            }
        }
        "toHaveLength" => {
            let want = numeric(expected)
                .ok_or_else(|| "`toHaveLength` expects a number".to_string())?;
            length(actual).map(|l| l as f64 == want).unwrap_or(false)
        }
        "toBeGreaterThan" => {
            let want = numeric(expected)
                .ok_or_else(|| "`toBeGreaterThan` expects a number".to_string())?;
            numeric(actual).map(|a| a > want).unwrap_or(false)
        }
        "toBeLessThan" => {
            let want = numeric(expected)
                .ok_or_else(|| "`toBeLessThan` expects a number".to_string())?;
            numeric(actual).map(|a| a < want).unwrap_or(false)
        }
        "toBeTruthy" => truthy(actual),
        "toBeNull" => actual.is_null(),
        other => return Err(format!("unknown matcher `{other}`")),
    })
}

pub fn run_tests(assertions: &[TestAssertion], ctx: &ResponseCtx) -> Vec<TestResult> {
    assertions
        .iter()
        .map(|a| {
            let matcher = a.matcher.keys().next().cloned();
            let expected = matcher
                .as_ref()
                .and_then(|m| a.matcher.get(m))
                .cloned()
                .unwrap_or(Value::Bool(true));

            let (passed, actual, message) = match resolve_expression(&a.expect, ctx) {
                Err(e) => (false, Value::Null, Some(e)),
                Ok(actual) => match &matcher {
                    None => (truthy(&actual), actual.clone(), None),
                    Some(matcher) => match compare(matcher, &expected, &actual) {
                        Ok(passed) => (passed, actual.clone(), None),
                        Err(e) => (false, actual.clone(), Some(e)),
                    },
                },
            };
            TestResult {
                expect: a.expect.clone(),
                matcher,
                expected,
                actual: value_to_actual(Some(actual)),
                passed,
                message,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn ctx() -> ResponseCtx {
        ResponseCtx {
            status: Some(200),
            time_ms: 120.5,
            size: 2048,
            headers: vec![
                ("Content-Type".to_string(), "application/json".to_string()),
                ("X-Trace".to_string(), "abc123".to_string()),
            ],
            cookies: vec![("session".to_string(), "xyz".to_string())],
            body: Some(r#"{"data":{"items":[{"id":1},{"id":2}]},"count":2}"#.to_string()),
            json: Some(serde_json::json!({"data":{"items":[{"id":1},{"id":2}]},"count":2})),
        }
    }

    fn assert1(expect: &str, matcher: &str, expected: Value) -> TestAssertion {
        let mut m = BTreeMap::new();
        m.insert(matcher.to_string(), expected);
        TestAssertion {
            expect: expect.to_string(),
            matcher: m,
        }
    }

    #[test]
    fn status_matchers() {
        let results = run_tests(
            &[
                assert1("response.status", "toBe", Value::from(200)),
                assert1("response.status", "toBe", Value::from(404)),
                assert1("response.status", "toBeGreaterThan", Value::from(100)),
            ],
            &ctx(),
        );
        assert!(results[0].passed);
        assert!(!results[1].passed);
        assert!(results[2].passed);
    }

    #[test]
    fn header_case_insensitive() {
        let results = run_tests(
            &[assert1("response.headers.content-type", "toBe", Value::from("application/json"))],
            &ctx(),
        );
        assert!(results[0].passed);
    }

    #[test]
    fn json_paths_and_types() {
        let results = run_tests(
            &[
                assert1("response.json.count", "toBe", Value::from(2)),
                assert1("response.json.data.items.1.id", "toBe", Value::from(2)),
                assert1("response.json.data.items", "toBeType", Value::from("array")),
                assert1("response.json", "toHaveLength", Value::from(2)),
            ],
            &ctx(),
        );
        assert!(results.iter().all(|r| r.passed), "{results:?}");
    }

    #[test]
    fn string_and_regex() {
        let results = run_tests(
            &[
                assert1("response.headers.x-trace", "toContain", Value::from("abc")),
                assert1("response.headers.x-trace", "toMatch", Value::from(r"^abc\d+$")),
                assert1("response.body", "toContain", Value::from("items")),
            ],
            &ctx(),
        );
        assert!(results.iter().all(|r| r.passed), "{results:?}");
    }

    #[test]
    fn deep_equality() {
        let results = run_tests(
            &[
                assert1("response.json.data", "toEqual", serde_json::json!({"items":[{"id":1},{"id":2}]})),
                assert1("response.json.count", "toEqual", Value::from(2.0)),
            ],
            &ctx(),
        );
        assert!(results.iter().all(|r| r.passed), "{results:?}");
    }

    #[test]
    fn truthiness_and_null() {
        let results = run_tests(
            &[
                assert1("response.status", "toBeTruthy", Value::Bool(true)),
                assert1("response.json.missing", "toBeNull", Value::Bool(true)),
            ],
            &ctx(),
        );
        assert!(results.iter().all(|r| r.passed), "{results:?}");
    }

    #[test]
    fn bad_regex_reports_message() {
        let results = run_tests(
            &[assert1("response.body", "toMatch", Value::from("([bad"))],
            &ctx(),
        );
        assert!(!results[0].passed);
        assert!(results[0].message.as_deref().unwrap().contains("regex"));
    }

    #[test]
    fn unknown_expression_fails() {
        let results = run_tests(
            &[assert1("response.nope", "toBe", Value::from(1))],
            &ctx(),
        );
        assert!(!results[0].passed);
        assert!(results[0].message.is_some());
    }

    #[test]
    fn no_matcher_means_truthy() {
        let results = run_tests(
            &[TestAssertion { expect: "response.status".into(), matcher: BTreeMap::new() }],
            &ctx(),
        );
        assert!(results[0].passed);
    }
}
