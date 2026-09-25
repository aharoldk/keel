//! gRPC over HTTP/2. The request is a JSON object; field numbers and types
//! come from a minimal proto3 service definition (or an explicit field map
//! when no proto is supplied). Unary and server-streaming RPCs are supported;
//! client-streaming and bidi are not.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex as StdMutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtoField {
    pub name: String,
    pub number: u32,
    /// proto3 type: string, bytes, bool, int32, int64, uint32, uint64,
    /// sint32, sint64, float, double, or message.
    pub type_name: String,
    #[serde(default)]
    pub repeated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtoMessage {
    pub name: String,
    pub fields: Vec<ProtoField>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtoMethod {
    pub name: String,
    pub input: String,
    pub output: String,
    #[serde(default)]
    pub client_streaming: bool,
    #[serde(default)]
    pub server_streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtoService {
    pub name: String,
    pub package: String,
    pub methods: Vec<ProtoMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtoFile {
    pub package: String,
    pub services: Vec<ProtoService>,
    pub messages: Vec<ProtoMessage>,
}

/// Parses a small proto3 subset: package, message, service, rpc.
/// Nested messages, imports, options and oneofs are skipped.
pub fn parse_proto(source: &str) -> Result<ProtoFile, String> {
    let cleaned = strip_comments(source);
    let mut package = String::new();
    let mut messages = Vec::new();
    let mut services = Vec::new();
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        skip_ws(bytes, &mut i);
        if i >= bytes.len() {
            break;
        }
        let word = read_ident(bytes, &mut i);
        match word.as_str() {
            "package" => {
                skip_ws(bytes, &mut i);
                package = read_dotted(bytes, &mut i);
                expect_semi(bytes, &mut i)?;
            }
            "message" => {
                skip_ws(bytes, &mut i);
                let name = read_ident(bytes, &mut i);
                if name.is_empty() {
                    return Err("message is missing a name".into());
                }
                let body = read_block(bytes, &mut i)?;
                messages.push(parse_message(&name, &body)?);
            }
            "service" => {
                skip_ws(bytes, &mut i);
                let name = read_ident(bytes, &mut i);
                if name.is_empty() {
                    return Err("service is missing a name".into());
                }
                let body = read_block(bytes, &mut i)?;
                services.push(parse_service(&package, &name, &body)?);
            }
            "syntax" | "option" | "import" | "enum" => {
                if word == "enum" {
                    let _ = read_block(bytes, &mut i);
                } else {
                    skip_statement(bytes, &mut i);
                }
            }
            "" => break,
            other => return Err(format!("unsupported proto token `{other}`")),
        }
    }
    Ok(ProtoFile { package, services, messages })
}

fn parse_message(name: &str, body: &str) -> Result<ProtoMessage, String> {
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut fields = Vec::new();
    while i < bytes.len() {
        skip_ws(bytes, &mut i);
        if i >= bytes.len() {
            break;
        }
        let mut repeated = false;
        let mut type_name = read_ident(bytes, &mut i);
        if type_name == "repeated" {
            repeated = true;
            skip_ws(bytes, &mut i);
            type_name = read_ident(bytes, &mut i);
        } else if type_name == "optional" || type_name == "required" {
            skip_ws(bytes, &mut i);
            type_name = read_ident(bytes, &mut i);
        }
        if type_name == "reserved" || type_name == "option" || type_name == "extensions" {
            skip_statement(bytes, &mut i);
            continue;
        }
        if type_name == "message" || type_name == "enum" || type_name == "oneof" {
            let _ = read_block(bytes, &mut i);
            continue;
        }
        if type_name.is_empty() {
            break;
        }
        skip_ws(bytes, &mut i);
        let field_name = read_ident(bytes, &mut i);
        skip_ws(bytes, &mut i);
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
        }
        skip_ws(bytes, &mut i);
        let number = read_number(bytes, &mut i)?;
        skip_statement(bytes, &mut i);
        fields.push(ProtoField {
            name: field_name,
            number,
            type_name,
            repeated,
        });
    }
    Ok(ProtoMessage { name: name.to_string(), fields })
}

fn parse_service(package: &str, name: &str, body: &str) -> Result<ProtoService, String> {
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut methods = Vec::new();
    while i < bytes.len() {
        skip_ws(bytes, &mut i);
        if i >= bytes.len() {
            break;
        }
        let word = read_ident(bytes, &mut i);
        if word != "rpc" {
            if word == "option" {
                skip_statement(bytes, &mut i);
                continue;
            }
            if word.is_empty() {
                break;
            }
            return Err(format!("expected rpc in service `{name}`, got `{word}`"));
        }
        skip_ws(bytes, &mut i);
        let method = read_ident(bytes, &mut i);
        skip_ws(bytes, &mut i);
        let (input, client_streaming) = read_rpc_type(bytes, &mut i)?;
        skip_ws(bytes, &mut i);
        let returns = read_ident(bytes, &mut i);
        if returns != "returns" {
            return Err(format!("rpc `{method}` is missing returns"));
        }
        skip_ws(bytes, &mut i);
        let (output, server_streaming) = read_rpc_type(bytes, &mut i)?;
        skip_ws(bytes, &mut i);
        if i < bytes.len() && bytes[i] == b'{' {
            let _ = read_block(bytes, &mut i);
        } else {
            expect_semi(bytes, &mut i)?;
        }
        methods.push(ProtoMethod {
            name: method,
            input,
            output,
            client_streaming,
            server_streaming,
        });
    }
    Ok(ProtoService {
        name: name.to_string(),
        package: package.to_string(),
        methods,
    })
}

fn read_rpc_type(bytes: &[u8], i: &mut usize) -> Result<(String, bool), String> {
    if *i >= bytes.len() || bytes[*i] != b'(' {
        return Err("expected `(` after rpc name".into());
    }
    *i += 1;
    skip_ws(bytes, i);
    let mut streaming = false;
    let mut name = read_ident(bytes, i);
    if name == "stream" {
        streaming = true;
        skip_ws(bytes, i);
        name = read_ident(bytes, i);
    }
    skip_ws(bytes, i);
    if *i < bytes.len() && bytes[*i] == b')' {
        *i += 1;
    }
    if name.is_empty() {
        return Err("rpc type is missing".into());
    }
    Ok((name, streaming))
}

/// Encodes a JSON object as a protobuf binary message.
pub fn encode_message(json: &Value, fields: &[ProtoField], messages: &[ProtoMessage]) -> Result<Vec<u8>, String> {
    let obj = json.as_object().ok_or_else(|| "gRPC body must be a JSON object".to_string())?;
    let mut out = Vec::new();
    for field in fields {
        let Some(value) = obj.get(&field.name) else { continue };
        if value.is_null() {
            continue;
        }
        if field.repeated {
            let items = value.as_array().ok_or_else(|| format!("`{}` must be an array", field.name))?;
            for item in items {
                write_field(&mut out, field, item, messages)?;
            }
        } else {
            write_field(&mut out, field, value, messages)?;
        }
    }
    Ok(out)
}

fn write_field(out: &mut Vec<u8>, field: &ProtoField, value: &Value, messages: &[ProtoMessage]) -> Result<(), String> {
    let wire = wire_type(&field.type_name);
    write_key(out, field.number, wire);
    match field.type_name.as_str() {
        "string" => {
            let s = json_string(value, &field.name)?;
            write_varint(out, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        "bytes" => {
            let s = json_string(value, &field.name)?;
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(s.as_bytes())
                .unwrap_or_else(|_| s.into_bytes());
            write_varint(out, bytes.len() as u64);
            out.extend_from_slice(&bytes);
        }
        "bool" => out.push(if value.as_bool().unwrap_or(false) { 1 } else { 0 }),
        "int32" | "int64" | "uint32" | "uint64" => {
            write_varint(out, json_u64(value, &field.name)?);
        }
        "sint32" | "sint64" => {
            let n = json_i64(value, &field.name)?;
            write_varint(out, ((n << 1) ^ (n >> 63)) as u64);
        }
        "float" => {
            let n = value.as_f64().ok_or_else(|| format!("`{}` must be a number", field.name))? as f32;
            out.extend_from_slice(&n.to_le_bytes());
        }
        "double" => {
            let n = value.as_f64().ok_or_else(|| format!("`{}` must be a number", field.name))?;
            out.extend_from_slice(&n.to_le_bytes());
        }
        _ => {
            let nested = messages
                .iter()
                .find(|m| m.name == field.type_name)
                .ok_or_else(|| format!("unknown message type `{}`", field.type_name))?;
            let bytes = encode_message(value, &nested.fields, messages)?;
            write_varint(out, bytes.len() as u64);
            out.extend_from_slice(&bytes);
        }
    }
    Ok(())
}

/// Decodes a protobuf binary message into JSON using the same field list.
pub fn decode_message(mut bytes: &[u8], fields: &[ProtoField], messages: &[ProtoMessage]) -> Result<Value, String> {
    let by_number: BTreeMap<u32, &ProtoField> = fields.iter().map(|f| (f.number, f)).collect();
    let mut obj = serde_json::Map::new();
    while !bytes.is_empty() {
        let key = read_varint(&mut bytes)?;
        let number = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        let Some(field) = by_number.get(&number).copied() else {
            skip_wire(&mut bytes, wire)?;
            continue;
        };
        let value = read_value(&mut bytes, wire, field, messages)?;
        if field.repeated {
            let entry = obj.entry(field.name.clone()).or_insert_with(|| Value::Array(Vec::new()));
            if let Some(arr) = entry.as_array_mut() {
                arr.push(value);
            }
        } else {
            obj.insert(field.name.clone(), value);
        }
    }
    Ok(Value::Object(obj))
}

fn read_value(bytes: &mut &[u8], wire: u8, field: &ProtoField, messages: &[ProtoMessage]) -> Result<Value, String> {
    match field.type_name.as_str() {
        "string" => Ok(Value::String(String::from_utf8_lossy(&read_len(bytes)?).into_owned())),
        "bytes" => {
            use base64::Engine as _;
            Ok(Value::String(base64::engine::general_purpose::STANDARD.encode(read_len(bytes)?)))
        }
        "bool" => Ok(Value::Bool(read_varint(bytes)? != 0)),
        "int32" | "int64" | "uint32" | "uint64" => Ok(json_from_u64(read_varint(bytes)?)),
        "sint32" | "sint64" => {
            let n = read_varint(bytes)?;
            let decoded = ((n >> 1) as i64) ^ -((n & 1) as i64);
            Ok(Value::from(decoded))
        }
        "float" => {
            if bytes.len() < 4 {
                return Err("truncated float".into());
            }
            let mut buf = [0u8; 4];
            buf.copy_from_slice(&bytes[..4]);
            *bytes = &bytes[4..];
            Ok(Value::from(f32::from_le_bytes(buf) as f64))
        }
        "double" => {
            if bytes.len() < 8 {
                return Err("truncated double".into());
            }
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&bytes[..8]);
            *bytes = &bytes[8..];
            Ok(Value::from(f64::from_le_bytes(buf)))
        }
        _ => {
            if wire != 2 {
                return Err(format!("bad wire type for `{}`", field.name));
            }
            let nested_bytes = read_len(bytes)?;
            let nested = messages
                .iter()
                .find(|m| m.name == field.type_name)
                .ok_or_else(|| format!("unknown message type `{}`", field.type_name))?;
            decode_message(&nested_bytes, &nested.fields, messages)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcResult {
    pub status: i64,
    pub grpc_status: i32,
    pub grpc_message: String,
    pub time_ms: f64,
    pub body_text: String,
    pub headers: Vec<(String, String)>,
}

pub struct GrpcCall {
    pub url: String,
    pub service: String,
    pub method: String,
    pub body: Value,
    pub fields: Vec<ProtoField>,
    pub messages: Vec<ProtoMessage>,
    pub headers: Vec<(String, String)>,
    pub timeout_secs: u64,
    pub insecure_tls: bool,
}

pub async fn call_unary(call: GrpcCall) -> Result<GrpcResult, String> {
    if call.service.is_empty() || call.method.is_empty() {
        return Err("gRPC service and method are required".into());
    }
    let started = std::time::Instant::now();
    let timeout_secs = call.timeout_secs.max(1);
    let out = timeout(Duration::from_secs(timeout_secs), exchange(&call))
        .await
        .map_err(|_| format!("gRPC call timed out after {timeout_secs}s"))?
        .map_err(|e| format!("gRPC call: {e}"))?;
    let StreamResult {
        parts,
        body: raw,
        trailers,
    } = out;
    let headers = header_pairs(&parts);
    let status = parts.status.as_u16() as i64;
    let head_has_status = headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("grpc-status"));
    let (trailer_status, trailer_message) = extract_status(trailers.as_ref());
    let grpc_status = if head_has_status {
        header_int(&headers, "grpc-status")
    } else {
        // Real gRPC carries the status in trailers. HTTP 200 without a
        // trailer is *not* OK — the stream ended abnormally.
        trailer_status.unwrap_or(-1)
    };
    let grpc_message = match (head_has_status, trailer_message) {
        (false, Some(m)) => m,
        _ => headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("grpc-message"))
            .map(|(_, v)| v.clone())
            .unwrap_or_default(),
    };
    let body = decode_frame(&raw, &call.fields, &call.messages)?;
    Ok(GrpcResult {
        status,
        grpc_status,
        grpc_message,
        time_ms: started.elapsed().as_secs_f64() * 1000.0,
        body_text: serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".into()),
        headers,
    })
}

// ---------- HTTP/2 transport ----------
//
// reqwest does not expose HTTP/2 trailers, and gRPC carries its status in
// them. This module therefore talks HTTP/2 directly through hyper.

pub(crate) struct StreamResult {
    pub(crate) parts: http::response::Parts,
    pub(crate) body: Vec<u8>,
    pub(crate) trailers: Option<http::HeaderMap>,
}

type GrpcClient = hyper_util::client::legacy::Client<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    http_body_util::Full<bytes::Bytes>,
>;

fn target(url: &str) -> Result<(String, u16, String, bool), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("invalid url: {e}"))?;
    let secure = match parsed.scheme() {
        "http" => false,
        "https" => true,
        other => return Err(format!("gRPC url must use http or https, got `{other}`")),
    };
    let host = parsed
        .host_str()
        .ok_or_else(|| "gRPC url has no host".to_string())?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(if secure { 443 } else { 80 });
    let path = {
        let mut p = parsed.path().to_string();
        if p.is_empty() {
            p = "/".into();
        }
        if let Some(q) = parsed.query() {
            p.push('?');
            p.push_str(q);
        }
        p
    };
    Ok((host, port, path, secure))
}

fn authority(host: &str, port: u16, secure: bool) -> String {
    if port == if secure { 443 } else { 80 } {
        host.to_string()
    } else {
        format!("{host}:{port}")
    }
}

fn full_uri(call: &GrpcCall) -> Result<String, String> {
    let (host, port, base_path, secure) = target(&call.url)?;
    let path = join_url(&base_path, &grpc_path(&call.service, &call.method));
    let scheme = if secure { "https" } else { "http" };
    Ok(format!("{scheme}://{}{}", authority(&host, port, secure), path))
}

fn build_client(call: &GrpcCall) -> Result<GrpcClient, String> {
    let (_, _, _, _) = target(&call.url)?;
    let provider = rustls::crypto::ring::default_provider();
    let builder = rustls::ClientConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .expect("static config");
    #[derive(Debug)]
    struct AcceptAll;
    impl rustls::client::danger::ServerCertVerifier for AcceptAll {
        fn verify_server_cert(
            &self,
            _end_entity: &rustls::pki_types::CertificateDer<'_>,
            _intermediates: &[rustls::pki_types::CertificateDer<'_>],
            _server_name: &rustls::pki_types::ServerName<'_>,
            _ocsp: &[u8],
            _now: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &rustls::pki_types::CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &rustls::pki_types::CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            use rustls::SignatureScheme::*;
            vec![
                RSA_PKCS1_SHA256,
                RSA_PKCS1_SHA384,
                RSA_PKCS1_SHA512,
                ECDSA_NISTP256_SHA256,
                ECDSA_NISTP384_SHA384,
                ECDSA_NISTP521_SHA512,
                ED25519,
                RSA_PSS_SHA256,
                RSA_PSS_SHA384,
                RSA_PSS_SHA512,
            ]
        }
    }
    let tls = if call.insecure_tls {
        builder
            .dangerous()
            .with_custom_certificate_verifier(std::sync::Arc::new(AcceptAll))
            .with_no_client_auth()
    } else {
        builder
            .with_root_certificates(rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            })
            .with_no_client_auth()
    };
    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config(tls)
        .https_or_http()
        .enable_http2()
        .build();
    Ok(
        hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .http2_only(true)
            .build(connector),
    )
}

pub(crate) fn grpc_path(service: &str, method: &str) -> String {
    format!("/{}/{}", service.trim_matches('/'), method.trim_matches('/'))
}

fn request_frame(call: &GrpcCall) -> Result<bytes::Bytes, String> {
    let payload = encode_message(&call.body, &call.fields, &call.messages)?;
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.push(0);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(bytes::Bytes::from(frame))
}

fn build_request(call: &GrpcCall, frame: bytes::Bytes) -> Result<http::Request<http_body_util::Full<bytes::Bytes>>, String> {
    let mut b = http::Request::builder()
        .method(http::Method::POST)
        .uri(full_uri(call)?)
        .header("content-type", "application/grpc")
        .header("te", "trailers")
        .header("grpc-accept-encoding", "identity");
    for (name, value) in &call.headers {
        if name.eq_ignore_ascii_case("content-type") || name.eq_ignore_ascii_case("te") {
            continue;
        }
        if http::header::HeaderName::from_bytes(name.as_bytes()).is_err() {
            continue;
        }
        b = b.header(name.as_str(), value.as_str());
    }
    b.body(http_body_util::Full::new(frame)).map_err(|e| e.to_string())
}

/// Unary exchange: send one frame, collect the whole response and trailers.
async fn exchange(call: &GrpcCall) -> Result<StreamResult, String> {
    let frame = request_frame(call)?;
    let client = build_client(call)?;
    let response = client
        .request(build_request(call, frame)?)
        .await
        .map_err(|e| format!("gRPC call: {e}"))?;
    let (parts, mut body) = response.into_parts();
    use http_body_util::BodyExt as _;
    let mut bytes_out = Vec::new();
    let mut trailers = None;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|e| format!("body: {e}"))?;
        if let Some(data) = frame.data_ref() {
            bytes_out.extend_from_slice(data);
        } else if let Some(t) = frame.trailers_ref() {
            trailers = Some(t.clone());
        }
    }
    Ok(StreamResult {
        parts,
        body: bytes_out,
        trailers,
    })
}

