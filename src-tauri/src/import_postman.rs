//! Postman Collection v2.0/v2.1 importer.
//!
//! Normalizes a Postman export **into Keel-native files** (`*.yaml` request
//! docs, `folder.yaml` metadata, one `environments/*.yaml` per collection):
//! Postman JSON is never used as an intermediate format. Everything that
//! cannot be represented is dropped with an explicit warning — the import is
//! deterministic and fully reported via `OpenApiResultDto`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::model::*;
use crate::workspace::{self, slugify, ENV_DIR};

/// Imports a Postman collection JSON file into the Keel workspace at `root`,
/// placing requests under `target_folder` ("" = workspace root).
pub fn import_postman(
    source: &Path,
    root: &Path,
    target_folder: &str,
) -> Result<OpenApiResultDto, String> {
    let text = std::fs::read_to_string(source).map_err(|e| format!("read file: {e}"))?;
    let doc: Value =
        serde_json::from_str(&text).map_err(|e| format!("parse Postman collection: {e}"))?;

    // Postman environment exports are a different shape than collections.
    if looks_like_postman_environment(&doc) {
        return import_postman_environment(source, root);
    }

    let mut files: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    let mut examples_skipped = 0usize;

    let info = doc.get("info");
    let collection_name = info
        .and_then(|i| i.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("Postman Collection")
        .to_string();
    let schema = info
        .and_then(|i| i.get("schema"))
        .and_then(|s| s.as_str())
        .unwrap_or("");
    if !schema.is_empty()
        && !schema.contains("/v2.0/")
        && !schema.contains("/v2.1/")
        && !schema.contains("/v2.0.0/")
        && !schema.contains("/v2.1.0/")
    {
        warnings.push(format!(
            "Unrecognized Postman schema `{schema}` — attempting v2 import anyway."
        ));
    }

    let target_root = workspace::safe_join(root, target_folder)?;
    std::fs::create_dir_all(&target_root).map_err(|e| e.to_string())?;

    // Collection-level auth / scripts cannot be attached to the target folder
    // without inventing inheritance semantics; report the loss.
    if let Some(auth) = doc.get("auth") {
        let kind = auth.get("type").and_then(|t| t.as_str()).unwrap_or("?");
        warnings.push(format!(
            "Collection-level auth `{kind}` not imported — apply it in collection/folder settings."
        ));
    }
    if doc.get("event").and_then(|e| e.as_array()).map(|a| !a.is_empty()).unwrap_or(false) {
        warnings.push("Collection-level scripts not imported (kept in the source file only).".into());
    }

    // Collection variable[] → one environment file (merged if it exists).
    if let Some(vars) = doc.get("variable").and_then(|v| v.as_array()) {
        if !vars.is_empty() {
            let mut map = BTreeMap::new();
            for v in vars {
                let key = v.get("key").and_then(|k| k.as_str()).unwrap_or("");
                if key.is_empty() {
                    continue;
                }
                let value = match v.get("value") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Null) | None => String::new(),
                    Some(other) => other.to_string(),
                };
                map.insert(key.to_string(), value);
            }
            if !map.is_empty() {
                let name = slugify(&collection_name);
                upsert_collection_env(root, &name, &collection_name, &map)?;
            }
        }
    }

    let items = doc
        .get("item")
        .and_then(|i| i.as_array())
        .cloned()
        .unwrap_or_default();
    walk_items(
        root,
        &target_root,
        &items,
        &mut files,
        &mut warnings,
        &mut skipped,
        &mut examples_skipped,
    )?;

    if examples_skipped > 0 {
        warnings.push(format!(
            "{examples_skipped} saved response example(s) skipped (Keel has no example-response store)."
        ));
    }

    Ok(OpenApiResultDto {
        files,
        skipped,
        warnings,
    })
}

/// True when the JSON looks like a Postman **environment** export
/// (`{ name, values: [{ key, value, enabled?, type? }] }`).
pub fn looks_like_postman_environment(doc: &Value) -> bool {
    doc.get("values").and_then(|v| v.as_array()).is_some()
        && doc.get("item").is_none()
        && doc.get("info").is_none()
        && doc
            .get("name")
            .and_then(|n| n.as_str())
            .map(|n| !n.is_empty())
            .unwrap_or(false)
}

