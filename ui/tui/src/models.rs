use rx4::{ModelInfo, ModelRegistry};

use std::collections::HashMap;

use crate::app::ConfiguredProvider;
use crate::opencode_go;
use crate::provider_catalog;
use crate::providers::{ZAI_DEFAULT_MODEL, ZAI_MODELS};

pub(crate) const GPT_5_CONTEXT_WINDOW: usize = 1_050_000;

/// pi 0.83.0 `openai-codex.json` — models beyond rs_ai_oauth's
/// `CHATGPT_CODEX_MODELS`, completing the exact ChatGPT Codex catalog:
/// gpt-5.3-codex-spark, gpt-5.4, gpt-5.4-mini, gpt-5.5, gpt-5.6-luna,
/// gpt-5.6-sol, gpt-5.6-terra.
pub(crate) const PI_CODEX_GPT56: [&str; 3] = ["gpt-5.6-luna", "gpt-5.6-sol", "gpt-5.6-terra"];

/// ChatGPT Codex models the offline fallback must not offer.
///
/// A ChatGPT account cannot call these — the Codex API answers
/// `400 {"detail":"The 'gpt-5.3-codex-spark' model is not supported when using
/// Codex with a ChatGPT account."}` and `GET /codex/models` omits them. The
/// account listing is authoritative; this only keeps the fallback honest.
pub(crate) const CODEX_UNSUPPORTED_ON_CHATGPT: [&str; 1] = ["gpt-5.3-codex-spark"];

/// `GET {base}/codex/models`, which reports what the signed-in plan can use.
const CODEX_MODELS_URL: &str = "https://chatgpt.com/backend-api/codex/models";

/// The backend gates that listing on a client version and returns an empty
/// list below the newest model's `minimal_client_version` (0.155.0 today).
/// This is the Codex protocol level tk implements, not tk's own version.
pub(crate) const CODEX_CLIENT_VERSION: &str = "1.0.0";

#[derive(serde::Deserialize)]
struct CodexModelEntry {
    slug: String,
    #[serde(default)]
    context_window: Option<u64>,
    #[serde(default)]
    input_modalities: Vec<String>,
    /// Level objects (`{effort, description}`); presence is what matters.
    #[serde(default)]
    supported_reasoning_levels: Vec<serde_json::Value>,
    #[serde(default)]
    visibility: Option<String>,
}

/// Models the signed-in ChatGPT plan can actually call, with the windows and
/// reasoning levels the backend reports. Internal entries (`visibility:
/// "hide"`, e.g. the reserve and auto-review models) stay out of the picker.
pub(crate) async fn fetch_codex_models(
    token: String,
    account_id: Option<String>,
) -> Vec<ModelInfo> {
    let Some(client) = models_client() else {
        return Vec::new();
    };
    let mut request = client
        .get(CODEX_MODELS_URL)
        .query(&[("client_version", CODEX_CLIENT_VERSION)])
        .header("originator", "telekinesis")
        .header("accept", "application/json")
        .bearer_auth(&token);
    if let Some(account_id) = account_id.filter(|id| !id.trim().is_empty()) {
        request = request.header("chatgpt-account-id", account_id);
    }
    let Ok(response) = request.send().await else {
        return Vec::new();
    };
    let Ok(response) = response.error_for_status() else {
        return Vec::new();
    };
    let Ok(value) = response.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    codex_model_infos(&value)
}

/// Offline ChatGPT Codex catalog: the pi list minus models a ChatGPT account
/// cannot call. `fetch_codex_models` replaces it once the account answers.
pub(crate) fn offline_codex_models() -> impl Iterator<Item = &'static str> {
    rs_ai_oauth::codex::CHATGPT_CODEX_MODELS
        .iter()
        .chain(PI_CODEX_GPT56.iter())
        .copied()
        .filter(|id| !CODEX_UNSUPPORTED_ON_CHATGPT.contains(id))
}

