use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, connect_async};

use crate::host::HostSnapshot;
use crate::layout::SessionRow;
use crate::session::MessageItem;

pub const DEFAULT_COMPANION_BIND: &str = "127.0.0.1:17421";

pub type BridgeInbox = UnboundedSender<(String, UnboundedSender<String>)>;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BridgeRequest {
    Snapshot,
    Prompt { text: String },
    Queue { text: String },
    Interrupt,
    Select { session: usize },
    Effort,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeSnapshot {
    pub model: String,
    pub effort: String,
    pub busy: bool,
    pub connected: bool,
    pub status: String,
    pub queued: usize,
    pub input: String,
    pub session_name: String,
    pub composer_action: String,
    pub messages: Vec<BridgeMessage>,
    pub sessions: Vec<SessionRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BridgeResponse {
    Ok,
    Snapshot { data: BridgeSnapshot },
    Error { message: String },
}

impl From<&MessageItem> for BridgeMessage {
    fn from(message: &MessageItem) -> Self {
        Self {
            role: message.role.clone(),
            content: message.content.clone(),
        }
    }
}

impl From<&HostSnapshot> for BridgeSnapshot {
    fn from(snap: &HostSnapshot) -> Self {
        Self {
            model: snap.model.clone(),
            effort: snap.effort.clone(),
            busy: snap.busy,
            connected: snap.connected,
            status: snap.status.clone(),
            queued: snap.queued,
            input: snap.input.clone(),
            session_name: snap.session_name.clone(),
            composer_action: snap.composer_action.clone(),
            messages: snap.messages.iter().map(BridgeMessage::from).collect(),
            sessions: snap.sessions.clone(),
        }
    }
}

pub fn parse_bridge_request(raw: &str) -> Result<BridgeRequest, String> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if value.get("v").and_then(|v| v.as_u64()) != Some(1) {
        return Err("unsupported companion protocol version".into());
    }
    serde_json::from_value(value).map_err(|error| error.to_string())
}

pub fn encode_bridge_response(response: &BridgeResponse) -> String {
    let mut value = serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({}));
    if let serde_json::Value::Object(map) = &mut value {
        map.insert("v".into(), serde_json::json!(1));
    }
    value.to_string()
}

pub fn spawn_loopback_bind(inbox: BridgeInbox, addr: SocketAddr) {
    crate::agent::runtime().handle().spawn(async move {
        match serve_companion(addr, inbox).await {
            Ok(bound) => eprintln!("companion loopback ws {bound}"),
            Err(error) => eprintln!("companion bind skipped: {error}"),
        }
    });
}

pub async fn serve_companion(addr: SocketAddr, inbox: BridgeInbox) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let inbox = inbox.clone();
            tokio::spawn(async move {
                let Ok(mut socket) = accept_async(stream).await else {
                    return;
                };
                use futures::StreamExt;
                while let Some(Ok(message)) = socket.next().await {
                    let text = match message {
                        Message::Text(text) => text.to_string(),
                        Message::Binary(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                        Message::Close(_) => break,
                        _ => continue,
                    };
                    let (reply_tx, mut reply_rx) = unbounded_channel();
                    if inbox.send((text, reply_tx)).is_err() {
                        break;
                    }
                    if let Some(reply) = reply_rx.recv().await {
                        use futures::SinkExt;
                        if socket.send(Message::Text(reply.into())).await.is_err() {
                            break;
                        }
                    }
                }
            });
        }
    });
    Ok(bound)
}

pub async fn companion_roundtrip(url: &str, request: &str) -> Result<String, String> {
    let (mut socket, _) = connect_async(url)
        .await
        .map_err(|error| error.to_string())?;
    use futures::SinkExt;
    use futures::StreamExt;
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .map_err(|error| error.to_string())?;
    match socket.next().await {
        Some(Ok(Message::Text(text))) => Ok(text.to_string()),
        Some(Ok(other)) => Err(format!("unexpected companion frame: {other}")),
        Some(Err(error)) => Err(error.to_string()),
        None => Err("companion closed".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::runtime;

    #[test]
    fn rejects_unknown_protocol_version() {
        let error = parse_bridge_request(r#"{"v":2,"op":"snapshot"}"#).unwrap_err();
        assert!(error.contains("version"));
    }

    #[test]
    fn parses_prompt_and_queue() {
        let prompt = parse_bridge_request(r#"{"v":1,"op":"prompt","text":"hi"}"#).unwrap();
        assert_eq!(prompt, BridgeRequest::Prompt { text: "hi".into() });
        let queue = parse_bridge_request(r#"{"v":1,"op":"queue","text":"next"}"#).unwrap();
        assert_eq!(
            queue,
            BridgeRequest::Queue {
                text: "next".into()
            }
        );
    }

    #[test]
    fn loopback_ws_answers_snapshot() {
        let (inbox, mut rx) = unbounded_channel();
        let addr = runtime().block_on(async {
            serve_companion("127.0.0.1:0".parse().unwrap(), inbox)
                .await
                .unwrap()
        });
        runtime().handle().spawn(async move {
            if let Some((raw, reply)) = rx.recv().await {
                assert!(raw.contains("snapshot"));
                let _ = reply.send(encode_bridge_response(&BridgeResponse::Ok));
            }
        });
        let reply = runtime()
            .block_on(companion_roundtrip(
                &format!("ws://{addr}"),
                r#"{"v":1,"op":"snapshot"}"#,
            ))
            .unwrap();
        assert!(reply.contains(r#""op":"ok""#));
        assert!(reply.contains(r#""v":1"#));
    }
}