/// Imports a Postman **environment** export into `environments/<slug>.yaml`.
/// `type: "secret"` values are pushed to the OS keychain (Keel keeps only
/// names in YAML); `enabled: false` values are imported but reported.
pub fn import_postman_environment(
    source: &Path,
    root: &Path,
) -> Result<OpenApiResultDto, String> {
    let text = std::fs::read_to_string(source).map_err(|e| format!("read file: {e}"))?;
    let doc: Value =
        serde_json::from_str(&text).map_err(|e| format!("parse Postman environment: {e}"))?;
    if !looks_like_postman_environment(&doc) {
        return Err("Not a Postman environment export (expected `{ name, values: [...] }`)".into());
    }

    let name = doc
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Postman Environment")
        .to_string();

    let mut vars = BTreeMap::new();
    let mut secret_values: BTreeMap<String, String> = BTreeMap::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut disabled = 0usize;

    for v in doc
        .get("values")
        .and_then(|vals| vals.as_array())
        .into_iter()
        .flatten()
    {
        let key = v.get("key").and_then(|k| k.as_str()).unwrap_or("");
        if key.is_empty() {
            continue;
        }
        let value = match v.get("value") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        };
        if v.get("enabled").and_then(|e| e.as_bool()) == Some(false) {
            disabled += 1;
        }
        if v.get("type").and_then(|t| t.as_str()) == Some("secret") {
            secret_values.insert(key.to_string(), value);
        } else {
            vars.insert(key.to_string(), value);
        }
    }

    if disabled > 0 {
        warnings.push(format!(
            "{disabled} variable(s) were disabled in Postman — imported as enabled."
        ));
    }

    let slug = slugify(&name);
    let env_name_for_keychain = slug.clone();

    // Secret values go to the keychain; a failure must not lose the import.
    let mut failed_secrets: Vec<String> = Vec::new();
    for (k, v) in &secret_values {
        if crate::secrets::set(&root.to_string_lossy(), &env_name_for_keychain, k, v).is_err() {
            failed_secrets.push(k.clone());
        }
    }
    if !failed_secrets.is_empty() {
        warnings.push(format!(
            "Could not store secret value(s) {} in the keychain — set them in the secrets panel.",
            failed_secrets.join(", ")
        ));
    }

    let secret_names: Vec<String> = secret_values.keys().cloned().collect();
    upsert_environment(root, &slug, &name, &vars, &secret_names)?;

    Ok(OpenApiResultDto {
        files: vec![format!("{ENV_DIR}/{slug}.yaml")],
        skipped: 0,
        warnings,
    })
}

