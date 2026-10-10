//! Multi-language code generation from a Keel `RequestDoc`.
//!
//! Targets: curl (delegates to `export_curl`), fetch, axios, python-requests,
//! httpie, native-node, go-http, java-okhttp. `{{template}}` variables pass
//! through verbatim inside emitted strings — codegen never resolves them.

use base64::Engine as _;

use serde_json::Value;

use crate::model::*;

/// Generates a code snippet for `doc` in the requested `target` language.
pub fn generate_code(doc: &RequestDoc, target: &str) -> Result<String, String> {
    let target = target.trim().to_ascii_lowercase();
    match target.as_str() {
        "curl" => Ok(crate::export_curl::request_to_curl(doc)),
        "fetch" => Ok(fetch_snippet(doc)),
        "axios" => Ok(axios_snippet(doc)),
        "python-requests" => Ok(python_snippet(doc)),
        "httpie" => Ok(httpie_snippet(doc)),
        "native-node" => Ok(node_snippet(doc)),
        "go-http" => Ok(go_snippet(doc)),
        "java-okhttp" => Ok(java_snippet(doc)),
        other => Err(format!(
            "unknown codegen target `{other}` (expected curl|fetch|axios|python-requests|httpie|native-node|go-http|java-okhttp)"
        )),
    }
}

// ---------- shared pieces ----------

/// Method, URL (query rows appended, apikey-in-query included), enabled
/// headers and auth-derived headers.
struct Plan {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
}

fn plan(doc: &RequestDoc) -> Plan {
    let mut url = doc.request.url.clone();
    let mut pairs: Vec<(String, String)> = doc
        .request
        .params
        .iter()
        .flatten()
        .filter(|kv| kv.enabled && !kv.name.trim().is_empty())
        .map(|kv| (kv.name.clone(), kv.value.clone()))
        .collect();
    if let Some(auth) = &doc.auth {
        if auth.auth_type == AuthType::Apikey && auth.location.as_deref() == Some("query") {
            pairs.push((
                auth.key.clone().unwrap_or_default(),
                auth.value.clone().unwrap_or_default(),
            ));
        }
    }
    if !pairs.is_empty() {
        let joiner = if url.contains('?') { "&" } else { "?" };
        let query = pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        url = format!("{url}{joiner}{query}");
    }

    let mut headers: Vec<(String, String)> = doc
        .request
        .headers
        .iter()
        .flatten()
        .filter(|kv| kv.enabled && !kv.name.trim().is_empty())
        .map(|kv| (kv.name.clone(), kv.value.clone()))
        .collect();

    if let Some(auth) = &doc.auth {
        match auth.auth_type {
            AuthType::Bearer => headers.push((
                "Authorization".into(),
                format!("Bearer {}", auth.token.clone().unwrap_or_default()),
            )),
            AuthType::Basic | AuthType::Digest => {
                let user = auth.username.clone().unwrap_or_default();
                let pass = auth.password.clone().unwrap_or_default();
                if auth.auth_type == AuthType::Digest {
                    // Digest needs a challenge round-trip; a precomputed
                    // Basic header is the closest static snippet.
                    headers.push(("X-Keel-Auth".into(), "digest (negotiated at runtime)".into()));
                }
                headers.push((
                    "Authorization".into(),
                    format!("Basic {}", b64(&format!("{user}:{pass}"))),
                ));
            }
            AuthType::Apikey => {
                if auth.location.as_deref() != Some("query") {
                    headers.push((
                        auth.key.clone().unwrap_or_default(),
                        auth.value.clone().unwrap_or_default(),
                    ));
                }
            }
            AuthType::Oauth2 => {
                headers.push((
                    "Authorization".into(),
                    "Bearer <oauth2: resolve token first>".into(),
                ));
            }
            AuthType::None => {}
        }
    }

    // Content type for textual bodies unless the caller set one.
    if let Some(body) = &doc.request.body {
        let ct = match body.body_type {
            BodyType::Json => Some("application/json"),
            BodyType::Text => Some("text/plain"),
            BodyType::Xml => Some("application/xml"),
            BodyType::Graphql => Some("application/json"),
            BodyType::FormUrlencoded => Some("application/x-www-form-urlencoded"),
            _ => None,
        };
        if let Some(ct) = ct {
            if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("content-type")) {
                headers.push(("Content-Type".into(), ct.into()));
            }
        }
    }

    Plan {
        url,
        method: doc.request.method.as_str().to_string(),
        headers,
    }
}

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

/// JSON string literal — valid JS, Java (we don't emit `/`) and Go double-
/// quoted strings, with `{{templates}}` preserved verbatim.
fn lit(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
}

fn raw_json(body: &Body) -> String {
    body.content.clone().unwrap_or_default()
}

fn graphql_payload(body: &Body) -> Value {
    let variables = body
        .variables
        .as_deref()
        .and_then(|v| serde_json::from_str::<Value>(v).ok())
        .unwrap_or(Value::String(body.variables.clone().unwrap_or_default()));
    json_payload(&body.query.clone().unwrap_or_default(), &variables)
}

fn json_payload(query: &str, variables: &Value) -> Value {
    serde_json::json!({ "query": query, "variables": variables.clone() })
}

fn enabled_items(body: &Body) -> Vec<&KV> {
    body.items
        .iter()
        .flatten()
        .filter(|kv| kv.enabled && !kv.name.trim().is_empty())
        .collect()
}

/// The parsed JSON value of a body, if it parses (used for dict/object
/// rendering); templates survive inside string values.
fn parsed_body(body: &Body) -> Option<Value> {
    serde_json::from_str(&raw_json(body)).ok()
}

// ---------- JavaScript (fetch / axios / native node) ----------