fn extract_status(trailers: Option<&http::HeaderMap>) -> (Option<i32>, Option<String>) {
    let Some(map) = trailers else {
        return (None, None);
    };
    let status = map
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i32>().ok());
    let message = map
        .get("grpc-message")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    (status, message)
}

fn header_pairs(parts: &http::response::Parts) -> Vec<(String, String)> {
    parts
        .headers
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect()
}

/// One streamed event. `kind`: message | trailers | error | end.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcEvent {
    pub session_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grpc_status: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grpc_message: Option<String>,
}

#[derive(Default)]
pub struct GrpcHub {
    sessions: StdMutex<HashMap<String, mpsc::UnboundedSender<()>>>,
}

impl GrpcHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn close(&self, session_id: &str) {
        if let Some(tx) = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(session_id)
        {
            let _ = tx.send(());
        }
    }
}

/// Runs a server-streaming call: one request frame in, many response frames
/// out. Emits `message` per decoded frame, then `trailers` and `end`.
/// When `cancel_rx` fires, the read loop stops without emitting further
/// events (the hub already removed the session).
pub async fn run_stream<F>(call: &GrpcCall, session_id: &str, mut cancel_rx: Option<&mut mpsc::UnboundedReceiver<()>>, emit: F)
where
    F: Fn(GrpcEvent) + Send + Sync,
{
    let sid = session_id.to_string();
    let emit_event = |kind: &str, body_text: Option<String>, grpc_status: Option<i32>, grpc_message: Option<String>| {
        emit(GrpcEvent {
            session_id: sid.clone(),
            kind: kind.into(),
            body_text,
            grpc_status,
            grpc_message,
        })
    };
    let frame = match request_frame(call) {
        Ok(f) => f,
        Err(e) => return emit_event("error", Some(e), None, None),
    };
    let client = match build_client(call) {
        Ok(c) => c,
        Err(e) => return emit_event("error", Some(e), None, None),
    };
    let secs = call.timeout_secs.max(1);
    let stream = async {
        let response = match client.request(build_request(call, frame)?).await {
            Ok(r) => r,
            Err(e) => return Err(format!("gRPC call: {e}")),
        };
        let (head, mut body) = response.into_parts();
        let headers = header_pairs(&head);
        use http_body_util::BodyExt as _;
        // A grpc-status in the *headers* means the call failed before any
        // message arrived.
        if let Some(s) = header_int_opt(&headers, "grpc-status") {
            emit_event(
                "trailers",
                None,
                Some(s),
                header_value_opt(&headers, "grpc-message"),
            );
            emit_event("end", None, Some(s), None);
            return Ok(());
        }
        let mut buf: Vec<u8> = Vec::new();
        let mut trailers: Option<http::HeaderMap> = None;
        loop {
            let frame = if let Some(rx) = cancel_rx.as_deref_mut() {
                tokio::select! {
                    _ = rx.recv() => return Err("cancelled".to_string()),
                    f = body.frame() => f,
                }
            } else {
                body.frame().await
            };
            let Some(frame) = frame else { break };
            let frame = frame.map_err(|e| format!("body: {e}"))?;
            if let Some(data) = frame.data_ref() {
                buf.extend_from_slice(data);
            } else if let Some(t) = frame.trailers_ref() {
                trailers = Some(t.clone());
            }
            while buf.len() >= 5 {
                if buf[0] != 0 {
                    return Err("compressed gRPC responses are not supported".into());
                }
                let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
                if buf.len() < 5 + len {
                    break;
                }
                let message = buf[5..5 + len].to_vec();
                buf.drain(..5 + len);
                if call.fields.is_empty() {
                    emit_event("message", Some(hex_preview(&message)), None, None);
                    continue;
                }
                match decode_message(&message, &call.fields, &call.messages) {
                    Ok(v) => emit_event(
                        "message",
                        Some(serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into())),
                        None,
                        None,
                    ),
                    Err(e) => emit_event("error", Some(e), None, None),
                }
            }
        }
        let (trailer_status, trailer_message) = extract_status(trailers.as_ref());
        let grpc_status = match trailer_status {
            Some(s) => Some(s),
            None if head.status.is_success() => None,
            None => Some(-1),
        };
        emit_event("trailers", None, grpc_status, trailer_message);
        emit_event("end", None, grpc_status, None);
        Ok(())
    };
    match timeout(Duration::from_secs(secs), stream).await {
        Err(_) => emit_event("error", Some(format!("gRPC call timed out after {secs}s")), None, None),
        Ok(Err(e)) => {
            if e != "cancelled" {
                emit_event("error", Some(e), None, None);
            }
        }
        Ok(Ok(())) => {}
    }
}

