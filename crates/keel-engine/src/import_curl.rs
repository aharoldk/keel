//! cURL command → Keel request.

use crate::model::*;

#[derive(Debug, Default, Clone)]
pub struct CurlParts {
    method: Option<String>,
    url: Option<String>,
    headers: Vec<(String, String)>,
    data: Vec<String>,
    form: Vec<(String, String)>, // (name, value-or-@path)
    basic: Option<(String, String)>,
    head_only: bool,
    cookies: Vec<String>,
}

/// Tokenizes a shell-style command line honoring single quotes, double quotes
/// and backslash escapes.
pub fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut has_token = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                has_token = true;
                for c2 in chars.by_ref() {
                    if c2 == '\'' {
                        break;
                    }
                    cur.push(c2);
                }
            }
            '"' => {
                has_token = true;
                while let Some(c2) = chars.next() {
                    match c2 {
                        '"' => break,
                        '\\' => {
                            if let Some(n) = chars.next() {
                                cur.push(n);
                            }
                        }
                        _ => cur.push(c2),
                    }
                }
            }
            '\\' => {
                has_token = true;
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() => {
                if has_token {
                    tokens.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                has_token = true;
                cur.push(c);
            }
        }
    }
    if has_token {
        tokens.push(cur);
    }
    tokens
}

/// Parses a curl command into request parts.
pub fn parse_curl(input: &str) -> Result<CurlParts, String> {
    let tokens = tokenize(input);
    let mut it = tokens.iter().peekable();
    // Skip leading argv[0] if it looks like `curl`.
    if let Some(first) = it.peek() {
        let f = first.trim();
        if f.ends_with("curl") || f == "curl" {
            it.next();
        }
    }
    let mut parts = CurlParts::default();
    while let Some(tok) = it.next() {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        let is_long = tok.starts_with("--");
        let (flag, inline) = if let Some(rest) = tok.strip_prefix("--") {
            (rest.to_string(), None)
        } else if let Some(rest) = tok.strip_prefix('-') {
            (rest.to_string(), None)
        } else {
            // positional: URL
            if parts.url.is_none() && looks_like_url(tok) {
                parts.url = Some(tok.to_string());
            }
            continue;
        };
        // Support `--flag=value`
        let (flag, inline_val) = if let Some((f, v)) = flag.split_once('=') {
            (f.to_string(), Some(v.to_string()))
        } else {
            (flag, inline)
        };
        // Combined short boolean flags, e.g. `-skL`. Only when every char is a
        // known boolean short flag — otherwise fall through to normal parsing.
        if !is_long
            && inline_val.is_none()
            && flag.chars().count() > 1
            && flag.chars().all(|c| BOOL_SHORT_FLAGS.contains(c))
        {
            if flag.contains('I') {
                parts.head_only = true;
            }
            continue;
        }
        let mut value = || -> Option<String> {
            inline_val.clone().or_else(|| it.next().cloned())
        };
        match flag.as_str() {
            "X" | "request" => parts.method = value().map(|v| v.to_uppercase()),
            "H" | "header" => {
                if let Some(v) = value() {
                    if let Some((k, hv)) = v.split_once(':') {
                        parts.headers.push((k.trim().to_string(), hv.trim().to_string()));
                    }
                }
            }
            "d" | "data" | "data-raw" | "data-ascii" | "data-urlencode" | "data-binary" => {
                if let Some(v) = value() {
                    parts.data.push(v);
                }
            }
            "json" => {
                if let Some(v) = value() {
                    parts
                        .headers
                        .push(("Content-Type".into(), "application/json".into()));
                    parts.headers.push(("Accept".into(), "application/json".into()));
                    parts.data.push(v);
                }
            }
            "F" | "form" => {
                if let Some(v) = value() {
                    if let Some((k, fv)) = v.split_once('=') {
                        parts.form.push((k.to_string(), fv.to_string()));
                    }
                }
            }
            "u" | "user" => {
                if let Some(v) = value() {
                    let (user, pass) = v.split_once(':').unwrap_or((v.as_str(), ""));
                    parts.basic = Some((user.to_string(), pass.to_string()));
                }
            }
            "A" | "user-agent" => {
                if let Some(v) = value() {
                    parts.headers.push(("User-Agent".into(), v));
                }
            }
            "e" | "referer" => {
                if let Some(v) = value() {
                    parts.headers.push(("Referer".into(), v));
                }
            }
            "b" | "cookie" => {
                if let Some(v) = value() {
                    parts.cookies.push(v);
                }
            }
            "url" => parts.url = value(),
            "I" | "head" => parts.head_only = true,
            // Ignored-but-harmless flags that take a value — consume it.
            "x" | "proxy" | "noproxy" | "o" | "output" | "m" | "max-time"
            | "connect-timeout" | "retry" => {
                let _ = value();
            }
            _ if is_bool_flag(&flag) => {
                // Boolean flags take no value; crucially they must NOT
                // swallow the next token (usually the URL).
            }
            _ => {
                // Unknown flag with a possible value — consume it to stay safe.
                let _ = value();
            }
        }
    }
    if let Some(first_cookie) = parts.cookies.first().cloned() {
        parts.headers.push(("Cookie".into(), first_cookie));
    }
    Ok(parts)
}

