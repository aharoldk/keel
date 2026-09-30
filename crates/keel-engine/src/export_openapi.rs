//! Keel workspace → OpenAPI 3.0.3 JSON export.
//!
//! Reads Keel-native files and emits an OpenAPI document directly (Keel is the
//! single intermediate format). Output is deterministic: every JSON object is
//! built as a `serde_json::Value` whose map is a `BTreeMap`.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::model::*;
use crate::workspace::{self, ENV_DIR};

const IGNORED_DIRS: &[&str] = &[".git", ".keel", ENV_DIR, "node_modules", "target"];

/// Exports `folder_rel` ("" = the whole collection) under workspace `root` as
/// an OpenAPI 3.0.3 JSON string.
pub fn export_openapi(root: &Path, folder_rel: &str) -> Result<String, String> {
    let collection: CollectionDoc = workspace::load_collection(root)?;
    let start = workspace::safe_join(root, folder_rel)?;
    if !start.is_dir() {
        return Err(format!("folder `{folder_rel}` not found"));
    }

    let mut requests: Vec<(Vec<String>, RequestDoc)> = Vec::new();
    collect_requests(root, &start, &mut Vec::new(), &mut requests)?;

    let (server_url, server_note) = resolve_server(root, &collection);

    let mut paths: serde_json::Map<String, Value> = serde_json::Map::new();
    let mut schemes: BTreeMap<String, Value> = BTreeMap::new();

    for (tags, doc) in &requests {
        let (path, path_param_names) = normalize_path(&doc.request.url);
        if path.is_empty() {
            continue;
        }
        let method = doc.request.method.as_str().to_lowercase();

        let mut op = serde_json::Map::new();
        op.insert("summary".into(), json!(doc.name));
        if let Some(desc) = &doc.description {
            op.insert("description".into(), json!(desc));
        }
        op.insert("tags".into(), json!(tags));

        let mut parameters: Vec<Value> = Vec::new();
        for name in &path_param_names {
            parameters.push(json!({
                "name": name, "in": "path", "required": true,
                "schema": { "type": "string" }
            }));
        }
        for kv in enabled_rows(doc.request.params.as_ref()) {
            parameters.push(json!({
                "name": kv.name, "in": "query",
                "schema": { "type": "string" }
            }));
        }
        for kv in enabled_rows(doc.request.headers.as_ref()) {
            parameters.push(json!({
                "name": kv.name, "in": "header",
                "schema": { "type": "string" }
            }));
        }
        if !parameters.is_empty() {
            op.insert("parameters".into(), Value::Array(parameters));
        }

        if let Some(body) = &doc.request.body {
            if body.body_type != BodyType::None {
                if let Some(content) = request_body_content(body) {
                    op.insert(
                        "requestBody".into(),
                        json!({ "content": Value::Object(content) }),
                    );
                }
            }
        }

        if let Some(auth) = &doc.auth {
            if let Some(scheme) = register_scheme(auth, &mut schemes) {
                op.insert("security".into(), json!([{ scheme: [] }]));
            }
        }

        op.insert(
            "responses".into(),
            json!({ "200": { "description": "Response" } }),
        );

        let item = paths
            .entry(path)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if item.get(&method).is_some() {
            // Deterministic: first (sorted) request wins; report the clash.
            continue;
        }
        item.as_object_mut().expect("object").insert(method, Value::Object(op));
    }

    let mut doc_root = serde_json::Map::new();
    doc_root.insert("openapi".into(), json!("3.0.3"));
    let mut info = serde_json::Map::new();
    info.insert("title".into(), json!(collection.name));
    info.insert("version".into(), json!("1.0.0"));
    let mut description = collection.description.clone().unwrap_or_default();
    if !server_note.is_empty() {
        if !description.is_empty() {
            description.push('\n');
        }
        description.push_str(&server_note);
    }
    if !description.is_empty() {
        info.insert("description".into(), json!(description));
    }
    doc_root.insert("info".into(), Value::Object(info));
    doc_root.insert(
        "servers".into(),
        json!([{ "url": server_url }]),
    );
    doc_root.insert("paths".into(), Value::Object(paths));

    if collection.auth.as_ref().map(|a| a.auth_type != AuthType::None).unwrap_or(false) {
        let mut schemes = schemes.clone();
        if let Some(scheme) = register_scheme(collection.auth.as_ref().expect("auth"), &mut schemes)
        {
            doc_root.insert("security".into(), json!([{ scheme: [] }]));
        }
        doc_root.insert("components".into(), components(&schemes));
    } else if !schemes.is_empty() {
        doc_root.insert("components".into(), components(&schemes));
    }

    Ok(serde_json::to_string_pretty(&Value::Object(doc_root)).expect("serialize"))
}