/// Opens a server-stream and registers it with the hub until it ends or is
/// closed via [`GrpcHub::close`].
pub async fn call_server_stream(
    hub: Arc<GrpcHub>,
    session_id: String,
    call: GrpcCall,
    emit: impl Fn(GrpcEvent) + Send + Sync + 'static,
) -> Result<(), String> {
    if call.service.is_empty() || call.method.is_empty() {
        return Err("gRPC service and method are required".into());
    }
    let (cancel_tx, mut cancel_rx) = mpsc::unbounded_channel::<()>();
    hub.sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(session_id.clone(), cancel_tx);
    run_stream(&call, &session_id, Some(&mut cancel_rx), &emit).await;
    hub.sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&session_id);
    Ok(())
}

fn header_int_opt(headers: &[(String, String)], name: &str) -> Option<i32> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .and_then(|(_, v)| v.parse::<i32>().ok())
}

fn header_value_opt(headers: &[(String, String)], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

// ---------- shared helpers ----------

fn decode_frame(bytes: &[u8], fields: &[ProtoField], messages: &[ProtoMessage]) -> Result<Value, String> {
    if bytes.is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    if bytes.len() < 5 {
        return Err("truncated gRPC frame".into());
    }
    if bytes[0] != 0 {
        return Err("compressed gRPC responses are not supported".into());
    }
    let len = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize;
    let end = 5 + len;
    if bytes.len() < end {
        return Err("truncated gRPC message".into());
    }
    if fields.is_empty() {
        return Ok(Value::String(hex_preview(&bytes[5..end])));
    }
    decode_message(&bytes[5..end], fields, messages)
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(64)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with(path) {
        return base.to_string();
    }
    format!("{base}{path}")
}

fn header_int(headers: &[(String, String)], name: &str) -> i32 {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0)
}

