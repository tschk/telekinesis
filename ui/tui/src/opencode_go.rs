use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use rx4::agent::ToolCall;
use rx4::cost::TokenUsage;
use rx4::provider::{Message, Provider, ProviderError, StreamEvent, StreamResult};
use serde_json::{json, Value};

use crate::codex_provider::{messages_to_responses_input, tools_to_responses_tools};
use crate::openai_chat::{session_id, OpenAiChatProvider};

/// Base URL for OpenCode Go chat/completions models such as DeepSeek.
pub const OPENCODE_GO_BASE_URL: &str = "https://opencode.ai/zen/go/v1";
/// Full chat/completions endpoint for DeepSeek V4.1 Flash.
pub const OPENCODE_GO_CHAT_URL: &str = "https://opencode.ai/zen/go/v1/chat/completions";
/// Full Responses endpoint for Muse Spark Contributor models.
pub const OPENCODE_GO_RESPONSES_URL: &str = "https://opencode.ai/zen/go/v1/responses";
/// Provider id used with `tk exec --provider`.
pub const OPENCODE_GO_ID: &str = "opencode-go";
/// Only these Go models are wired; nothing else (no Qwen, no GLM-via-Go).
pub const OPENCODE_GO_MODELS: [&str; 2] = ["muse-spark-1.3-contributor", "deepseek-v4.1-flash"];
/// Default model: the chat/completions path rx4 speaks natively.
pub const OPENCODE_GO_DEFAULT_MODEL: &str = "deepseek-v4.1-flash";

/// Muse Spark models only serve the OpenAI Responses API; everything else
/// on Go (DeepSeek V4.1 Flash) uses chat/completions. Accepts an optional
/// `opencode-go/` config-slug prefix.
pub fn is_responses_model(model: &str) -> bool {
    let bare = model.split('/').next_back().unwrap_or(model);
    bare.starts_with("muse-spark")
}

pub struct OpencodeGoProvider {
    key: String,
    chat: OpenAiChatProvider,
}

impl OpencodeGoProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        let key: String = api_key.into();
        Self {
            chat: OpenAiChatProvider::new(
                key.clone(),
                OPENCODE_GO_ID,
                "OpenCode Go",
                OPENCODE_GO_CHAT_URL,
            )
            .with_session(),
            key,
        }
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
        headers.insert(
            "x-opencode-session",
            HeaderValue::from_str(session_id())
                .map_err(|error| ProviderError::Api(error.to_string()))?,
        );
        headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }

    async fn stream_responses(
        &self,
        messages: &[Message],
        system: &Option<String>,
        model: &str,
        tools: &[Value],
        reasoning_effort: Option<&str>,
    ) -> Result<StreamResult, ProviderError> {
        let bare = model.split('/').next_back().unwrap_or(model);
        let body = responses_request_body(bare, system, messages, tools, reasoning_effort);
        let response = reqwest::Client::new()
            .post(OPENCODE_GO_RESPONSES_URL)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await
            .map_err(|error| ProviderError::Api(format!("OpenCode Go request failed: {error}")))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!(
                "OpenCode Go request failed (HTTP {status}): {}",
                text.chars().take(300).collect::<String>()
            )));
        }

        // Same SSE fan-out shape as the Codex Responses path: deltas stream
        // to the UI, tool calls accumulate and emit at the end of the body.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<StreamEvent, ProviderError>>();
        let mut byte_stream = response.bytes_stream();
        tokio::spawn(async move {
            let mut buffer = String::new();
            let mut calls: BTreeMap<usize, ToolCall> = BTreeMap::new();
            let mut failed: Option<ProviderError> = None;
            while let Some(chunk) = byte_stream.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        failed = Some(ProviderError::Api(format!(
                            "opencode-go stream read failed: {error}"
                        )));
                        break;
                    }
                };
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(position) = buffer.find("\n\n") {
                    let block = buffer[..position].to_string();
                    buffer = buffer[position + 2..].to_string();
                    if let Err(error) = handle_sse_block(&block, &tx, &mut calls) {
                        failed = Some(error);
                        break;
                    }
                }
                if failed.is_some() {
                    break;
                }
            }
            if failed.is_none() && !buffer.trim().is_empty() {
                failed = handle_sse_block(&buffer, &tx, &mut calls).err();
            }
            if let Some(error) = failed {
                let _ = tx.send(Err(error));
            } else {
                for call in calls.into_values() {
                    let _ = tx.send(Ok(StreamEvent::ToolCall(call)));
                }
            }
            let _ = tx.send(Ok(StreamEvent::Done));
        });

        let stream = stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });
        Ok(Box::new(Box::pin(stream)))
    }
}

