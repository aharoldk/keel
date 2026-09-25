//! Keel request → cURL command. Template variables stay unresolved and
//! secrets are never substituted (export is always safe to share/commit).

use crate::model::*;

/// Renders a shell-safe single-quoted string.
fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub fn request_to_curl(doc: &RequestDoc) -> String {
    let mut parts: Vec<String> = vec!["curl".to_string()];
    parts.push("-X".into());
    parts.push(doc.request.method.as_str().to_string());

    let mut url = doc.request.url.clone();
    let params: Vec<&KV> = doc
        .request
        .params
        .iter()
        .flatten()
        .filter(|kv| kv.enabled && !kv.name.trim().is_empty())
        .collect();
    if !params.is_empty() {
        let query = params
            .iter()
            .map(|kv| {
                format!(
                    "{}={}",
                    encode(kv.name.trim()),
                    encode(kv.value.trim())
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        url.push('?');
        url.push_str(&query);
    }
    parts.push(q(&url));

    let mut content_type_set = false;
    for kv in doc.request.headers.iter().flatten().filter(|kv| kv.enabled) {
        if kv.name.trim().is_empty() {
            continue;
        }
        if kv.name.eq_ignore_ascii_case("content-type") {
            content_type_set = true;
        }
        parts.push("-H".into());
        parts.push(q(&format!("{}: {}", kv.name, kv.value)));
    }

    if let Some(auth) = &doc.auth {
        match auth.auth_type {
            AuthType::Bearer => {
                let token = auth.token.clone().unwrap_or_default();
                parts.push("-H".into());
                parts.push(q(&format!("Authorization: Bearer {token}")));
            }
            AuthType::Basic => {
                let username = auth.username.clone().unwrap_or_default();
                let password = auth.password.clone().unwrap_or_default();
                parts.push("-u".into());
                parts.push(q(&format!("{username}:{password}")));
            }
            AuthType::Apikey => {
                let key = auth.key.clone().unwrap_or_default();
                let value = auth.value.clone().unwrap_or_default();
                match auth.location.as_deref().unwrap_or("header") {
                    "query" => {
                        parts.push(q(&format!(
                            "{url}{}{}={}",
                            if url.contains('?') { "&" } else { "?" },
                            encode(&key),
                            encode(&value)
                        )));
                    }
                    _ => {
                        parts.push("-H".into());
                        parts.push(q(&format!("{key}: {value}")));
                    }
                }
            }
            AuthType::None => {}
            AuthType::Digest | AuthType::Oauth2 => {
                // Placeholder flags keep the export honest about what curl
                // would negotiate client-side.
                if auth.auth_type == AuthType::Digest {
                    parts.push("--digest".into());
                    let user = auth.username.clone().unwrap_or_default();
                    let pass = auth.password.clone().unwrap_or_default();
                    parts.push("-u".into());
                    parts.push(q(&format!("{user}:{pass}")));
                }
            }
        }
    }

    if let Some(body) = &doc.request.body {
        match body.body_type {
            BodyType::None => {}
            BodyType::Json => {
                if !content_type_set {
                    parts.push("-H".into());
                    parts.push(q("Content-Type: application/json"));
                }
                parts.push("--data-raw".into());
                parts.push(q(body.content.as_deref().unwrap_or("")));
            }
            BodyType::Text => {
                if !content_type_set {
                    parts.push("-H".into());
                    parts.push(q("Content-Type: text/plain"));
                }
                parts.push("--data-raw".into());
                parts.push(q(body.content.as_deref().unwrap_or("")));
            }
            BodyType::Xml => {
                if !content_type_set {
                    parts.push("-H".into());
                    parts.push(q("Content-Type: application/xml"));
                }
                parts.push("--data-raw".into());
                parts.push(q(body.content.as_deref().unwrap_or("")));
            }
            BodyType::FormUrlencoded => {
                for kv in body.items.iter().flatten().filter(|kv| kv.enabled) {
                    if kv.name.trim().is_empty() {
                        continue;
                    }
                    parts.push("--data-urlencode".into());
                    parts.push(q(&format!("{}={}", kv.name, kv.value)));
                }
            }
            BodyType::Multipart => {
                for kv in body.items.iter().flatten().filter(|kv| kv.enabled) {
                    if kv.name.trim().is_empty() {
                        continue;
                    }
                    parts.push("-F".into());
                    if kv.kind.as_deref() == Some("file") {
                        parts.push(q(&format!("{}=@{}", kv.name, kv.value)));
                    } else {
                        parts.push(q(&format!("{}={}", kv.name, kv.value)));
                    }
                }
            }
            BodyType::Graphql => {
                parts.push("-H".into());
                parts.push(q("Content-Type: application/json"));
                let variables = body
                    .variables
                    .as_deref()
                    .map(|v| v.trim().to_string())
                    .unwrap_or_default();
                let vars: serde_json::Value =
                    serde_json::from_str(if variables.is_empty() { "null" } else { &variables })
                        .unwrap_or(serde_json::Value::String(variables));
                let payload = serde_json::json!({
                    "query": body.query.clone().unwrap_or_default(),
                    "variables": vars,
                });
                parts.push("--data-raw".into());
                parts.push(q(&serde_json::to_string(&payload).expect("json")));
            }
            BodyType::Binary => {
                let path = body.path.clone().unwrap_or_default();
                parts.push("--data-binary".into());
                parts.push(q(&format!("@{path}")));
            }
        }
    }

    parts.join(" ")
}

fn encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC)
        .to_string()
        .replace("%2F", "/")
        .replace("%3A", ":")
        .replace("%3F", "?")
        .replace("%3D", "=")
        .replace("%26", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(yaml: &str) -> RequestDoc {
        yaml_to(yaml).expect("parse")
    }

    #[test]
    fn get_with_params_and_headers() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: GET
  url: "{{baseUrl}}/users"
  params:
    - name: page
      value: "2"
      enabled: true
    - name: q
      value: "a b"
      enabled: false
  headers:
    - name: Accept
      value: application/json
      enabled: true
"#,
        );
        let curl = request_to_curl(&d);
        assert!(curl.contains("-X GET"), "{curl}");
        assert!(curl.contains("'{{baseUrl}}/users?page=2'"), "{curl}");
        assert!(!curl.contains("q="), "disabled param excluded");
        assert!(curl.contains("-H 'Accept: application/json'"));
    }

    #[test]
    fn json_body_and_bearer() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test/users"
  body:
    type: json
    content: '{"name": "Ada"}'
auth:
  type: bearer
  token: "{{accessToken}}"
"#,
        );
        let curl = request_to_curl(&d);
        assert!(curl.contains("--data-raw '{\"name\": \"Ada\"}'"), "{curl}");
        assert!(curl.contains("-H 'Authorization: Bearer {{accessToken}}'"));
        assert!(curl.contains("Content-Type: application/json"));
    }

    #[test]
    fn basic_and_multipart() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test"
  body:
    type: multipart
    items:
      - name: file
        value: ./doc.pdf
        kind: file
auth:
  type: basic
  username: ada
  password: "p@ss"
"#,
        );
        let curl = request_to_curl(&d);
        assert!(curl.contains("-u 'ada:p@ss'"), "{curl}");
        assert!(curl.contains("-F 'file=@./doc.pdf'"), "{curl}");
    }

    #[test]
    fn shell_escaping() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test"
  body:
    type: text
    content: "it's here"
"#,
        );
        let curl = request_to_curl(&d);
        assert!(curl.contains(r"it'\''s here"), "{curl}");
    }
}
