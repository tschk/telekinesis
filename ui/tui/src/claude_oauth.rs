//! Anthropic Messages streaming for Claude subscription (OAuth) logins.
//!
//! rx4's `OpenAIProvider::anthropic` sends its key as `x-api-key`, which
//! Anthropic rejects for `sk-ant-oat…` OAuth tokens. Subscription tokens
//! need Bearer auth plus the Claude Code request profile (beta headers,
//! billing system block, body checksum) that `rs_ai_oauth::claude_code`
//! owns. This provider builds the same Messages body rx4 would, applies
//! that profile, and parses the Anthropic SSE stream.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::{stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use rs_ai_oauth::claude_code;
use rx4::agent::ToolCall;
use rx4::cost::TokenUsage;
use rx4::provider::{Message, Provider, ProviderError, Role, StreamEvent, StreamResult};
use serde_json::{json, Value};

use crate::openai_chat::{parse_token_usage, session_id, stream_idle_timeout};

const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";

/// Thinking blocks (with signatures) from responses that called tools,
/// keyed by tool-use id. rx4 messages cannot carry them, and Anthropic
/// rejects a tool-result turn whose assistant thinking was dropped or
/// altered, so they are replayed verbatim on the next request.
type ThinkingCache = Arc<Mutex<HashMap<String, Vec<Value>>>>;

pub struct ClaudeOAuthProvider {
    token: String,
    url: String,
    thinking: ThinkingCache,
}

impl ClaudeOAuthProvider {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            url: MESSAGES_URL.to_string(),
            thinking: ThinkingCache::default(),
        }
    }

    #[cfg(test)]
    fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    fn headers(&self) -> Result<HeaderMap, ProviderError> {
        let value = |text: &str| {
            HeaderValue::from_str(text).map_err(|error| ProviderError::Api(error.to_string()))
        };
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, value(&format!("Bearer {}", self.token))?);
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let request_id = uuid::Uuid::new_v4().to_string();
        for (name, text) in claude_code::headers(Some(session_id()), &request_id) {
            headers.insert(HeaderName::from_static(name), value(&text)?);
        }
        Ok(headers)
    }
}

#[async_trait]
impl Provider for ClaudeOAuthProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn name(&self) -> &str {
        "Claude (subscription)"
    }

    async fn stream(
        &self,
        messages: &[Message],
        system: &Option<String>,
        model: &str,
        tools: &[Value],
        reasoning_effort: Option<&str>,
    ) -> Result<StreamResult, ProviderError> {
        let body = {
            let thinking = self.thinking.lock().unwrap_or_else(|e| e.into_inner());
            messages_body(messages, system, model, tools, reasoning_effort, &thinking)
        };
        let first_prompt = messages
            .iter()
            .find(|message| message.role == Role::User)
            .map_or("", |message| message.content.as_str());
        let identity = claude_code::discover_identity();
        let body = claude_code::transform_payload(
            body,
            first_prompt,
            Some(session_id()),
            identity.as_ref(),
        )
        .map_err(|error| ProviderError::Api(error.to_string()))?;
        let body = claude_code::patch_cch(&body.to_string())
            .map_err(|error| ProviderError::Api(error.to_string()))?;

        let response = reqwest::Client::new()
            .post(&self.url)
            .headers(self.headers()?)
            .body(body)
            .send()
            .await
            .map_err(|error| ProviderError::Http(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!(
                "Claude request failed (HTTP {status}): {}",
                text.chars().take(300).collect::<String>()
            )));
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(pump_anthropic_sse(
            response.bytes_stream(),
            tx,
            Arc::clone(&self.thinking),
        ));
        let stream = stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });
        Ok(Box::new(Box::pin(stream)))
    }
}

