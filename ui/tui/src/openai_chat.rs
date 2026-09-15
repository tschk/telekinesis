use std::collections::BTreeMap;
use std::sync::OnceLock;

use async_trait::async_trait;
use futures::stream;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use rx4::agent::ToolCall;
use rx4::cost::TokenUsage;
use rx4::provider::{Message, Provider, ProviderError, StreamEvent, StreamResult};
use serde_json::{json, Value};

/// Stable per-process session id for providers that route on it
/// (OpenCode Go requires `x-opencode-session`).
pub(crate) fn session_id() -> &'static str {
    static SESSION: OnceLock<String> = OnceLock::new();
    SESSION.get_or_init(|| uuid::Uuid::new_v4().to_string())
}

/// OpenAI-compatible chat/completions streaming with tk-owned headers.
///
/// rx4's `OpenAIProvider` cannot send custom headers and drops tool calls
/// when a provider bundles `usage` and `finish_reason` in one SSE chunk
/// (observed on Z.ai GLM). This implementation handles both: usage events
/// never short-circuit finish-reason processing.
pub struct OpenAiChatProvider {
    key: String,
    id: &'static str,
    name: &'static str,
    chat_url: &'static str,
    use_session: bool,
}

impl OpenAiChatProvider {
    pub fn new(
        api_key: impl Into<String>,
        id: &'static str,
        name: &'static str,
        chat_url: &'static str,
    ) -> Self {
        Self {
            key: api_key.into(),
            id,
            name,
            chat_url,
            use_session: false,
        }
    }

    pub fn with_session(mut self) -> Self {
        self.use_session = true;
        self
    }

    fn headers(&self) -> Result<HeaderMap, ProviderError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.key))
                .map_err(|error| ProviderError::Api(error.to_string()))?,
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(concat!("telekinesis/", env!("CARGO_PKG_VERSION"))),
        );
        if self.use_session {
            headers.insert(
                "x-opencode-session",
                HeaderValue::from_str(session_id())
                    .map_err(|error| ProviderError::Api(error.to_string()))?,
            );
        }
        headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }
}

#[async_trait]
impl Provider for OpenAiChatProvider {
    fn id(&self) -> &str {
        self.id
    }

    fn name(&self) -> &str {
        self.name
    }

    async fn stream(
        &self,
        messages: &[Message],
        system: &Option<String>,
        model: &str,
        tools: &[Value],
        reasoning_effort: Option<&str>,
    ) -> Result<StreamResult, ProviderError> {
        let bare = model.split('/').next_back().unwrap_or(model);
        let body = chat_request_body(messages, system, bare, tools, reasoning_effort);
        let response = reqwest::Client::new()
            .post(self.chat_url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await
            .map_err(|error| ProviderError::Api(format!("{} request failed: {error}", self.name)))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!(
                "{} request failed (HTTP {status}): {}",
                self.name,
                text.chars().take(300).collect::<String>()
            )));
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<StreamEvent, ProviderError>>();
        let mut byte_stream = response.bytes_stream();
        tokio::spawn(async move {
            let mut buffer = String::new();
            let mut state = ChatStreamState::default();
            let mut failed: Option<ProviderError> = None;
            while let Some(chunk) = byte_stream.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        failed = Some(ProviderError::Stream(format!(
                            "chat stream read failed: {error}"
                        )));
                        break;
                    }
                };
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(position) = buffer.find("\n\n") {
                    let block = buffer[..position].to_string();
                    buffer = buffer[position + 2..].to_string();
                    if let Err(error) = handle_chat_sse_block(&block, &tx, &mut state) {
                        failed = Some(error);
                        break;
                    }
                }
                if failed.is_some() {
                    break;
                }
            }
            if failed.is_none() && !buffer.trim().is_empty() {
                failed = handle_chat_sse_block(&buffer, &tx, &mut state).err();
            }
            if let Some(error) = failed {
                let _ = tx.send(Err(error));
            } else {
                for call in state.calls.into_values() {
                    let _ = tx.send(Ok(StreamEvent::ToolCall(call)));
                }
                if !state.terminated {
                    let _ = tx.send(Ok(StreamEvent::Done));
                }
            }
        });

        let stream = stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });
        Ok(Box::new(Box::pin(stream)))
    }
}