fn components(schemes: &BTreeMap<String, Value>) -> Value {
    let mut map = serde_json::Map::new();
    for (name, def) in schemes {
        map.insert(name.clone(), def.clone());
    }
    json!({ "securitySchemes": Value::Object(map) })
}

/// Registers the auth scheme and returns its name. `none` and unsupported
/// types return `None` (no security entry).
fn register_scheme(auth: &Auth, schemes: &mut BTreeMap<String, Value>) -> Option<String> {
    match auth.auth_type {
        AuthType::None => None,
        AuthType::Basic => {
            schemes
                .entry("basic".into())
                .or_insert_with(|| json!({ "type": "http", "scheme": "basic" }));
            Some("basic".into())
        }
        AuthType::Bearer => {
            schemes
                .entry("bearer".into())
                .or_insert_with(|| json!({ "type": "http", "scheme": "bearer", "bearerFormat": "JWT" }));
            Some("bearer".into())
        }
        AuthType::Digest => {
            schemes
                .entry("digest".into())
                .or_insert_with(|| json!({ "type": "http", "scheme": "digest" }));
            Some("digest".into())
        }
        AuthType::Apikey => {
            let name = "apikey".to_string();
            schemes.entry(name.clone()).or_insert_with(|| {
                json!({
                    "type": "apiKey",
                    "name": auth.key.clone().unwrap_or_else(|| "X-Api-Key".into()),
                    "in": if auth.location.as_deref() == Some("query") { "query" } else { "header" },
                })
            });
            Some(name)
        }
        AuthType::Oauth2 => None,
    }
}

/// Strips scheme/host/`{{baseUrl}}` prefixes and the query string, converts
/// `:name` segments to `{name}`, and returns the ordered path-param names.
fn normalize_path(url: &str) -> (String, Vec<String>) {
    let mut rest = url.split('?').next().unwrap_or(url).to_string();

    // Remove a leading template prefix, e.g. `{{baseUrl}}`.
    while rest.starts_with("{{") {
        match rest.find("}}") {
            Some(end) => rest = rest[end + 2..].to_string(),
            None => break,
        }
    }
    // Remove scheme://host[:port].
    if let Some(idx) = rest.find("://") {
        rest = rest[idx + 3..].to_string();
        match rest.find('/') {
            Some(slash) => rest = rest[slash..].to_string(),
            None => rest = "/".to_string(),
        }
    }
    if rest.is_empty() {
        rest = "/".to_string();
    }
    if !rest.starts_with('/') {
        rest = format!("/{rest}");
    }

    let mut names = Vec::new();
    let segments: Vec<String> = rest
        .split('/')
        .map(|seg| {
            if let Some(name) = seg.strip_prefix(':') {
                if !name.is_empty() {
                    names.push(name.to_string());
                    return format!("{{{name}}}");
                }
            }
            seg.to_string()
        })
        .collect();
    (segments.join("/"), names)
}

/// Resolves `servers[0].url` from the default environment's `baseUrl` when
/// possible; otherwise a `{{baseUrl}}` placeholder plus a description note.
fn resolve_server(root: &Path, collection: &CollectionDoc) -> (String, String) {
    if let Some(env_name) = &collection.default_environment {
        let dir = root.join(ENV_DIR);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut names: Vec<_> = entries.flatten().collect();
            names.sort_by_key(|e| e.file_name());
            for entry in names {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if let Ok(doc) = yaml_to::<EnvDoc>(&text) {
                    if &doc.name == env_name {
                        if let Some(base) = doc.variables.as_ref().and_then(|v| v.get("baseUrl")) {
                            if !base.trim().is_empty() {
                                return (base.trim_end_matches('/').to_string(), String::new());
                            }
                        }
                    }
                }
            }
        }
    }
    (
        "{{baseUrl}}".into(),
        "servers[0].url is a placeholder: set a `baseUrl` variable in the default environment."
            .into(),
    )
}

fn enabled_rows(rows: Option<&Vec<KV>>) -> Vec<&KV> {
    rows.into_iter()
        .flatten()
        .filter(|kv| kv.enabled && !kv.name.trim().is_empty())
        .collect()
}