fn js_headers_obj(headers: &[(String, String)]) -> String {
    let entries: Vec<String> = headers
        .iter()
        .map(|(k, v)| format!("    {}: {},", lit(k), lit(v)))
        .collect();
    if entries.is_empty() {
        "{}".into()
    } else {
        format!("{{\n{}\n  }}", entries.join("\n"))
    }
}

/// The JS `body:` expression (or comment) for a Keel body.
fn js_body_expr(body: Option<&Body>) -> (Option<String>, Option<String>) {
    // returns (expression, prelude)
    let Some(body) = body else {
        return (None, None);
    };
    match body.body_type {
        BodyType::None => (None, None),
        BodyType::Json | BodyType::Text | BodyType::Xml => {
            (Some(lit(&raw_json(body))), None)
        }
        BodyType::Graphql => (
            Some(format!(
                "JSON.stringify({})",
                serde_json::to_string(&graphql_payload(body)).expect("json")
            )),
            None,
        ),
        BodyType::FormUrlencoded => {
            let entries: Vec<String> = enabled_items(body)
                .into_iter()
                .map(|kv| format!("{}: {}", lit(&kv.name), lit(&kv.value)))
                .collect();
            (Some(format!("new URLSearchParams({{ {} }})", entries.join(", "))), None)
        }
        BodyType::Multipart => {
            let files: Vec<&KV> = enabled_items(body)
                .into_iter()
                .filter(|kv| kv.kind.as_deref() == Some("file"))
                .collect();
            let mut lines: Vec<String> = Vec::new();
            if !files.is_empty() {
                lines.push("import { readFile } from \"node:fs/promises\";".into());
            }
            lines.push("const formData = new FormData();".into());
            for kv in enabled_items(body) {
                if kv.kind.as_deref() == Some("file") {
                    let file_name = kv.value.rsplit(['/', '\\']).next().unwrap_or("file");
                    lines.push(format!(
                        "formData.append({}, new Blob([await readFile({})]), {});",
                        lit(&kv.name),
                        lit(&kv.value),
                        lit(file_name)
                    ));
                } else {
                    lines.push(format!(
                        "formData.append({}, {});",
                        lit(&kv.name),
                        lit(&kv.value)
                    ));
                }
            }
            (Some("formData".into()), Some(lines.join("\n")))
        }
        BodyType::Binary => {
            let path = body.path.clone().unwrap_or_default();
            (
                Some("fileData".into()),
                Some(format!(
                    "import {{ readFile }} from \"node:fs/promises\";\n\nconst fileData = await readFile({});",
                    lit(&path)
                )),
            )
        }
    }
}

fn fetch_snippet(doc: &RequestDoc) -> String {
    let p = plan(doc);
    let (body_expr, prelude) = js_body_expr(doc.request.body.as_ref());
    let mut out = String::new();
    if let Some(pre) = &prelude {
        out.push_str(pre);
        out.push('\n');
    }
    out.push_str(&format!("const response = await fetch({}, {{\n", lit(&p.url)));
    out.push_str(&format!("  method: {},\n", lit(&p.method)));
    if !p.headers.is_empty() {
        out.push_str(&format!("  headers: {},\n", js_headers_obj(&p.headers)));
    }
    if let Some(expr) = body_expr {
        out.push_str(&format!("  body: {expr},\n"));
    }
    out.push_str("});\n\nconsole.log(await response.json());\n");
    out
}

fn axios_snippet(doc: &RequestDoc) -> String {
    let p = plan(doc);
    let (body_expr, prelude) = js_body_expr(doc.request.body.as_ref());
    let mut out = String::from("import axios from \"axios\";\n\n");
    if let Some(pre) = &prelude {
        out.push_str(pre);
        out.push('\n');
    }
    out.push_str("const response = await axios({\n");
    out.push_str(&format!(
        "  method: {},\n  url: {},\n",
        lit(&p.method.to_lowercase()),
        lit(&p.url)
    ));
    if !p.headers.is_empty() {
        out.push_str(&format!("  headers: {},\n", js_headers_obj(&p.headers)));
    }
    if let Some(expr) = body_expr {
        out.push_str(&format!("  data: {expr},\n"));
    }
    out.push_str("});\n\nconsole.log(response.data);\n");
    out
}