/// Anthropic Messages body, mirroring rx4's `anthropic_request`, with
/// cached thinking blocks put back at the start of assistant tool turns.
fn messages_body(
    messages: &[Message],
    system: &Option<String>,
    model: &str,
    tools: &[Value],
    reasoning_effort: Option<&str>,
    thinking: &HashMap<String, Vec<Value>>,
) -> Value {
    let cached = |message: &Message| {
        message
            .tool_calls
            .first()
            .and_then(|call| thinking.get(&call.id))
    };
    let converted = messages
        .iter()
        .filter_map(|message| match message.role {
            Role::System => None,
            Role::Tool => Some(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": message.tool_call_id,
                    "content": message.content
                }]
            })),
            Role::Assistant => {
                let mut content = cached(message).cloned().unwrap_or_default();
                if !message.content.is_empty() {
                    content.push(json!({"type": "text", "text": message.content}));
                }
                content.extend(message.tool_calls.iter().map(|call| {
                    let input = serde_json::from_str(&call.arguments)
                        .unwrap_or_else(|_| json!({"raw": call.arguments}));
                    json!({"type": "tool_use", "id": call.id, "name": call.name, "input": input})
                }));
                Some(json!({"role": "assistant", "content": content}))
            }
            Role::User => Some(json!({"role": "user", "content": message.content})),
        })
        .collect::<Vec<_>>();
    let mut body = json!({
        "model": model.split('/').next_back().unwrap_or(model),
        "stream": true,
        "max_tokens": 8192,
        "messages": converted,
    });
    if let Some(system) = system {
        body["system"] = json!(system);
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(
            tools
                .iter()
                .map(|tool| {
                    let function = tool.get("function").unwrap_or(tool);
                    json!({
                        "name": function["name"],
                        "description": function["description"],
                        "input_schema": function["parameters"]
                    })
                })
                .collect(),
        );
    }
    // Mid tool loop, the latest assistant turn must start with its original
    // thinking when thinking is on. If those blocks are unknown (e.g. a
    // resumed session), leave thinking off for this request instead of
    // sending a request Anthropic rejects.
    let mid_tool_loop = messages.last().is_some_and(|m| m.role == Role::Tool);
    let replayable = messages
        .iter()
        .rev()
        .find(|m| m.role == Role::Assistant)
        .is_some_and(|m| cached(m).is_some());
    if let Some(effort) = reasoning_effort.filter(|_| !mid_tool_loop || replayable) {
        let budget = match effort {
            "medium" => 4_096,
            "high" => 8_192,
            "xhigh" => 16_384,
            _ => 1_024,
        };
        body["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
        body["max_tokens"] = json!(budget + 8_192);
    }
    body
}

#[derive(Default)]
struct AnthropicStreamState {
    calls: BTreeMap<usize, ToolCall>,
    usage: TokenUsage,
    done: bool,
    /// Thinking blocks being streamed, by content index.
    thinking_open: BTreeMap<usize, Value>,
    /// Completed thinking blocks in response order.
    thinking: Vec<Value>,
    tool_ids: Vec<String>,
}

async fn pump_anthropic_sse<S, T, E>(
    mut bytes: S,
    tx: tokio::sync::mpsc::UnboundedSender<Result<StreamEvent, ProviderError>>,
    cache: ThinkingCache,
) where
    S: futures::Stream<Item = Result<T, E>> + Unpin,
    T: AsRef<[u8]>,
    E: std::fmt::Display,
{
    let mut buffer = String::new();
    let mut state = AnthropicStreamState::default();
    loop {
        let chunk = match tokio::time::timeout(stream_idle_timeout(), bytes.next()).await {
            Err(_) => {
                let _ = tx.send(Err(ProviderError::Stream(
                    "Claude stream idle; connection presumed dead".into(),
                )));
                return;
            }
            Ok(None) => break,
            Ok(Some(Err(error))) => {
                let _ = tx.send(Err(ProviderError::Stream(error.to_string())));
                return;
            }
            Ok(Some(Ok(chunk))) => chunk,
        };
        buffer.push_str(&String::from_utf8_lossy(chunk.as_ref()));
        while let Some(position) = buffer.find("\n\n") {
            let block = buffer[..position].to_string();
            buffer.drain(..position + 2);
            for event in anthropic_events(&block, &mut state) {
                let _ = tx.send(event);
            }
        }
    }
    for event in anthropic_events(&buffer, &mut state) {
        let _ = tx.send(event);
    }
    if !state.thinking.is_empty() {
        let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        for id in &state.tool_ids {
            cache.insert(id.clone(), state.thinking.clone());
        }
    }
    if !state.done {
        let _ = tx.send(Ok(StreamEvent::Done));
    }
}