/// Parse a `/codex/models` payload into host metadata.
pub(crate) fn codex_model_infos(value: &serde_json::Value) -> Vec<ModelInfo> {
    let Some(entries) = value.get("models").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| serde_json::from_value::<CodexModelEntry>(entry.clone()).ok())
        .filter(|entry| entry.visibility.as_deref() != Some("hide"))
        .map(|entry| {
            let mut info = host_model_info("openai-codex", &entry.slug);
            if let Some(context_window) = entry
                .context_window
                .and_then(|window| usize::try_from(window).ok())
                .filter(|window| *window > 0)
            {
                info.context_window = context_window;
            }
            info.supports_vision = entry
                .input_modalities
                .iter()
                .any(|modality| modality == "image");
            info.supports_reasoning = !entry.supported_reasoning_levels.is_empty();
            info.supports_reasoning_effort = info.supports_reasoning;
            info
        })
        .collect()
}

/// pi 0.83.0 `openai.json` GPT-5.x family, injected for the API-key provider
/// and deduped against rx4's registry.
pub(crate) const PI_OPENAI_GPT5: [&str; 21] = [
    "gpt-5",
    "gpt-5-chat-latest",
    "gpt-5-mini",
    "gpt-5-nano",
    "gpt-5-pro",
    "gpt-5.1",
    "gpt-5.2",
    "gpt-5.2-chat-latest",
    "gpt-5.2-pro",
    "gpt-5.3-chat-latest",
    "gpt-5.3-codex",
    "gpt-5.3-codex-spark",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.4-nano",
    "gpt-5.4-pro",
    "gpt-5.5",
    "gpt-5.5-pro",
    "gpt-5.6-luna",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
];

pub(crate) fn context_window_for_model(model: &str) -> usize {
    // pi 0.83.0 context windows for models outside rx4's registry; newer
    // models synced from the models.dev snapshot (2026-08).
    if model.starts_with("gpt-5.5") || model.starts_with("gpt-5.6") {
        GPT_5_CONTEXT_WINDOW
    } else {
        let lower = model.to_ascii_lowercase();
        match model {
            "gpt-5.4-pro" => GPT_5_CONTEXT_WINDOW,
            "gpt-5.4-nano" | "gpt-5-mini" | "gpt-5-nano" | "gpt-5-pro" | "gpt-5.1" | "gpt-5.2"
            | "gpt-5.2-pro" | "gpt-5.3-codex" => 400_000,
            "gpt-5-chat-latest"
            | "gpt-5.2-chat-latest"
            | "gpt-5.3-chat-latest"
            | "gpt-5.3-codex-spark" => 128_000,
            _ => context_from_family(&lower),
        }
    }
}

/// Prefix-based windows for the models.dev catalog generation. Matched on
/// lowercased ids so provider prefixes (`cline-pass/…`) don't hide the family.
fn context_from_family(lower: &str) -> usize {
    const M1M: usize = 1_000_000;
    const M2M: usize = 2_000_000;
    if lower.starts_with("deepseek-v4") {
        return M1M;
    }
    if lower.starts_with("deepseek") {
        return M1M; // deepseek-chat/reasoner also 1M in models.dev
    }
    if lower.starts_with("grok-4.20") || lower.contains("grok-4-1-fast") {
        return M2M;
    }
    if lower.starts_with("grok-4") || lower.starts_with("grok-3") {
        return M1M;
    }
    if lower.starts_with("claude-opus-4") || lower.starts_with("claude-sonnet-4") {
        return M1M;
    }
    if lower.starts_with("claude-haiku") {
        return 200_000;
    }
    if lower.starts_with("gemini-3") || lower.starts_with("gemini-2.5") {
        return 1_048_576;
    }
    if lower.starts_with("kimi-k2") {
        return 262_144;
    }
    if lower.starts_with("minimax-m2") || lower.starts_with("minimax-m") {
        return 204_800;
    }
    if lower.starts_with("qwen3.6-max") || lower.starts_with("qwen3.6-plus") {
        return 262_144;
    }
    if lower.starts_with("qwen3.6") || lower.starts_with("qwen3.5") {
        return M1M;
    }
    if lower.starts_with("glm-5") || lower.starts_with("glm-4") {
        return 200_000;
    }
    if lower.starts_with("mimo-v2") {
        return 256_000;
    }
    128_000
}