fn node_snippet(doc: &RequestDoc) -> String {
    let p = plan(doc);
    let is_https = !p.url.starts_with("http://");
    let module = if is_https { "https" } else { "http" };

    let mut prelude: Vec<String> = Vec::new();
    let mut write_expr: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut needs_fs = false;

    // `req.write` only accepts a string or a Buffer, so every body type is
    // serialized to one of those here — FormData and URLSearchParams objects
    // are rejected with ERR_INVALID_ARG_TYPE.
    if let Some(body) = &doc.request.body {
        match body.body_type {
            BodyType::None => {}
            BodyType::Json | BodyType::Text | BodyType::Xml => {
                prelude.push(format!("const payload = {};", lit(&raw_json(body))));
                write_expr = Some("payload".into());
            }
            BodyType::Graphql => {
                prelude.push(format!(
                    "const payload = JSON.stringify({});",
                    serde_json::to_string(&graphql_payload(body)).expect("json")
                ));
                write_expr = Some("payload".into());
            }
            BodyType::FormUrlencoded => {
                let query = enabled_items(body)
                    .into_iter()
                    .map(|kv| format!("{}={}", kv.name, kv.value))
                    .collect::<Vec<_>>()
                    .join("&");
                prelude.push(format!("const payload = {};", lit(&query)));
                write_expr = Some("payload".into());
            }
            BodyType::Multipart => {
                prelude.push(
                    "const boundary = \"--------------------------\" + Math.random().toString(36).slice(2);"
                        .into(),
                );
                prelude.push("const parts = [];".into());
                for kv in enabled_items(body) {
                    if kv.kind.as_deref() == Some("file") {
                        let file_name = kv.value.rsplit(['/', '\\']).next().unwrap_or("file");
                        prelude.push(format!(
                            "parts.push(Buffer.from(\"--\" + boundary + \"\\r\\nContent-Disposition: form-data; name=\" + {} + \"; filename=\" + {} + \"\\r\\nContent-Type: application/octet-stream\\r\\n\\r\\n\"));",
                            lit(&kv.name),
                            lit(file_name)
                        ));
                        prelude.push(format!("parts.push(fs.readFileSync({}));", lit(&kv.value)));
                        prelude.push("parts.push(Buffer.from(\"\\r\\n\"));".into());
                        needs_fs = true;
                    } else {
                        prelude.push(format!(
                            "parts.push(Buffer.from(\"--\" + boundary + \"\\r\\nContent-Disposition: form-data; name=\" + {} + \"\\r\\n\\r\\n\" + {} + \"\\r\\n\"));",
                            lit(&kv.name),
                            lit(&kv.value)
                        ));
                    }
                }
                prelude.push("parts.push(Buffer.from(\"--\" + boundary + \"--\\r\\n\"));".into());
                prelude.push("const payload = Buffer.concat(parts);".into());
                write_expr = Some("payload".into());
                content_type = Some("\"multipart/form-data; boundary=\" + boundary".into());
            }
            BodyType::Binary => {
                let path = body.path.clone().unwrap_or_default();
                prelude.push(format!("const payload = fs.readFileSync({});", lit(&path)));
                write_expr = Some("payload".into());
                needs_fs = true;
            }
        }
    }

    let mut out = format!("const {module} = require(\"node:{module}\");\n");
    if needs_fs {
        out.push_str("const fs = require(\"node:fs\");\n");
    }
    out.push('\n');
    for line in &prelude {
        out.push_str(line);
        out.push('\n');
    }
    if !prelude.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!(
        "const req = {module}.request({}, {{\n  method: {},\n  headers: {},\n}}, (res) => {{\n  let data = \"\";\n  res.on(\"data\", (chunk) => (data += chunk));\n  res.on(\"end\", () => console.log(data));\n}});\n\nreq.on(\"error\", console.error);\n",
        lit(&p.url),
        lit(&p.method),
        js_headers_obj(&p.headers).replace("\n    ", "\n      ")
    ));
    if let Some(ct) = content_type {
        out.push_str(&format!("req.setHeader(\"Content-Type\", {ct});\n"));
    }
    if let Some(expr) = write_expr {
        out.push_str(&format!("req.write({expr});\n"));
    }
    out.push_str("req.end();\n");
    out
}

// ---------- Python ----------