/// OpenAI-compatible chat/completions body, mirroring rx4's `openai_request`
/// (system message, tool_calls round-trip, function tool wrapping, effort
/// passthrough) plus `stream_options.include_usage` for cost tracking.
pub(crate) fn chat_request_body(
    messages: &[Message],
    system: &Option<String>,
    model: &str,
    tools: &[Value],
    reasoning_effort: Option<&str>,
) -> Value {
    let mut body = json!({
        "model": model,
        "stream": true,
        "stream_options": {"include_usage": true},
        "messages": [],
    });
    let output = body["messages"]
        .as_array_mut()
        .expect("messages is initialized as an array");
    if let Some(sys) = system {
        output.push(json!({"role": "system", "content": sys}));
    }
    for message in messages {
        let mut entry = json!({"role": message.role.to_string(), "content": message.content});
        if let Some(id) = &message.tool_call_id {
            entry["tool_call_id"] = json!(id);
        }
        if !message.tool_calls.is_empty() {
            entry["tool_calls"] = Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "type": "function",
                            "function": {"name": call.name, "arguments": call.arguments}
                        })
                    })
                    .collect(),
            );
        }
        output.push(entry);
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(
            tools
                .iter()
                .map(|tool| {
                    if tool.get("type").is_some() {
                        tool.clone()
                    } else {
                        json!({"type": "function", "function": tool})
                    }
                })
                .collect(),
        );
    }
    if let Some(effort) = reasoning_effort {
        body["reasoning_effort"] = json!(effort);
    }
    body
}

#[derive(Default)]
struct ChatStreamState {
    calls: BTreeMap<usize, ToolCall>,
    terminated: bool,
}

/// OpenAI-compatible chat SSE events, mirroring rx4's `parse_sse_events`
/// OpenAI branch — except usage chunks never short-circuit: some providers
/// (Z.ai GLM) bundle `usage` with the `finish_reason: tool_calls` chunk,
/// and returning early there drops the tool calls.
fn handle_chat_sse_block(
    block: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<Result<StreamEvent, ProviderError>>,
    state: &mut ChatStreamState,
) -> Result<(), ProviderError> {
    let data = block
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() || data == "[DONE]" {
        return Ok(());
    }
    let event: Value = serde_json::from_str(&data)
        .map_err(|error| ProviderError::Api(format!("invalid chat SSE event: {error}")))?;
    if let Some(usage) = event.get("usage").and_then(parse_token_usage) {
        let _ = tx.send(Ok(StreamEvent::Usage(usage)));
    }
    let delta = &event["choices"][0]["delta"];
    if let Some(content) = delta.get("content").and_then(Value::as_str) {
        if !content.is_empty() {
            let _ = tx.send(Ok(StreamEvent::Delta(content.to_string())));
        }
    }
    if let Some(fragments) = delta.get("tool_calls").and_then(Value::as_array) {
        for fragment in fragments {
            let index = fragment
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let call = state.calls.entry(index).or_insert_with(|| ToolCall {
                id: String::new(),
                name: String::new(),
                arguments: String::new(),
            });
            if let Some(id) = fragment.get("id").and_then(Value::as_str) {
                call.id.push_str(id);
            }
            if let Some(function) = fragment.get("function") {
                if let Some(name) = function.get("name").and_then(Value::as_str) {
                    call.name.push_str(name);
                }
                if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                    call.arguments.push_str(arguments);
                }
            }
        }
    }
    match event["choices"][0]
        .get("finish_reason")
        .and_then(Value::as_str)
    {
        Some("stop") => {
            state.terminated = true;
            let _ = tx.send(Ok(StreamEvent::Done));
        }
        Some("tool_calls") => {
            for call in state.calls.split_off(&0).into_values() {
                let _ = tx.send(Ok(StreamEvent::ToolCall(call)));
            }
            state.terminated = true;
        }
        _ => {}
    }
    Ok(())
}