pub(crate) fn host_model_info(provider: &str, id: &str) -> ModelInfo {
    let mut info = ModelInfo::new(provider, id, context_window_for_model(id), 8_192);
    info.supports_tools = true;
    info.supports_reasoning = id.contains("reason")
        || id.starts_with("o1")
        || id.starts_with("o3")
        || id.starts_with("gpt-5");
    info.supports_reasoning_effort = info.supports_reasoning;
    info
}

/// A configured provider that publishes a live `/models` listing.
pub(crate) struct LiveModelsTarget {
    pub(crate) provider: String,
    pub(crate) url: String,
    pub(crate) key: String,
    pub(crate) api: provider_catalog::ProviderApi,
}

/// Live `/models` targets for the configured providers.
///
/// The offline catalog only moves when a new `rs_ai_providers` release ships,
/// so a provider that adds a model today should appear in the picker today.
/// OAuth plans come from `rs_ai_oauth` discovery and OpenRouter keeps its
/// richer capability parsing, so both stay on their dedicated paths.
pub(crate) fn live_models_targets(provider_ids: &[String]) -> Vec<LiveModelsTarget> {
    live_models_targets_with(provider_ids, provider_catalog::env_key)
}

fn live_models_targets_with(
    provider_ids: &[String],
    key_of: impl Fn(&provider_catalog::ProviderSpec) -> Option<String>,
) -> Vec<LiveModelsTarget> {
    provider_ids
        .iter()
        .filter(|id| id.as_str() != "openrouter")
        .filter_map(|id| {
            let spec = provider_catalog::by_id(id)?;
            if spec.base_url.is_empty() || matches!(spec.api, provider_catalog::ProviderApi::Custom)
            {
                return None;
            }
            Some(LiveModelsTarget {
                provider: id.clone(),
                url: format!("{}/models", spec.base_url.trim_end_matches('/')),
                key: key_of(spec)?,
                api: spec.api,
            })
        })
        .collect()
}

/// Parse a `/models` payload into host metadata. OpenAI-compatible and
/// Anthropic listings both return a `data` array of objects carrying `id`; a
/// window the provider reports wins over the family heuristic.
pub(crate) fn live_model_infos(provider: &str, value: &serde_json::Value) -> Vec<ModelInfo> {
    let Some(entries) = value.get("data").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id").and_then(serde_json::Value::as_str)?.trim();
            if id.is_empty() {
                return None;
            }
            let mut info = host_model_info(provider, id);
            if let Some(context_window) = entry
                .get("context_length")
                .or_else(|| {
                    entry
                        .get("top_provider")
                        .and_then(|provider| provider.get("context_length"))
                })
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|context_window| *context_window > 0)
            {
                info.context_window = context_window;
            }
            Some(info)
        })
        .collect()
}

/// Shared client for the model-listing endpoints: bounded, so one slow
/// provider cannot hold a refresh open.
fn models_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .ok()
}

/// Fetch one provider's live `/models` listing. A provider that fails, times
/// out, or answers in an unexpected shape contributes nothing.
pub(crate) async fn fetch_live_models(target: LiveModelsTarget) -> (String, Vec<ModelInfo>) {
    let LiveModelsTarget {
        provider,
        url,
        key,
        api,
    } = target;
    let Some(client) = models_client() else {
        return (provider, Vec::new());
    };
    let request = match api {
        provider_catalog::ProviderApi::Anthropic => client
            .get(&url)
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01"),
        _ => client.get(&url).bearer_auth(&key),
    };
    let Ok(response) = request.send().await else {
        return (provider, Vec::new());
    };
    let Ok(response) = response.error_for_status() else {
        return (provider, Vec::new());
    };
    let Ok(value) = response.json::<serde_json::Value>().await else {
        return (provider, Vec::new());
    };
    let infos = live_model_infos(&provider, &value);
    (provider, infos)
}

