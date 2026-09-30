//! OpenAPI 3 → Keel collection import (pragmatic subset).
//!
//! - One file per (path, method); folders per tag.
//! - `servers[0].url` becomes `baseUrl` in an `imported` environment;
//!   request URLs use `{{baseUrl}}`.
//! - JSON request bodies get generated examples from the schema
//!   (`example` → `default` → type-based sample).
//! - Unsupported constructs are skipped with warnings, never silently.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::model::*;
use crate::workspace::{slugify, ENV_DIR};

pub struct OpenApiImport {
    pub files: Vec<String>,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

pub fn import_openapi(source: &Path, root: &Path, target_folder: &str) -> Result<OpenApiImport, String> {
    let text = std::fs::read_to_string(source).map_err(|e| format!("read file: {e}"))?;
    let doc: Value = crate::model::yaml_to(&text).map_err(|e| format!("parse OpenAPI: {e}"))?;

    let mut warnings: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut skipped = 0usize;

    let title = doc
        .get("info")
        .and_then(|i| i.get("title"))
        .and_then(|t| t.as_str())
        .unwrap_or("Imported API");

    // servers[0].url → baseUrl in an `imported` environment.
    let server_url = doc
        .get("servers")
        .and_then(|s| s.as_array())
        .and_then(|s| s.first())
        .and_then(|s| s.get("url"))
        .and_then(|u| u.as_str())
        .map(|u| u.to_string());
    match &server_url {
        Some(url) => upsert_env(root, "imported", url)?,
        None => warnings.push("No `servers` found — set {{baseUrl}} manually.".into()),
    }

    let paths = doc
        .get("paths")
        .and_then(|p| p.as_object())
        .ok_or_else(|| "no `paths` in document".to_string())?;

    let target_root = crate::workspace::safe_join(root, target_folder)?;
    std::fs::create_dir_all(&target_root).map_err(|e| e.to_string())?;

    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for (method, op) in item {
            let Some(method_enum) = method_to_enum(method) else {
                continue;
            };
            if !method_enum.allows_body() && op.get("requestBody").is_some() {
                // GET/HEAD with requestBody — unusual; keep but warn.
                warnings.push(format!("{method} {path} declares a requestBody"));
            }
            let op = match op.as_object() {
                Some(o) => o,
                None => {
                    skipped += 1;
                    continue;
                }
            };

            let name = op
                .get("summary")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    op.get("operationId")
                        .and_then(|s| s.as_str())
                        .map(prettify_operation_id)
                })
                .unwrap_or_else(|| format!("{method} {path}"));

            let folder_name = op
                .get("tags")
                .and_then(|t| t.as_array())
                .and_then(|t| t.first())
                .and_then(|t| t.as_str())
                .unwrap_or("imported");
            let folder = target_root.join(sanitize_segment(&slugify(folder_name)));
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;

            let target = crate::workspace::unique_file(&folder, &slugify(&name));
            let rel_target = crate::workspace::rel_path_public(root, &target);

            let params = collect_params(op, item, &mut warnings);
            let headers = params_headers(&params);
            let query = params_query(&params);

            let url = format!("{{{{baseUrl}}}}{path}");
            let body = build_body(op, &doc, &mut warnings);
            let auth = detect_security(op, &doc);

            let request_doc = RequestDoc {
                schema_version: SCHEMA_VERSION.into(),
                name,
                kind: "request".into(),
                description: op.get("description").and_then(|d| d.as_str()).map(String::from),
                protocol: None,
                graphql: None,
                websocket: None,
                grpc: None,
                request: RequestBlock {
                    method: method_enum,
                    url,
                    params: if query.is_empty() { None } else { Some(query) },
                    headers: if headers.is_empty() { None } else { Some(headers) },
                    path_params: None,
                    body,
                },
                auth,
                variables: None,
                scripts: None,
                tests: None,
            };

            let yaml = yaml_of(&request_doc)?;
            if std::fs::write(&target, yaml).is_ok() {
                files.push(rel_target);
            } else {
                skipped += 1;
                warnings.push(format!("could not write `{}`", target.display()));
            }
        }
    }

    let _ = title;
    Ok(OpenApiImport {
        files,
        skipped,
        warnings,
    })
}