fn parse_token_usage(value: &Value) -> Option<TokenUsage> {
    let number = |key: &str| value.get(key).and_then(Value::as_u64).unwrap_or(0) as usize;
    let usage = TokenUsage {
        input_tokens: number("input_tokens").max(number("prompt_tokens")),
        output_tokens: number("output_tokens").max(number("completion_tokens")),
        cache_read_tokens: number("cache_read_input_tokens").max(
            value
                .get("prompt_tokens_details")
                .and_then(|details| details.get("cached_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
        ),
        cache_write_tokens: number("cache_creation_input_tokens"),
    };
    (usage.input_tokens > 0
        || usage.output_tokens > 0
        || usage.cache_read_tokens > 0
        || usage.cache_write_tokens > 0)
        .then_some(usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_body_carries_system_tools_and_effort() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "read",
                "description": "read a file",
                "parameters": {"type": "object"}
            }
        })];
        let body = chat_request_body(
            &[Message::user("hi")],
            &Some("sys".to_string()),
            "glm-5.3",
            &tools,
            Some("low"),
        );
        assert_eq!(body["model"], "glm-5.3");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "hi");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["reasoning_effort"], "low");
    }

    fn drain(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<Result<StreamEvent, ProviderError>>,
    ) -> (String, usize, Vec<ToolCall>, usize) {
        let mut deltas = String::new();
        let mut usages = 0;
        let mut tool_calls = Vec::new();
        let mut dones = 0;
        while let Ok(item) = rx.try_recv() {
            match item.unwrap() {
                StreamEvent::Delta(delta) => deltas.push_str(&delta),
                StreamEvent::Usage(_) => usages += 1,
                StreamEvent::ToolCall(call) => tool_calls.push(call),
                StreamEvent::Done => dones += 1,
            }
        }
        (deltas, usages, tool_calls, dones)
    }

    #[test]
    fn chat_stream_parses_deltas_tool_calls_and_usage() {
        let (tx, mut rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<StreamEvent, ProviderError>>();
        let mut state = ChatStreamState::default();
        let blocks = [
            "data: {\"choices\":[{\"delta\":{\"content\":\"po\"}}]}",
            "data: {\"choices\":[{\"delta\":{\"content\":\"ng\"}}]}",
            "data: {\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2}}",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"read\",\"arguments\":\"{\"}}]},\"finish_reason\":null}]}",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"}\"}}]},\"finish_reason\":null}]}",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}",
            "data: [DONE]",
        ];
        for block in blocks {
            handle_chat_sse_block(block, &tx, &mut state).unwrap();
        }
        drop(tx);

        let (deltas, usages, tool_calls, dones) = drain(&mut rx);
        assert_eq!(deltas, "pong");
        assert_eq!(usages, 1);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "c1");
        assert_eq!(tool_calls[0].name, "read");
        assert_eq!(tool_calls[0].arguments, "{}");
        assert_eq!(dones, 0);
        assert!(state.terminated);
    }

    #[test]
    fn usage_bundled_with_finish_still_emits_tool_calls() {
        // Z.ai GLM sends `usage` in the same chunk as
        // `finish_reason: tool_calls`; the tool calls must survive.
        let (tx, mut rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<StreamEvent, ProviderError>>();
        let mut state = ChatStreamState::default();
        let blocks = [
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"ls\",\"arguments\":\"{}\"}}]}}]}",
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2}}",
            "data: [DONE]",
        ];
        for block in blocks {
            handle_chat_sse_block(block, &tx, &mut state).unwrap();
        }
        drop(tx);

        let (_, usages, tool_calls, _) = drain(&mut rx);
        assert_eq!(usages, 1);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "c1");
        assert_eq!(tool_calls[0].name, "ls");
        assert!(state.terminated);
    }
}