/// OpenRouter's `/models` listing, which reports capability fields the generic
/// parser ignores. No key means the provider is not configured.
pub(crate) async fn fetch_openrouter_models(key: Option<String>) -> Vec<ModelInfo> {
    let Some(key) = key else {
        return Vec::new();
    };
    let Some(client) = models_client() else {
        return Vec::new();
    };
    let response = match client
        .get("https://openrouter.ai/api/v1/models")
        .bearer_auth(key)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        _ => return Vec::new(),
    };
    let Ok(value) = response.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .map(|entries| entries.iter().filter_map(openrouter_model_info).collect())
        .unwrap_or_default()
}

/// models.dev catalog endpoint: the shared model database behind OpenCode and
/// pi, carrying the windows and capability flags provider listings omit.
const MODELS_DEV_URL: &str = "https://models.dev/api.json";

#[derive(serde::Deserialize)]
struct ModelsDevModel {
    #[serde(default)]
    tool_call: bool,
    #[serde(default)]
    reasoning: bool,
    #[serde(default)]
    attachment: bool,
    #[serde(default)]
    modalities: Option<ModelsDevModalities>,
    #[serde(default)]
    limit: Option<ModelsDevLimit>,
}

#[derive(serde::Deserialize)]
struct ModelsDevModalities {
    #[serde(default)]
    input: Vec<String>,
}

#[derive(serde::Deserialize)]
struct ModelsDevLimit {
    context: Option<u64>,
    output: Option<u64>,
}

#[derive(serde::Deserialize, Default)]
struct ModelsDevProvider {
    #[serde(default)]
    models: HashMap<String, ModelsDevModel>,
}

/// models.dev metadata for the models this host already lists, grouped by
/// provider.
///
/// Membership stays with the provider's own listing; models.dev supplies what
/// that listing leaves out (context/output windows, tool, reasoning, vision).
/// Providers whose ids are vendor-namespaced (`z-ai/glm-5.3-flash` where
/// models.dev lists `cline-pass/glm-5.3-flash`) match on the last segment.
pub(crate) async fn fetch_models_dev_models(
    known: &[(String, String)],
) -> Vec<(String, Vec<ModelInfo>)> {
    if known.is_empty() {
        return Vec::new();
    }
    let Some(catalog) = fetch_models_dev_catalog().await else {
        return Vec::new();
    };
    let mut by_provider: HashMap<String, Vec<ModelInfo>> = HashMap::new();
    for (provider, id) in known {
        let Some(key) = models_dev_key(&catalog, provider) else {
            continue;
        };
        let Some(models) = catalog.get(&key).map(|entry| &entry.models) else {
            continue;
        };
        let Some(model) = models.get(id).or_else(|| {
            let segment = id.rsplit('/').next();
            models
                .iter()
                .find(|(candidate, _)| candidate.rsplit('/').next() == segment)
                .map(|(_, model)| model)
        }) else {
            continue;
        };
        let mut info = host_model_info(provider, id);
        apply_models_dev(&mut info, model);
        by_provider.entry(provider.clone()).or_default().push(info);
    }
    by_provider.into_iter().collect()
}

async fn fetch_models_dev_catalog() -> Option<HashMap<String, ModelsDevProvider>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .ok()?;
    let response = client.get(MODELS_DEV_URL).send().await.ok()?;
    let response = response.error_for_status().ok()?;
    response.json().await.ok()
}

/// models.dev provider key for a host provider id: the id itself, the catalog
/// id it resolves to, or one of that provider's aliases.
fn models_dev_key(
    catalog: &HashMap<String, ModelsDevProvider>,
    provider_id: &str,
) -> Option<String> {
    let spec = provider_catalog::find(provider_id);
    let mut candidates: Vec<&str> = vec![provider_id];
    if let Some(spec) = spec {
        candidates.push(spec.id);
        candidates.extend(spec.aliases.iter().copied());
    }
    candidates
        .into_iter()
        .find(|candidate| catalog.contains_key(*candidate))
        .map(str::to_string)
}

