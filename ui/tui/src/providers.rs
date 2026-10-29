use std::io::{stdin, stdout, Write};
use std::sync::Arc;

use rx4::provider::{OpenAIProvider, Provider};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::app::{App, ChatMessage, ConfiguredProvider};
use crate::codex_provider;
use crate::opencode_go;
use crate::provider_catalog;

/// Z.ai GLM Coding Plan endpoint (OpenAI Chat Completions). This is the
/// coding-plan quota endpoint — not `https://api.z.ai/api/paas/v4`.
pub(crate) const ZAI_CHAT_URL: &str = "https://api.z.ai/api/coding/paas/v4/chat/completions";
pub(crate) const ZAI_DEFAULT_MODEL: &str = "glm-5.3";
pub(crate) const ZAI_MODELS: [&str; 2] = ["glm-5.3", "glm-5.3-flash"];
/// Gemini's OpenAI-compatible base; providers append `/chat/completions`.
pub(crate) const GEMINI_OPENAI_BASE_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/openai";
const GOOGLE_KEY_ENV: [&str; 3] = [
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_GENERATIVE_AI_API_KEY",
];

pub(crate) fn oauth_provider(name: &str) -> Option<rs_ai_oauth::OAuthProvider> {
    rs_ai_oauth::OAuthProvider::parse(name)
}

/// Match discovered models to provider clients telekinesis already has.
pub(crate) fn configured_provider_id(
    oauth: rs_ai_oauth::OAuthProvider,
    provider_ids: &[String],
) -> Option<String> {
    let name = oauth.name();
    provider_ids
        .iter()
        .find(|provider| match name {
            "chatgpt" => provider.as_str() == "openai-codex" || provider.as_str() == "openai",
            "grok" => provider.as_str() == "xai",
            "gemini" => provider.as_str() == "google",
            _ => provider.as_str() == name,
        })
        .cloned()
}

pub(crate) fn run_login(provider: Option<&str>) -> anyhow::Result<()> {
    // Ask rather than assume. Silently defaulting to one provider sends the
    // user through an OAuth flow for an account they may not even have.
    let provider = match provider {
        Some(name) => name,
        None => choose_provider()?,
    };
    let Some(oauth) = oauth_provider(provider) else {
        let available = rs_ai_oauth::OAuthProvider::all()
            .iter()
            .map(rs_ai_oauth::OAuthProvider::name)
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::bail!("Unknown provider: {provider}. Available: {available}");
    };
    println!("Starting OAuth flow for {provider}...");
    let tokens = rs_ai_oauth::start_oauth_flow(oauth)?;
    // The shared store, so a login here is visible to every rs_ai_oauth tool.
    let path = rs_ai_oauth::credentials::save(&oauth, &tokens)?;
    println!("Token saved to {}", path.display());
    if oauth == rs_ai_oauth::OAuthProvider::Antigravity {
        println!("Antigravity is not supported yet: the login is saved, but tk cannot use it.");
    }
    Ok(())
}

pub(crate) fn run_login_from_tui(provider: Option<&str>) -> anyhow::Result<()> {
    let raw_mode_was_enabled = disable_raw_mode().is_ok();
    println!("\r\n");
    let login_result = run_login(provider);
    let restore_result = raw_mode_was_enabled
        .then(enable_raw_mode)
        .transpose()
        .map_err(anyhow::Error::from);
    login_result.and(restore_result.map(|_| ()))
}

pub(crate) fn provider_is_configured(provider: &str) -> bool {
    provider_catalog::find(provider)
        .and_then(provider_catalog::env_key)
        .is_some()
        || oauth_provider(provider)
            .and_then(|oauth| rs_ai_oauth::credentials::load(&oauth))
            .is_some_and(|tokens| !tokens.access_token.is_empty())
}

pub(crate) fn push_system_message(app: &mut App, content: impl Into<String>) {
    app.messages.push(ChatMessage {
        role: "system".to_string(),
        content: content.into(),
        is_tool: false,
        tool_name: String::new(),
        tool_call_id: String::new(),
        is_streaming: false,
    });
}