fn wire_type(type_name: &str) -> u8 {
    match type_name {
        "bool" | "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" => 0,
        "float" => 5,
        "double" => 1,
        _ => 2,
    }
}

fn write_key(out: &mut Vec<u8>, number: u32, wire: u8) {
    write_varint(out, ((number as u64) << 3) | wire as u64);
}

fn write_varint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
}

fn read_varint(bytes: &mut &[u8]) -> Result<u64, String> {
    let mut n = 0u64;
    let mut shift = 0;
    for _ in 0..10 {
        if bytes.is_empty() {
            return Err("truncated varint".into());
        }
        let b = bytes[0];
        *bytes = &bytes[1..];
        n |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok(n);
        }
        shift += 7;
    }
    Err("varint too long".into())
}

fn read_len(bytes: &mut &[u8]) -> Result<Vec<u8>, String> {
    let len = read_varint(bytes)? as usize;
    if bytes.len() < len {
        return Err("truncated length-delimited field".into());
    }
    let (head, tail) = bytes.split_at(len);
    *bytes = tail;
    Ok(head.to_vec())
}

fn skip_wire(bytes: &mut &[u8], wire: u8) -> Result<(), String> {
    match wire {
        0 => {
            read_varint(bytes)?;
            Ok(())
        }
        1 => {
            if bytes.len() < 8 {
                return Err("truncated 64-bit field".into());
            }
            *bytes = &bytes[8..];
            Ok(())
        }
        2 => {
            read_len(bytes)?;
            Ok(())
        }
        5 => {
            if bytes.len() < 4 {
                return Err("truncated 32-bit field".into());
            }
            *bytes = &bytes[4..];
            Ok(())
        }
        other => Err(format!("unsupported wire type {other}")),
    }
}

