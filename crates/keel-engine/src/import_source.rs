//! Detects an import payload (file text or a fetched URL) and routes it to
//! the matching importer. Keel requests stay YAML; foreign formats are
//! converted, never stored as-is.
//!
//! Recognized:
//! - Keel request YAML
//! - OpenCollection
//! - Postman collection / environment
//! - Insomnia v4 / v5
//! - OpenAPI 3.x / Swagger 2.0
//!
//! WSDL is recognized and rejected with a warning — SOAP requests are not
//! part of the Keel format yet.

use std::io::{Cursor, Read};
use std::path::Path;

use serde_json::Value;

use crate::model::{
    yaml_of, yaml_to, Body, BodyType, HttpMethod, OpenApiResultDto, RequestBlock, RequestDoc,
    SCHEMA_VERSION,
};
use crate::workspace::{self, slugify};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Keel,
    OpenCollection,
    Postman,
    Insomnia,
    OpenApi,
    Wsdl,
    Unknown,
}

/// Classifies already-parsed JSON/YAML. WSDL (plain XML text) is handled
/// separately by [`detect_text`].
pub fn detect_value(doc: &Value) -> SourceKind {
    if is_openapi(doc) {
        return SourceKind::OpenApi;
    }
    if is_postman(doc) || crate::import_postman::looks_like_postman_environment(doc) {
        return SourceKind::Postman;
    }
    if is_insomnia(doc) {
        return SourceKind::Insomnia;
    }
    if is_opencollection(doc) {
        return SourceKind::OpenCollection;
    }
    if is_keel_request(doc) {
        return SourceKind::Keel;
    }
    SourceKind::Unknown
}

pub fn detect_text(text: &str) -> SourceKind {
    let trimmed = text.trim_start();
    if trimmed.starts_with('<') && looks_like_wsdl(trimmed) {
        return SourceKind::Wsdl;
    }
    match yaml_to::<Value>(text) {
        Ok(doc) => detect_value(&doc),
        Err(_) => SourceKind::Unknown,
    }
}

fn is_openapi(doc: &Value) -> bool {
    let info_ok = doc.get("info").map(|i| i.is_object()).unwrap_or(false);
    if !info_ok {
        return false;
    }
    nonempty_str(doc, "openapi") || nonempty_str(doc, "swagger")
}

fn is_postman(doc: &Value) -> bool {
    let info = doc.get("info").or_else(|| doc.get("collection").and_then(|c| c.get("info")));
    let schema = info.and_then(|i| i.get("schema")).and_then(|s| s.as_str()).unwrap_or("");
    schema.contains("schema.getpostman.com/json/collection/")
}

fn is_insomnia(doc: &Value) -> bool {
    if doc
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t.starts_with("collection.insomnia.rest/5"))
        .unwrap_or(false)
    {
        return doc.get("collection").map(|c| c.is_array()).unwrap_or(false);
    }
    doc.get("_type").and_then(|t| t.as_str()) == Some("export")
        && doc.get("__export_format").and_then(|n| n.as_u64()).is_some()
        && doc.get("resources").map(|r| r.is_array()).unwrap_or(false)
}

fn is_opencollection(doc: &Value) -> bool {
    nonempty_str(doc, "opencollection") && doc.get("info").map(|i| i.is_object()).unwrap_or(false)
}

fn is_keel_request(doc: &Value) -> bool {
    doc.get("kind").and_then(|k| k.as_str()) == Some("request")
        && nonempty_str(doc, "name")
        && doc.get("request").map(|r| r.is_object()).unwrap_or(false)
}

fn looks_like_wsdl(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("wsdl:definitions")
        || lower.contains("xmlns:wsdl")
        || lower.contains("http://schemas.xmlsoap.org/wsdl/")
        || (lower.contains("<definitions") && lower.contains("wsdl"))
}