/// Short flags that never take a value (used to recognize combined forms
/// like `-skL`).
const BOOL_SHORT_FLAGS: &str = "skLivNgSf46IOG";

/// Known boolean curl flags: they take no value, so the parser must not
/// consume the next token (which is usually the URL).
fn is_bool_flag(flag: &str) -> bool {
    matches!(
        flag,
        "s" | "silent"
            | "k" | "insecure"
            | "L" | "location"
            | "compressed"
            | "i" | "include"
            | "v" | "verbose"
            | "N" | "no-buffer"
            | "g" | "globoff"
            | "S" | "show-error"
            | "f" | "fail"
            | "4" | "ipv4"
            | "6" | "ipv6"
            | "O" | "remote-name"
            | "http1.1" | "http2"
            | "no-progress-meter"
            | "fail-with-body"
            | "path-as-is"
            | "get"
    )
}

fn looks_like_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.contains("://")
        || (s.contains('.') && !s.contains(' ') && !s.starts_with('-'))
}

/// Converts a parsed curl into a Keel request doc.
pub fn curl_to_request(input: &str, name_hint: Option<String>) -> Result<RequestDoc, String> {
    let parts = parse_curl(input)?;
    let url = parts
        .url
        .clone()
        .or_else(|| {
            parts
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("host"))
                .map(|(_, v)| format!("https://{v}/"))
        })
        .ok_or_else(|| "no URL found in the curl command".to_string())?;

    let method = if parts.head_only {
        "HEAD".to_string()
    } else {
        parts.method.clone().unwrap_or_else(|| {
            if parts.data.is_empty() && parts.form.is_empty() {
                "GET".into()
            } else {
                "POST".into()
            }
        })
    };

    let headers: Vec<KV> = parts
        .headers
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("content-type"))
        .map(|(k, v)| KV {
            name: k.clone(),
            value: v.clone(),
            ..KV::default()
        })
        .collect();

    let content_type = parts
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.to_ascii_lowercase());

    let body: Option<Body> = if !parts.form.is_empty() {
        Some(Body {
            body_type: BodyType::Multipart,
            items: Some(
                parts
                    .form
                    .iter()
                    .map(|(k, v)| KV {
                        name: k.clone(),
                        value: v.strip_prefix('@').map(|p| p.to_string()).unwrap_or_else(|| v.clone()),
                        enabled: true,
                        kind: v.starts_with('@').then(|| "file".to_string()),
                    })
                    .collect(),
            ),
            ..Body::default()
        })
    } else if !parts.data.is_empty() {
        let joined = parts.data.join("&");
        let ct = content_type.as_deref().unwrap_or("");
        let is_json = ct.contains("json")
            || (ct.is_empty() && {
                let t = joined.trim_start();
                t.starts_with('{') || t.starts_with('[')
            });
        // curl sends `application/x-www-form-urlencoded` for `-d` unless the
        // caller overrode it, so `-d 'a=1&b=2'` is a form body, not plain text.
        let is_form = ct.contains("x-www-form-urlencoded")
            || (ct.is_empty() && !is_json && looks_like_form_pairs(&joined));
        let (body_type, content, items) = if is_json {
            // Preserve the payload byte-for-byte: reformatting it would change
            // what a re-exported request actually sends.
            (BodyType::Json, Some(joined.clone()), None)
        } else if is_form {
            (BodyType::FormUrlencoded, None, Some(parse_form_pairs(&joined)))
        } else {
            (BodyType::Text, Some(joined.clone()), None)
        };
        Some(Body {
            body_type,
            content,
            items,
            ..Body::default()
        })
    } else {
        None
    };

    let auth: Option<Auth> = parts
        .basic
        .clone()
        .map(|(user, pass)| Auth {
            auth_type: AuthType::Basic,
            username: Some(user),
            password: Some(pass),
            ..Auth::default()
        });

    let name = name_hint.unwrap_or_else(|| display_name(&method, &url));
    let method = parse_method(&method)?;

    // Split `?a=b&c=d` into params rows when present.
    let (url_base, params) = match url.split_once('?') {
        Some((_base, _query)) => (url[..url.find('?').expect("split")].to_string(), split_url_params(Some(url.as_str()))),
        None => (url.clone(), None),
    };

    Ok(RequestDoc {
        schema_version: SCHEMA_VERSION.into(),
        name,
        kind: "request".into(),
        description: Some("Imported from cURL".into()),
        protocol: None,
        graphql: None,
        websocket: None,
        grpc: None,
        request: RequestBlock {
            method,
            url: url_base,
            params,
            headers: Some(headers),
            path_params: None,
            body,
        },
        auth,
        variables: None,
        scripts: None,
        tests: None,
    })
}

