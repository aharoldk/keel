//! WebSocket sessions. One connection per session id; frames are emitted to
//! the UI on `ws://message`. Text and binary (base64) frames are both kept.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use base64::Engine as _;
use rand::RngCore;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsEvent {
    pub session_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opcode: Option<String>,
}

enum Outbound {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

struct Session {
    tx: mpsc::UnboundedSender<Outbound>,
}

#[derive(Default)]
pub struct WsHub {
    sessions: Mutex<HashMap<String, Session>>,
}

impl WsHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn close(&self, session_id: &str) {
        if let Some(session) = self.sessions.lock().unwrap_or_else(|e| e.into_inner()).remove(session_id) {
            let _ = session.tx.send(Outbound::Close);
        }
    }

    pub fn send_text(&self, session_id: &str, text: String) -> Result<(), String> {
        self.send(session_id, Outbound::Text(text))
    }

    pub fn send_binary(&self, session_id: &str, bytes: Vec<u8>) -> Result<(), String> {
        self.send(session_id, Outbound::Binary(bytes))
    }

    fn send(&self, session_id: &str, msg: Outbound) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let session = sessions
            .get(session_id)
            .ok_or_else(|| "websocket session is not open".to_string())?;
        session.tx.send(msg).map_err(|_| "websocket session closed".to_string())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsConnect {
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub protocols: Vec<String>,
}

/// Opens a connection and registers it under `session_id`. `emit` is called
/// for every inbound frame and for open/close/error.
pub async fn connect<F>(hub: Arc<WsHub>, session_id: String, spec: WsConnect, emit: F) -> Result<(), String>
where
    F: Fn(WsEvent) + Send + Sync + 'static,
{
    let url: reqwest::Url = spec
        .url
        .parse()
        .map_err(|e| format!("invalid websocket url: {e}"))?;
    match url.scheme() {
        "ws" | "wss" => {}
        other => return Err(format!("websocket url must use ws or wss, got `{other}`")),
    }
    let mut key = [0u8; 16];
    rand::rng().fill_bytes(&mut key);
    let host = match (url.host_str().unwrap_or("localhost"), url.port()) {
        (host, Some(port)) => format!("{host}:{port}"),
        (host, None) => host.to_string(),
    };
    let mut request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(spec.url.clone())
        .header("host", host)
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", base64::engine::general_purpose::STANDARD.encode(key));
    if !spec.protocols.is_empty() {
        request = request.header("sec-websocket-protocol", spec.protocols.join(", "));
    }
    for (name, value) in &spec.headers {
        if name.eq_ignore_ascii_case("host") || name.eq_ignore_ascii_case("upgrade") {
            continue;
        }
        request = request.header(name.as_str(), value.as_str());
    }
    let request = request.body(()).map_err(|e| format!("websocket request: {e}"))?;

    let (stream, response) = tokio::time::timeout(
        Duration::from_secs(30),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| "websocket connect timed out".to_string())?
    .map_err(|e| format!("websocket connect: {e}"))?;

    let protocol = response
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let (mut write, mut read) = stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel();
    {
        let mut sessions = hub.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = sessions.insert(session_id.clone(), Session { tx }) {
            let _ = old.tx.send(Outbound::Close);
        }
    }
    emit(WsEvent {
        session_id: session_id.clone(),
        kind: "open".into(),
        data: protocol,
        opcode: Some("open".into()),
    });

    let sid_in = session_id.clone();
    let emit_in = Arc::new(emit);
    let emit_out = emit_in.clone();
    let sid_out = session_id.clone();
    let hub_out = hub.clone();

    tokio::spawn(async move {
        while let Some(frame) = read.next().await {
            match frame {
                Ok(Message::Text(text)) => emit_in(WsEvent {
                    session_id: sid_in.clone(),
                    kind: "message".into(),
                    data: Some(text.to_string()),
                    opcode: Some("text".into()),
                }),
                Ok(Message::Binary(bytes)) => {
                    use base64::Engine as _;
                    emit_in(WsEvent {
                        session_id: sid_in.clone(),
                        kind: "message".into(),
                        data: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                        opcode: Some("binary".into()),
                    })
                }
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
                Ok(Message::Close(frame)) => {
                    emit_in(WsEvent {
                        session_id: sid_in.clone(),
                        kind: "close".into(),
                        data: frame.map(|f| f.reason.to_string()),
                        opcode: Some("close".into()),
                    });
                    break;
                }
                Err(err) => {
                    emit_in(WsEvent {
                        session_id: sid_in.clone(),
                        kind: "error".into(),
                        data: Some(err.to_string()),
                        opcode: Some("error".into()),
                    });
                    break;
                }
            }
        }
        hub_out
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&sid_in);
    });

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let frame = match msg {
                Outbound::Text(text) => Message::Text(text.into()),
                Outbound::Binary(bytes) => Message::Binary(bytes.into()),
                Outbound::Close => Message::Close(None),
            };
            let closing = matches!(frame, Message::Close(_));
            if let Err(err) = write.send(frame).await {
                emit_out(WsEvent {
                    session_id: sid_out.clone(),
                    kind: "error".into(),
                    data: Some(err.to_string()),
                    opcode: Some("error".into()),
                });
                break;
            }
            if closing {
                let _ = write.close().await;
                break;
            }
        }
    });
    Ok(())
}