fn py_lit(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True".into() } else { "False".into() },
        Value::Number(n) => n.to_string(),
        Value::String(s) => lit(s),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(py_lit).collect::<Vec<_>>().join(", ")
        ),
        Value::Object(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(k, v)| format!("{}: {}", lit(k), py_lit(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn python_snippet(doc: &RequestDoc) -> String {
    let p = plan(doc);
    let mut headers = p.headers.clone();
    let mut auth_kwargs: Option<String> = None;
    if let Some(auth) = doc.auth.as_ref().filter(|a| a.auth_type == AuthType::Basic) {
        let user = auth.username.clone().unwrap_or_default();
        let pass = auth.password.clone().unwrap_or_default();
        auth_kwargs = Some(format!("auth=({}, {})", lit(&user), lit(&pass)));
        headers.retain(|(k, v)| !(k == "Authorization" && v.starts_with("Basic ")));
    }

    let mut out = String::from("import requests\n\n");
    out.push_str(&format!("url = {}\n", lit(&p.url)));
    if !headers.is_empty() {
        let entries: Vec<String> = headers
            .iter()
            .map(|(k, v)| format!("    {}: {},", lit(k), lit(v)))
            .collect();
        out.push_str(&format!("headers = {{\n{}\n}}\n", entries.join("\n")));
    }

    let mut kwargs: Vec<String> = Vec::new();
    if !headers.is_empty() {
        kwargs.push("headers=headers".into());
    }
    if let Some(auth) = auth_kwargs {
        kwargs.push(auth);
    }

    if let Some(body) = &doc.request.body {
        match body.body_type {
            BodyType::None => {}
            BodyType::Json => match parsed_body(body) {
                Some(v) => kwargs.push(format!("json={}", py_lit(&v))),
                None => kwargs.push(format!("data={}", lit(&raw_json(body)))),
            },
            BodyType::Graphql => {
                kwargs.push(format!("json={}", py_lit(&graphql_payload(body))))
            }
            BodyType::Text | BodyType::Xml => {
                kwargs.push(format!("data={}", lit(&raw_json(body))))
            }
            BodyType::FormUrlencoded => {
                let entries: Vec<String> = enabled_items(body)
                    .into_iter()
                    .map(|kv| format!("{}: {}", lit(&kv.name), lit(&kv.value)))
                    .collect();
                kwargs.push(format!("data={{{}}}", entries.join(", ")));
            }
            BodyType::Multipart => {
                let entries: Vec<String> = enabled_items(body)
                    .into_iter()
                    .map(|kv| {
                        if kv.kind.as_deref() == Some("file") {
                            format!(
                                "{}: open({}, \"rb\")",
                                lit(&kv.name),
                                lit(&kv.value)
                            )
                        } else {
                            format!("{}: (None, {})", lit(&kv.name), lit(&kv.value))
                        }
                    })
                    .collect();
                kwargs.push(format!("files={{{}}}", entries.join(", ")));
            }
            BodyType::Binary => kwargs.push(format!(
                "data=open({}, \"rb\").read()",
                lit(body.path.as_deref().unwrap_or(""))
            )),
        }
    }

    let call = if kwargs.is_empty() {
        format!("response = requests.request({}, url)", lit(&p.method))
    } else {
        format!(
            "response = requests.request({}, url, {})",
            lit(&p.method),
            kwargs.join(", ")
        )
    };
    out.push('\n');
    out.push_str(&call);
    out.push_str("\nprint(response.text)\n");
    out
}

// ---------- HTTPie ----------

fn httpie_snippet(doc: &RequestDoc) -> String {
    let p = plan(doc);
    // `--ignore-stdin` keeps the command from hanging on (or mixing with) a
    // non-TTY stdin when the snippet is run from a script or a pipe.
    let mut args: Vec<String> = vec!["http --ignore-stdin".into()];
    let mut pipe_prefix = String::new();

    let mut body = doc.request.body.as_ref();
    // Bodies that must be piped (raw text/xml, opaque JSON) bypass field mode.
    if let Some(b) = body {
        let needs_pipe = match b.body_type {
            BodyType::Text | BodyType::Xml => true,
            BodyType::Json => !matches!(parsed_body(b), Some(Value::Object(_))),
            _ => false,
        };
        if needs_pipe {
            pipe_prefix = format!("echo {} | ", lit(&raw_json(b)));
            body = None;
        }
    }

    if let Some(b) = body {
        match b.body_type {
            BodyType::FormUrlencoded => args.push("--form".into()),
            BodyType::Multipart | BodyType::Binary => args.push("--multipart".into()),
            _ => {}
        }
    }
    args.push(p.method.clone());
    args.push(lit(&p.url));
    for (k, v) in &p.headers {
        args.push(format!("'{k}:{v}'"));
    }

    if let Some(b) = body {
        match b.body_type {
            BodyType::Json | BodyType::Graphql => {
                let value = if b.body_type == BodyType::Graphql {
                    graphql_payload(b)
                } else {
                    parsed_body(b).unwrap_or(Value::Null)
                };
                if let Some(map) = value.as_object() {
                    for (k, v) in map {
                        let rendered = match v {
                            Value::String(s) => lit(s),
                            other => other.to_string(),
                        };
                        args.push(format!("'{k}:={rendered}'"));
                    }
                }
            }
            BodyType::FormUrlencoded => {
                for kv in enabled_items(b) {
                    args.push(format!("'{}={}'", kv.name, kv.value));
                }
            }
            BodyType::Multipart => {
                for kv in enabled_items(b) {
                    if kv.kind.as_deref() == Some("file") {
                        args.push(format!("'{}@{}'", kv.name, kv.value));
                    } else {
                        args.push(format!("'{}={}'", kv.name, kv.value));
                    }
                }
            }
            BodyType::Binary => {
                args.push(format!(
                    "'file@{}'",
                    b.path.clone().unwrap_or_default()
                ));
            }
            _ => {}
        }
    }
    format!("{}{}\n", pipe_prefix, args.join(" "))
}

// ---------- Go ----------

fn go_snippet(doc: &RequestDoc) -> String {
    let p = plan(doc);
    let mut imports: Vec<&str> = vec!["fmt", "io", "net/http"];
    let mut body_expr = "nil".to_string();
    let mut prelude: Vec<String> = Vec::new();
    let mut multipart_ctype = false;

    if let Some(body) = &doc.request.body {
        match body.body_type {
            BodyType::None => {}
            BodyType::Json | BodyType::Text | BodyType::Xml => {
                imports.push("bytes");
                prelude.push(format!("payload := []byte({})", lit(&raw_json(body))));
                body_expr = "bytes.NewReader(payload)".into();
            }
            BodyType::Graphql => {
                imports.push("bytes");
                prelude.push(format!(
                    "payload := []byte({})",
                    lit(&serde_json::to_string(&graphql_payload(body)).expect("json"))
                ));
                body_expr = "bytes.NewReader(payload)".into();
            }
            BodyType::FormUrlencoded => {
                imports.push("bytes");
                let query = enabled_items(body)
                    .into_iter()
                    .map(|kv| format!("{}={}", kv.name, kv.value))
                    .collect::<Vec<_>>()
                    .join("&");
                prelude.push(format!("payload := []byte({})", lit(&query)));
                body_expr = "bytes.NewReader(payload)".into();
            }
            BodyType::Multipart => {
                imports.push("bytes");
                imports.push("mime/multipart");
                if enabled_items(body)
                    .iter()
                    .any(|kv| kv.kind.as_deref() == Some("file"))
                {
                    imports.push("os");
                }
                prelude.push("var buf bytes.Buffer".into());
                prelude.push("writer := multipart.NewWriter(&buf)".into());
                for kv in enabled_items(body) {
                    if kv.kind.as_deref() == Some("file") {
                        prelude.push(format!(
                            "part, err := writer.CreateFormFile({}, {})",
                            lit(&kv.name),
                            lit(&kv.value)
                        ));
                        prelude.push("if err != nil {\n\tpanic(err)\n}".into());
                        prelude.push(format!("fileData, err := os.ReadFile({})", lit(&kv.value)));
                        prelude.push("if err != nil {\n\tpanic(err)\n}".into());
                        prelude.push("_, err = part.Write(fileData)".into());
                        prelude.push("if err != nil {\n\tpanic(err)\n}".into());
                    } else {
                        prelude.push(format!(
                            "if err := writer.WriteField({}, {}); err != nil {{\n\tpanic(err)\n}}",
                            lit(&kv.name),
                            lit(&kv.value)
                        ));
                    }
                }
                prelude.push("if err := writer.Close(); err != nil {\n\tpanic(err)\n}".into());
                prelude.push("contentType := writer.FormDataContentType()".into());
                body_expr = "bytes.NewReader(buf.Bytes())".into();
                multipart_ctype = true;
            }
            BodyType::Binary => {
                imports.push("bytes");
                imports.push("os");
                prelude.push(format!(
                    "fileData, err := os.ReadFile({})",
                    lit(body.path.as_deref().unwrap_or(""))
                ));
                prelude.push("if err != nil {\n\tpanic(err)\n}".into());
                prelude.push("payload := fileData".into());
                body_expr = "bytes.NewReader(payload)".into();
            }
        }
    }
    imports.sort();
    imports.dedup();

    let mut out = String::from("package main\n\nimport (\n");
    for i in &imports {
        out.push_str(&format!("\t\"{i}\"\n"));
    }
    out.push_str(")\n\nfunc main() {\n");
    for line in &prelude {
        // Multi-line entries (error checks) carry their own relative indent;
        // prefix every line so the block stays inside `main`.
        for part in line.split('\n') {
            out.push_str(&format!("\t{part}\n"));
        }
    }
    out.push_str(&format!(
        "\treq, err := http.NewRequest({}, {}, {body_expr})\n\tif err != nil {{\n\t\tpanic(err)\n\t}}\n",
        lit(&p.method),
        lit(&p.url)
    ));
    for (k, v) in &p.headers {
        out.push_str(&format!(
            "\treq.Header.Set({}, {})\n",
            lit(k),
            lit(v)
        ));
    }
    if multipart_ctype {
        out.push_str("\treq.Header.Set(\"Content-Type\", contentType)\n");
    }
    out.push_str(
        "\tresp, err := http.DefaultClient.Do(req)\n\tif err != nil {\n\t\tpanic(err)\n\t}\n\tdefer resp.Body.Close()\n\tbody, _ := io.ReadAll(resp.Body)\n\tfmt.Println(string(body))\n}\n",
    );
    out
}

// ---------- Java (OkHttp) ----------

/// A valid Java class name derived from the request name.
fn java_class_name(doc: &RequestDoc) -> String {
    let cleaned: String = doc
        .name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if cleaned.is_empty() || cleaned.starts_with(|c: char| c.is_ascii_digit()) {
        return format!("Keel{cleaned}");
    }
    cleaned
}

fn java_snippet(doc: &RequestDoc) -> String {
    let p = plan(doc);
    let mut imports: Vec<&str> = vec![
        "okhttp3.OkHttpClient",
        "okhttp3.Request",
        "okhttp3.Response",
    ];

    let mut body_decl = String::new();
    let call = p.method.clone();
    if let Some(body) = &doc.request.body {
        let (content_type, content) = match body.body_type {
            BodyType::None => (None, None),
            BodyType::Json | BodyType::Graphql => (
                Some("application/json"),
                Some(if body.body_type == BodyType::Graphql {
                    serde_json::to_string(&graphql_payload(body)).expect("json")
                } else {
                    raw_json(body)
                }),
            ),
            BodyType::Text => (Some("text/plain"), Some(raw_json(body))),
            BodyType::Xml => (Some("application/xml"), Some(raw_json(body))),
            BodyType::FormUrlencoded => (
                Some("application/x-www-form-urlencoded"),
                Some(
                    enabled_items(body)
                        .into_iter()
                        .map(|kv| format!("{}={}", kv.name, kv.value))
                        .collect::<Vec<_>>()
                        .join("&"),
                ),
            ),
            BodyType::Multipart => (None, None),
            BodyType::Binary => (Some("application/octet-stream"), None),
        };
        if body.body_type == BodyType::Multipart {
            imports.push("okhttp3.MultipartBody");
            imports.push("okhttp3.RequestBody");
            if enabled_items(body)
                .iter()
                .any(|kv| kv.kind.as_deref() == Some("file"))
            {
                imports.push("okhttp3.MediaType");
                imports.push("java.io.File");
            }
            let mut builder =
                "RequestBody body = new MultipartBody.Builder().setType(MultipartBody.FORM)\n"
                    .to_string();
            for kv in enabled_items(body) {
                if kv.kind.as_deref() == Some("file") {
                    builder.push_str(&format!(
                        "    .addFormDataPart({}, {}, RequestBody.create(MediaType.parse(\"application/octet-stream\"), new File({})))\n",
                        lit(&kv.name),
                        lit(kv.value.rsplit(['/', '\\']).next().unwrap_or("file")),
                        lit(&kv.value)
                    ));
                } else {
                    builder.push_str(&format!(
                        "    .addFormDataPart({}, {})\n",
                        lit(&kv.name),
                        lit(&kv.value)
                    ));
                }
            }
            builder.push_str("    .build();");
            body_decl.push_str(&builder);
            body_decl.push('\n');
        } else if let Some(ct) = content_type {
            imports.push("okhttp3.RequestBody");
            imports.push("okhttp3.MediaType");
            let java_body = if body.body_type == BodyType::Binary {
                imports.push("java.nio.file.Files");
                imports.push("java.nio.file.Paths");
                format!(
                    "RequestBody body = RequestBody.create(MediaType.parse(\"{ct}\"), Files.readAllBytes(Paths.get({})));",
                    lit(body.path.as_deref().unwrap_or(""))
                )
            } else {
                format!(
                    "RequestBody body = RequestBody.create(MediaType.parse(\"{ct}\"), {});",
                    lit(content.as_deref().unwrap_or(""))
                )
            };
            body_decl.push_str(&java_body);
            body_decl.push('\n');
        }
    }
    imports.sort();
    imports.dedup();

    let mut stmts = String::from("OkHttpClient client = new OkHttpClient();\n\n");
    stmts.push_str(&body_decl);
    if !body_decl.is_empty() {
        stmts.push('\n');
    }
    stmts.push_str("Request request = new Request.Builder()\n");
    stmts.push_str(&format!("    .url({})\n", lit(&p.url)));
    for (k, v) in &p.headers {
        stmts.push_str(&format!("    .addHeader({}, {})\n", lit(k), lit(v)));
    }
    let has_body = !body_decl.trim().is_empty();
    if has_body {
        stmts.push_str(&format!("    .method({}, body)\n", lit(&call)));
    } else if call == "GET" {
        stmts.push_str("    .get()\n");
    } else if matches!(call.as_str(), "POST" | "PUT" | "PATCH") {
        // OkHttp rejects .method("POST", null) — send an empty body instead.
        imports.push("okhttp3.RequestBody");
        imports.sort();
        imports.dedup();
        stmts.push_str(&format!(
            "    .method({}, RequestBody.create(null, new byte[0]))\n",
            lit(&call)
        ));
    } else {
        stmts.push_str(&format!("    .method({}, null)\n", lit(&call)));
    }
    stmts.push_str("    .build();\n\n");
    stmts.push_str(
        "try (Response response = client.newCall(request).execute()) {\n    System.out.println(response.body().string());\n}",
    );

    let class_name = java_class_name(doc);
    let mut out = String::new();
    for imp in &imports {
        out.push_str(&format!("import {imp};\n"));
    }
    out.push('\n');
    // Not `public` so the snippet compiles in a file of any name.
    out.push_str(&format!("class {class_name} {{\n"));
    out.push_str("    public static void main(String[] args) throws Exception {\n");
    for line in stmts.lines() {
        if line.is_empty() {
            out.push('\n');
        } else {
            out.push_str(&format!("    {line}\n"));
        }
    }
    out.push_str("    }\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(yaml: &str) -> RequestDoc {
        crate::model::yaml_to(yaml).expect("parse")
    }

    const FULL: &str = r#"
schemaVersion: "1"
name: Create User
request:
  method: POST
  url: "{{baseUrl}}/users"
  params:
    - name: notify
      value: "true"
      enabled: true
  headers:
    - name: Accept
      value: application/json
      enabled: true
  body:
    type: json
    content: '{"name": "Ada", "team": "{{teamId}}"}'
auth:
  type: bearer
  token: "{{accessToken}}"
"#;

    #[test]
    fn curl_delegates_to_export_curl() {
        let d = doc(FULL);
        assert_eq!(
            generate_code(&d, "curl").expect("curl"),
            crate::export_curl::request_to_curl(&d)
        );
    }

    #[test]
    fn unknown_target_errors() {
        let d = doc(FULL);
        assert!(generate_code(&d, "brainfuck").is_err());
    }

    #[test]
    fn fetch_includes_method_url_headers_body_and_auth() {
        let s = generate_code(&doc(FULL), "fetch").expect("fetch");
        assert!(s.contains("await fetch(\"{{baseUrl}}/users?notify=true\""), "{s}");
        assert!(s.contains("method: \"POST\""));
        assert!(s.contains("\"Accept\": \"application/json\""));
        assert!(s.contains("\"Authorization\": \"Bearer {{accessToken}}\""));
        assert!(s.contains(r#""Content-Type": "application/json""#));
        assert!(s.contains(r#"body: "{\"name\": \"Ada\", \"team\": \"{{teamId}}\"}""#), "{s}");
    }

    #[test]
    fn fetch_multipart_and_binary_notes() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test/u"
  body:
    type: multipart
    items:
      - name: note
        value: hello
      - name: file
        value: ./doc.pdf
        kind: file
"#,
        );
        let s = generate_code(&d, "fetch").expect("fetch");
        assert!(s.contains("const formData = new FormData();"), "{s}");
        assert!(s.contains("formData.append(\"note\", \"hello\");"));
        assert!(
            s.contains("formData.append(\"file\", new Blob([await readFile(\"./doc.pdf\")]), \"doc.pdf\");"),
            "{s}"
        );

        let b = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test/u"
  body:
    type: binary
    path: ./blob.bin
"#,
        );
        let s = generate_code(&b, "fetch").expect("fetch binary");
        assert!(s.contains("const fileData = await readFile(\"./blob.bin\");"), "{s}");
        assert!(s.contains("body: fileData"), "{s}");
    }

    #[test]
    fn axios_uses_data_and_lowercase_method() {
        let s = generate_code(&doc(FULL), "axios").expect("axios");
        assert!(s.contains("method: \"post\""), "{s}");
        assert!(s.contains("url: \"{{baseUrl}}/users?notify=true\""));
        assert!(s.contains("data: "));
        assert!(s.contains("import axios from \"axios\""));
    }

    #[test]
    fn python_dict_bodies_and_basic_auth_tuple() {
        let s = generate_code(&doc(FULL), "python-requests").expect("python");
        assert!(s.contains("import requests"), "{s}");
        assert!(s.contains("requests.request(\"POST\", url,"));
        assert!(s.contains(r#"json={"name": "Ada", "team": "{{teamId}}"}"#), "{s}");
        assert!(s.contains(r#""Authorization": "Bearer {{accessToken}}""#));

        let basic = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: GET
  url: "https://x.test"
auth:
  type: basic
  username: ada
  password: "p@ss"
"#,
        );
        let s = generate_code(&basic, "python-requests").expect("python basic");
        assert!(s.contains("auth=(\"ada\", \"p@ss\")"), "{s}");
        assert!(!s.contains("Basic "), "no duplicated precomputed header: {s}");
    }

    #[test]
    fn python_multipart_uses_files() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test/u"
  body:
    type: multipart
    items:
      - name: note
        value: hello
      - name: file
        value: ./doc.pdf
        kind: file
"#,
        );
        let s = generate_code(&d, "python-requests").expect("python multipart");
        assert!(s.contains("files={"), "{s}");
        assert!(s.contains("(None, \"hello\")"));
        assert!(s.contains("open(\"./doc.pdf\", \"rb\")"));
    }

    #[test]
    fn httpie_variants() {
        let get = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: GET
  url: "https://x.test/items"
  params:
    - name: q
      value: "a"
"#,
        );
        let s = generate_code(&get, "httpie").expect("httpie");
        assert!(
            s.contains("http --ignore-stdin GET \"https://x.test/items?q=a\""),
            "{s}"
        );

        let form = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test/u"
  body:
    type: form-urlencoded
    items:
      - name: a
        value: "1"
"#,
        );
        let s = generate_code(&form, "httpie").expect("httpie form");
        assert!(s.contains("--form"), "{s}");
        assert!(s.contains("'a=1'"));

        let multi = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test/u"
  body:
    type: multipart
    items:
      - name: file
        value: ./doc.pdf
        kind: file
"#,
        );
        let s = generate_code(&multi, "httpie").expect("httpie multipart");
        assert!(s.contains("--multipart"), "{s}");
        assert!(s.contains("'file@./doc.pdf'"));
    }

    #[test]
    fn native_node_multipart_encodes_parts_by_hand() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test/u"
  body:
    type: multipart
    items:
      - name: note
        value: hello
      - name: file
        value: ./doc.pdf
        kind: file
"#,
        );
        let s = generate_code(&d, "native-node").expect("node multipart");
        assert!(!s.contains("TODO"), "no TODO left: {s}");
        assert!(!s.contains("FormData"), "req.write rejects FormData: {s}");
        assert!(s.contains("const fs = require(\"node:fs\");"), "{s}");
        assert!(s.contains("const boundary = "), "{s}");
        assert!(
            s.contains("Content-Disposition: form-data; name=\" + \"note\" + \"\\r\\n\\r\\n\""),
            "{s}"
        );
        assert!(s.contains("\"hello\""), "{s}");
        assert!(
            s.contains("Content-Disposition: form-data; name=\" + \"file\" + \"; filename=\" + \"doc.pdf\""),
            "{s}"
        );
        assert!(s.contains("fs.readFileSync(\"./doc.pdf\")"), "{s}");
        assert!(s.contains("const payload = Buffer.concat(parts);"), "{s}");
        assert!(s.contains("req.write(payload);"), "{s}");
        assert!(
            s.contains("req.setHeader(\"Content-Type\", \"multipart/form-data; boundary=\" + boundary);"),
            "{s}"
        );
    }

    #[test]
    fn native_node_form_urlencoded_writes_a_query_string() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test/u"
  body:
    type: form-urlencoded
    items:
      - name: note
        value: hello
"#,
        );
        let s = generate_code(&d, "native-node").expect("node form");
        assert!(!s.contains("URLSearchParams"), "req.write rejects it: {s}");
        assert!(s.contains("const payload = \"note=hello\";"), "{s}");
        assert!(s.contains("req.write(payload);"), "{s}");
    }

    #[test]
    fn native_node_binary_reads_the_file_synchronously() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test/u"
  body:
    type: binary
    path: ./blob.bin