fn json_from_u64(n: u64) -> Value {
    if n <= i64::MAX as u64 {
        Value::from(n as i64)
    } else {
        Value::from(n)
    }
}

fn json_string(value: &Value, field: &str) -> Result<String, String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Null => Ok(String::new()),
        _ => Err(format!("`{field}` must be a string")),
    }
}

fn json_u64(value: &Value, field: &str) -> Result<u64, String> {
    match value {
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_i64().and_then(|i| u64::try_from(i).ok()))
            .ok_or_else(|| format!("`{field}` must be an unsigned integer")),
        _ => Err(format!("`{field}` must be a number")),
    }
}

fn json_i64(value: &Value, field: &str) -> Result<i64, String> {
    match value {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_u64().and_then(|u| i64::try_from(u).ok()))
            .ok_or_else(|| format!("`{field}` must be an integer")),
        _ => Err(format!("`{field}` must be a number")),
    }
}

// ---------- proto3 subset tokenizer ----------

fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i += 2;
            continue;
        }
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            out.push(bytes[i] as char);
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    out.push(bytes[i] as char);
                    i += 1;
                }
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() {
                out.push(quote as char);
                i += 1;
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn skip_ws(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && (bytes[*i] as char).is_whitespace() {
        *i += 1;
    }
}

fn is_ident_byte(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

fn read_ident(bytes: &[u8], i: &mut usize) -> String {
    skip_ws(bytes, i);
    let start = *i;
    while *i < bytes.len() {
        let c = bytes[*i] as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            *i += 1;
        } else {
            break;
        }
    }
    String::from_utf8_lossy(&bytes[start..*i]).into_owned()
}

fn read_dotted(bytes: &[u8], i: &mut usize) -> String {
    skip_ws(bytes, i);
    let start = *i;
    while *i < bytes.len() && is_ident_byte(bytes[*i] as char) {
        *i += 1;
    }
    String::from_utf8_lossy(&bytes[start..*i]).into_owned()
}

fn expect_semi(bytes: &[u8], i: &mut usize) -> Result<(), String> {
    skip_ws(bytes, i);
    if *i < bytes.len() && bytes[*i] == b';' {
        *i += 1;
        return Ok(());
    }
    Err("expected `;`".into())
}

fn skip_statement(bytes: &[u8], i: &mut usize) {
    let mut depth = 0i32;
    while *i < bytes.len() {
        match bytes[*i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth <= 0 {
                    *i += 1;
                    return;
                }
            }
            b';' if depth == 0 => {
                *i += 1;
                return;
            }
            _ => {}
        }
        *i += 1;
    }
}