fn request_body_content(body: &Body) -> Option<serde_json::Map<String, Value>> {
    let mut map = serde_json::Map::new();
    match body.body_type {
        BodyType::None => return None,
        BodyType::Json => {
            let mut entry = serde_json::Map::new();
            if let Some(content) = &body.content {
                if let Ok(parsed) = serde_json::from_str::<Value>(content) {
                    entry.insert("example".into(), parsed);
                } else {
                    entry.insert("schema".into(), json!({ "type": "object" }));
                }
            }
            map.insert("application/json".into(), Value::Object(entry));
        }
        BodyType::Text => {
            map.insert(
                "text/plain".into(),
                json!({ "example": body.content.clone().unwrap_or_default() }),
            );
        }
        BodyType::Xml => {
            map.insert(
                "application/xml".into(),
                json!({ "example": body.content.clone().unwrap_or_default() }),
            );
        }
        BodyType::Graphql => {
            let variables = body
                .variables
                .as_deref()
                .and_then(|v| serde_json::from_str::<Value>(v).ok())
                .unwrap_or(Value::String(body.variables.clone().unwrap_or_default()));
            map.insert(
                "application/json".into(),
                json!({
                    "schema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string" },
                            "variables": { "type": "object" },
                        }
                    },
                    "example": {
                        "query": body.query.clone().unwrap_or_default(),
                        "variables": variables,
                    }
                }),
            );
        }
        BodyType::FormUrlencoded => {
            let mut props = serde_json::Map::new();
            for kv in enabled_rows(body.items.as_ref()) {
                props.insert(kv.name.clone(), json!({ "type": "string" }));
            }
            map.insert(
                "application/x-www-form-urlencoded".into(),
                json!({ "schema": { "type": "object", "properties": Value::Object(props) } }),
            );
        }
        BodyType::Multipart => {
            let mut props = serde_json::Map::new();
            for kv in enabled_rows(body.items.as_ref()) {
                let def = if kv.kind.as_deref() == Some("file") {
                    json!({ "type": "string", "format": "binary" })
                } else {
                    json!({ "type": "string" })
                };
                props.insert(kv.name.clone(), def);
            }
            map.insert(
                "multipart/form-data".into(),
                json!({ "schema": { "type": "object", "properties": Value::Object(props) } }),
            );
        }
        BodyType::Binary => {
            map.insert(
                "application/octet-stream".into(),
                json!({ "schema": { "type": "string", "format": "binary" } }),
            );
        }
    }
    Some(map)
}