pub(crate) fn responses_request_body(
    model: &str,
    system: &Option<String>,
    messages: &[Message],
    tools: &[Value],
    reasoning_effort: Option<&str>,
) -> Value {
    let mut body = json!({
        "model": model,
        "stream": true,
        "instructions": system.as_deref().unwrap_or("You are a helpful assistant."),
        "input": messages_to_responses_input(messages),
    });
    let converted = tools_to_responses_tools(tools);
    if !converted.is_empty() {
        body["tools"] = Value::Array(converted);
    }
    if let Some(effort) = reasoning_effort {
        body["reasoning"] = json!({"effort": effort});
    }
    body
}

#[async_trait]
impl Provider for OpencodeGoProvider {
    fn id(&self) -> &str {
        OPENCODE_GO_ID
    }

    fn name(&self) -> &str {
        "OpenCode Go"
    }

    async fn stream(
        &self,
        messages: &[Message],
        system: &Option<String>,
        model: &str,
        tools: &[Value],
        reasoning_effort: Option<&str>,
    ) -> Result<StreamResult, ProviderError> {
        if is_responses_model(model) {
            return self
                .stream_responses(messages, system, model, tools, reasoning_effort)
                .await;
        }
        self.chat
            .stream(messages, system, model, tools, reasoning_effort)
            .await
    }
}