/// Creates or merges `environments/<slug>.yaml`, tracking secret-valued vars.
fn upsert_environment(
    root: &Path,
    env_slug: &str,
    display_name: &str,
    vars: &BTreeMap<String, String>,
    secret_names: &[String],
) -> Result<(), String> {
    let dir = root.join(ENV_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(format!("{env_slug}.yaml"));
    let mut doc: EnvDoc = std::fs::read_to_string(&target)
        .ok()
        .and_then(|t| yaml_to::<EnvDoc>(&t).ok())
        .unwrap_or(EnvDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: display_name.to_string(),
            description: Some(format!("Imported from Postman environment `{display_name}`")),
            variables: None,
            secrets: None,
        });
    let existing = doc.variables.get_or_insert_with(BTreeMap::new);
    for (k, v) in vars {
        existing.insert(k.clone(), v.clone());
    }
    if !secret_names.is_empty() {
        let existing_secrets = doc.secrets.get_or_insert_with(BTreeMap::new);
        for s in secret_names {
            existing_secrets.entry(s.clone()).or_default();
        }
    }
    std::fs::write(&target, yaml_of(&doc)?).map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
fn walk_items(
    root: &Path,
    dir: &Path,
    items: &[Value],
    files: &mut Vec<String>,
    warnings: &mut Vec<String>,
    skipped: &mut usize,
    examples: &mut usize,
) -> Result<(), String> {
    for item in items {
        if item.get("item").and_then(|i| i.as_array()).is_some() {
            // Folder: item without a request.
            let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("folder");
            let folder = unique_dir(dir, &slugify(name));
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;

            let scripts = map_events(item.get("event"), name, warnings);
            let auth = item.get("auth").and_then(|a| map_auth(a, name, warnings));
            let has_meta = scripts.is_some() || auth.is_some();
            if has_meta {
                let folder_doc = FolderDoc {
                    schema_version: SCHEMA_VERSION.into(),
                    kind: Some("folder".into()),
                    name: Some(name.to_string()),
                    description: description_of(item),
                    variables: None,
                    auth,
                    headers: None,
                    scripts,
                    order: None,
                };
                workspace::save_folder(&folder, &folder_doc)?;
            }

            let children = item.get("item").and_then(|i| i.as_array()).cloned().unwrap_or_default();
            walk_items(root, &folder, &children, files, warnings, skipped, examples)?;
        } else if item.get("request").is_some() {
            import_request(root, dir, item, files, warnings, skipped, examples)?;
        } else {
            *skipped += 1;
            warnings.push(format!(
                "Skipped item `{}` (no request and no sub-items).",
                item.get("name").and_then(|n| n.as_str()).unwrap_or("?")
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn import_request(
    root: &Path,
    dir: &Path,
    item: &Value,
    files: &mut Vec<String>,
    warnings: &mut Vec<String>,
    skipped: &mut usize,
    examples: &mut usize,
) -> Result<(), String> {
    let name = item
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Request")
        .to_string();

    let request = match item.get("request") {
        Some(Value::String(url)) => {
            let mut obj = serde_json::Map::new();
            obj.insert("method".into(), Value::String("GET".into()));
            obj.insert("url".into(), Value::String(url.clone()));
            Value::Object(obj)
        }
        Some(r) => r.clone(),
        None => {
            *skipped += 1;
            return Ok(());
        }
    };

    let method_raw = request
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let method = match method_raw.as_str() {
        "GET" => HttpMethod::GET,
        "POST" => HttpMethod::POST,
        "PUT" => HttpMethod::PUT,
        "PATCH" => HttpMethod::PATCH,
        "DELETE" => HttpMethod::DELETE,
        "HEAD" => HttpMethod::HEAD,
        "OPTIONS" => HttpMethod::OPTIONS,
        "TRACE" => HttpMethod::TRACE,
        other => {
            *skipped += 1;
            warnings.push(format!("Skipped request `{name}`: unsupported method `{other}`."));
            return Ok(());
        }
    };

    let mut req_warnings: Vec<String> = Vec::new();
    let (url, params, path_params) = build_url(&request, &mut req_warnings);
    for w in req_warnings {
        warnings.push(format!("`{name}`: {w}"));
    }

    let headers = request
        .get("header")
        .map(|h| map_header_rows(h, &name, warnings))
        .unwrap_or_default();

    let body = request
        .get("body")
        .and_then(|b| map_body(b, &name, warnings));

    let auth = request.get("auth").and_then(|a| map_auth(a, &name, warnings));

    let scripts = map_events(item.get("event"), &name, warnings);

    if let Some(responses) = item.get("response").and_then(|r| r.as_array()) {
        *examples += responses.len();
    }

    let doc = RequestDoc {
        schema_version: SCHEMA_VERSION.into(),
        name: name.clone(),
        kind: "request".into(),
        description: description_of(&request),
        protocol: None,
        graphql: None,
        websocket: None,
        grpc: None,
        request: RequestBlock {
            method,
            url,
            params: if params.is_empty() { None } else { Some(params) },
            headers: if headers.is_empty() { None } else { Some(headers) },
            path_params: if path_params.is_empty() { None } else { Some(path_params) },
            body,
        },
        auth,
        variables: None,
        scripts,
        tests: None,
    };

    let target = workspace::unique_file(dir, &slugify(&name));
    std::fs::write(&target, yaml_of(&doc)?).map_err(|e| e.to_string())?;
    files.push(workspace::rel_path_public(root, &target));
    Ok(())
}

/// Resolves a Postman `url` (raw string or object) into a Keel URL (query
/// stripped, `:name` path segments preserved), query param rows and path
/// param rows from `url.variable`.
fn build_url(request: &Value, warnings: &mut Vec<String>) -> (String, Vec<KV>, Vec<KV>) {
    let url_value = match request.get("url") {
        Some(v) => v.clone(),
        None => return (String::new(), Vec::new(), Vec::new()),
    };

    let mut query_rows: Vec<KV> = Vec::new();
    let mut path_rows: Vec<KV> = Vec::new();

    let raw_opt = match &url_value {
        Value::String(s) => Some(s.clone()),
        Value::Object(_) => url_value
            .get("raw")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string()),
        _ => None,
    };

    let base = match &url_value {
        Value::Object(_) => {
            let protocol = url_value.get("protocol").and_then(|p| p.as_str());
            let host = url_value
                .get("host")
                .and_then(|h| h.as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p.as_str())
                        .collect::<Vec<_>>()
                        .join(".")
                })
                .unwrap_or_default();
            let path = url_value
                .get("path")
                .and_then(|p| p.as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p.as_str())
                        .collect::<Vec<_>>()
                        .join("/")
                })
                .unwrap_or_default();
            if let Some(qs) = url_value.get("query").and_then(|q| q.as_array()) {
                for q in qs {
                    let key = q.get("key").and_then(|k| k.as_str()).unwrap_or("");
                    if key.is_empty() {
                        continue;
                    }
                    let value = match q.get("value") {
                        Some(Value::String(s)) => s.clone(),
                        Some(Value::Null) | None => String::new(),
                        Some(other) => other.to_string(),
                    };
                    let enabled = !q.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false);
                    query_rows.push(KV {
                        name: key.to_string(),
                        value,
                        enabled,
                        kind: None,
                    });
                }
            }
            match protocol {
                Some(p) if !host.is_empty() => {
                    let path = if path.is_empty() {
                        String::new()
                    } else if path.starts_with('/') {
                        path
                    } else {
                        format!("/{path}")
                    };
                    format!("{p}://{host}{path}")
                }
                _ => {
                    if !host.is_empty() {
                        format!("{host}/{}", path.trim_start_matches('/'))
                    } else {
                        format!("/{path}")
                    }
                }
            }
        }
        _ => raw_opt.clone().unwrap_or_default(),
    };

    // `{{var}}` prefixes are preserved verbatim by construction; a raw string
    // URL gets its query split into rows.
    let (mut url, inline_query) = match base.split_once('?') {
        Some((before, after)) => (before.to_string(), Some(after.to_string())),
        None => (base, None),
    };
    if url.is_empty() {
        if let Some(raw) = &raw_opt {
            url = raw.split_once('?').map(|(b, _)| b.to_string()).unwrap_or_default();
        }
    }

    // Query parsed from a raw string (object form already handled above).
    let has_object_query = matches!(&url_value, Value::Object(o) if o.contains_key("query"));
    if !has_object_query {
        if let Some(qs) = inline_query {
            for pair in qs.split('&').filter(|p| !p.trim().is_empty()) {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                query_rows.push(KV {
                    name: k.to_string(),
                    value: v.to_string(),
                    enabled: true,
                    kind: None,
                });
            }
        }
    }

    // Path variables → pathParams rows. Keys appear as `id` or `:id`.
    if let Some(vars) = url_value.get("variable").and_then(|v| v.as_array()) {
        for v in vars {
            let key = v.get("key").and_then(|k| k.as_str()).unwrap_or("");
            if key.is_empty() {
                continue;
            }
            let value = match v.get("value") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Null) | None => String::new(),
                Some(other) => other.to_string(),
            };
            let enabled = !v.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false);
            path_rows.push(KV {
                name: key.trim_start_matches(':').to_string(),
                value,
                enabled,
                kind: None,
            });
        }
    }

    // Warn about template tokens we could not associate with a row.
    if let Some(raw) = &raw_opt {
        for seg in raw
            .split('/')
            .filter(|s| s.starts_with(':') && s.len() > 1)
            .map(|s| s.trim_start_matches(':').split('?').next().unwrap_or(s))
        {
            if !path_rows.iter().any(|kv| kv.name == seg) {
                warnings.push(format!("path variable `:{seg}` has no value in the collection"));
            }
        }
    }

    (url, query_rows, path_rows)
}