fn collect_requests(
    root: &Path,
    dir: &Path,
    tags: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, RequestDoc)>,
) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("read `{}`: {e}", dir.display()))?
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name.starts_with('.') || IGNORED_DIRS.contains(&name.as_str()) {
            continue;
        }
        if path.is_dir() {
            tags.push(name);
            collect_requests(root, &path, tags, out)?;
            tags.pop();
        } else if name.ends_with(".yaml") && name != "folder.yaml" && name != "collection.yaml" {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(doc) = yaml_to::<RequestDoc>(&text) {
                if doc.kind == "request" {
                    out.push((tags.clone(), doc));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("dir");
        workspace::init_workspace(dir.path(), "Test API").expect("init");
        dir
    }

    fn save(root: &Path, rel: &str, text: &str) {
        let doc: RequestDoc = yaml_to(text).expect("request yaml");
        workspace::save_request(root, rel, &doc).expect("save");
    }

    const REQ_YAMLS: &[(&str, &str)] = &[
        (
            "users/list-users.yaml",
            r#"schemaVersion: "1"
name: List Users
description: Page through users
request:
  method: GET
  url: "{{baseUrl}}/users"
  params:
    - name: page
      value: "1"
      enabled: true
    - name: gone
      value: "x"
      enabled: false
  headers:
    - name: Accept
      value: application/json
      enabled: true
"#,
        ),
        (
            "users/get-user.yaml",
            r#"schemaVersion: "1"
name: Get User
request:
  method: GET
  url: "{{baseUrl}}/users/:id"
  pathParams:
    - name: id
      value: "7"
      enabled: true
auth:
  type: bearer
  token: "{{accessToken}}"
"#,
        ),
        (
            "posts/create-post.yaml",
            r#"schemaVersion: "1"
name: Create Post
request:
  method: POST
  url: "{{baseUrl}}/posts"
  body:
    type: json
    content: '{"title": "hi"}'
"#,
        ),
    ];

    fn export(dir: &tempfile::TempDir) -> Value {
        let root = dir.path();
        for (rel, text) in REQ_YAMLS {
            save(root, rel, text);
        }
        let json = export_openapi(root, "").expect("export");
        serde_json::from_str(&json).expect("valid json")
    }

    #[test]
    fn paths_tags_and_methods() {
        let dir = ws();
        let doc = export(&dir);
        assert_eq!(doc["openapi"], "3.0.3");
        assert_eq!(doc["info"]["title"], "Test API");
        assert_eq!(doc["info"]["version"], "1.0.0");
        let paths = doc["paths"].as_object().expect("paths");
        assert_eq!(paths.len(), 3, "{:?}", paths.keys());
        assert!(paths.contains_key("/users"));
        assert!(paths.contains_key("/users/{id}"), "braces from :id segment");
        assert!(paths.contains_key("/posts"));
        assert_eq!(doc["paths"]["/users"]["get"]["summary"], "List Users");
        assert_eq!(doc["paths"]["/users/{id}"]["get"]["tags"][0], "users");
        assert_eq!(doc["paths"]["/posts"]["post"]["tags"][0], "posts");
        assert_eq!(doc["paths"]["/users"]["get"]["description"], "Page through users");
    }

    #[test]
    fn parameters_query_header_path() {
        let dir = ws();
        let doc = export(&dir);
        let list = doc["paths"]["/users"]["get"]["parameters"].as_array().unwrap();
        assert!(list.iter().any(|p| p["name"] == "page" && p["in"] == "query"));
        assert!(list.iter().any(|p| p["name"] == "Accept" && p["in"] == "header"));
        assert!(
            !list.iter().any(|p| p["name"] == "gone"),
            "disabled rows excluded"
        );
        let get = doc["paths"]["/users/{id}"]["get"]["parameters"].as_array().unwrap();
        let id = get.iter().find(|p| p["name"] == "id").expect("path param");
        assert_eq!(id["required"], true);
        assert_eq!(id["schema"]["type"], "string");
    }

    #[test]
    fn request_body_and_security() {
        let dir = ws();
        let doc = export(&dir);
        let body = &doc["paths"]["/posts"]["post"]["requestBody"]["content"];
        assert_eq!(body["application/json"]["example"]["title"], "hi");
        assert_eq!(
            doc["paths"]["/users/{id}"]["get"]["security"][0]["bearer"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(doc["components"]["securitySchemes"]["bearer"]["scheme"], "bearer");
        assert!(doc["paths"]["/users"]["get"].get("security").is_none());
    }

    #[test]
    fn server_prefers_default_env_base_url() {
        let dir = ws();
        let doc = export(&dir);
        // init_workspace provides `local` env with baseUrl http://localhost:8080
        assert_eq!(doc["servers"][0]["url"], "http://localhost:8080");
    }

    #[test]
    fn server_placeholder_and_note_without_env() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path();
        std::fs::write(
            root.join("collection.yaml"),
            "schemaVersion: \"1\"\nname: No Env\n",
        )
        .expect("collection");
        save(
            root,
            "a.yaml",
            r#"schemaVersion: "1"
name: Root Get
request:
  method: GET
  url: "https://api.example.com/v1/things"
"#,
        );
        let doc: Value = serde_json::from_str(&export_openapi(root, "").expect("export"))
            .expect("json");
        assert_eq!(doc["servers"][0]["url"], "{{baseUrl}}");
        assert!(doc["info"]["description"]
            .as_str()
            .unwrap_or("")
            .contains("placeholder"));
        assert!(
            doc["paths"].get("/v1/things").is_some(),
            "scheme://host stripped: {:?}",
            doc["paths"]
        );
    }

    #[test]
    fn folder_scoped_export() {
        let dir = ws();
        let doc = export(&dir);
        let _ = doc;
        let scoped = export_openapi(dir.path(), "users").expect("scoped");
        let scoped: Value = serde_json::from_str(&scoped).unwrap();
        assert!(scoped["paths"].get("/users").is_some());
        assert!(scoped["paths"].get("/posts").is_none());
        // Tags are relative to the export root, so the scoped run has none.
        assert_eq!(scoped["paths"]["/users"]["get"]["tags"].as_array().unwrap().len(), 0);
        export_openapi(dir.path(), "does-not-exist").expect_err("missing folder errors");
    }

    #[test]
    fn deterministic_output() {
        let dir = ws();
        export(&dir);
        let a = export_openapi(dir.path(), "").expect("a");
        let b = export_openapi(dir.path(), "").expect("b");
        assert_eq!(a, b);
    }
}