fn method_to_enum(method: &str) -> Option<HttpMethod> {
    match method.to_ascii_lowercase().as_str() {
        "get" => Some(HttpMethod::GET),
        "post" => Some(HttpMethod::POST),
        "put" => Some(HttpMethod::PUT),
        "patch" => Some(HttpMethod::PATCH),
        "delete" => Some(HttpMethod::DELETE),
        "head" => Some(HttpMethod::HEAD),
        "options" => Some(HttpMethod::OPTIONS),
        "trace" => Some(HttpMethod::TRACE),
        _ => None,
    }
}

fn sanitize_segment(seg: &str) -> String {
    seg.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect()
}

fn prettify_operation_id(id: &str) -> String {
    id.replace(['_', '-'], " ")
}

fn collect_params(
    op: &serde_json::Map<String, Value>,
    path_item: &serde_json::Map<String, Value>,
    warnings: &mut Vec<String>,
) -> Vec<(String, String, String)> {
    // (name, in, value)
    let mut out = Vec::new();
    for source in [path_item, op] {
        if let Some(list) = source.get("parameters").and_then(|p| p.as_array()) {
            for p in list {
                let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let location = p.get("in").and_then(|i| i.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                if out.iter().any(|(n, l, _)| n == name && l == location) {
                    continue;
                }
                let value = sample_value(
                    p.get("schema"),
                    p.get("example").or_else(|| p.get("default")),
                    warnings,
                );
                out.push((name.to_string(), location.to_string(), value));
            }
        }
    }
    out
}

fn params_query(params: &[(String, String, String)]) -> Vec<KV> {
    params
        .iter()
        .filter(|(_, location, _)| location == "query")
        .map(|(name, _, value)| KV {
            name: name.clone(),
            value: value.clone(),
            ..KV::default()
        })
        .collect()
}

fn params_headers(params: &[(String, String, String)]) -> Vec<KV> {
    params
        .iter()
        .filter(|(_, location, _)| location == "header")
        .map(|(name, _, value)| KV {
            name: name.clone(),
            value: value.clone(),
            ..KV::default()
        })
        .collect()
}

fn build_body(
    op: &serde_json::Map<String, Value>,
    doc: &Value,
    warnings: &mut Vec<String>,
) -> Option<Body> {
    let content = op
        .get("requestBody")
        .and_then(|rb| rb.get("content"))
        .and_then(|c| c.as_object())?;
    if let Some(json) = content.get("application/json") {
        let schema = json.get("schema");
        let sample = schema.map(|s| sample_from_schema(s, doc, 0, warnings));
        return Some(Body {
            body_type: BodyType::Json,
            content: Some(
                serde_json::to_string_pretty(&sample.unwrap_or_else(|| Value::Object(serde_json::Map::new())))
                    .unwrap_or_else(|_| "{}".into()),
            ),
            ..Body::default()
        });
    }
    if let Some((ct, _)) = content.iter().next() {
        warnings.push(format!("skipped body with content type `{ct}`"));
    }
    None
}

fn detect_security(op: &serde_json::Map<String, Value>, doc: &Value) -> Option<Auth> {
    let scheme = op
        .get("security")
        .and_then(|s| s.as_array())
        .and_then(|s| s.first())
        .and_then(|s| s.as_object())
        .and_then(|s| s.keys().next().cloned())
        .or_else(|| {
            doc.get("security")
                .and_then(|s| s.as_array())
                .and_then(|s| s.first())
                .and_then(|s| s.as_object())
                .and_then(|s| s.keys().next().cloned())
        });
    let scheme_name = scheme?;
    let def = doc
        .get("components")
        .and_then(|c| c.get("securitySchemes"))
        .and_then(|s| s.get(&scheme_name))?;
    let type_ = def.get("type").and_then(|t| t.as_str())?;
    match type_ {
        "http" => match def.get("scheme").and_then(|s| s.as_str()) {
            Some("bearer") => Some(Auth {
                auth_type: AuthType::Bearer,
                token: Some("{{accessToken}}".into()),
                ..Auth::default()
            }),
            Some("basic") => Some(Auth {
                auth_type: AuthType::Basic,
                username: Some("{{username}}".into()),
                password: Some("{{password}}".into()),
                ..Auth::default()
            }),
            _ => None,
        },
        "apiKey" => {
            let key_name = def.get("name").and_then(|n| n.as_str()).unwrap_or("X-Api-Key");
            let location = def.get("in").and_then(|i| i.as_str()).unwrap_or("header");
            Some(Auth {
                auth_type: AuthType::Apikey,
                key: Some(key_name.into()),
                value: Some("{{apiKey}}".into()),
                location: Some(location.into()),
                ..Auth::default()
            })
        }
        _ => None,
    }
}


/// Generates a sample value from a JSON schema. `$ref`s are resolved against
/// the document root with a depth limit.
fn sample_from_schema(schema: &Value, doc: &Value, depth: usize, warnings: &mut Vec<String>) -> Value {
    if depth > 8 {
        warnings.push("stopped deep schema recursion".into());
        return Value::Null;
    }
    if let Some(example) = schema.get("example") {
        return example.clone();
    }
    if let Some(def) = schema.get("default") {
        return def.clone();
    }
    if let Some(renums) = schema.get("enum").and_then(|e| e.as_array()) {
        if let Some(first) = renums.first() {
            return first.clone();
        }
    }
    if let Some(raw_ref) = schema.get("$ref").and_then(|r| r.as_str()) {
        if let Some(resolved) = resolve_ref(raw_ref, doc) {
            return sample_from_schema(&resolved, doc, depth + 1, warnings);
        }
        warnings.push(format!("unresolved $ref `{raw_ref}`"));
        return Value::Null;
    }
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("string") => {
            let format = schema.get("format").and_then(|f| f.as_str()).unwrap_or("");
            match format {
                "email" => Value::String("user@example.com".into()),
                "date-time" => Value::String("2026-01-01T00:00:00Z".into()),
                "date" => Value::String("2026-01-01".into()),
                "uuid" => Value::String("00000000-0000-0000-0000-000000000000".into()),
                "uri" | "url" => Value::String("https://example.com".into()),
                _ => {
                    if let Some(max) = schema.get("maxLength").and_then(|m| m.as_u64()) {
                        Value::String("a".repeat(max.min(8) as usize))
                    } else {
                        Value::String("string".into())
                    }
                }
            }
        }
        Some("integer") | Some("number") => {
            let min = schema.get("minimum").and_then(|m| m.as_f64()).unwrap_or(0.0);
            if schema.get("type").and_then(|t| t.as_str()) == Some("integer") {
                Value::Number(serde_json::Number::from(min as i64))
            } else {
                serde_json::Number::from_f64(min)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
        }
        Some("boolean") => Value::Bool(false),
        Some("array") => {
            let item = schema.get("items").cloned().unwrap_or(Value::Null);
            match &item {
                Value::Null => Value::Array(vec![]),
                _ => Value::Array(vec![sample_from_schema(&item, doc, depth + 1, warnings)]),
            }
        }
        Some("object") | None => {
            let mut map = serde_json::Map::new();
            if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                for (name, prop) in props {
                    map.insert(
                        name.clone(),
                        sample_from_schema(prop, doc, depth + 1, warnings),
                    );
                }
            }
            Value::Object(map)
        }
        Some(other) => {
            warnings.push(format!("unknown schema type `{other}`"));
            Value::Null
        }
    }
}