fn nonempty_str(doc: &Value, key: &str) -> bool {
    doc.get(key)
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// Writes `text` into the open workspace under `folder`. OpenAPI and Postman
/// reuse their file importers via a temp file so detection and import stay
/// one path.
/// Imports every JSON/YAML/XML file in a ZIP. A Keel `collection.yaml` is
/// skipped so it doesn't overwrite the open workspace.
pub fn import_zip(bytes: &[u8], root: &Path, folder: &str) -> Result<OpenApiResultDto, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("read zip: {e}"))?;
    if archive.len() == 0 {
        return Err("The ZIP is empty.".into());
    }
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let mut skipped = 0usize;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|e| format!("read zip entry: {e}"))?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        if name.contains("..") || name.starts_with('/') || name.starts_with('\\') {
            skipped += 1;
            warnings.push(format!("Skipped `{name}` — unsafe path."));
            continue;
        }
        let lower = name.to_ascii_lowercase();
        let importable = lower.ends_with(".json")
            || lower.ends_with(".yaml")
            || lower.ends_with(".yml")
            || lower.ends_with(".wsdl")
            || lower.ends_with(".xml");
        if !importable || lower.ends_with("collection.yaml") || lower.ends_with("folder.yaml") {
            continue;
        }
        if file.size() > 20 * 1024 * 1024 {
            skipped += 1;
            warnings.push(format!("Skipped `{name}` — larger than 20 MB."));
            continue;
        }
        let mut text = String::new();
        if file.read_to_string(&mut text).is_err() {
            skipped += 1;
            warnings.push(format!("Skipped `{name}` — not a text file."));
            continue;
        }
        match import_text(&text, root, folder) {
            Ok(res) => {
                files.extend(res.files);
                skipped += res.skipped;
                warnings.extend(res.warnings);
            }
            Err(err) => {
                skipped += 1;
                warnings.push(format!("Skipped `{name}`: {err}"));
            }
        }
    }
    if files.is_empty() {
        return Err(if warnings.is_empty() {
            "No importable files in the ZIP.".into()
        } else {
            warnings.join("; ")
        });
    }
    Ok(OpenApiResultDto {
        files,
        skipped,
        warnings,
    })
}

pub fn import_text(text: &str, root: &Path, folder: &str) -> Result<OpenApiResultDto, String> {
    let kind = detect_text(text);
    match kind {
        SourceKind::Wsdl => Err(
            "WSDL is recognized but not imported yet — SOAP requests aren't part of the Keel format."
                .into(),
        ),
        SourceKind::Unknown => Err(
            "Unrecognized format. Use OpenCollection, Postman, Insomnia, OpenAPI 3 / Swagger 2, or a Keel request."
                .into(),
        ),
        SourceKind::OpenApi => import_via_temp(text, "spec.yaml", root, folder, |p, r, f| {
            crate::import_openapi::import_openapi(p, r, f).map(|res| OpenApiResultDto {
                files: res.files,
                skipped: res.skipped,
                warnings: res.warnings,
            })
        }),
        SourceKind::Postman => import_via_temp(text, "postman.json", root, folder, |p, r, f| {
            crate::import_postman::import_postman(p, r, f)
        }),
        SourceKind::Keel => import_keel(text, root, folder),
        SourceKind::OpenCollection => import_opencollection(text, root, folder),
        SourceKind::Insomnia => import_insomnia(text, root, folder),
    }
}