/// Enrichment only adds: a catalog gap must not take a capability away.
fn apply_models_dev(info: &mut ModelInfo, model: &ModelsDevModel) {
    if let Some(context) = model
        .limit
        .as_ref()
        .and_then(|limit| limit.context)
        .and_then(|context| usize::try_from(context).ok())
        .filter(|context| *context > 0)
    {
        info.context_window = context;
    }
    if let Some(output) = model
        .limit
        .as_ref()
        .and_then(|limit| limit.output)
        .and_then(|output| usize::try_from(output).ok())
        .filter(|output| *output > 0)
    {
        info.max_output_tokens = output;
    }
    if model.tool_call {
        info.supports_tools = true;
    }
    if model.reasoning {
        info.supports_reasoning = true;
        info.supports_reasoning_effort = true;
    }
    let image_input = model.attachment
        || model
            .modalities
            .as_ref()
            .is_some_and(|modalities| modalities.input.iter().any(|modality| modality == "image"));
    if image_input {
        info.supports_vision = true;
    }
}

/// Build the metadata Rotary receives from this host's configured providers.
/// Dynamic provider snapshots extend this registry later; these entries only
/// keep the picker and compaction useful before the first network refresh.
pub(crate) fn initial_model_registry(providers: &[(ConfiguredProvider, String)]) -> ModelRegistry {
    let mut registry = ModelRegistry::new();
    for (provider, default_model) in providers {
        registry.register(host_model_info(&provider.id, default_model));
        if provider.id == "openai-codex" {
            for id in offline_codex_models() {
                registry.register(host_model_info(&provider.id, id));
            }
        }
        if let Some(spec) = provider_catalog::by_id(&provider.id) {
            // `zai` and `opencode-go` are wired explicitly with curated model
            // lists; the shared catalog still carries stale entries for them.
            if provider.id != "zai" && provider.id != opencode_go::OPENCODE_GO_ID {
                for id in spec.models {
                    registry.register(host_model_info(&provider.id, id));
                }
            }
        }
        if provider.id == "zai" {
            registry.register(host_model_info(&provider.id, ZAI_DEFAULT_MODEL));
            for id in ZAI_MODELS {
                registry.register(host_model_info(&provider.id, id));
            }
        }
        if provider.id == opencode_go::OPENCODE_GO_ID {
            for id in opencode_go::OPENCODE_GO_MODELS {
                registry.register(host_model_info(&provider.id, id));
            }
        }
        if provider.id == "openai" {
            for id in PI_OPENAI_GPT5 {
                registry.register(host_model_info(&provider.id, id));
            }
        }
    }
    registry
}

pub(crate) fn oauth_model_info(provider: &str, model: rs_ai_oauth::ModelInfo) -> ModelInfo {
    let context_window = model
        .limits
        .context_window
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_else(|| context_window_for_model(&model.id));
    let max_output_tokens = model
        .limits
        .max_output_tokens
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(8_192);
    let mut info = ModelInfo::new(provider, model.id, context_window, max_output_tokens);
    info.supports_tools = model.capabilities.contains("tool_calling")
        || model
            .supported_parameters
            .iter()
            .any(|parameter| matches!(parameter.as_str(), "tools" | "tool_choice"));
    info.supports_vision = model.capabilities.contains("image_input")
        || model
            .input_modalities
            .iter()
            .any(|modality| modality == "image");
    info.supports_reasoning = model.capabilities.contains("extended_thinking")
        || model
            .supported_parameters
            .iter()
            .any(|parameter| matches!(parameter.as_str(), "reasoning" | "include_reasoning"));
    info.supports_reasoning_effort = info.supports_reasoning;
    info
}