fn map_header_rows(header: &Value, name: &str, warnings: &mut Vec<String>) -> Vec<KV> {
    let list = match header.as_array() {
        Some(l) => l,
        None => {
            warnings.push(format!("`{name}`: unsupported `header` shape — headers dropped."));
            return Vec::new();
        }
    };
    let mut rows = Vec::new();
    for h in list {
        match h {
            Value::String(s) => {
                // "Key: value" string form.
                let (k, v) = s.split_once(':').unwrap_or((s.as_str(), ""));
                if !k.trim().is_empty() {
                    rows.push(KV {
                        name: k.trim().to_string(),
                        value: v.trim().to_string(),
                        enabled: true,
                        kind: None,
                    });
                }
            }
            Value::Object(_) => {
                let key = h.get("key").and_then(|k| k.as_str()).unwrap_or("").trim().to_string();
                if key.is_empty() {
                    continue;
                }
                let value = match h.get("value") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Null) | None => String::new(),
                    Some(other) => other.to_string(),
                };
                let enabled = !h.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false);
                rows.push(KV {
                    name: key,
                    value,
                    enabled,
                    kind: None,
                });
            }
            _ => {}
        }
    }
    rows
}

fn map_body(body: &Value, name: &str, warnings: &mut Vec<String>) -> Option<Body> {
    let mode = body.get("mode").and_then(|m| m.as_str()).unwrap_or("");
    match mode {
        "raw" => {
            let content = body.get("raw").and_then(|r| r.as_str()).unwrap_or("").to_string();
            let language = body
                .get("options")
                .and_then(|o| o.get("raw"))
                .and_then(|r| r.get("language"))
                .and_then(|l| l.as_str())
                .unwrap_or("text");
            let body_type = match language {
                "json" => BodyType::Json,
                "xml" => BodyType::Xml,
                _ => BodyType::Text,
            };
            Some(Body {
                body_type,
                content: Some(content),
                ..Body::default()
            })
        }
        "urlencoded" => {
            let rows = kv_rows(body.get("urlencoded"), name, warnings);
            Some(Body {
                body_type: BodyType::FormUrlencoded,
                items: Some(rows),
                ..Body::default()
            })
        }
        "formdata" => {
            let mut rows = Vec::new();
            for entry in body.get("formdata").and_then(|f| f.as_array()).into_iter().flatten() {
                let key = entry.get("key").and_then(|k| k.as_str()).unwrap_or("").trim().to_string();
                if key.is_empty() {
                    continue;
                }
                let enabled = !entry.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false);
                if entry.get("type").and_then(|t| t.as_str()) == Some("file") {
                    let src = entry
                        .get("src")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .trim_start_matches('@')
                        .to_string();
                    rows.push(KV {
                        name: key,
                        value: src,
                        enabled,
                        kind: Some("file".into()),
                    });
                } else {
                    let value = match entry.get("value") {
                        Some(Value::String(s)) => s.clone(),
                        Some(Value::Null) | None => String::new(),
                        Some(other) => other.to_string(),
                    };
                    rows.push(KV {
                        name: key,
                        value,
                        enabled,
                        kind: None,
                    });
                }
            }
            Some(Body {
                body_type: BodyType::Multipart,
                items: Some(rows),
                ..Body::default()
            })
        }
        "file" => {
            let src = body
                .get("file")
                .and_then(|f| f.get("src"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim_start_matches('@')
                .to_string();
            Some(Body {
                body_type: BodyType::Binary,
                path: Some(src),
                ..Body::default()
            })
        }
        "graphql" => {
            let graphql = body.get("graphql");
            let query = graphql
                .and_then(|g| g.get("query"))
                .and_then(|q| q.as_str())
                .unwrap_or("")
                .to_string();
            let variables = graphql
                .and_then(|g| g.get("variables"))
                .and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Null => None,
                    other => Some(other.to_string()),
                });
            Some(Body {
                body_type: BodyType::Graphql,
                query: Some(query),
                variables,
                ..Body::default()
            })
        }
        other => {
            warnings.push(format!(
                "`{name}`: unsupported body mode `{other}` — body dropped."
            ));
            None
        }
    }
}

fn kv_rows(list: Option<&Value>, name: &str, warnings: &mut Vec<String>) -> Vec<KV> {
    let arr = match list {
        Some(Value::Array(a)) => a,
        Some(_) => {
            warnings.push(format!("`{name}`: malformed body items — dropped."));
            return Vec::new();
        }
        None => return Vec::new(),
    };
    let mut rows = Vec::new();
    for entry in arr {
        let key = entry.get("key").and_then(|k| k.as_str()).unwrap_or("").trim().to_string();
        if key.is_empty() {
            continue;
        }
        let value = match entry.get("value") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        };
        let enabled = !entry.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false);
        rows.push(KV {
            name: key,
            value,
            enabled,
            kind: None,
        });
    }
    rows
}