/// Events for one SSE block, mirroring rx4's `parse_anthropic_event`.
fn anthropic_events(
    block: &str,
    state: &mut AnthropicStreamState,
) -> Vec<Result<StreamEvent, ProviderError>> {
    let data = block
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    let Ok(json) = serde_json::from_str::<Value>(&data) else {
        return Vec::new();
    };
    let index = json["index"].as_u64().unwrap_or(0) as usize;
    match json["type"].as_str() {
        Some("content_block_start")
            if matches!(
                json["content_block"]["type"].as_str(),
                Some("thinking" | "redacted_thinking")
            ) =>
        {
            state
                .thinking_open
                .insert(index, json["content_block"].clone());
            Vec::new()
        }
        Some("content_block_delta")
            if matches!(
                json["delta"]["type"].as_str(),
                Some("thinking_delta" | "signature_delta")
            ) =>
        {
            if let Some(block) = state.thinking_open.get_mut(&index) {
                for key in ["thinking", "signature"] {
                    if let Some(part) = json["delta"][key].as_str() {
                        let joined = format!("{}{part}", block[key].as_str().unwrap_or(""));
                        block[key] = json!(joined);
                    }
                }
            }
            Vec::new()
        }
        Some("content_block_start") if json["content_block"]["type"] == "tool_use" => {
            let block = &json["content_block"];
            state.calls.insert(
                index,
                ToolCall {
                    id: block["id"].as_str().unwrap_or_default().to_string(),
                    name: block["name"].as_str().unwrap_or_default().to_string(),
                    arguments: String::new(),
                },
            );
            Vec::new()
        }
        Some("content_block_delta") if json["delta"]["type"] == "text_delta" => json["delta"]
            ["text"]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(|text| vec![Ok(StreamEvent::Delta(text.to_string()))])
            .unwrap_or_default(),
        Some("content_block_delta") if json["delta"]["type"] == "input_json_delta" => {
            if let Some(call) = state.calls.get_mut(&index) {
                call.arguments
                    .push_str(json["delta"]["partial_json"].as_str().unwrap_or_default());
            }
            Vec::new()
        }
        Some("content_block_stop") => {
            if let Some(block) = state.thinking_open.remove(&index) {
                state.thinking.push(block);
            }
            state
                .calls
                .remove(&index)
                .map(|call| {
                    state.tool_ids.push(call.id.clone());
                    vec![Ok(StreamEvent::ToolCall(call))]
                })
                .unwrap_or_default()
        }
        Some("message_start") => {
            if let Some(usage) = parse_token_usage(&json["message"]["usage"]) {
                state.usage.input_tokens = usage.input_tokens;
                state.usage.cache_read_tokens = usage.cache_read_tokens;
                state.usage.cache_write_tokens = usage.cache_write_tokens;
            }
            Vec::new()
        }
        Some("message_delta") => {
            if let Some(usage) = parse_token_usage(&json["usage"]) {
                state.usage.output_tokens = usage.output_tokens;
            }
            Vec::new()
        }
        Some("message_stop") => {
            state.done = true;
            vec![Ok(StreamEvent::Usage(state.usage)), Ok(StreamEvent::Done)]
        }
        Some("error") => vec![Err(ProviderError::Api(
            json["error"]["message"]
                .as_str()
                .unwrap_or("Anthropic stream error")
                .to_string(),
        ))],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// One-shot HTTP server: returns the raw request it received.
    fn serve_once(response_body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1/messages", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = socket.read(&mut buf).unwrap();
                request.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&request);
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length || n == 0 {
                        break;
                    }
                }
            }
            write!(
                socket,
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response_body}",
                response_body.len()
            )
            .unwrap();
            String::from_utf8_lossy(&request).into_owned()
        });
        (url, handle)
    }

    #[tokio::test]
    async fn oauth_token_is_sent_as_bearer_with_claude_code_profile() {
        let sse = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":7}}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"read\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\\\"a\\\"}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":3}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let (url, server) = serve_once(sse);
        let provider = ClaudeOAuthProvider::new("sk-ant-oat01-test").with_url(url);
        let mut events = provider
            .stream(
                &[Message::user("hello")],
                &Some("be brief".into()),
                "claude-sonnet-4-5",
                &[],
                Some("high"),
            )
            .await
            .unwrap();
        let mut seen = Vec::new();
        while let Some(event) = events.next().await {
            seen.push(event.unwrap());
        }
        let request = server.join().unwrap();
        let lower = request.to_ascii_lowercase();

        assert!(lower.contains("\r\nauthorization: bearer sk-ant-oat01-test\r\n"));
        assert!(
            !lower.contains("x-api-key"),
            "OAuth token leaked as x-api-key"
        );
        assert!(lower.contains("oauth-2025-04-20"));
        assert!(lower.contains("anthropic-version: 2023-06-01"));
        let body: Value =
            serde_json::from_str(&request[request.find("\r\n\r\n").unwrap() + 4..]).unwrap();
        assert_eq!(body["model"], "claude-sonnet-4-5");
        assert_eq!(
            body["thinking"]["type"], "enabled",
            "effort enables thinking"
        );
        assert_eq!(body["messages"][0]["content"], "hello");
        let system = body["system"].as_array().unwrap();
        assert!(system[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("x-anthropic-billing-header: "));
        assert_eq!(system.last().unwrap()["text"], "be brief");

        assert!(matches!(&seen[0], StreamEvent::Delta(text) if text == "hi"));
        assert!(matches!(
            &seen[1],
            StreamEvent::ToolCall(call) if call.id == "tu_1" && call.name == "read" && call.arguments == "{\"path\":\"a\"}"
        ));
        assert!(matches!(
            &seen[2],
            StreamEvent::Usage(usage) if usage.input_tokens == 7 && usage.output_tokens == 3
        ));
        assert!(matches!(seen[3], StreamEvent::Done));
        assert_eq!(seen.len(), 4);
    }

    #[tokio::test]
    async fn thinking_blocks_are_replayed_on_the_tool_result_turn() {
        let sse = concat!(
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"need \"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"a file\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig-123\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"redacted_thinking\",\"data\":\"opaque\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_9\",\"name\":\"read\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let cache = ThinkingCache::default();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let bytes = futures::stream::iter([Ok::<_, std::io::Error>(sse.as_bytes().to_vec())]);
        pump_anthropic_sse(bytes, tx, Arc::clone(&cache)).await;
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event.unwrap());
        }
        assert!(
            !events.iter().any(|e| matches!(e, StreamEvent::Delta(_))),
            "thinking is not shown as reply text"
        );

        let history = [
            Message::user("read a"),
            Message::assistant_with_tools(
                "",
                vec![ToolCall {
                    id: "tu_9".into(),
                    name: "read".into(),
                    arguments: "{}".into(),
                }],
            ),
            Message::tool("tu_9", "contents"),
        ];
        let cache = cache.lock().unwrap();
        let body = messages_body(
            &history,
            &None,
            "claude-sonnet-4-5",
            &[],
            Some("high"),
            &cache,
        );
        let assistant = &body["messages"][1]["content"];
        assert_eq!(
            assistant[0],
            json!({"type": "thinking", "thinking": "need a file", "signature": "sig-123"})
        );
        assert_eq!(
            assistant[1],
            json!({"type": "redacted_thinking", "data": "opaque"})
        );
        assert_eq!(assistant[2]["type"], "tool_use");
        assert_eq!(body["thinking"]["type"], "enabled");

        // Unknown thinking for the pending tool turn: send without thinking
        // rather than a request Anthropic rejects.
        let empty = HashMap::new();
        let body = messages_body(
            &history,
            &None,
            "claude-sonnet-4-5",
            &[],
            Some("high"),
            &empty,
        );
        assert!(body.get("thinking").is_none());
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
    }
}