fn sample_value(
    schema: Option<&Value>,
    explicit: Option<&Value>,
    warnings: &mut Vec<String>,
) -> String {
    if let Some(v) = explicit {
        return match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
    }
    match schema {
        Some(s) => match sample_from_schema(s, &Value::Null, 0, warnings) {
            Value::String(s) => s,
            other => other.to_string(),
        },
        None => "string".into(),
    }
}

fn resolve_ref(ref_path: &str, doc: &Value) -> Option<Value> {
    let path = ref_path.strip_prefix("#/")?;
    let mut cur = doc;
    for seg in path.split('/') {
        cur = cur.get(seg.replace("~1", "/").replace("~0", "~").as_str())?;
    }
    Some(cur.clone())
}

fn upsert_env(root: &Path, env_name: &str, base_url: &str) -> Result<(), String> {
    let dir = root.join(ENV_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Find an existing env by display name; else create `imported.yaml`.
    let mut target = dir.join(format!("{}.yaml", slugify(env_name)));
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(doc) = yaml_to::<EnvDoc>(&text) {
                    if doc.name == env_name {
                        target = path;
                        break;
                    }
                }
            }
        }
    }
    let mut doc: EnvDoc = std::fs::read_to_string(&target)
        .ok()
        .and_then(|t| yaml_to::<EnvDoc>(&t).ok())
        .unwrap_or(EnvDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: env_name.into(),
            description: Some("Imported from OpenAPI".into()),
            variables: None,
            secrets: None,
        });
    let vars = doc.variables.get_or_insert_with(BTreeMap::new);
    vars.insert("baseUrl".into(), base_url.into());
    std::fs::write(&target, yaml_of(&doc)?).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r#"
