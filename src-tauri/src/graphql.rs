//! GraphQL helpers: introspection and a query built from a field selection.
//!
//! The HTTP exchange itself stays in the send engine (graphql body type). This
//! module only talks to the schema and turns a selection into a query document.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const INTROSPECTION: &str = r#"
query KeelIntrospection {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types {
      kind
      name
      description
      fields(includeDeprecated: true) {
        name
        description
        args {
          name
          description
          defaultValue
          type { ...TypeRef }
        }
        type { ...TypeRef }
      }
      inputFields { name description type { ...TypeRef } }
      enumValues(includeDeprecated: true) { name description }
    }
  }
}
fragment TypeRef on __Type {
  kind
  name
  ofType {
    kind
    name
    ofType {
      kind
      name
      ofType { kind name ofType { kind name } }
    }
  }
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GqlTypeRef {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of_type: Option<Box<GqlTypeRef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GqlArg {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(rename = "type")]
    pub type_ref: GqlTypeRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GqlField {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub args: Vec<GqlArg>,
    #[serde(rename = "type")]
    pub type_ref: GqlTypeRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GqlEnumValue {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GqlType {
    pub kind: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<GqlField>,
    #[serde(default)]
    pub enum_values: Vec<GqlEnumValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GqlSchema {
    pub query_type: Option<String>,
    pub mutation_type: Option<String>,
    pub subscription_type: Option<String>,
    pub types: Vec<GqlType>,
}

/// POSTs the standard introspection query and returns a compact schema.
pub async fn introspect(url: &str, headers: &[(String, String)], timeout_secs: u64) -> Result<GqlSchema, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs.max(1)))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .json(&json!({ "query": INTROSPECTION }));
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        req = req.header(name.as_str(), value.as_str());
    }
    let response = req.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("introspection HTTP {status}: {}", truncate(&text, 240)));
    }
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("introspection JSON: {e}"))?;
    if let Some(errors) = value.get("errors").filter(|e| !e.is_null()) {
        return Err(format!("introspection errors: {}", truncate(&errors.to_string(), 240)));
    }
    parse_schema(value.get("data").unwrap_or(&Value::Null))
}