fn split_url_params(url: Option<&str>) -> Option<Vec<KV>> {
    url.and_then(|u| u.split_once('?')).map(|(_, q)| {
        q.split('&')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                KV {
                    name: k.to_string(),
                    value: v.to_string(),
                    ..KV::default()
                }
            })
            .collect()
    })
}

/// Splits `a=1&b=2` into form rows. Segments without `=` become value-less
/// fields, mirroring how a server would parse them.
fn parse_form_pairs(data: &str) -> Vec<KV> {
    data.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            KV {
                name: k.to_string(),
                value: v.to_string(),
                enabled: true,
                ..KV::default()
            }
        })
        .collect()
}

/// True when `-d` data reads as urlencoded pairs (`a=1&b=2`) rather than an
/// opaque payload. Rejects JSON/XML-looking data so those keep their type.
fn looks_like_form_pairs(data: &str) -> bool {
    let t = data.trim();
    if t.is_empty() || !t.contains('=') || t.contains(['{', '[', '"', '<']) {
        return false;
    }
    t.split('&').all(|pair| pair.is_empty() || pair.contains('='))
}

fn parse_method(m: &str) -> Result<HttpMethod, String> {
    match m {
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

fn display_name(method: &str, url: &str) -> String {
    let host = reqwest::Url::parse(url)
        .ok()
        .map(|u| {
            let host = u.host_str().unwrap_or("api");
            let path = u.path().trim_matches('/');
            if path.is_empty() {
                host.to_string()
            } else {
                let last = path.rsplit('/').next().unwrap_or(path);
                if last.parse::<u64>().is_ok() || last.len() > 24 {
                    format!("{host}/{}/…", path.split('/').next().unwrap_or(path))
                } else {
                    format!("{host}/{last}")
                }
            }
        })
        .unwrap_or_else(|| "request".into());
    format!("{method} {host}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_get() {
        let doc = curl_to_request("curl https://api.example.com/users?page=2", None).expect("ok");
        assert_eq!(doc.request.method, HttpMethod::GET);
        assert_eq!(doc.request.url, "https://api.example.com/users");
        let params = doc.request.params.expect("params");
        assert_eq!(params[0].name, "page");
        assert_eq!(params[0].value, "2");
        assert_eq!(doc.name, "GET api.example.com/users");
    }

    #[test]
    fn post_json_with_headers() {
        let doc = curl_to_request(
            r#"curl -X POST 'https://api.example.com/users' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer t0k3n' \
  --data-raw '{"name": "Ada", "age": 36}'"#,
            None,
        )
        .expect("ok");
        assert_eq!(doc.request.method, HttpMethod::POST);
        let body = doc.request.body.expect("body");
        assert_eq!(body.body_type, BodyType::Json);
        assert!(body.content.as_ref().expect("content").contains("\"name\": \"Ada\""));
        let headers = doc.request.headers.expect("headers");
        assert!(headers.iter().any(|h| h.name == "Authorization" && h.value == "Bearer t0k3n"));
        assert!(!headers.iter().any(|h| h.name.eq_ignore_ascii_case("content-type")));
    }

    #[test]
    fn quoted_args_and_flags() {
        let doc = curl_to_request(
            r#"curl --request PUT "https://x.test/api/v1/things/42" -u "ada:secret!" -d 'k=v' -H "X-One: 1""#,
            None,
        )
        .expect("ok");
        assert_eq!(doc.request.method, HttpMethod::PUT);
        let auth = doc.auth.expect("auth");
        assert_eq!(auth.username.as_deref(), Some("ada"));
        assert_eq!(auth.password.as_deref(), Some("secret!"));
        // `-d 'k=v'` with no explicit Content-Type is form-urlencoded, which
        // is what curl itself sends.
        let body = doc.request.body.expect("body");
        assert_eq!(body.body_type, BodyType::FormUrlencoded);
        let items = body.items.expect("items");
        assert_eq!(items[0].name, "k");
        assert_eq!(items[0].value, "v");
    }

    #[test]
    fn form_upload() {
        let doc = curl_to_request(
            r#"curl -F "name=report" -F "file=@./doc.pdf" https://up.test/upload"#,
            None,
        )
        .expect("ok");
        let body = doc.request.body.expect("body");
        assert_eq!(body.body_type, BodyType::Multipart);
        let items = body.items.expect("items");
        assert_eq!(items[0].name, "name");
        assert_eq!(items[0].value, "report");
        assert_eq!(items[1].name, "file");
        assert_eq!(items[1].kind.as_deref(), Some("file"));
        assert_eq!(items[1].value, "./doc.pdf");
    }

    #[test]
    fn tokenizer_respects_quotes() {
        let t = tokenize(r#"curl -H 'a: b c' -d "x=1\&2" https://h"#);
        assert_eq!(t[1], "-H");
        assert_eq!(t[2], "a: b c");
        assert_eq!(t[4], "x=1&2");
    }

    #[test]
    fn head_request() {
        let doc = curl_to_request("curl -I https://x.test/ping", None).expect("ok");
        assert_eq!(doc.request.method, HttpMethod::HEAD);
    }

    #[test]
    fn bool_flags_do_not_swallow_url() {
        for cmd in [
            "curl -s https://api.example.com/users",
            "curl --silent https://api.example.com/users",
            "curl -k https://api.example.com/users",
            "curl --insecure https://api.example.com/users",
            "curl -L https://api.example.com/users",
            "curl --location https://api.example.com/users",
            "curl --compressed https://api.example.com/users",
            "curl -i https://api.example.com/users",
            "curl -v https://api.example.com/users",
            "curl -N https://api.example.com/users",
            "curl -g https://api.example.com/users",
        ] {
            let doc = curl_to_request(cmd, None).unwrap_or_else(|e| panic!("{cmd}: {e}"));
            assert_eq!(doc.request.url, "https://api.example.com/users", "{cmd}");
        }
    }

    #[test]
    fn combined_short_bool_flags() {
        let doc = curl_to_request("curl -skL https://api.example.com/x -H 'a: b'", None).expect("ok");
        assert_eq!(doc.request.url, "https://api.example.com/x");
        let headers = doc.request.headers.expect("headers");
        assert!(headers.iter().any(|h| h.name == "a" && h.value == "b"));
    }

    #[test]
    fn silent_flag_with_post_data() {
        let doc = curl_to_request(
            "curl -s -X POST https://api.example.com/x -d 'x=1'",
            None,
        )
        .expect("ok");
        assert_eq!(doc.request.method, HttpMethod::POST);
        assert_eq!(doc.request.url, "https://api.example.com/x");
        let body = doc.request.body.expect("body");
        assert_eq!(body.body_type, BodyType::FormUrlencoded);
        let items = body.items.expect("items");
        assert_eq!(items[0].name, "x");
        assert_eq!(items[0].value, "1");
    }

    #[test]
    fn data_without_content_type_is_form_urlencoded() {
        // Real curl sends `application/x-www-form-urlencoded` for `-d` when
        // no Content-Type is given, so these must import as form bodies.
        let doc = curl_to_request(
            "curl -X POST https://api.example.com/x -d 'a=1&b=2'",
            None,
        )
        .expect("ok");
        let body = doc.request.body.expect("body");
        assert_eq!(body.body_type, BodyType::FormUrlencoded);
        let items = body.items.expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].name.as_str(), items[0].value.as_str()), ("a", "1"));
        assert_eq!((items[1].name.as_str(), items[1].value.as_str()), ("b", "2"));

        // Values with spaces survive as a single field.
        let doc = curl_to_request(
            "curl -X POST https://api.example.com/x -d 'name=John Doe'",
            None,
        )
        .expect("ok");
        let items = doc.request.body.expect("body").items.expect("items");
        assert_eq!(items[0].value, "John Doe");
    }

    #[test]
    fn explicit_content_type_wins_over_heuristics() {
        // Explicit urlencoded stays urlencoded.
        let doc = curl_to_request(
            "curl -X POST -H 'Content-Type: application/x-www-form-urlencoded' -d 'a=1' https://api.example.com/x",
            None,
        )
        .expect("ok");
        assert_eq!(
            doc.request.body.expect("body").body_type,
            BodyType::FormUrlencoded
        );

        // Explicit text/plain stays text even though it looks like a pair.
        let doc = curl_to_request(
            "curl -X POST -H 'Content-Type: text/plain' -d 'a=1' https://api.example.com/x",
            None,
        )
        .expect("ok");
        assert_eq!(doc.request.body.expect("body").body_type, BodyType::Text);

        // JSON-looking data with no explicit type still imports as JSON.
        let doc = curl_to_request(
            "curl -X POST -d '{\"a\":1}' https://api.example.com/x",
            None,
        )
        .expect("ok");
        assert_eq!(doc.request.body.expect("body").body_type, BodyType::Json);
    }

    #[test]
    fn json_body_is_preserved_byte_for_byte() {
        // Reformatting the payload would change what a re-exported request
        // sends, so the imported text must come back unchanged.
        let doc = curl_to_request(
            r#"curl -X POST -H 'Content-Type: application/json' -d '{"a":1}' https://api.example.com/x"#,
            None,
        )
        .expect("ok");
        let body = doc.request.body.expect("body");
        assert_eq!(body.body_type, BodyType::Json);
        assert_eq!(body.content.as_deref(), Some("{\"a\":1}"));

        // Nested/minified JSON survives too.
        let doc = curl_to_request(
            r#"curl -X POST -d '{"n":{"x":[1,2]},"s":"v"}' https://api.example.com/x"#,
            None,
        )
        .expect("ok");
        assert_eq!(
            doc.request.body.expect("body").content.as_deref(),
            Some("{\"n\":{\"x\":[1,2]},\"s\":\"v\"}")
        );
    }

    #[test]
    fn looks_like_form_pairs_guards() {
        assert!(looks_like_form_pairs("a=1"));
        assert!(looks_like_form_pairs("a=1&b=2"));
        assert!(looks_like_form_pairs("name=John Doe"));
        assert!(!looks_like_form_pairs("plain text"));
        assert!(!looks_like_form_pairs("{\"a\":1}"));
        assert!(!looks_like_form_pairs("[1,2]"));
        assert!(!looks_like_form_pairs("<xml/>"));
        assert!(!looks_like_form_pairs(""));
        assert!(!looks_like_form_pairs("noequals"));
    }
}