fn import_via_temp(
    text: &str,
    file_name: &str,
    root: &Path,
    folder: &str,
    import: impl FnOnce(&Path, &Path, &str) -> Result<OpenApiResultDto, String>,
) -> Result<OpenApiResultDto, String> {
    let dir = std::env::temp_dir().join(format!("keel-import-{}", uuid_ish()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(file_name);
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let result = import(&path, root, folder);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn uuid_ish() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos}-{}", std::process::id())
}

fn import_keel(text: &str, root: &Path, folder: &str) -> Result<OpenApiResultDto, String> {
    let doc: RequestDoc = yaml_to(text).map_err(|e| format!("parse Keel request: {e}"))?;
    let dir = workspace::safe_join(root, folder)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = workspace::unique_file(&dir, &slugify(&doc.name));
    std::fs::write(&target, yaml_of(&doc)?).map_err(|e| e.to_string())?;
    Ok(OpenApiResultDto {
        files: vec![workspace::rel_path_public(root, &target)],
        skipped: 0,
        warnings: vec![],
    })
}

fn text_body(body_type: BodyType, content: Option<&str>) -> Body {
    Body {
        body_type,
        content: Some(content.unwrap_or("").to_string()),
        ..Body::default()
    }
}

fn kv_body(body_type: BodyType, rows: Option<&Value>) -> Option<Body> {
    let items = kv_rows(rows);
    if items.is_empty() {
        None
    } else {
        Some(Body {
            body_type,
            items: Some(items),
            ..Body::default()
        })
    }
}

fn kv_rows(value: Option<&Value>) -> Vec<crate::model::KV> {
    value
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let name = row.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    if name.is_empty() {
                        return None;
                    }
                    if row.get("enabled").and_then(|e| e.as_bool()) == Some(false) {
                        return None;
                    }
                    Some(crate::model::KV {
                        name: name.to_string(),
                        value: row.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        enabled: true,
                        kind: row
                            .get("type")
                            .and_then(|t| t.as_str())
                            .filter(|t| *t == "file")
                            .map(|t| t.to_string()),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn import_opencollection(text: &str, root: &Path, folder: &str) -> Result<OpenApiResultDto, String> {
    let doc: Value = yaml_to(text).map_err(|e| format!("parse OpenCollection: {e}"))?;
    let name = doc
        .get("info")
        .and_then(|i| i.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("OpenCollection");
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let mut skipped = 0usize;
    let dest = collection_dir(root, folder, name)?;
    let items = doc.get("items").or_else(|| doc.get("item"));
    walk_opencollection(
        &dest,
        root,
        items.and_then(|i| i.as_array()).map(|a| a.as_slice()).unwrap_or(&[]),
        &mut files,
        &mut warnings,
        &mut skipped,
    )?;
    Ok(OpenApiResultDto {
        files,
        skipped,
        warnings,
    })
}

fn walk_opencollection(
    dir: &Path,
    root: &Path,
    items: &[Value],
    files: &mut Vec<String>,
    warnings: &mut Vec<String>,
    skipped: &mut usize,
) -> Result<(), String> {
    for item in items {
        let name = item
            .get("name")
            .or_else(|| item.get("info").and_then(|i| i.get("name")))
            .and_then(|n| n.as_str())
            .unwrap_or("Item");
        let nested = item.get("items").or_else(|| item.get("item"));
        if nested.and_then(|n| n.as_array()).is_some() && item.get("request").is_none() {
            let child = workspace::safe_join(dir, &slugify(name))?;
            std::fs::create_dir_all(&child).map_err(|e| e.to_string())?;
            walk_opencollection(
                &child,
                root,
                nested.and_then(|n| n.as_array()).map(|a| a.as_slice()).unwrap_or(&[]),
                files,
                warnings,
                skipped,
            )?;
            continue;
        }
        let request = item.get("request").unwrap_or(item);
        match simple_request(name, request) {
            Some(doc) => write_doc(dir, root, &doc, files)?,
            None => {
                *skipped += 1;
                warnings.push(format!("Skipped `{name}` — missing URL or unsupported method."));
            }
        }
    }
    Ok(())
}

fn import_insomnia(text: &str, root: &Path, folder: &str) -> Result<OpenApiResultDto, String> {
    let doc: Value = yaml_to(text).map_err(|e| format!("parse Insomnia export: {e}"))?;
    if doc
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t.starts_with("collection.insomnia.rest/5"))
        .unwrap_or(false)
    {
        import_insomnia_v5(&doc, root, folder)
    } else {
        import_insomnia_v4(&doc, root, folder)
    }
}

fn import_insomnia_v5(doc: &Value, root: &Path, folder: &str) -> Result<OpenApiResultDto, String> {
    let name = doc.get("name").and_then(|n| n.as_str()).unwrap_or("Insomnia");
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let mut skipped = 0usize;
    let dest = collection_dir(root, folder, name)?;
    let items = doc
        .get("collection")
        .and_then(|c| c.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    walk_insomnia_v5(&dest, root, items, &mut files, &mut warnings, &mut skipped)?;
    Ok(OpenApiResultDto {
        files,
        skipped,
        warnings,
    })
}

fn walk_insomnia_v5(
    dir: &Path,
    root: &Path,
    items: &[Value],
    files: &mut Vec<String>,
    warnings: &mut Vec<String>,
    skipped: &mut usize,
) -> Result<(), String> {
    for item in items {
        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("Item");
        if let Some(children) = item.get("children").and_then(|c| c.as_array()) {
            let child = workspace::safe_join(dir, &slugify(name))?;
            std::fs::create_dir_all(&child).map_err(|e| e.to_string())?;
            walk_insomnia_v5(&child, root, children, files, warnings, skipped)?;
            continue;
        }
        match insomnia_request(name, item) {
            Some(doc) => write_doc(dir, root, &doc, files)?,
            None => {
                *skipped += 1;
                warnings.push(format!("Skipped `{name}` — not an HTTP request."));
            }
        }
    }
    Ok(())
}

fn import_insomnia_v4(doc: &Value, root: &Path, folder: &str) -> Result<OpenApiResultDto, String> {
    let resources = doc
        .get("resources")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();
    let workspace = resources.iter().find(|r| r.get("_type").and_then(|t| t.as_str()) == Some("workspace"));
    let name = workspace
        .and_then(|w| w.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("Insomnia");
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let mut skipped = 0usize;
    let dest = collection_dir(root, folder, name)?;

    fn parent_path(resources: &[Value], id: &str, dest: &Path) -> std::path::PathBuf {
        let mut parts: Vec<String> = Vec::new();
        let mut current = resources.iter().find(|r| r.get("_id").and_then(|i| i.as_str()) == Some(id));
        let mut guard = 0;
        while let Some(node) = current {
            guard += 1;
            if guard > 32 {
                break;
            }
            let kind = node.get("_type").and_then(|t| t.as_str()).unwrap_or("");
            if kind == "request_group" {
                if let Some(name) = node.get("name").and_then(|n| n.as_str()) {
                    parts.push(slugify(name));
                }
            }
            if kind == "workspace" {
                break;
            }
            let parent = node.get("parentId").and_then(|p| p.as_str()).unwrap_or("");
            current = resources.iter().find(|r| r.get("_id").and_then(|i| i.as_str()) == Some(parent));
        }
        parts.iter().rev().fold(dest.to_path_buf(), |acc, part| acc.join(part))
    }

    for resource in &resources {
        if resource.get("_type").and_then(|t| t.as_str()) != Some("request") {
            continue;
        }
        let name = resource.get("name").and_then(|n| n.as_str()).unwrap_or("Request");
        let parent = resource.get("parentId").and_then(|p| p.as_str()).unwrap_or("");
        let dir = parent_path(&resources, parent, &dest);
        if let Err(err) = std::fs::create_dir_all(&dir) {
            warnings.push(format!("Skipped `{name}`: {err}"));
            skipped += 1;
            continue;
        }
        match insomnia_request(name, resource) {
            Some(doc) => write_doc(&dir, root, &doc, &mut files)?,
            None => {
                skipped += 1;
                warnings.push(format!("Skipped `{name}` — unsupported method."));
            }
        }
    }
    Ok(OpenApiResultDto {
        files,
        skipped,
        warnings,
    })
}

fn insomnia_request(name: &str, item: &Value) -> Option<RequestDoc> {
    let method = method_of(item.get("method").and_then(|m| m.as_str()).unwrap_or("GET"))?;
    let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
    if url.is_empty() && item.get("body").is_none() {
        return None;
    }
    let headers = kv_rows(item.get("headers"));
    let params = kv_rows(item.get("parameters"));
    let body = insomnia_body(item.get("body"));
    Some(RequestDoc {
        schema_version: SCHEMA_VERSION.into(),
        name: name.to_string(),
        kind: "request".into(),
        description: item
            .get("description")
            .and_then(|d| d.as_str())
            .or_else(|| {
                item.get("meta")
                    .and_then(|m| m.get("description"))
                    .and_then(|d| d.as_str())
            })
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        protocol: None,
        graphql: None,
        websocket: None,
        grpc: None,
        request: RequestBlock {
            method,
            url,
            params: if params.is_empty() { None } else { Some(params) },
            headers: if headers.is_empty() { None } else { Some(headers) },
            path_params: None,
            body,
        },
        auth: None,
        variables: None,
        scripts: None,
        tests: None,
    })
}

fn insomnia_body(body: Option<&Value>) -> Option<Body> {
    let body = body?;
    if body.is_null() || (body.is_object() && body.as_object().map(|o| o.is_empty()).unwrap_or(false)) {
        return None;
    }
    let mime = body.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");
    let text = body.get("text").and_then(|t| t.as_str()).unwrap_or("");
    if mime.contains("json") {
        return Some(text_body(BodyType::Json, Some(text)));
    }
    if mime.contains("xml") {
        return Some(text_body(BodyType::Xml, Some(text)));
    }
    if mime.contains("x-www-form-urlencoded") {
        return kv_body(BodyType::FormUrlencoded, body.get("params"));
    }
    if mime.contains("form-data") || mime.contains("multipart") {
        return kv_body(BodyType::Multipart, body.get("params"));
    }
    if mime.contains("graphql") {
        return Some(Body {
            body_type: BodyType::Graphql,
            query: Some(text.to_string()),
            ..Body::default()
        });
    }
    if text.is_empty() {
        None
    } else {
        Some(text_body(BodyType::Text, Some(text)))
    }
}

fn simple_request(name: &str, request: &Value) -> Option<RequestDoc> {
    let method = method_of(
        request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("GET"),
    )?;
    let url = request
        .get("url")
        .and_then(|u| u.as_str())
        .or_else(|| request.get("http").and_then(|h| h.get("url")).and_then(|u| u.as_str()))
        .unwrap_or("")
        .to_string();
    if url.is_empty() {
        return None;
    }
    Some(RequestDoc {
        schema_version: SCHEMA_VERSION.into(),
        name: name.to_string(),
        kind: "request".into(),
        description: None,
        protocol: None,
        graphql: None,
        websocket: None,
        grpc: None,
        request: RequestBlock {
            method,
            url,
            params: None,
            headers: None,
            path_params: None,
            body: None,
        },
        auth: None,
        variables: None,
        scripts: None,
        tests: None,
    })
}

fn method_of(raw: &str) -> Option<HttpMethod> {
    match raw.to_ascii_uppercase().as_str() {
        "GET" => Some(HttpMethod::GET),
        "POST" => Some(HttpMethod::POST),
        "PUT" => Some(HttpMethod::PUT),
        "PATCH" => Some(HttpMethod::PATCH),
        "DELETE" => Some(HttpMethod::DELETE),
        "HEAD" => Some(HttpMethod::HEAD),
        "OPTIONS" => Some(HttpMethod::OPTIONS),
        "TRACE" => Some(HttpMethod::TRACE),
        _ => None,
    }
}

fn collection_dir(root: &Path, folder: &str, name: &str) -> Result<std::path::PathBuf, String> {
    let parent = workspace::safe_join(root, folder)?;
    std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let dest = unique_dir(&parent, &slugify(name));
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

fn unique_dir(dir: &Path, stem: &str) -> std::path::PathBuf {
    let mut candidate = dir.join(stem);
    let mut i = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{i}"));
        i += 1;
    }
    candidate
}

fn write_doc(dir: &Path, root: &Path, doc: &RequestDoc, files: &mut Vec<String>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let target = workspace::unique_file(dir, &slugify(&doc.name));
    std::fs::write(&target, yaml_of(doc)?).map_err(|e| e.to_string())?;
    files.push(workspace::rel_path_public(root, &target));
    Ok(())
}

/// True for `git@host:owner/repo` and `https://host/owner/repo` (optional `.git`).
/// A bare host (`https://example.com/spec.yaml`) is not a repository URL.
pub fn is_git_repository_url(raw: &str) -> bool {
    let url = raw.trim();
    if url.is_empty() || url.contains(' ') {
        return false;
    }
    if let Some(rest) = url.strip_prefix("git@") {
        let Some((host, path)) = rest.split_once(':') else {
            return false;
        };
        return host.contains('.') && path.split('/').filter(|s| !s.is_empty()).count() >= 2;
    }
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("ssh://"))
        .or_else(|| url.strip_prefix("git://"))
    else {
        return false;
    };
    let path = rest.split_once('/').map(|(_, p)| p).unwrap_or("");
    let segments: Vec<&str> = path
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .split('/')
        .filter(|s| !s.is_empty() && !s.contains('?'))
        .collect();
    if segments.len() < 2 {
        return false;
    }
    let last = segments.last().copied().unwrap_or("");
    if last.contains('.') && !url.ends_with(".git") {
        return false;
    }
    true
}

pub fn repo_name_from_url(raw: &str) -> String {
    let url = raw.trim().trim_end_matches('/').trim_end_matches(".git");
    let name = url.rsplit(['/', ':']).next().unwrap_or("repository");
    let slug = slugify(name);
    if slug == "request" {
        "repository".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_each_format() {
        assert_eq!(
            detect_text(r#"{"openapi":"3.0.0","info":{"title":"P"},"paths":{}}"#),
            SourceKind::OpenApi
        );
        assert_eq!(
            detect_text(r#"{"swagger":"2.0","info":{"title":"P"},"paths":{}}"#),
            SourceKind::OpenApi
        );
        assert_eq!(
            detect_text(
                r#"{"info":{"name":"C","schema":"https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},"item":[]}"#
            ),
            SourceKind::Postman
        );
        assert_eq!(
            detect_text(r#"{"name":"dev","values":[{"key":"base","value":"http://x"}]}"#),
            SourceKind::Postman
        );
        assert_eq!(
            detect_text(r#"{"_type":"export","__export_format":4,"resources":[]}"#),
            SourceKind::Insomnia
        );
        assert_eq!(
            detect_text(r#"{"type":"collection.insomnia.rest/5.0","collection":[]}"#),
            SourceKind::Insomnia
        );
        assert_eq!(
            detect_text(r#"{"opencollection":"1.0.0","info":{"name":"C"}}"#),
            SourceKind::OpenCollection
        );
        assert_eq!(
            detect_text(r#"{"name":"Get","kind":"request","request":{"method":"GET","url":"/"}}"#),
            SourceKind::Keel
        );
        assert_eq!(
            detect_text(r#"<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"></wsdl:definitions>"#),
            SourceKind::Wsdl
        );
        assert_eq!(detect_text("hello"), SourceKind::Unknown);
    }

    #[test]
    fn git_urls() {
        assert!(is_git_repository_url("https://github.com/user/repo"));
        assert!(is_git_repository_url("https://github.com/user/repo.git"));
        assert!(is_git_repository_url("git@github.com:user/repo.git"));
        assert!(!is_git_repository_url("https://petstore.swagger.io/v2/swagger.json"));
        assert!(!is_git_repository_url("https://example.com"));
        assert!(!is_git_repository_url("not a url"));
    }

    fn imported(text: &str) -> (tempfile::TempDir, std::path::PathBuf, OpenApiResultDto) {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().join("ws");
        workspace::init_workspace(&root, "ws").expect("init");
        let result = import_text(text, &root, "").expect("import");
        (dir, root, result)
    }

    #[test]
    fn imports_opencollection_folders() {
        let text = r#"{
          "opencollection": "1.0.0",
          "info": {"name": "Pets"},
          "items": [
            {"name": "Auth", "items": [
              {"name": "Login", "request": {"method": "POST", "url": "{{baseUrl}}/login"}}
            ]},
            {"name": "List", "request": {"method": "GET", "url": "https://example.com/pets"}}
          ]
        }"#;
        let (_dir, root, result) = imported(text);
        assert_eq!(result.files.len(), 2, "{:?}", result.files);
        assert!(result.files.iter().any(|f| f.contains("pets/auth/login.yaml")), "{:?}", result.files);
        assert!(result.files.iter().any(|f| f.contains("pets/list.yaml")), "{:?}", result.files);
        let login = result.files.iter().find(|f| f.contains("login.yaml")).unwrap();
        let saved = std::fs::read_to_string(root.join(login)).expect("read");
        assert!(saved.contains("POST"));
        assert!(saved.contains("{{baseUrl}}/login"));
    }

    #[test]
    fn imports_insomnia_v5_requests() {
        let text = r#"{
          "type": "collection.insomnia.rest/5.0",
          "name": "Shop",
          "collection": [
            {"name": "Catalog", "children": [
              {"name": "Get item", "method": "GET", "url": "https://example.com/items/1", "headers": [{"name": "Accept", "value": "application/json"}]}
            ]}
          ]
        }"#;
        let (_dir, root, result) = imported(text);
        assert_eq!(result.files.len(), 1, "{:?}", result.files);
        assert!(result.files[0].contains("shop/catalog/get-item.yaml"), "{}", result.files[0]);
        let saved = std::fs::read_to_string(root.join(&result.files[0])).expect("read");
        assert!(saved.contains("GET"));
        assert!(saved.contains("https://example.com/items/1"));
        assert!(saved.contains("Accept"));
    }

    #[test]
    fn imports_insomnia_v4_requests() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().join("ws");
        workspace::init_workspace(&root, "ws").expect("init");
        let text = r#"{
          "_type": "export",
          "__export_format": 4,
          "resources": [
            {"_id":"wrk_1","_type":"workspace","name":"Demo","parentId":null},
            {"_id":"fld_1","_type":"request_group","name":"Auth","parentId":"wrk_1"},
            {"_id":"req_1","_type":"request","name":"Login","parentId":"fld_1","method":"POST","url":"{{baseUrl}}/login","body":{"mimeType":"application/json","text":"{}"},"headers":[{"name":"Accept","value":"application/json"}]}
          ]
        }"#;
        let result = import_text(text, &root, "").expect("import");
        assert_eq!(result.files.len(), 1, "{:?}", result.files);
        assert!(result.files[0].contains("demo/auth/login.yaml"), "{}", result.files[0]);
        let saved = std::fs::read_to_string(root.join(&result.files[0])).expect("read");
        assert!(saved.contains("POST"));
        assert!(saved.contains("{{baseUrl}}/login"));
    }
}