pub(crate) fn providers_summary(app: &App) -> String {
    let api_keys = provider_catalog::API_KEY_PROVIDERS
        .iter()
        // `zai` and `opencode-go` are wired explicitly with their own rows
        // below; the catalog copies carry stale env vars and URLs.
        .filter(|provider| provider.id != "zai" && provider.id != "opencode-go")
        .map(|provider| {
            let status = if provider_catalog::env_key(provider).is_some() {
                "configured"
            } else {
                "not configured"
            };
            format!(
                "  {name:<25} {status:<14} {}",
                provider.env_vars.join(", "),
                name = provider.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let first_class: [(&str, &str, &[&str]); 2] = [
        ("Z.AI Coding Plan", "zai", &["ZAI_API_KEY"]),
        (
            "OpenCode Go",
            "opencode-go",
            &["OPENCODE_GO_API_KEY", "OPENCODE_API_KEY"],
        ),
    ];
    let first_class = first_class
        .iter()
        .map(|(name, id, env_vars)| {
            let status = if api_key(env_vars, id).is_some() {
                "configured"
            } else {
                "not configured"
            };
            format!("  {name:<25} {status:<14} {}", env_vars.join(", "),)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let oauth = [
        ("ChatGPT Codex", "openai"),
        ("Claude", "claude"),
        ("xAI", "grok"),
        ("Google Gemini", "gemini"),
        ("GitHub Copilot", "copilot"),
        ("Kimi", "kimi"),
        ("Antigravity", "antigravity"),
    ]
    .iter()
    .map(|(name, id)| {
        let status = match (provider_is_configured(id), *id) {
            // Antigravity serves the Cloud Code Assist protocol, which tk
            // has no client for; do not claim the login is usable.
            (true, "antigravity") => "not supported yet (login saved, unused)",
            (false, "antigravity") => "not supported yet",
            (true, _) => "configured",
            (false, _) => "not configured",
        };
        format!("  {name:<25} {status}")
    })
    .collect::<Vec<_>>()
    .join("\n");
    let credentials = rs_ai_oauth::credentials::credentials_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "unavailable".to_string());
    let workspace = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    format!(
        "Providers\n  workspace: {workspace}\n  active model: {}\n  credentials: {credentials}\n\nOAuth plans\n{oauth}\n\nAPI-key providers\n{api_keys}\n{first_class}\n\nCommands\n  /providers               searchable provider menu\n  /apikey <provider>       exact API-key setup\n  /login [provider]        OAuth browser login\n  /model [name]            pick a model after setup",
        app.model
    )
}

pub(crate) fn choose_provider() -> anyhow::Result<&'static str> {
    println!("Which provider do you want to log in with?");
    for (index, provider) in rs_ai_oauth::OAuthProvider::all().iter().enumerate() {
        println!("  {}) {}", index + 1, provider.name());
    }
    loop {
        print!("Provider: ");
        stdout().flush()?;
        let mut choice = String::new();
        if stdin().read_line(&mut choice)? == 0 {
            anyhow::bail!("Provider selection cancelled");
        }
        let choice = choice.trim().to_ascii_lowercase();
        if let Some(provider) = rs_ai_oauth::OAuthProvider::all()
            .iter()
            .enumerate()
            .find(|(index, provider)| {
                choice == (index + 1).to_string() || choice == provider.name()
            })
            .map(|(_, provider)| provider.name())
        {
            return Ok(provider);
        }
        println!("Choose a listed number or enter a provider name.");
    }
}

/// A token left by an older telekinesis login whose file name does not match
/// the shared store's provider name — `openai` was written where the store
/// looks for `chatgpt`, so those logins would otherwise read as logged out.
pub(crate) fn legacy_telekinesis_token(provider: &str) -> Option<rs_ai_oauth::OAuthTokens> {
    let path = dirs::home_dir()?
        .join(".telekinesis")
        .join(format!("{provider}_token.json"));
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

pub(crate) fn saved_token(provider: &str, rt: &tokio::runtime::Runtime) -> Option<String> {
    rt.block_on(saved_token_async(provider))
}

/// Token for background model discovery: refresh and persist an expired one.
pub(crate) async fn saved_token_async(provider: &str) -> Option<String> {
    let oauth = oauth_provider(provider)?;
    let mut tokens =
        rs_ai_oauth::credentials::load(&oauth).or_else(|| legacy_telekinesis_token(provider))?;
    if rs_ai_oauth::credentials::is_expired(&tokens) {
        tokens = rs_ai_oauth::refresh_oauth_token(oauth, &tokens)
            .await
            .ok()?;
        rs_ai_oauth::credentials::save(&oauth, &tokens).ok()?;
    }
    (!tokens.access_token.is_empty()).then_some(tokens.access_token)
}

/// Env vars first, then the `~/.telekinesis/keys.json` store (same order as
/// the catalog helper, for providers the catalog does not describe).
fn api_key(env_vars: &[&str], id: &str) -> Option<String> {
    for var in env_vars {
        if let Ok(value) = std::env::var(var) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    provider_catalog::load_provider_key(id).ok().flatten()
}

/// Base URL for the OpenAI provider; `OPENAI_BASE_URL` overrides the default.
pub(crate) fn openai_base_url() -> String {
    std::env::var("OPENAI_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
}

pub(crate) fn setup_providers(rt: &tokio::runtime::Runtime) -> Vec<(ConfiguredProvider, String)> {
    let mut configured = Vec::new();

    if let Some(token) = saved_token("openai", rt) {
        configured.push((
            ConfiguredProvider {
                id: "openai-codex".to_string(),
                name: "ChatGPT Codex".to_string(),
                client: codex_provider::provider_arc(token),
            },
            "gpt-5.5".to_string(),
        ));
    }
    // A stored or env API key is its own provider (`openai`), independent of
    // a ChatGPT Codex login (`openai-codex`).
    if let Some(key) = api_key(&["OPENAI_API_KEY"], "openai") {
        configured.push((
            ConfiguredProvider {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                client: Arc::new(OpenAIProvider::with_base_url(
                    openai_base_url(),
                    key,
                    "openai",
                    "OpenAI",
                )),
            },
            "gpt-5.4".to_string(),
        ));
    }

    // Z.ai GLM Coding Plan — first-class OpenAI-compatible provider on the
    // coding quota endpoint. The shared catalog still points `zai` at the
    // non-coding paas/v4 URL, so this explicit entry owns the id. Chat
    // streaming is tk-native: Z.ai bundles `usage` with the
    // `finish_reason: tool_calls` chunk, which rx4's parser drops.
    if let Some(key) = api_key(&["ZAI_API_KEY"], "zai") {
        configured.push((
            ConfiguredProvider {
                id: "zai".to_string(),
                name: "Z.AI Coding Plan".to_string(),
                client: Arc::new(crate::openai_chat::OpenAiChatProvider::new(
                    key,
                    "zai",
                    "Z.AI Coding Plan",
                    ZAI_CHAT_URL,
                )),
            },
            ZAI_DEFAULT_MODEL.to_string(),
        ));
    }

    // OpenCode Go — DeepSeek V4.1 Flash over chat/completions, Muse Spark
    // 1.3 Contributor over the Responses API (routed by model inside the
    // provider). Only these two Go models are wired.
    if let Some(key) = api_key(&["OPENCODE_GO_API_KEY", "OPENCODE_API_KEY"], "opencode-go") {
        configured.push((
            ConfiguredProvider {
                id: opencode_go::OPENCODE_GO_ID.to_string(),
                name: "OpenCode Go".to_string(),
                client: opencode_go::provider_arc(key),
            },
            opencode_go::OPENCODE_GO_DEFAULT_MODEL.to_string(),
        ));
    }

    if let Some(token) = saved_token("claude", rt) {
        configured.push((
            ConfiguredProvider {
                id: "anthropic".to_string(),
                name: "Claude".to_string(),
                // Subscription OAuth tokens need Bearer auth and the Claude
                // Code profile; rx4's Anthropic client sends `x-api-key`.
                client: Arc::new(crate::claude_oauth::ClaudeOAuthProvider::new(token)),
            },
            "claude-sonnet-4-5".to_string(),
        ));
    }
    if let Some(token) = saved_token("kimi", rt) {
        configured.push((
            ConfiguredProvider {
                id: "moonshot".to_string(),
                name: "Kimi".to_string(),
                client: Arc::new(OpenAIProvider::with_base_url(
                    "https://api.moonshot.ai/v1",
                    token,
                    "moonshot",
                    "Kimi",
                )),
            },
            "kimi-k2.5".to_string(),
        ));
    }

    if let Some(token) = saved_token("copilot", rt) {
        configured.push((
            ConfiguredProvider {
                id: crate::copilot::COPILOT_ID.to_string(),
                name: "GitHub Copilot".to_string(),
                client: Arc::new(crate::copilot::CopilotProvider::new(token)),
            },
            crate::copilot::COPILOT_DEFAULT_MODEL.to_string(),
        ));
    }

    // API key (env, then `/apikey` store) first, then the OAuth login.
    let key_or_login_providers = [
        (
            ["XAI_API_KEY"].as_slice(),
            "grok",
            "https://api.x.ai/v1",
            "xai",
            "xAI",
            "grok-4.5",
        ),
        (
            GOOGLE_KEY_ENV.as_slice(),
            "gemini",
            GEMINI_OPENAI_BASE_URL,
            "google",
            "Google Gemini",
            "gemini-2.0-flash",
        ),
    ];
    configured.extend(key_or_login_providers.iter().filter_map(
        |(env_vars, login, base_url, id, name, model)| {
            api_key(env_vars, id)
                .or_else(|| saved_token(login, rt))
                .map(|key| {
                    (
                        ConfiguredProvider {
                            id: (*id).to_string(),
                            name: (*name).to_string(),
                            client: Arc::new(OpenAIProvider::with_base_url(
                                *base_url, key, *id, *name,
                            )),
                        },
                        (*model).to_string(),
                    )
                })
        },
    ));
    configured.extend(
        provider_catalog::API_KEY_PROVIDERS
            .iter()
            // `zai` and `opencode-go` are wired explicitly above (coding-plan
            // endpoint, Go key names, curated models); the catalog copies
            // still point at stale URLs and model lists.
            .filter(|spec| !matches!(spec.id, "openai" | "xai" | "google" | "zai" | "opencode-go"))
            .filter_map(|spec| {
                let key = provider_catalog::env_key(spec)?;
                let client: Arc<dyn Provider> = match spec.api {
                    provider_catalog::ProviderApi::OpenAiCompatible => Arc::new(
                        OpenAIProvider::with_base_url(spec.base_url, key, spec.id, spec.name),
                    ),
                    provider_catalog::ProviderApi::Anthropic => {
                        Arc::new(OpenAIProvider::anthropic(key))
                    }
                    provider_catalog::ProviderApi::Custom => return None,
                };
                Some((
                    ConfiguredProvider {
                        id: spec.id.to_string(),
                        name: spec.name.to_string(),
                        client,
                    },
                    spec.default_model.to_string(),
                ))
            }),
    );
    if let Some(key) = std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|key| !key.is_empty())
    {
        configured.push((
            ConfiguredProvider {
                id: "openrouter".to_string(),
                name: "OpenRouter".to_string(),
                client: Arc::new(OpenAIProvider::with_base_url(
                    "https://openrouter.ai/api/v1",
                    key,
                    "openrouter",
                    "OpenRouter",
                )),
            },
            "openai/gpt-4o-mini".to_string(),
        ));
    }
    configured.sort_by(|(left, _), (right, _)| left.name.cmp(&right.name));
    configured.dedup_by(|(left, _), (right, _)| left.id == right.id);
    configured
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
    }

    fn with_env(var: &str, value: Option<&str>) -> Option<String> {
        let previous = std::env::var(var).ok();
        match value {
            Some(value) => std::env::set_var(var, value),
            None => std::env::remove_var(var),
        }
        previous
    }

    fn restore_env(var: &str, previous: Option<String>) {
        match previous {
            Some(value) => std::env::set_var(var, value),
            None => std::env::remove_var(var),
        }
    }

    #[test]
    fn zai_base_url_is_the_coding_endpoint() {
        assert_eq!(
            ZAI_CHAT_URL,
            "https://api.z.ai/api/coding/paas/v4/chat/completions"
        );
        assert!(ZAI_CHAT_URL.starts_with("https://api.z.ai/api/coding/paas/v4"));
        assert!(!ZAI_CHAT_URL.starts_with("https://api.z.ai/api/paas/v4/"));
        assert!(!ZAI_CHAT_URL.contains("openai.com"));
        assert_eq!(ZAI_DEFAULT_MODEL, "glm-5.3");
        assert_eq!(ZAI_MODELS, ["glm-5.3", "glm-5.3-flash"]);
    }

    #[test]
    fn openai_base_url_defaults_and_honors_override() {
        let previous = with_env("OPENAI_BASE_URL", None);
        assert_eq!(openai_base_url(), "https://api.openai.com/v1");
        with_env("OPENAI_BASE_URL", Some("https://proxy.local/v1"));
        assert_eq!(openai_base_url(), "https://proxy.local/v1");
        with_env("OPENAI_BASE_URL", Some("   "));
        assert_eq!(openai_base_url(), "https://api.openai.com/v1");
        restore_env("OPENAI_BASE_URL", previous);
    }

    #[test]
    fn setup_providers_wires_zai_coding_plan() {
        let previous = with_env("ZAI_API_KEY", Some("test-key-not-real"));
        let rt = test_runtime();
        let configured = setup_providers(&rt);
        restore_env("ZAI_API_KEY", previous);
        let entry = configured
            .iter()
            .find(|(provider, _)| provider.id == "zai")
            .expect("zai provider configured");
        assert_eq!(entry.0.name, "Z.AI Coding Plan");
        assert_eq!(entry.1, "glm-5.3");
    }

    /// Serializes tests that point process-wide credential paths at temp dirs.
    static CREDENTIAL_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn find<'a>(
        configured: &'a [(ConfiguredProvider, String)],
        id: &str,
    ) -> Option<&'a (ConfiguredProvider, String)> {
        configured.iter().find(|(provider, _)| provider.id == id)
    }

    #[test]
    fn gemini_base_url_is_the_openai_compatible_endpoint() {
        assert_eq!(
            format!("{GEMINI_OPENAI_BASE_URL}/chat/completions"),
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
    }

    #[test]
    fn setup_providers_uses_stored_openai_xai_and_google_keys() {
        let _lock = CREDENTIAL_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let env_vars = ["OPENAI_API_KEY", "XAI_API_KEY"]
            .into_iter()
            .chain(GOOGLE_KEY_ENV)
            .collect::<Vec<_>>();
        let saved = env_vars
            .iter()
            .map(|var| (*var, with_env(var, None)))
            .collect::<Vec<_>>();
        let keys = with_env(
            "TELEKINESIS_KEYS_PATH",
            Some(dir.path().join("keys.json").to_str().unwrap()),
        );
        let creds = with_env("RS_AI_CREDENTIALS_DIR", dir.path().to_str());
        for id in ["openai", "xai", "google"] {
            provider_catalog::save_provider_key(id, "stored-test-key").unwrap();
        }

        let configured = setup_providers(&test_runtime());

        for id in ["openai", "xai", "google"] {
            let _ = provider_catalog::delete_provider_key(id);
        }
        restore_env("RS_AI_CREDENTIALS_DIR", creds);
        restore_env("TELEKINESIS_KEYS_PATH", keys);
        for (var, previous) in saved {
            restore_env(var, previous);
        }
        for id in ["openai", "xai", "google"] {
            assert!(find(&configured, id).is_some(), "stored {id} key ignored");
        }
    }

    #[test]
    fn setup_providers_loads_claude_and_copilot_logins() {
        let _lock = CREDENTIAL_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        for name in ["claude", "copilot"] {
            std::fs::write(
                dir.path().join(format!("{name}.json")),
                format!(
                    r#"{{"access_token":"{name}-test-token","refresh_token":null,"expires_at":0}}"#
                ),
            )
            .unwrap();
        }
        let creds = with_env("RS_AI_CREDENTIALS_DIR", dir.path().to_str());
        let configured = setup_providers(&test_runtime());
        restore_env("RS_AI_CREDENTIALS_DIR", creds);

        let copilot = find(&configured, "copilot").expect("copilot login loaded");
        assert_eq!(copilot.0.client.id(), "copilot");
        assert_eq!(copilot.1, crate::copilot::COPILOT_DEFAULT_MODEL);
        let claude = find(&configured, "anthropic").expect("claude login loaded");
        assert_eq!(claude.0.client.name(), "Claude (subscription)");
    }

    #[test]
    fn setup_providers_wires_opencode_go_with_deepseek_default() {
        let previous = with_env("OPENCODE_GO_API_KEY", Some("test-key-not-real"));
        let rt = test_runtime();
        let configured = setup_providers(&rt);
        restore_env("OPENCODE_GO_API_KEY", previous);
        let entry = configured
            .iter()
            .find(|(provider, _)| provider.id == "opencode-go")
            .expect("opencode-go provider configured");
        assert_eq!(entry.0.name, "OpenCode Go");
        assert_eq!(entry.1, "deepseek-v4.1-flash");
    }
}
