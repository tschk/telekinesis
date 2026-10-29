//! GitHub Copilot chat for `tk login copilot`.
//!
//! The saved login is a GitHub OAuth token. Copilot's chat API wants a
//! short-lived Copilot token minted from it at
//! `api.github.com/copilot_internal/v2/token`, sent with editor identity
//! headers to the account's API host (`proxy-ep` in the token, or
//! `endpoints.api` in the exchange response). The Copilot token is cached
//! and re-minted shortly before it expires.

use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rx4::provider::{Message, Provider, ProviderError, StreamResult};
use serde_json::Value;

use crate::openai_chat::OpenAiChatProvider;

pub(crate) const COPILOT_ID: &str = "copilot";
pub(crate) const COPILOT_DEFAULT_MODEL: &str = "gpt-4.1";
const TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const DEFAULT_API: &str = "https://api.individual.githubcopilot.com";
const HEADERS: [(&str, &str); 4] = [
    ("user-agent", "GitHubCopilotChat/0.35.0"),
    ("editor-version", "vscode/1.107.0"),
    ("editor-plugin-version", "copilot-chat/0.35.0"),
    ("copilot-integration-id", "vscode-chat"),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CopilotSession {
    pub token: String,
    pub api_base: String,
    pub expires_at: u64,
}

/// Parse the token-exchange response into a usable chat session.
pub(crate) fn parse_session(value: &Value) -> Option<CopilotSession> {
    let token = value.get("token")?.as_str()?.to_string();
    let expires_at = value.get("expires_at")?.as_u64()?;
    let api_base = value
        .pointer("/endpoints/api")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            token
                .split(';')
                .find_map(|part| part.strip_prefix("proxy-ep="))
                .map(|host| format!("https://{}", host.replacen("proxy.", "api.", 1)))
        })
        .unwrap_or_else(|| DEFAULT_API.to_string());
    Some(CopilotSession {
        token,
        api_base: api_base.trim_end_matches('/').to_string(),
        expires_at,
    })
}

pub struct CopilotProvider {
    github_token: String,
    session: tokio::sync::Mutex<Option<CopilotSession>>,
}

impl CopilotProvider {
    pub fn new(github_token: impl Into<String>) -> Self {
        Self {
            github_token: github_token.into(),
            session: tokio::sync::Mutex::new(None),
        }
    }

    async fn session(&self) -> Result<CopilotSession, ProviderError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let mut cached = self.session.lock().await;
        if let Some(session) = cached.as_ref().filter(|s| s.expires_at > now + 300) {
            return Ok(session.clone());
        }
        let mut request = reqwest::Client::new()
            .get(TOKEN_URL)
            .bearer_auth(&self.github_token)
            .header("accept", "application/json");
        for (name, value) in HEADERS {
            request = request.header(name, value);
        }
        let response = request
            .send()
            .await
            .map_err(|error| ProviderError::Http(format!("Copilot token exchange: {error}")))?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(ProviderError::Api(format!(
                "Copilot token exchange failed (HTTP {status}); run `tk login copilot`: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        let session = serde_json::from_str(&text)
            .ok()
            .as_ref()
            .and_then(parse_session)
            .ok_or_else(|| ProviderError::Api("Copilot token exchange: bad response".into()))?;
        *cached = Some(session.clone());
        Ok(session)
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    fn id(&self) -> &str {
        COPILOT_ID
    }

    fn name(&self) -> &str {
        "GitHub Copilot"
    }

    async fn stream(
        &self,
        messages: &[Message],
        system: &Option<String>,
        model: &str,
        tools: &[Value],
        reasoning_effort: Option<&str>,
    ) -> Result<StreamResult, ProviderError> {
        let session = self.session().await?;
        OpenAiChatProvider::new(
            session.token,
            COPILOT_ID,
            "GitHub Copilot",
            format!("{}/chat/completions", session.api_base),
        )
        .with_headers(&HEADERS)
        .stream(messages, system, model, tools, reasoning_effort)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_prefers_endpoint_then_proxy_host() {
        let with_endpoint = json!({
            "token": "tid=1;exp=9;proxy-ep=proxy.business.githubcopilot.com;",
            "expires_at": 1_900_000_000u64,
            "endpoints": {"api": "https://api.enterprise.githubcopilot.com/"}
        });
        assert_eq!(
            parse_session(&with_endpoint).unwrap(),
            CopilotSession {
                token: "tid=1;exp=9;proxy-ep=proxy.business.githubcopilot.com;".into(),
                api_base: "https://api.enterprise.githubcopilot.com".into(),
                expires_at: 1_900_000_000,
            }
        );
        let proxy_only = json!({
            "token": "tid=1;exp=9;proxy-ep=proxy.business.githubcopilot.com;sku=x",
            "expires_at": 5u64
        });
        assert_eq!(
            parse_session(&proxy_only).unwrap().api_base,
            "https://api.business.githubcopilot.com"
        );
        assert!(parse_session(&json!({"token": "t"})).is_none());
    }
}