/// Maps Postman `auth` to Keel `Auth`. Unsupported types produce a warning
/// and no auth block (Keel `none`).
fn map_auth(auth: &Value, name: &str, warnings: &mut Vec<String>) -> Option<Auth> {
    let kind = auth.get("type").and_then(|t| t.as_str()).unwrap_or("noauth");
    let list = |a: &Value| -> Value {
        // auth.<type> is an array of {key,value}; accept an object too.
        match a.get(kind) {
            Some(Value::Array(_)) | Some(Value::Object(_)) => a.get(kind).cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        }
    };
    let field = |a: &Value, key: &str| -> String {
        let v = list(a);
        match &v {
            Value::Array(items) => items
                .iter()
                .find(|i| i.get("key").and_then(|k| k.as_str()) == Some(key))
                .and_then(|i| match i.get("value") {
                    Some(Value::String(s)) => Some(s.clone()),
                    Some(Value::Null) | None => Some(String::new()),
                    Some(other) => Some(other.to_string()),
                })
                .unwrap_or_default(),
            Value::Object(map) => map
                .get(key)
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string(),
            _ => String::new(),
        }
    };

    match kind {
        "noauth" | "" => None,
        "basic" => Some(Auth {
            auth_type: AuthType::Basic,
            username: Some(field(auth, "username")),
            password: Some(field(auth, "password")),
            ..Auth::default()
        }),
        "bearer" => Some(Auth {
            auth_type: AuthType::Bearer,
            token: Some(field(auth, "token")),
            ..Auth::default()
        }),
        "apikey" => {
            let location = field(auth, "in");
            Some(Auth {
                auth_type: AuthType::Apikey,
                key: Some(field(auth, "key")),
                value: Some(field(auth, "value")),
                location: Some(if location == "query" { "query".into() } else { "header".into() }),
                ..Auth::default()
            })
        }
        "digest" => Some(Auth {
            auth_type: AuthType::Digest,
            username: Some(field(auth, "username")),
            password: Some(field(auth, "password")),
            ..Auth::default()
        }),
        "oauth2" => {
            let grant_raw = ["grant_type", "grantType", "granttype"]
                .iter()
                .map(|key| field(auth, key))
                .find(|v| !v.is_empty())
                .unwrap_or_default()
                .to_ascii_lowercase()
                .replace(' ', "_");
            let grant = match grant_raw.as_str() {
                "" | "authorization_code" | "authorizationcode" => {
                    OAuth2GrantType::AuthorizationCode
                }
                "client_credentials" | "clientcredentials" => OAuth2GrantType::ClientCredentials,
                "password" | "password_credentials" | "passwordcredentials" => {
                    OAuth2GrantType::Password
                }
                other => {
                    warnings.push(format!(
                        "`{name}`: OAuth2 grant `{other}` not supported — auth dropped."
                    ));
                    return None;
                }
            };
            let opt = |key: &str| {
                let v = field(auth, key);
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            };
            Some(Auth {
                auth_type: AuthType::Oauth2,
                grant_type: Some(grant),
                access_token_url: opt("accessTokenUrl").or_else(|| opt("access_token_url")),
                authorization_url: opt("authUrl").or_else(|| opt("auth_url")),
                client_id: opt("clientId").or_else(|| opt("clientid")),
                client_secret: opt("clientSecret").or_else(|| opt("client_secret")),
                scope: opt("scope"),
                callback_url: opt("redirect_uri").or_else(|| opt("callbackUrl")),
                username: opt("username"),
                password: opt("password"),
                ..Auth::default()
            })
        }
        other => {
            warnings.push(format!(
                "`{name}`: auth type `{other}` not supported by the importer — auth dropped."
            ));
            None
        }
    }
}