fn handle_sse_block(
    block: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<Result<StreamEvent, ProviderError>>,
    calls: &mut BTreeMap<usize, ToolCall>,
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
        .map_err(|error| ProviderError::Api(format!("invalid OpenCode Go SSE event: {error}")))?;
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match event_type {
        "response.output_text.delta" | "response.reasoning_summary_text.delta" => {
            let delta = event
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !delta.is_empty() {
                let _ = tx.send(Ok(StreamEvent::Delta(delta)));
            }
        }
        "response.output_item.added" | "response.output_item.done" => {
            let item = event.get("item").unwrap_or(&Value::Null);
            if item.get("type").and_then(Value::as_str) == Some("function_call") {
                let index = event
                    .get("output_index")
                    .and_then(Value::as_u64)
                    .unwrap_or(calls.len() as u64) as usize;
                let call = calls.entry(index).or_insert_with(|| ToolCall {
                    id: String::new(),
                    name: String::new(),
                    arguments: String::new(),
                });
                if let Some(value) = item.get("call_id").and_then(Value::as_str) {
                    call.id = value.to_string();
                }
                if let Some(value) = item.get("name").and_then(Value::as_str) {
                    call.name = value.to_string();
                }
                if let Some(value) = item.get("arguments").and_then(Value::as_str) {
                    call.arguments = value.to_string();
                }
            }
        }
        "response.function_call_arguments.delta" => {
            let index = event
                .get("output_index")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let call = calls.entry(index).or_insert_with(|| ToolCall {
                id: event
                    .get("call_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                name: String::new(),
                arguments: String::new(),
            });
            call.arguments.push_str(
                event
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
        }
        "response.function_call_arguments.done" => {
            let index = event
                .get("output_index")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let call = calls.entry(index).or_insert_with(|| ToolCall {
                id: event
                    .get("call_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                name: String::new(),
                arguments: String::new(),
            });
            call.arguments = event
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
        }
        "response.completed" | "response.incomplete" => {
            if let Some(usage) = event
                .get("response")
                .and_then(|response| response.get("usage"))
                .and_then(responses_usage)
            {
                let _ = tx.send(Ok(StreamEvent::Usage(usage)));
            }
        }
        "error" | "response.failed" => {
            return Err(ProviderError::Api(format!(
                "OpenCode Go error: {}",
                event.to_string().chars().take(300).collect::<String>()
            )));
        }
        _ => {}
    }
    Ok(())
}

pub fn provider_arc(api_key: impl Into<String>) -> Arc<dyn Provider> {
    Arc::new(OpencodeGoProvider::new(api_key))
}

fn responses_usage(value: &Value) -> Option<TokenUsage> {
    let number = |key: &str| value.get(key).and_then(Value::as_u64).unwrap_or(0) as usize;
    let usage = TokenUsage {
        input_tokens: number("input_tokens"),
        output_tokens: number("output_tokens"),
        cache_read_tokens: value
            .get("input_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        cache_write_tokens: 0,
    };
    (usage.input_tokens > 0
        || usage.output_tokens > 0
        || usage.cache_read_tokens > 0)
        .then_some(usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_muse_to_responses_and_deepseek_to_chat() {
        assert!(is_responses_model("muse-spark-1.3-contributor"));
        assert!(is_responses_model(
            "opencode-go/muse-spark-1.3-contributor"
        ));
        assert!(!is_responses_model("deepseek-v4.1-flash"));
        assert!(!is_responses_model(
            "opencode-go/deepseek-v4.1-flash"
        ));
    }

    #[test]
    fn allowlist_holds_only_the_two_go_models() {
        assert_eq!(
            OPENCODE_GO_MODELS,
            ["muse-spark-1.3-contributor", "deepseek-v4.1-flash"]
        );
        assert_eq!(OPENCODE_GO_DEFAULT_MODEL, "deepseek-v4.1-flash");
    }

    #[test]
    fn chat_url_is_the_go_chat_completions_endpoint() {
        assert_eq!(
            OPENCODE_GO_CHAT_URL,
            "https://opencode.ai/zen/go/v1/chat/completions"
        );
    }

    #[test]
    fn responses_body_targets_bare_model_with_streaming() {
        let body = responses_request_body(
            "muse-spark-1.3-contributor",
            &Some("sys".to_string()),
            &[Message::user("hi")],
            &[],
            Some("low"),
        );
        assert_eq!(body["model"], "muse-spark-1.3-contributor");
        assert_eq!(body["stream"], true);
        assert_eq!(body["instructions"], "sys");
        assert_eq!(body["reasoning"]["effort"], "low");
        assert!(body["input"].is_array());
    }

    #[test]
    fn streams_sse_deltas_and_collects_tool_calls() {
        let (tx, mut rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<StreamEvent, ProviderError>>();
        let mut calls = BTreeMap::new();
        let blocks = [
            r#"data: {"type":"response.output_text.delta","delta":"pong"}"#,
            r#"data: {"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"c1","name":"read","arguments":""}}"#,
            r#"data: {"type":"response.function_call_arguments.delta","output_index":0,"call_id":"c1","delta":"{\"path\":\""}"#,
            r#"data: {"type":"response.function_call_arguments.done","output_index":0,"call_id":"c1","arguments":"{\"path\":\"a.txt\"}"}"#,
            "data: [DONE]",
        ];
        for block in blocks {
            handle_sse_block(block, &tx, &mut calls).unwrap();
        }
        drop(tx);
        let mut deltas = Vec::new();
        while let Ok(item) = rx.try_recv() {
            if let StreamEvent::Delta(delta) = item.unwrap() {
                deltas.push(delta);
            }
        }
        assert_eq!(deltas, vec!["pong".to_string()]);
        let call = calls.get(&0).expect("collected call");
        assert_eq!(call.id, "c1");
        assert_eq!(call.name, "read");
        assert_eq!(call.arguments, r#"{"path":"a.txt"}"#);
    }

    #[test]
    fn sse_error_blocks_fail_the_stream() {
        let (tx, _rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<StreamEvent, ProviderError>>();
        let mut calls = BTreeMap::new();
        let result = handle_sse_block(
            r#"data: {"type":"error","message":"boom"}"#,
            &tx,
            &mut calls,
        );
        assert!(result.is_err());
    }

    #[test]
    fn completed_event_reports_usage_for_cost_tracking() {
        let (tx, mut rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<StreamEvent, ProviderError>>();
        let mut calls = BTreeMap::new();
        handle_sse_block(
            r#"data: {"type":"response.completed","response":{"usage":{"input_tokens":12,"output_tokens":3,"input_tokens_details":{"cached_tokens":4}}}}"#,
            &tx,
            &mut calls,
        )
        .unwrap();
        drop(tx);
        let mut usages = Vec::new();
        while let Ok(item) = rx.try_recv() {
            if let StreamEvent::Usage(usage) = item.unwrap() {
                usages.push(usage);
            }
        }
        assert_eq!(usages.len(), 1);
        assert_eq!(usages[0].input_tokens, 12);
        assert_eq!(usages[0].output_tokens, 3);
        assert_eq!(usages[0].cache_read_tokens, 4);
    }
}