fn read_block(bytes: &[u8], i: &mut usize) -> Result<String, String> {
    skip_ws(bytes, i);
    if *i >= bytes.len() || bytes[*i] != b'{' {
        return Err("expected `{`".into());
    }
    let start = *i + 1;
    let mut depth = 1i32;
    *i += 1;
    while *i < bytes.len() {
        match bytes[*i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let content = String::from_utf8_lossy(&bytes[start..*i]).into_owned();
                    *i += 1;
                    return Ok(content);
                }
            }
            _ => {}
        }
        *i += 1;
    }
    Err("unclosed `{`".into())
}

fn read_number(bytes: &[u8], i: &mut usize) -> Result<u32, String> {
    skip_ws(bytes, i);
    let start = *i;
    while *i < bytes.len() && (bytes[*i] as char).is_ascii_digit() {
        *i += 1;
    }
    let text = String::from_utf8_lossy(&bytes[start..*i]).into_owned();
    text.parse::<u32>()
        .map_err(|_| format!("expected a field number, got `{text}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROTO: &str = r#"
        syntax = "proto3";
        package demo.v1;
        message User {
          string name = 1;
          int32 id = 2;
          bool active = 3;
        }
        service Users {
          rpc Get (User) returns (User);
        }
    "#;

    #[test]
    fn parses_service_and_message() {
        let file = parse_proto(PROTO).unwrap();
        assert_eq!(file.package, "demo.v1");
        assert_eq!(file.services[0].methods[0].name, "Get");
        assert_eq!(file.messages[0].fields.len(), 3);
    }

    #[test]
    fn roundtrips_json() {
        let file = parse_proto(PROTO).unwrap();
        let msg = &file.messages[0];
        let json = serde_json::json!({"name": "ada", "id": 7, "active": true});
        let bytes = encode_message(&json, &msg.fields, &file.messages).unwrap();
        let back = decode_message(&bytes, &msg.fields, &file.messages).unwrap();
        assert_eq!(back["name"], "ada");
        assert_eq!(back["id"], 7);
        assert_eq!(back["active"], true);
    }
}