/// Maps Postman `event[]` (prerequest/test) to Keel scripts. `pm.*` is
/// executed by the script runtime, so the source is kept as written.
fn map_events(events: Option<&Value>, name: &str, warnings: &mut Vec<String>) -> Option<Scripts> {
    let arr = events?.as_array()?;
    let mut pre: Vec<String> = Vec::new();
    let mut post: Vec<String> = Vec::new();
    for event in arr {
        let listen = event.get("listen").and_then(|l| l.as_str()).unwrap_or("");
        let exec = event
            .get("script")
            .and_then(|s| s.get("exec"))
            .and_then(|e| e.as_array())
            .map(|lines| {
                lines
                    .iter()
                    .filter_map(|l| l.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if exec.trim().is_empty() {
            continue;
        }
        match listen {
            "prerequest" => pre.push(exec),
            "test" => post.push(exec),
            other => {
                warnings.push(format!(
                    "`{name}`: unsupported event listener `{other}` — script dropped."
                ));
            }
        }
    }
    let scripts = Scripts {
        pre_request: if pre.is_empty() {
            None
        } else {
            Some(pre.join("\n\n"))
        },
        post_response: if post.is_empty() {
            None
        } else {
            Some(post.join("\n\n"))
        },
    };
    if scripts.pre_request.is_none() && scripts.post_response.is_none() {
        return None;
    }
    Some(scripts)
}

fn description_of(value: &Value) -> Option<String> {
    match value.get("description")? {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Object(o) => o
            .get("content")
            .and_then(|c| c.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(String::from),
        _ => None,
    }
}

fn upsert_collection_env(
    root: &Path,
    env_slug: &str,
    display_name: &str,
    vars: &BTreeMap<String, String>,
) -> Result<(), String> {
    let dir = root.join(ENV_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(format!("{env_slug}.yaml"));
    let mut doc: EnvDoc = std::fs::read_to_string(&target)
        .ok()
        .and_then(|t| yaml_to::<EnvDoc>(&t).ok())
        .unwrap_or(EnvDoc {
            schema_version: SCHEMA_VERSION.into(),
            name: display_name.to_string(),
            description: Some(format!("Imported from Postman collection `{display_name}`")),
            variables: None,
            secrets: None,
        });
    let existing = doc.variables.get_or_insert_with(BTreeMap::new);
    for (k, v) in vars {
        existing.insert(k.clone(), v.clone());
    }
    std::fs::write(&target, yaml_of(&doc)?).map_err(|e| e.to_string())
}

fn unique_dir(dir: &Path, stem: &str) -> PathBuf {
    let mut candidate = dir.join(stem);
    let mut i = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{i}"));
        i += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLLECTION: &str = r#"{
  "info": {
    "name": "Demo API",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "variable": [ { "key": "baseUrl", "value": "https://demo.test" } ],
  "item": [
    {
      "name": "Users",
      "event": [
        { "listen": "prerequest", "script": { "exec": ["console.log('hi');"] } }
      ],
      "item": [
        {
          "name": "Admin",
          "auth": {
            "type": "basic",
            "basic": [ { "key": "username", "value": "ada" }, { "key": "password", "value": "{{pw}}" } ]
          },
          "item": [
            {
              "name": "Get Order",
              "request": {
                "method": "GET",
                "url": {
                  "raw": "https://{{baseUrl}}/orders/:id?expand=profile",
                  "protocol": "https",
                  "host": ["{{baseUrl}}"],
                  "path": ["orders", ":id"],
                  "query": [ { "key": "expand", "value": "profile" }, { "key": "off", "value": "1", "disabled": true } ],
                  "variable": [ { "key": "id", "value": "42" } ]
                },
                "header": [
                  { "key": "Accept", "value": "application/json" },
                  "X-Trace: abc"
                ]
              },
              "response": [ { "name": "ok", "code": 200 }, { "name": "err", "code": 500 } ]
            }
          ]
        },
        {
          "name": "Create User",
          "event": [
            { "listen": "test", "script": { "exec": ["pm.test('ok', function () {", "  pm.expect(pm.response.code).to.eql(200);", "});"] } }
          ],
          "request": {
            "method": "POST",
            "url": {
              "raw": "https://api.test/users",
              "protocol": "https",
              "host": ["api", "test"],
              "path": ["users"]
            },
            "header": [ { "key": "Content-Type", "value": "application/json", "disabled": false } ],
            "body": {
              "mode": "raw",
              "raw": "{\"name\": \"Ada\", \"note\": \"{{noteVar}}\"}",
              "options": { "raw": { "language": "json" } }
            },
            "auth": {
              "type": "bearer",
              "bearer": [ { "key": "token", "value": "{{accessToken}}" } ]
            }
          }
        }
      ]
    },
    {
      "name": "Upload File",
      "request": {
        "method": "POST",
        "url": "https://api.test/upload?tag=x",
        "body": {
          "mode": "formdata",
          "formdata": [
            { "key": "note", "value": "hello", "type": "text" },
            { "key": "file", "src": "@/tmp/doc.pdf", "type": "file" }
          ]
        },
        "auth": { "type": "apikey", "apikey": [ { "key": "key", "value": "X-Api-Key" }, { "key": "value", "value": "{{apiKey}}" }, { "key": "in", "value": "query" } ] }
      }
    },
    {
      "name": "Post Form",
      "request": {
        "method": "PUT",
        "url": "https://api.test/form",
        "body": { "mode": "urlencoded", "urlencoded": [ { "key": "a", "value": "1" } ] }
      }
    },
    {
      "name": "GraphQL Query",
      "request": {
        "method": "POST",
        "url": "https://api.test/graphql",
        "body": { "mode": "graphql", "graphql": { "query": "{ me { name } }", "variables": "{\"id\": 1}" } }
      }
    },
    {
      "name": "Send Binary",
      "request": {
        "method": "POST",
        "url": "https://api.test/bin",
        "body": { "mode": "file", "file": { "src": "./fixtures/blob.bin" } }
      }
    },
    {
      "name": "Digest Req",
      "request": {
        "method": "GET",
        "url": "https://api.test/digest",
        "auth": { "type": "digest", "digest": [ { "key": "username", "value": "u" }, { "key": "password", "value": "p" } ] }
      }
    },
    {
      "name": "OAuth Req",
      "request": {
        "method": "GET",
        "url": "https://api.test/oauth",
        "auth": { "type": "oauth2", "oauth2": [ { "key": "grant_type", "value": "Client Credentials" }, { "key": "accessTokenUrl", "value": "https://x/token" }, { "key": "authUrl", "value": "https://x/authorize" }, { "key": "clientId", "value": "cid" }, { "key": "clientSecret", "value": "csecret" }, { "key": "scope", "value": "read write" } ] }
      }
    },
    {
      "name": "Weird",
      "request": { "method": "TRACE", "url": "https://api.test/trace" }
    }
  ]
}"#;

    fn setup() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("dir");
        workspace::init_workspace(&dir.path().join("ws"), "ws").expect("init");
        dir
    }

    fn import_fixture(root: &Path) -> OpenApiResultDto {
        let src = root.join("collection.json");
        std::fs::write(&src, COLLECTION).expect("write");
        import_postman(&src, root, "demo").expect("import")
    }

    #[test]
    fn creates_nested_folders_and_files() {
        let dir = setup();
        let root = dir.path().join("ws");
        let result = import_fixture(&root);

        assert!(result.files.contains(&"demo/upload-file.yaml".to_string()), "{:?}", result.files);
        assert!(result
            .files
            .contains(&"demo/users/admin/get-order.yaml".to_string()));
        assert!(result
            .files
            .contains(&"demo/users/create-user.yaml".to_string()));
        assert_eq!(result.skipped, 0);
        assert!(result
            .files
            .contains(&"demo/weird.yaml".to_string()), "{:?}", result.files);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("2 saved response example")));
    }

    #[test]
    fn collection_variables_become_environment() {
        let dir = setup();
        let root = dir.path().join("ws");
        import_fixture(&root);
        let env = std::fs::read_to_string(root.join("environments").join("demo-api.yaml"))
            .expect("env file");
        assert!(env.contains("https://demo.test"), "{env}");
    }

    #[test]
    fn url_object_yields_params_and_path_params() {
        let dir = setup();
        let root = dir.path().join("ws");
        import_fixture(&root);
        let text =
            std::fs::read_to_string(root.join("demo/users/admin/get-order.yaml")).expect("read");
        let doc: RequestDoc = yaml_to(&text).expect("parse");
        assert_eq!(doc.request.url, "https://{{baseUrl}}/orders/:id");
        assert!(doc.request.url.contains("{{baseUrl}}"), "template preserved");
        let params = doc.request.params.expect("query params");
        assert_eq!(params[0].name, "expand");
        assert!(!params.iter().any(|p| p.name == "off" && p.enabled), "disabled row kept false");
        assert!(params.iter().any(|p| p.name == "off" && !p.enabled));
        let path_params = doc.request.path_params.expect("path params");
        assert_eq!(path_params[0].name, "id");
        assert_eq!(path_params[0].value, "42");
        let headers = doc.request.headers.expect("headers");
        assert_eq!(headers.len(), 2);
        assert!(headers.iter().any(|h| h.name == "X-Trace" && h.value == "abc"));
    }

    #[test]
    fn raw_url_splits_query_into_params() {
        let dir = setup();
        let root = dir.path().join("ws");
        import_fixture(&root);
        let text = std::fs::read_to_string(root.join("demo/upload-file.yaml")).expect("read");
        let doc: RequestDoc = yaml_to(&text).expect("parse");
        assert_eq!(doc.request.url, "https://api.test/upload");
        assert_eq!(doc.request.params.expect("params")[0].name, "tag");
    }

    #[test]
    fn body_modes_map_to_keel_types() {
        let dir = setup();
        let root = dir.path().join("ws");
        import_fixture(&root);

        let json: RequestDoc =
            yaml_to(&std::fs::read_to_string(root.join("demo/users/create-user.yaml")).unwrap())
                .unwrap();
        let body = json.request.body.expect("json body");
        assert_eq!(body.body_type, BodyType::Json);
        assert!(body.content.unwrap().contains("{{noteVar}}"), "template pass-through");

        let upload: RequestDoc =
            yaml_to(&std::fs::read_to_string(root.join("demo/upload-file.yaml")).unwrap()).unwrap();
        let body = upload.request.body.expect("multipart");
        assert_eq!(body.body_type, BodyType::Multipart);
        let file_row = body
            .items
            .as_ref()
            .unwrap()
            .iter()
            .find(|kv| kv.name == "file")
            .expect("file row");
        assert_eq!(file_row.kind.as_deref(), Some("file"));
        assert_eq!(file_row.value, "/tmp/doc.pdf", "@ prefix stripped");

        let form: RequestDoc =
            yaml_to(&std::fs::read_to_string(root.join("demo/post-form.yaml")).unwrap()).unwrap();
        let body = form.request.body.expect("form body");
        assert_eq!(body.body_type, BodyType::FormUrlencoded);
        assert_eq!(body.items.expect("items")[0].name, "a");

        let gql: RequestDoc =
            yaml_to(&std::fs::read_to_string(root.join("demo/graphql-query.yaml")).unwrap())
                .unwrap();
        let body = gql.request.body.expect("gql body");
        assert_eq!(body.body_type, BodyType::Graphql);
        assert_eq!(body.query.as_deref(), Some("{ me { name } }"));
        assert_eq!(body.variables.as_deref(), Some("{\"id\": 1}"));

        let bin: RequestDoc =
            yaml_to(&std::fs::read_to_string(root.join("demo/send-binary.yaml")).unwrap()).unwrap();
        let body = bin.request.body.expect("binary body");
        assert_eq!(body.body_type, BodyType::Binary);
        assert_eq!(body.path.as_deref(), Some("./fixtures/blob.bin"));
    }

    #[test]
    fn auth_mapping_and_unsupported_warning() {
        let dir = setup();
        let root = dir.path().join("ws");
        import_fixture(&root);

        let bearer: RequestDoc =
            yaml_to(&std::fs::read_to_string(root.join("demo/users/create-user.yaml")).unwrap())
                .unwrap();
        let auth = bearer.auth.expect("bearer auth");
        assert_eq!(auth.auth_type, AuthType::Bearer);
        assert_eq!(auth.token.as_deref(), Some("{{accessToken}}"));

        let apikey: RequestDoc =
            yaml_to(&std::fs::read_to_string(root.join("demo/upload-file.yaml")).unwrap()).unwrap();
        let auth = apikey.auth.expect("apikey auth");
        assert_eq!(auth.auth_type, AuthType::Apikey);
        assert_eq!(auth.key.as_deref(), Some("X-Api-Key"));
        assert_eq!(auth.location.as_deref(), Some("query"));

        let digest: RequestDoc =
            yaml_to(&std::fs::read_to_string(root.join("demo/digest-req.yaml")).unwrap()).unwrap();
        assert_eq!(digest.auth.expect("digest").auth_type, AuthType::Digest);

        let oauth: RequestDoc =
            yaml_to(&std::fs::read_to_string(root.join("demo/oauth-req.yaml")).unwrap()).unwrap();
        let auth = oauth.auth.expect("oauth2 auth mapped");
        assert_eq!(auth.auth_type, AuthType::Oauth2);
        assert_eq!(auth.grant_type, Some(OAuth2GrantType::ClientCredentials));
        assert_eq!(auth.access_token_url.as_deref(), Some("https://x/token"));
        assert_eq!(auth.authorization_url.as_deref(), Some("https://x/authorize"));
        assert_eq!(auth.client_id.as_deref(), Some("cid"));
        assert_eq!(auth.client_secret.as_deref(), Some("csecret"));
        assert_eq!(auth.scope.as_deref(), Some("read write"));
    }

    #[test]
    fn folder_auth_and_scripts_write_folder_yaml() {
        let dir = setup();
        let root = dir.path().join("ws");
        import_fixture(&root);
        let admin = workspace::read_folder(&root.join("demo/users/admin")).expect("folder.yaml");
        assert_eq!(admin.auth.expect("basic").auth_type, AuthType::Basic);
        let users = workspace::read_folder(&root.join("demo/users")).expect("users folder");
        assert!(users.scripts.expect("scripts").pre_request.expect("pre").contains("console.log"));
    }

    #[test]
    fn pm_scripts_imported_as_written() {
        let dir = setup();
        let root = dir.path().join("ws");
        let result = import_fixture(&root);
        let create: RequestDoc =
            yaml_to(&std::fs::read_to_string(root.join("demo/users/create-user.yaml")).unwrap())
                .unwrap();
        let script = create
            .scripts
            .expect("scripts")
            .post_response
            .expect("post");
        assert!(script.contains("pm.test"), "kept as written");
        assert!(
            !result.warnings.iter().any(|w| w.contains("pm.")),
            "{:?}",
            result.warnings
        );
    }

    #[test]
    fn rerun_does_not_collide_and_merges_env() {
        let dir = setup();
        let root = dir.path().join("ws");
        let first = import_fixture(&root);
        let second = import_fixture(&root);
        assert_eq!(first.files.len(), second.files.len());
        assert!(
            second.files.iter().any(|f| f.contains("-2.yaml")),
            "unique_file avoids overwrite: {:?}",
            second.files
        );
        // Environment merged, not duplicated.
        let envs = workspace::env_list(&root);
        assert!(envs.iter().any(|e| e.name == "Demo API" && e.variable_count == 1));
    }

    #[test]
    fn postman_environment_import() {
        let dir = setup();
        let root = dir.path().join("ws");
        let source = dir.path().join("qa.json");
        std::fs::write(
            &source,
            r#"{
              "name": "QA Env",
              "values": [
                { "key": "baseUrl", "value": "http://localhost:3000", "enabled": true },
                { "key": "apiKey", "value": "hunter2", "type": "secret", "enabled": true },
                { "key": "beta", "value": "off", "enabled": false }
              ]
            }"#,
        )
        .unwrap();
        let result = import_postman(&source, &root, "").expect("import");
        assert!(result.files.contains(&"environments/qa-env.yaml".to_string()));
        let env = std::fs::read_to_string(root.join("environments").join("qa-env.yaml"))
            .expect("env file");
        let doc: EnvDoc = yaml_to(&env).expect("parse");
        assert_eq!(doc.name, "QA Env");
        let vars = doc.variables.expect("variables");
        // Secrets live in the keychain, never in the YAML.
        assert_eq!(vars.get("baseUrl").map(String::as_str), Some("http://localhost:3000"));
        assert_eq!(vars.get("beta").map(String::as_str), Some("off"));
        assert!(!vars.contains_key("apiKey"));
        assert!(doc.secrets.expect("secrets").contains_key("apiKey"));
        assert!(result.warnings.iter().any(|w| w.contains("disabled")));
    }

    #[test]
    fn environment_shape_detection() {
        assert!(looks_like_postman_environment(
            &serde_json::json!({ "name": "Local", "values": [] })
        ));
        assert!(!looks_like_postman_environment(
            &serde_json::json!({ "info": { "name": "Col" }, "item": [] })
        ));
        assert!(!looks_like_postman_environment(&serde_json::json!({ "foo": 1 })));
    }
}