pub(crate) fn openrouter_model_info(value: &serde_json::Value) -> Option<ModelInfo> {
    let id = value.get("id")?.as_str()?;
    let context_window = value
        .get("top_provider")
        .and_then(|provider| provider.get("context_length"))
        .or_else(|| value.get("context_length"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(128_000);
    let max_output_tokens = value
        .get("top_provider")
        .and_then(|provider| provider.get("max_completion_tokens"))
        .or_else(|| value.get("max_output_tokens"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(8_192);
    let mut info = ModelInfo::new("openrouter", id, context_window, max_output_tokens);
    if let Some(parameters) = value
        .get("supported_parameters")
        .and_then(serde_json::Value::as_array)
    {
        info.supports_tools = parameters
            .iter()
            .any(|parameter| matches!(parameter.as_str(), Some("tools") | Some("tool_choice")));
        info.supports_reasoning = parameters.iter().any(|parameter| {
            matches!(
                parameter.as_str(),
                Some("reasoning") | Some("include_reasoning")
            )
        });
        info.supports_reasoning_effort = info.supports_reasoning;
    }
    info.supports_vision = value
        .get("architecture")
        .and_then(|architecture| architecture.get("input_modalities"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|modalities| {
            modalities
                .iter()
                .any(|modality| modality.as_str() == Some("image"))
        });
    Some(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn configured(id: &str, default_model: &str) -> (ConfiguredProvider, String) {
        (
            ConfiguredProvider {
                id: id.to_string(),
                name: id.to_string(),
                client: Arc::new(rx4::provider::OpenAIProvider::with_base_url(
                    "https://example.invalid/v1",
                    "test-key-not-real",
                    id,
                    id,
                )),
            },
            default_model.to_string(),
        )
    }

    fn model_ids(registry: &ModelRegistry, provider: &str) -> Vec<String> {
        let mut ids: Vec<String> = registry
            .models()
            .filter(|info| info.provider == provider)
            .map(|info| info.id.clone())
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn registry_registers_only_curated_go_models() {
        let registry = initial_model_registry(&[configured("opencode-go", "deepseek-v4.1-flash")]);
        assert_eq!(
            model_ids(&registry, "opencode-go"),
            vec![
                "deepseek-v4.1-flash".to_string(),
                "muse-spark-1.3-contributor".to_string(),
            ]
        );
    }

    #[test]
    fn registry_registers_only_curated_zai_models() {
        let registry = initial_model_registry(&[configured("zai", "glm-5.3")]);
        assert_eq!(
            model_ids(&registry, "zai"),
            vec!["glm-5.3".to_string(), "glm-5.3-flash".to_string()]
        );
    }

    #[test]
    fn parses_live_listing_ids_and_windows() {
        let value = serde_json::json!({
            "object": "list",
            "data": [
                {"id": "glm-5.3-flash"},
                {"id": "glm-5.3", "context_length": 131_072},
                {"id": "glm-5.3:batch", "top_provider": {"context_length": 65_536}},
                {"id": ""},
                {"object": "model"},
            ],
        });
        let infos = live_model_infos("zai-coding-plan", &value);
        let ids: Vec<&str> = infos.iter().map(|info| info.id.as_str()).collect();
        assert_eq!(ids, ["glm-5.3-flash", "glm-5.3", "glm-5.3:batch"]);
        // glm-5* has no reported window, so the family heuristic applies.
        assert_eq!(infos[0].context_window, 200_000);
        assert_eq!(infos[1].context_window, 131_072);
        assert_eq!(infos[2].context_window, 65_536);
        assert!(infos.iter().all(|info| info.provider == "zai-coding-plan"));
    }

    #[test]
    fn live_listing_ignores_unexpected_payloads() {
        assert!(live_model_infos("zai-coding-plan", &serde_json::json!({})).is_empty());
        assert!(
            live_model_infos("zai-coding-plan", &serde_json::json!({"data": "nope"})).is_empty()
        );
    }

    #[test]
    fn live_targets_cover_catalog_providers_only() {
        let ids = vec![
            "zai-coding-plan".to_string(),
            "openrouter".to_string(),
            "openai-codex".to_string(),
        ];
        let targets = live_models_targets_with(&ids, |_| Some("test-key".to_string()));
        assert_eq!(targets.len(), 1, "only the catalog provider is fetched");
        assert_eq!(targets[0].provider, "zai-coding-plan");
        assert_eq!(targets[0].url, "https://api.z.ai/api/coding/paas/v4/models");
        assert!(
            targets.iter().all(|target| target.provider != "openrouter"),
            "OpenRouter keeps its dedicated capability parsing"
        );
    }

    #[test]
    fn live_targets_require_a_key() {
        let ids = vec!["zai-coding-plan".to_string()];
        assert!(live_models_targets_with(&ids, |_| None).is_empty());
    }

    #[test]
    fn models_dev_metadata_enriches_without_removing_capabilities() {
        let model: ModelsDevModel = serde_json::from_value(serde_json::json!({
            "id": "glm-5.3-flash",
            "tool_call": true,
            "reasoning": true,
            "attachment": true,
            "modalities": {"input": ["text", "image"], "output": ["text"]},
            "limit": {"context": 1_000_000, "output": 131_072},
        }))
        .unwrap();
        let mut info = host_model_info("zai-coding-plan", "glm-5.3-flash");
        apply_models_dev(&mut info, &model);
        assert_eq!(info.context_window, 1_000_000);
        assert_eq!(info.max_output_tokens, 131_072);
        assert!(info.supports_tools);
        assert!(info.supports_reasoning);
        assert!(info.supports_vision);

        // A sparse entry leaves the host's own capabilities alone.
        let sparse: ModelsDevModel = serde_json::from_value(serde_json::json!({})).unwrap();
        let mut info = host_model_info("zai-coding-plan", "glm-5.3-flash");
        apply_models_dev(&mut info, &sparse);
        assert_eq!(info.context_window, 200_000);
        assert!(info.supports_tools);
        assert!(!info.supports_vision);
    }

    #[test]
    fn codex_listing_keeps_only_the_plan_models() {
        let value = serde_json::json!({"models": [
            {
                "slug": "gpt-6-astra",
                "context_window": 272_000,
                "input_modalities": ["text", "image"],
                "supported_reasoning_levels": [{"effort": "xhigh"}, {"effort": "max"}],
                "visibility": "list",
            },
            {
                "slug": "codex-auto-review",
                "context_window": 272_000,
                "input_modalities": ["text"],
                "visibility": "hide",
            },
            {"slug": "gpt-5.5", "input_modalities": ["text"]},
            {"nope": true},
        ]});
        let infos = codex_model_infos(&value);
        let ids: Vec<&str> = infos.iter().map(|info| info.id.as_str()).collect();
        assert_eq!(ids, ["gpt-6-astra", "gpt-5.5"], "hidden entries stay out");
        assert_eq!(infos[0].context_window, 272_000);
        assert!(infos[0].supports_vision);
        assert!(infos[0].supports_reasoning);
        assert!(infos[0].supports_reasoning_effort);
        assert!(!infos[1].supports_vision, "text-only model");
        assert!(!infos[1].supports_reasoning);
        assert!(infos.iter().all(|info| info.provider == "openai-codex"));
    }

    #[test]
    fn offline_codex_fallback_drops_models_a_chatgpt_account_cannot_call() {
        let ids: Vec<&str> = offline_codex_models().collect();
        assert!(!ids.contains(&"gpt-5.3-codex-spark"));
        assert!(ids.contains(&"gpt-5.5"));
        assert!(ids.contains(&"gpt-5.6-luna"));
    }

    #[test]
    fn models_dev_keys_follow_catalog_aliases() {
        let catalog: HashMap<String, ModelsDevProvider> = [
            ("cline-pass".to_string(), ModelsDevProvider::default()),
            ("moonshotai".to_string(), ModelsDevProvider::default()),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            models_dev_key(&catalog, "clinepass").as_deref(),
            Some("cline-pass")
        );
        assert_eq!(
            models_dev_key(&catalog, "moonshot").as_deref(),
            Some("moonshotai")
        );
        // OAuth-only providers have no models.dev key of their own.
        assert_eq!(models_dev_key(&catalog, "openai-codex"), None);
    }
}