openapi: 3.0.3
info:
  title: Pet Store
components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
servers:
  - url: https://petstore.example.com/v1
paths:
  /pets:
    get:
      summary: List pets
      tags: [pets]
      parameters:
        - name: limit
          in: query
          schema:
            type: integer
            default: 20
      responses:
        "200": { description: ok }
    post:
      summary: Create pet
      tags: [pets]
      security:
        - bearerAuth: []
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                name: { type: string, example: "Rex" }
                age: { type: integer }
      responses:
        "201": { description: created }
  /pets/{petId}:
    get:
      operationId: getPetById
      tags: [pets]
      parameters:
        - name: petId
          in: path
          schema: { type: integer }
      responses:
        "200": { description: ok }
"#;

    #[test]
    fn imports_paths_methods_and_env() {
        let dir = tempfile::tempdir().expect("dir");
        let spec_path = dir.path().join("spec.yaml");
        std::fs::write(&spec_path, SPEC).expect("write");
        let root = dir.path().join("ws");
        crate::workspace::init_workspace(&root, "ws").expect("init");

        let result = import_openapi(&spec_path, &root, "imported").expect("import");
        assert_eq!(result.files.len(), 3, "{:?}", result.files);
        assert_eq!(result.skipped, 0);

        let pets = crate::workspace::load_tree(&root.join("imported")).expect("tree");
        assert_eq!(pets.len(), 1, "one tag folder");
        assert_eq!(pets[0].name, "pets");
        assert_eq!(pets[0].children.as_ref().map(|c| c.len()), Some(3));

        let create = std::fs::read_to_string(
            root.join("imported").join("pets").join("create-pet.yaml"),
        )
        .expect("create file");
        let doc: RequestDoc = yaml_to(&create).expect("parse");
        assert_eq!(doc.request.method, HttpMethod::POST);
        assert_eq!(doc.request.url, "{{baseUrl}}/pets");
        let body = doc.request.body.expect("body");
        let parsed: Value = serde_json::from_str(&body.content.expect("content")).expect("json");
        assert_eq!(parsed["name"], "Rex");
        assert_eq!(parsed["age"], 0);
        let auth = doc.auth.expect("auth");
        assert_eq!(auth.auth_type, AuthType::Bearer);
        assert_eq!(auth.token.as_deref(), Some("{{accessToken}}"));

        let list = std::fs::read_to_string(
            root.join("imported").join("pets").join("list-pets.yaml"),
        )
        .expect("list file");
        let list_doc: RequestDoc = yaml_to(&list).expect("parse");
        let params = list_doc.request.params.expect("params");
        assert_eq!(params[0].name, "limit");
        assert_eq!(params[0].value, "20");

        let env = std::fs::read_to_string(root.join("environments").join("imported.yaml"))
            .expect("env");
        assert!(env.contains("https://petstore.example.com/v1"));
    }
}