"#,
        );
        let s = generate_code(&d, "native-node").expect("node binary");
        assert!(s.contains("const fs = require(\"node:fs\");"), "{s}");
        assert!(s.contains("const payload = fs.readFileSync(\"./blob.bin\");"), "{s}");
        assert!(!s.contains("import "), "no ESM import in a CJS snippet: {s}");
        assert!(s.contains("req.write(payload);"), "{s}");
    }

    #[test]
    fn native_node_http_module_and_body() {
        let s = generate_code(&doc(FULL), "native-node").expect("node");
        assert!(s.contains("require(\"node:https\")"), "{s}");
        assert!(s.contains("https.request(\"{{baseUrl}}/users?notify=true\""));
        assert!(s.contains("const payload = "));
        assert!(s.contains("req.write(payload);"));
        assert!(s.contains("req.end();"));

        let http_get = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: GET
  url: "http://x.test/ping"
"#,
        );
        let s = generate_code(&http_get, "native-node").expect("node http");
        assert!(s.contains("require(\"node:http\")"), "{s}");
        assert!(!s.contains("req.write"), "no body");
    }

    #[test]
    fn go_http_structure() {
        let s = generate_code(&doc(FULL), "go-http").expect("go");
        assert!(s.contains("package main"), "{s}");
        assert!(s.contains("\"net/http\""));
        assert!(s.contains("\"bytes\""));
        assert!(s.contains("http.NewRequest(\"POST\", \"{{baseUrl}}/users?notify=true\", bytes.NewReader(payload))"));
        assert!(s.contains("req.Header.Set(\"Accept\", \"application/json\")"));
        assert!(s.contains("defer resp.Body.Close()"));
        // imports sorted & deduped
        assert_eq!(s.matches("\"fmt\"").count(), 1);
    }

    #[test]
    fn java_post_without_a_body_sends_an_empty_body() {
        // OkHttp's Request.Builder.method("POST", null) throws
        // IllegalArgumentException, so body-requiring methods get an empty body.
        for method in ["POST", "PUT", "PATCH"] {
            let d = doc(&format!(
                "schemaVersion: \"1\"\nname: t\nrequest:\n  method: {method}\n  url: \"http://x.test/u\"\n"
            ));
            let s = generate_code(&d, "java-okhttp").expect("java");
            assert!(
                s.contains(&format!(
                    "        .method(\"{method}\", RequestBody.create(null, new byte[0]))"
                )),
                "{method}: {s}"
            );
            assert!(s.contains("import okhttp3.RequestBody;"), "{method}: {s}");
        }

        // GET and DELETE are fine with a null body.
        for method in ["GET", "DELETE"] {
            let d = doc(&format!(
                "schemaVersion: \"1\"\nname: t\nrequest:\n  method: {method}\n  url: \"http://x.test/u\"\n"
            ));
            let s = generate_code(&d, "java-okhttp").expect("java");
            assert!(!s.contains("new byte[0]"), "{method}: {s}");
        }
    }

    #[test]
    fn java_okhttp_structure() {
        let s = generate_code(&doc(FULL), "java-okhttp").expect("java");
        // compilable unit: imports, class wrapper, main
        assert!(s.contains("import okhttp3.OkHttpClient;"), "{s}");
        assert!(s.contains("import okhttp3.Request;"), "{s}");
        assert!(s.contains("import okhttp3.Response;"), "{s}");
        assert!(s.contains("import okhttp3.RequestBody;"), "{s}");
        assert!(s.contains("import okhttp3.MediaType;"), "{s}");
        assert!(s.contains("class CreateUser {"), "{s}");
        assert!(s.contains("public static void main(String[] args) throws Exception {"), "{s}");
        assert!(s.contains("    OkHttpClient client = new OkHttpClient();"), "{s}");
        assert!(s.contains("        System.out.println(response.body().string());"), "{s}");
        assert!(s.contains("new OkHttpClient()"), "{s}");
        assert!(s.contains("RequestBody.create(MediaType.parse(\"application/json\")"));
        assert!(s.contains("        .url(\"{{baseUrl}}/users?notify=true\")"), "{s}");
        assert!(s.contains("        .method(\"POST\", body)"));
        assert!(s.contains("        .addHeader(\"Accept\", \"application/json\")"));
        assert!(s.contains("client.newCall(request).execute()"));

        let http_get = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: GET
  url: "http://x.test/ping"
"#,
        );
        let s = generate_code(&http_get, "java-okhttp").expect("java http");
        assert!(s.contains("class t {"), "{s}");
        assert!(s.contains("        .get()"), "{s}");
        // a GET with no body needs no body imports
        assert!(!s.contains("import okhttp3.RequestBody;"), "{s}");
        assert!(!s.contains("import okhttp3.MediaType;"), "{s}");
    }

    #[test]
    fn java_class_name_is_a_valid_identifier() {
        for (name, expected) in [
            ("Create User", "CreateUser"),
            ("123 stats", "Keel123stats"),
            ("", "Keel"),
            ("a-b c.d", "abcd"),
        ] {
            let d = doc(&format!(
                "schemaVersion: \"1\"\nname: \"{name}\"\nrequest:\n  method: GET\n  url: \"http://x.test\"\n"
            ));
            let s = generate_code(&d, "java-okhttp").expect("java");
            assert!(s.contains(&format!("class {expected} {{")), "{name}: {s}");
        }
    }

    #[test]
    fn java_multipart_imports_match_the_parts() {
        let with_file = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "http://x.test/u"
  body:
    type: multipart
    items:
      - name: note
        value: hello
      - name: file
        value: doc.pdf
        kind: file
"#,
        );
        let s = generate_code(&with_file, "java-okhttp").expect("java");
        assert!(s.contains("import okhttp3.MultipartBody;"), "{s}");
        assert!(s.contains("import okhttp3.MediaType;"), "{s}");
        assert!(s.contains("import java.io.File;"), "{s}");
        assert!(
            s.contains("        .addFormDataPart(\"file\", \"doc.pdf\", RequestBody.create("),
            "{s}"
        );

        let text_only = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "http://x.test/u"
  body:
    type: multipart
    items:
      - name: note
        value: hello
