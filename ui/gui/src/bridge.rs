use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_hdr_async, connect_async};

use crate::host::HostSnapshot;
use crate::layout::SessionRow;
use crate::session::MessageItem;

pub const DEFAULT_COMPANION_BIND: &str = "127.0.0.1:17421";
pub const COMPANION_TOKEN_ENV: &str = "TK_COMPANION_TOKEN";

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

fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        if file.read_exact(&mut buf).is_ok() {
            return buf;
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        .to_le_bytes();
    for (index, byte) in buf.iter_mut().enumerate() {
        *byte ^= nanos[index % nanos.len()];
    }
    buf
}

pub fn mint_companion_token() -> String {
    random_bytes(16)
        .into_iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn token_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".telekinesis")
        .join("companion.token")
}

pub fn load_companion_token() -> String {
    if let Ok(existing) = std::env::var(COMPANION_TOKEN_ENV) {
        let trimmed = existing.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    let path = token_file_path();
    if let Ok(contents) = std::fs::read_to_string(&path) {
        let trimmed = contents.trim().to_string();
        if trimmed.len() >= 32 {
            return trimmed;
        }
    }
    let token = mint_companion_token();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::File::create(&path) {
        let _ = writeln!(file, "{token}");
    }
    token
}

fn tokens_equal(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn query_token(query: Option<&str>) -> Option<String> {
    let query = query?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next().unwrap_or("");
        if key == "token" && !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn header_token(request: &Request) -> Option<String> {
    if let Some(value) = request.headers().get("x-companion-token") {
        if let Ok(text) = value.to_str() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(value) = request.headers().get("authorization") {
        if let Ok(text) = value.to_str() {
            let trimmed = text.trim();
            let prefix = "Bearer ";
            if let Some(token) = trimmed.strip_prefix(prefix) {
                if !token.is_empty() {
                    return Some(token.to_string());
                }
            }
        }
    }
    None
}

pub fn origin_allowed(origin: Option<&str>) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    let trimmed = origin.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
        return true;
    }
    let Ok(uri) = trimmed.parse::<tokio_tungstenite::tungstenite::http::Uri>() else {
        return false;
    };
    match uri.host() {
        Some("127.0.0.1") | Some("localhost") | Some("[::1]") | Some("::1") => true,
        Some(host) if host.starts_with("127.") => true,
        _ => false,
    }
}

fn request_token(request: &Request) -> Option<String> {
    query_token(request.uri().query()).or_else(|| header_token(request))
}

fn handshake_error(status: StatusCode, message: &'static str) -> ErrorResponse {
    Response::builder()
        .status(status)
        .body(Some(message.into()))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(None)
                .expect("static handshake error")
        })
}

#[allow(clippy::result_large_err)]
fn authorize_handshake(request: &Request, expected: &str) -> Result<(), ErrorResponse> {
    let origin = request
        .headers()
        .get("origin")
        .and_then(|value| value.to_str().ok());
    if !origin_allowed(origin) {
        return Err(handshake_error(StatusCode::FORBIDDEN, "untrusted origin"));
    }
    let Some(provided) = request_token(request) else {
        return Err(handshake_error(
            StatusCode::UNAUTHORIZED,
            "missing companion token",
        ));
    };
    if !tokens_equal(&provided, expected) {
        return Err(handshake_error(
            StatusCode::UNAUTHORIZED,
            "invalid companion token",
        ));
    }
    Ok(())
}

pub fn spawn_loopback_bind(inbox: BridgeInbox, addr: SocketAddr, token: String) {
    crate::agent::runtime().handle().spawn(async move {
        match serve_companion(addr, inbox, token).await {
            Ok(bound) => eprintln!("companion loopback ws {bound}"),
            Err(error) => eprintln!("companion bind skipped: {error}"),
        }
    });
}

#[allow(clippy::result_large_err)]
pub async fn serve_companion(
    addr: SocketAddr,
    inbox: BridgeInbox,
    token: String,
) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let inbox = inbox.clone();
            let token = token.clone();
            tokio::spawn(async move {
                let Ok(mut socket) =
                    accept_hdr_async(stream, |request: &Request, response: Response| {
                        authorize_handshake(request, &token).map(|_| response)
                    })
                    .await
                else {
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
    fn origin_rejects_remote_pages() {
        assert!(origin_allowed(None));
        assert!(origin_allowed(Some("http://127.0.0.1:8787")));
        assert!(origin_allowed(Some("http://localhost")));
        assert!(!origin_allowed(Some("https://evil.example")));
    }

    #[test]
    fn loopback_ws_answers_snapshot() {
        let token = mint_companion_token();
        let (inbox, mut rx) = unbounded_channel();
        let addr = runtime().block_on(async {
            serve_companion("127.0.0.1:0".parse().unwrap(), inbox, token.clone())
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
                &format!("ws://{addr}/?token={token}"),
                r#"{"v":1,"op":"snapshot"}"#,
            ))
            .unwrap();
        assert!(reply.contains(r#""op":"ok""#));
        assert!(reply.contains(r#""v":1"#));
    }

    #[test]
    fn loopback_ws_rejects_missing_token() {
        let token = mint_companion_token();
        let (inbox, _rx) = unbounded_channel();
        let addr = runtime().block_on(async {
            serve_companion("127.0.0.1:0".parse().unwrap(), inbox, token)
                .await
                .unwrap()
        });
        let error = runtime()
            .block_on(companion_roundtrip(
                &format!("ws://{addr}"),
                r#"{"v":1,"op":"snapshot"}"#,
            ))
            .unwrap_err();
        assert!(!error.is_empty());
    }
}
