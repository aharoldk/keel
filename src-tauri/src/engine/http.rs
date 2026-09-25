//! HTTP engine: turns a fully-resolved request into a real HTTP exchange and
//! captures status, timing, size, headers, cookies and body (text or base64).

use std::time::Instant;

use base64::Engine as _;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub struct HttpOptions {
    pub timeout_secs: u64,
    pub follow_redirects: bool,
    /// All-protocol proxy URL; may embed `user:pass` credentials.
    pub proxy_url: Option<String>,
    /// Accept invalid/self-signed TLS certificates.
    pub insecure_tls: bool,
    /// Absolute path to a PEM file with extra root certificates.
    pub ca_cert_path: Option<String>,
    /// Maximum redirect hops when `follow_redirects` is true.
    pub max_redirects: u64,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            follow_redirects: true,
            proxy_url: None,
            insecure_tls: false,
            ca_cert_path: None,
            max_redirects: 10,
        }
    }
}

/// Everything needed to perform the exchange, after variable resolution.
#[derive(Debug, Clone)]
pub struct PreparedRequest {
    pub method: crate::model::HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: PreparedBody,
}

#[derive(Debug, Clone)]
pub enum PreparedBody {
    None,
    Raw { content_type: Option<&'static str>, bytes: Vec<u8> },
    Form(Vec<(String, String)>),
    Multipart(Vec<Part>),
}

#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    pub value: String,
    /// `true` → `value` is a filesystem path.
    pub is_file: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeResult {
    pub status: Option<i64>,
    pub status_text: String,
    pub time_ms: f64,
    pub size_bytes: u64,
    pub headers: Vec<(String, String)>,
    pub cookies: Vec<(String, String)>,
    /// Raw `set-cookie` header values (all of them, lossy-UTF-8).
    pub set_cookie_raw: Vec<String>,
    pub content_type: Option<String>,
    pub body_text: Option<String>,
    pub body_base64: Option<String>,
    pub truncated: bool,
}

const MAX_BODY_BYTES: u64 = 10 * 1024 * 1024;

/// Appends `k=v` query pairs to a URL (encoding values), preserving any
/// existing query string.
pub fn url_with_query(base: &str, pairs: &[(String, String)]) -> Result<String, String> {
    if pairs.is_empty() {
        return Ok(base.to_string());
    }
    let mut url =
        reqwest::Url::parse(base).map_err(|e| format!("invalid URL `{base}`: {e}"))?;
    {
        let mut qp = url.query_pairs_mut();
        for (k, v) in pairs {
            if k.trim().is_empty() {
                continue;
            }
            qp.append_pair(k, v);
        }
    }
    Ok(url.to_string())
}

fn textual_content_type(ct: &str) -> bool {
    let ct = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    ct.starts_with("text/")
        || matches!(
            ct.as_str(),
            "application/json"
                | "application/xml"
                | "application/yaml"
                | "application/javascript"
                | "application/x-www-form-urlencoded"
                | "application/graphql"
                | "application/xhtml+xml"
                | "application/soap+xml"
                | "application/rss+xml"
                | "application/atom+xml"
        )
        || ct.ends_with("+json")
        || ct.ends_with("+xml")
        || ct.ends_with("+yaml")
}

fn parse_cookie(raw: &str) -> Option<(String, String)> {
    let pair = raw.split(';').next()?.trim();
    let (name, value) = pair.split_once('=')?;
    Some((name.trim().to_string(), value.trim().to_string()))
}

/// Performs the exchange. Transport failures return `Err` with a short,
/// human-readable message.
pub async fn execute(prepared: &PreparedRequest, opts: &HttpOptions) -> Result<ExchangeResult, String> {
    let method = reqwest::Method::from_bytes(prepared.method.as_str().as_bytes())
        .map_err(|e| format!("invalid method: {e}"))?;

    let url = reqwest::Url::parse(&prepared.url).map_err(|e| format!("invalid URL: {e}"))?;

    let mut client_builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(opts.timeout_secs))
        .user_agent(concat!("keel/", env!("CARGO_PKG_VERSION")));
    client_builder = if opts.follow_redirects {
        client_builder.redirect(reqwest::redirect::Policy::limited(
            opts.max_redirects.max(1) as usize,
        ))
    } else {
        client_builder.redirect(reqwest::redirect::Policy::none())
    };
    if opts.insecure_tls {
        client_builder = client_builder.danger_accept_invalid_certs(true);
    }
    if let Some(proxy_url) = opts.proxy_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let proxy =
            reqwest::Proxy::all(proxy_url).map_err(|e| format!("invalid proxy URL: {e}"))?;
        client_builder = client_builder.proxy(proxy);
    }
    if let Some(ca_path) = opts.ca_cert_path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pem = std::fs::read(ca_path).map_err(|e| format!("cannot read CA cert `{ca_path}`: {e}"))?;
        let cert = reqwest::Certificate::from_pem(&pem)
            .map_err(|e| format!("invalid CA cert `{ca_path}`: {e}"))?;
        client_builder = client_builder.add_root_certificate(cert);
    }
    let client = client_builder
        .build()
        .map_err(|e| format!("http client error: {e}"))?;

    let mut req = client.request(method, url);
    for (name, value) in &prepared.headers {
        if name.is_empty() {
            continue;
        }
        req = req.header(name.clone(), value.clone());
    }

    req = match &prepared.body {
        PreparedBody::None => req,
        PreparedBody::Raw { content_type, bytes } => {
            if let Some(ct) = content_type {
                let has_ct = prepared
                    .headers
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
                if !has_ct {
                    req = req.header("content-type", *ct);
                }
            }
            req.body(bytes.clone())
        }
        PreparedBody::Form(pairs) => req.form(&pairs),
        PreparedBody::Multipart(parts) => {
            let mut form = reqwest::multipart::Form::new();
            for part in parts {
                if part.is_file {
                    let bytes = tokio::fs::read(&part.value)
                        .await
                        .map_err(|e| format!("cannot read file `{}`: {e}", part.value))?;
                    let filename = std::path::Path::new(&part.value)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("upload.bin")
                        .to_string();
                    let mime = mime_guess::from_path(&filename).first_or_octet_stream();
                    form = form.part(
                        part.name.clone(),
                        reqwest::multipart::Part::bytes(bytes)
                            .file_name(filename)
                            .mime_str(mime.essence_str())
                            .map_err(|e| format!("mime error: {e}"))?,
                    );
                } else {
                    form = form.part(part.name.clone(), reqwest::multipart::Part::text(part.value.clone()));
                }
            }
            req.multipart(form)
        }
    };

    let start = Instant::now();
    let response = req.send().await.map_err(|e| transport_message(&e))?;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    let status = response.status();
    let mut headers: Vec<(String, String)> = Vec::new();
    for (name, value) in response.headers() {
        if let Ok(v) = value.to_str() {
            headers.push((name.as_str().to_string(), v.to_string()));
        }
    }
    // Collect every raw `set-cookie` value. `to_str()` rejects non-visible
    // ASCII (e.g. latin-1 cookie values), so fall back to lossy UTF-8 of the
    // raw bytes to make sure none of them silently vanish.
    let set_cookie_raw: Vec<String> = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|v| match v.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => String::from_utf8_lossy(v.as_bytes()).into_owned(),
        })
        .collect();
    let cookies: Vec<(String, String)> =
        set_cookie_raw.iter().filter_map(|v| parse_cookie(v)).collect();

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let content_length = response.content_length();
    let (bytes, mut truncated) = collect_body(response, MAX_BODY_BYTES).await?;
    // A truthful Content-Length above the cap also means truncated, even if
    // the stream happened to deliver fewer bytes (e.g. compressed bodies).
    truncated |= content_length.map(|l| l > MAX_BODY_BYTES).unwrap_or(false);
    let size = content_length
        .map(|l| l.max(bytes.len() as u64))
        .unwrap_or(bytes.len() as u64);

    let (body_text, body_base64) = match (&content_type, bytes.is_empty()) {
        (None, true) => (Some(String::new()), None),
        (_, true) => (Some(String::new()), Some(String::new())),
        (ct, false) => {
            let is_textual = ct
                .as_deref()
                .map(textual_content_type)
                .unwrap_or(false)
                || std::str::from_utf8(&bytes).is_ok();
            if is_textual {
                (Some(String::from_utf8_lossy(&bytes).into_owned()), None)
            } else {
                (
                    None,
                    Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
                )
            }
        }
    };

    Ok(ExchangeResult {
        status: Some(status.as_u16() as i64),
        status_text: status
            .canonical_reason()
            .unwrap_or("")
            .to_string(),
        time_ms: elapsed_ms,
        size_bytes: size,
        headers,
        cookies,
        set_cookie_raw,
        content_type,
        body_text,
        body_base64,
        truncated,
    })
}