fn parse_schema(data: &Value) -> Result<GqlSchema, String> {
    let schema = data
        .get("__schema")
        .ok_or_else(|| "introspection response has no __schema".to_string())?;
    let named = |key: &str| {
        schema
            .get(key)
            .and_then(|v| v.get("name"))
            .and_then(|n| n.as_str())
            .map(str::to_string)
    };
    let raw_types = schema
        .get("types")
        .and_then(|t| t.as_array())
        .ok_or_else(|| "introspection response has no types".to_string())?;
    let mut types = Vec::new();
    for raw in raw_types {
        let name = raw.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if name.starts_with("__") || name.is_empty() {
            continue;
        }
        let kind = raw.get("kind").and_then(|k| k.as_str()).unwrap_or("").to_string();
        let fields = raw
            .get("fields")
            .and_then(|f| f.as_array())
            .map(|arr| arr.iter().filter_map(parse_field).collect())
            .unwrap_or_default();
        let enum_values = raw
            .get("enumValues")
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        Some(GqlEnumValue {
                            name: v.get("name")?.as_str()?.to_string(),
                            description: v.get("description").and_then(|d| d.as_str()).map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        types.push(GqlType {
            kind,
            name: name.to_string(),
            description: raw.get("description").and_then(|d| d.as_str()).map(str::to_string),
            fields,
            enum_values,
        });
    }
    Ok(GqlSchema {
        query_type: named("queryType"),
        mutation_type: named("mutationType"),
        subscription_type: named("subscriptionType"),
        types,
    })
}

fn parse_field(raw: &Value) -> Option<GqlField> {
    Some(GqlField {
        name: raw.get("name")?.as_str()?.to_string(),
        description: raw.get("description").and_then(|d| d.as_str()).map(str::to_string),
        args: raw
            .get("args")
            .and_then(|a| a.as_array())
            .map(|arr| arr.iter().filter_map(parse_arg).collect())
            .unwrap_or_default(),
        type_ref: parse_type_ref(raw.get("type")?)?,
    })
}

fn parse_arg(raw: &Value) -> Option<GqlArg> {
    Some(GqlArg {
        name: raw.get("name")?.as_str()?.to_string(),
        description: raw.get("description").and_then(|d| d.as_str()).map(str::to_string),
        default_value: raw.get("defaultValue").and_then(|d| d.as_str()).map(str::to_string),
        type_ref: parse_type_ref(raw.get("type")?)?,
    })
}

fn parse_type_ref(raw: &Value) -> Option<GqlTypeRef> {
    Some(GqlTypeRef {
        kind: raw.get("kind")?.as_str()?.to_string(),
        name: raw.get("name").and_then(|n| n.as_str()).map(str::to_string),
        of_type: raw
            .get("ofType")
            .filter(|v| !v.is_null())
            .and_then(|inner| parse_type_ref(inner).map(Box::new)),
    })
}

/// Named type at the bottom of a wrapping chain (`NON_NULL` / `LIST`).
pub fn named_type(type_ref: &GqlTypeRef) -> Option<&str> {
    let mut cur = type_ref;
    loop {
        if let Some(name) = cur.name.as_deref() {
            if !name.is_empty() {
                return Some(name);
            }
        }
        match cur.of_type.as_deref() {
            Some(inner) => cur = inner,
            None => return None,
        }
    }
}

pub fn is_list(type_ref: &GqlTypeRef) -> bool {
    let mut cur = type_ref;
    loop {
        if cur.kind == "LIST" {
            return true;
        }
        match cur.of_type.as_deref() {
            Some(inner) => cur = inner,
            None => return false,
        }
    }
}

/// Human-readable type, e.g. `[String!]!`.
pub fn type_string(type_ref: &GqlTypeRef) -> String {
    match type_ref.kind.as_str() {
        "NON_NULL" => format!("{}!", type_ref.of_type.as_ref().map(|t| type_string(t)).unwrap_or_default()),
        "LIST" => format!("[{}]", type_ref.of_type.as_ref().map(|t| type_string(t)).unwrap_or_default()),
        _ => type_ref.name.clone().unwrap_or_default(),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub name: String,
    #[serde(default)]
    pub args: Vec<SelectionArg>,
    #[serde(default)]
    pub children: Vec<Selection>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionArg {
    pub name: String,
    pub value: String,
}

/// Builds a GraphQL operation document from a field selection.
pub fn build_query(operation: &str, root_field: &str, selection: &[Selection]) -> Result<String, String> {
    let op = match operation {
        "query" | "mutation" | "subscription" => operation,
        _ => return Err(format!("unknown operation `{operation}`")),
    };
    if !is_name(root_field) {
        return Err("root field must be a GraphQL name".into());
    }
    let body = render_selection(selection, 2)?;
    if body.is_empty() {
        return Ok(format!("{op} {{\n  {root_field}\n}}\n"));
    }
    Ok(format!("{op} {{\n  {root_field} {{\n{body}  }}\n}}\n"))
}

fn render_selection(fields: &[Selection], indent: usize) -> Result<String, String> {
    let pad = "  ".repeat(indent);
    let mut out = String::new();
    for field in fields {
        if !is_name(&field.name) {
            return Err(format!("invalid field name `{}`", field.name));
        }
        let mut args = String::new();
        if !field.args.is_empty() {
            let parts: Result<Vec<_>, String> = field
                .args
                .iter()
                .filter(|a| !a.value.trim().is_empty())
                .map(|a| {
                    if !is_name(&a.name) {
                        return Err(format!("invalid argument name `{}`", a.name));
                    }
                    Ok(format!("{}: {}", a.name, a.value.trim()))
                })
                .collect();
            let parts = parts?;
            if !parts.is_empty() {
                args = format!("({})", parts.join(", "));
            }
        }
        if field.children.is_empty() {
            out.push_str(&format!("{pad}{}{args}\n", field.name));
        } else {
            let inner = render_selection(&field.children, indent + 1)?;
            out.push_str(&format!("{pad}{}{args} {{\n{inner}{pad}}}\n", field.name));
        }
    }
    Ok(out)
}

fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn truncate(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_nested_query_with_args() {
        let sel = vec![Selection {
            name: "user".into(),
            args: vec![SelectionArg { name: "id".into(), value: "$id".into() }],
            children: vec![
                Selection { name: "id".into(), args: vec![], children: vec![] },
                Selection { name: "name".into(), args: vec![], children: vec![] },
            ],
        }];
        let q = build_query("query", "user", &[]).unwrap();
        assert_eq!(q, "query {\n  user\n}\n");
        let q = build_query("query", "viewer", &sel).unwrap();
        assert!(q.contains("user(id: $id)"));
        assert!(q.contains("name"));
    }

    #[test]
    fn rejects_bad_names() {
        assert!(build_query("query", "user name", &[]).is_err());
        assert!(build_query("nope", "user", &[]).is_err());
    }

    #[test]
    fn type_string_unwraps_wrappers() {
        let inner = GqlTypeRef { kind: "SCALAR".into(), name: Some("String".into()), of_type: None };
        let list = GqlTypeRef { kind: "LIST".into(), name: None, of_type: Some(Box::new(inner)) };
        let nn = GqlTypeRef { kind: "NON_NULL".into(), name: None, of_type: Some(Box::new(list)) };
        assert_eq!(type_string(&nn), "[String]!");
        assert!(is_list(&nn));
        assert_eq!(named_type(&nn), Some("String"));
    }
}