"#,
        );
        let s = generate_code(&text_only, "java-okhttp").expect("java");
        assert!(s.contains("import okhttp3.MultipartBody;"), "{s}");
        // no file parts -> no File or MediaType needed
        assert!(!s.contains("import java.io.File;"), "{s}");
        assert!(!s.contains("import okhttp3.MediaType;"), "{s}");
    }

    #[test]
    fn java_binary_imports_nio() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "http://x.test/u"
  body:
    type: binary
    path: blob.bin
"#,
        );
        let s = generate_code(&d, "java-okhttp").expect("java");
        assert!(s.contains("import java.nio.file.Files;"), "{s}");
        assert!(s.contains("import java.nio.file.Paths;"), "{s}");
        assert!(s.contains("Files.readAllBytes(Paths.get(\"blob.bin\"))"), "{s}");
    }

    #[test]
    fn graphql_bodies_emit_query_variables_payload() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: POST
  url: "https://x.test/graphql"
  body:
    type: graphql
    query: "{ me { name } }"
    variables: '{"id": "{{userId}}"}'
"#,
        );
        for target in ["fetch", "python-requests", "go-http", "java-okhttp"] {
            let s = generate_code(&d, target).expect(target);
            assert!(s.contains("\\\"query\\\"") || s.contains("\"query\""), "{target}: {s}");
            assert!(s.contains("me { name }"), "{target}: {s}");
        }
    }

    #[test]
    fn apikey_query_location_appended_to_url() {
        let d = doc(
            r#"
schemaVersion: "1"
name: t
request:
  method: GET
  url: "https://x.test/s"
auth:
  type: apikey
  key: X-Api-Key
  value: "{{apiKey}}"
  in: query
"#,
        );
        let s = generate_code(&d, "fetch").expect("apikey fetch");
        assert!(s.contains("https://x.test/s?X-Api-Key={{apiKey}}"), "{s}");
        assert!(!s.contains("X-Api-Key\""), "not in headers: {s}");
    }
}