/// Streams the response body, keeping at most `max` bytes. Stops reading as
/// soon as the cap is hit so a huge response can't exhaust memory; the second
/// return value is `true` when the body was cut short.
async fn collect_body(response: reqwest::Response, max: u64) -> Result<(Vec<u8>, bool), String> {
    use futures_util::StreamExt;

    let mut stream = response.bytes_stream();
    let mut body: Vec<u8> = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| transport_message(&e))?;
        if chunk.is_empty() {
            continue;
        }
        let remaining = max.saturating_sub(body.len() as u64) as usize;
        if remaining == 0 {
            truncated = true;
            break;
        }
        if chunk.len() > remaining {
            body.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        body.extend_from_slice(&chunk);
    }
    Ok((body, truncated))
}

fn transport_message(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "Request timed out".to_string()
    } else if e.is_connect() {
        let msg = e.to_string();
        let short = msg
            .rsplit("error: ")
            .next()
            .unwrap_or(&msg)
            .to_string();
        format!("Connection failed: {short}")
    } else if e.is_decode() {
        format!("Failed to read response body: {e}")
    } else if e.is_request() {
        format!("Request error: {e}")
    } else {
        e.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_json_roundtrip() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/users"))
            .and(wiremock::matchers::header("x-trace", "abc"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(serde_json::json!({"id": 7})),
            )
            .mount(&server)
            .await;

        let prepared = PreparedRequest {
            method: crate::model::HttpMethod::GET,
            url: format!("{}/users", server.uri()),
            headers: vec![("x-trace".to_string(), "abc".to_string())],
            body: PreparedBody::None,
        };
        let result = execute(&prepared, &HttpOptions::default()).await.expect("ok");
        assert_eq!(result.status, Some(200));
        assert_eq!(result.body_text.as_deref(), Some(r#"{"id":7}"#));
        assert_eq!(result.content_type.as_deref(), Some("application/json"));
        assert!(result.body_base64.is_none());
    }

    #[tokio::test]
    async fn post_form_and_headers() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::body_string("a=1&b=hello"))
            .respond_with(wiremock::ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let prepared = PreparedRequest {
            method: crate::model::HttpMethod::POST,
            url: format!("{}/items", server.uri()),
            headers: vec![],
            body: PreparedBody::Form(vec![("a".into(), "1".into()), ("b".into(), "hello".into())]),
        };
        let result = execute(&prepared, &HttpOptions::default()).await.expect("ok");
        assert_eq!(result.status, Some(201));
    }

    #[tokio::test]
    async fn connection_refused_is_short_error() {
        // Port 1 is effectively guaranteed to be closed.
        let prepared = PreparedRequest {
            method: crate::model::HttpMethod::GET,
            url: "http://127.0.0.1:1/ping".to_string(),
            headers: vec![],
            body: PreparedBody::None,
        };
        let err = execute(&prepared, &HttpOptions::default())
            .await
            .expect_err("should fail");
        assert!(err.starts_with("Connection failed"), "{err}");
    }

    #[tokio::test]
    async fn cookies_captured() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("set-cookie", "sid=42; Path=/; HttpOnly"),
            )
            .mount(&server)
            .await;
        let prepared = PreparedRequest {
            method: crate::model::HttpMethod::GET,
            url: format!("{}/", server.uri()),
            headers: vec![],
            body: PreparedBody::None,
        };
        let result = execute(&prepared, &HttpOptions::default()).await.expect("ok");
        assert_eq!(result.cookies, vec![("sid".to_string(), "42".to_string())]);
        assert_eq!(result.set_cookie_raw.len(), 1);
        assert_eq!(result.set_cookie_raw[0], "sid=42; Path=/; HttpOnly");
    }

    #[test]
    fn options_defaults() {
        let opts = HttpOptions::default();
        assert_eq!(opts.max_redirects, 10);
        assert!(!opts.insecure_tls);
        assert_eq!(opts.proxy_url, None);
        assert_eq!(opts.ca_cert_path, None);
    }

    #[tokio::test]
    async fn set_cookie_raw_keeps_multiple_and_non_ascii() {
        let server = wiremock::MockServer::start().await;
        let weird = reqwest::header::HeaderValue::from_bytes(b"caf\xc3\xa9=1; Path=/")
            .expect("header bytes");
        let template = wiremock::ResponseTemplate::new(200)
            .append_header("set-cookie", "a=1; Path=/")
            .append_header("set-cookie", "b=2; Path=/")
            .append_header("set-cookie", weird);
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(template)
            .mount(&server)
            .await;
        let prepared = PreparedRequest {
            method: crate::model::HttpMethod::GET,
            url: format!("{}/", server.uri()),
            headers: vec![],
            body: PreparedBody::None,
        };
        let result = execute(&prepared, &HttpOptions::default()).await.expect("ok");
        assert_eq!(result.set_cookie_raw.len(), 3);
        assert!(result.set_cookie_raw.iter().any(|c| c.starts_with("caf\u{e9}=1")));
        // Parsed cookies survive too (all three pairs).
        assert_eq!(result.cookies.len(), 3);
    }

    #[tokio::test]
    async fn max_redirects_is_honored() {
        let server = wiremock::MockServer::start().await;
        let loop_url = format!("{}/loop", server.uri());
        let redirect_to = loop_url.clone();
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(move |_: &wiremock::Request| {
                wiremock::ResponseTemplate::new(302)
                    .insert_header("location", redirect_to.clone())
            })
            .mount(&server)
            .await;
        let prepared = PreparedRequest {
            method: crate::model::HttpMethod::GET,
            url: loop_url,
            headers: vec![],
            body: PreparedBody::None,
        };
        let opts = HttpOptions {
            max_redirects: 2,
            ..HttpOptions::default()
        };
        let err = execute(&prepared, &opts).await.expect_err("too many redirects");
        assert!(err.to_lowercase().contains("redirect"), "{err}");
    }

    #[tokio::test]
    async fn invalid_proxy_url_errors_out() {
        let prepared = PreparedRequest {
            method: crate::model::HttpMethod::GET,
            url: "http://example.invalid/".to_string(),
            headers: vec![],
            body: PreparedBody::None,
        };
        let opts = HttpOptions {
            proxy_url: Some("not a url".into()),
            ..HttpOptions::default()
        };
        let err = execute(&prepared, &opts).await.expect_err("bad proxy");
        assert!(err.contains("proxy"), "{err}");
    }

    /// Serves `size` bytes of `b'x'` and returns the mock server + URL.
    async fn body_server(size: usize) -> (wiremock::MockServer, String) {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "text/plain")
                    .set_body_bytes(vec![b'x'; size]),
            )
            .mount(&server)
            .await;
        let url = format!("{}/big", server.uri());
        (server, url)
    }

    #[tokio::test]
    async fn body_over_cap_is_truncated() {
        let (_server, url) = body_server(10_000).await;
        let response = reqwest::get(&url).await.expect("response");
        let (body, truncated) = collect_body(response, 100).await.expect("body");
        assert!(truncated);
        assert!(body.len() <= 100, "stored body must respect the cap");
    }

    #[tokio::test]
    async fn body_under_cap_is_not_truncated() {
        let (_server, url) = body_server(50).await;
        let response = reqwest::get(&url).await.expect("response");
        let (body, truncated) = collect_body(response, 100).await.expect("body");
        assert!(!truncated);
        assert_eq!(body.len(), 50);
    }

    #[tokio::test]
    async fn execute_reports_truncated_flag() {
        // End-to-end through `execute`: the mock lies about Content-Length
        // being huge, which must surface as `truncated` while the stored
        // body stays within the cap.
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "text/plain")
                    .set_body_string("hello"),
            )
            .mount(&server)
            .await;
        let prepared = PreparedRequest {
            method: crate::model::HttpMethod::GET,
            url: format!("{}/", server.uri()),
            headers: vec![],
            body: PreparedBody::None,
        };
        let result = execute(&prepared, &HttpOptions::default()).await.expect("ok");
        assert!(!result.truncated, "small body is never truncated");
        assert_eq!(result.body_text.as_deref(), Some("hello"));
    }
}
