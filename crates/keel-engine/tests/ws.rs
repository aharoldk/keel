//! End-to-end WebSocket tests: a real local `ws://` server, a real
//! connection through `keel_engine::ws`, text and binary frames both ways.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use keel_engine::ws::{self, WsConnect, WsEvent};
use tokio::sync::mpsc;

/// Spins up a WebSocket echo server that greets on connect and echoes
/// everything it receives. Returns the port and a join handle.
async fn echo_server() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = tokio::spawn(async move {
        // Accept one connection, echo frames back.
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let ws = tokio_tungstenite::accept_async(stream).await.expect("accept");
        let (mut write, mut read) = ws.split();
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        while let Some(Ok(frame)) = read.next().await {
            match frame {
                Message::Text(t) => {
                    let _ = write.send(Message::Text(t)).await;
                }
                Message::Binary(b) => {
                    let _ = write.send(Message::Binary(b)).await;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });
    (port, handle)
}

#[tokio::test]
async fn connects_sends_text_and_receives_the_echo() {
    let (port, server) = echo_server().await;
    let hub = Arc::new(ws::WsHub::new());
    let events: Arc<Mutex<Vec<WsEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let (tx, mut rx) = mpsc::unbounded_channel();

    let sid = "s1".to_string();
    let events_c = events.clone();
    ws::connect(
        hub.clone(),
        sid.clone(),
        WsConnect {
            url: format!("ws://127.0.0.1:{port}/echo"),
            headers: Vec::new(),
            protocols: Vec::new(),
        },
        move |ev| {
            let _ = tx.send(ev.clone());
            events_c.lock().expect("lock").push(ev);
        },
    )
    .await
    .expect("connect");

    // Drain the "open" event.
    let open = rx.recv().await.expect("open event");
    assert_eq!(open.kind, "open");

    hub.send_text(&sid, "hello".into()).expect("send text");

    let echoed = loop {
        let ev = rx.recv().await.expect("message event");
        if ev.kind == "message" {
            break ev;
        }
    };
    assert_eq!(echoed.data.as_deref(), Some("hello"));
    assert_eq!(echoed.opcode.as_deref(), Some("text"));

    hub.close(&sid);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}

#[tokio::test]
async fn sends_binary_and_receives_base64() {
    let (port, server) = echo_server().await;
    let hub = Arc::new(ws::WsHub::new());
    let (tx, mut rx) = mpsc::unbounded_channel();

    let sid = "s2".to_string();
    ws::connect(
        hub.clone(),
        sid.clone(),
        WsConnect {
            url: format!("ws://127.0.0.1:{port}/echo"),
            headers: Vec::new(),
            protocols: Vec::new(),
        },
        move |ev| {
            let _ = tx.send(ev);
        },
    )
    .await
    .expect("connect");

    let open = rx.recv().await.expect("open event");
    assert_eq!(open.kind, "open");

    hub.send_binary(&sid, vec![0x00, 0x01, 0xff]).expect("send binary");

    let echoed = loop {
        let ev = rx.recv().await.expect("message event");
        if ev.kind == "message" {
            break ev;
        }
    };
    assert_eq!(echoed.opcode.as_deref(), Some("binary"));
    // Binary frames are base64-encoded on the way out: 00 01 ff -> "AAH/"
    assert_eq!(echoed.data.as_deref(), Some("AAH/"));

    hub.close(&sid);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}

#[tokio::test]
async fn sending_to_a_closed_session_errors() {
    let hub = Arc::new(ws::WsHub::new());
    let err = hub
        .send_text("never-opened", "x".into())
        .expect_err("should error");
    assert!(err.contains("not open"), "{err}");
}

#[tokio::test]
async fn rejects_a_non_ws_url() {
    let hub = Arc::new(ws::WsHub::new());
    let err = ws::connect(
        hub,
        "s3".into(),
        WsConnect {
            url: "http://example.test/".into(),
            headers: Vec::new(),
            protocols: Vec::new(),
        },
        |_| {},
    )
    .await
    .expect_err("should reject http://");
    assert!(err.contains("ws or wss"), "{err}");
}
