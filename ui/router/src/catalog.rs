//! Provider catalog — re-exported from `rs_ai_providers::catalog`.
//!
//! Telekinesis-specific helpers (opencode auth path, cline-pass key
//! extraction, OS keychain) live here alongside the re-export.

pub use rs_ai_providers::catalog::{
    by_id, find, infer_from_model, normalize_model, ProviderApi, ProviderSpec, API_KEY_PROVIDERS,
};

/// Resolve a provider's API key: env var first, then keychain.
pub fn env_key(spec: &ProviderSpec) -> Option<String> {
    for var in spec.env_vars {
        if let Ok(val) = std::env::var(var) {
            if !val.trim().is_empty() {
                return Some(val);
            }
        }
    }
    load_provider_key(spec.id).ok().flatten()
}

/// File-backed key store: `~/.telekinesis/keys.json`, mode 0600.
///
/// Replaces the OS keychain: unsigned CLI binaries get re-prompted by the
/// keychain on every rebuild, which users rightly hate. Same threat model
/// as ~/.ssh keys — file permissions, no daemon prompts.
///
/// Reads are cached per process; menus render rows every frame.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

fn key_cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn store_path() -> Option<std::path::PathBuf> {
    if let Some(explicit) = std::env::var_os("TELEKINESIS_KEYS_PATH") {
        return Some(std::path::PathBuf::from(explicit));
    }
    dirs::home_dir().map(|home| home.join(".telekinesis").join("keys.json"))
}

fn load_store() -> HashMap<String, String> {
    let Some(path) = store_path() else {
        return HashMap::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_store(keys: &HashMap<String, String>) -> Result<(), String> {
    let Some(path) = store_path() else {
        return Err("no home directory".into());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let raw = serde_json::to_string_pretty(keys).map_err(|e| format!("serialize: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, raw).map_err(|e| format!("write: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("chmod: {e}"))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))
}

/// Save a provider's API key to the local key store.
pub fn save_provider_key(id: &str, key: &str) -> Result<(), String> {
    let mut keys = load_store();
    keys.insert(id.to_string(), key.to_string());
    save_store(&keys)?;
    key_cache().lock().unwrap().insert(id.to_string(), Some(key.to_string()));
    Ok(())
}

/// Load a provider's API key from the local key store (cached per process).
pub fn load_provider_key(id: &str) -> Result<Option<String>, String> {
    let cache = key_cache();
    if let Some(cached) = cache.lock().unwrap().get(id) {
        return Ok(cached.clone());
    }
    let value = load_store().get(id).map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    cache.lock().unwrap().insert(id.to_string(), value.clone());
    Ok(value)
}

/// Delete a provider's API key from the local key store.
pub fn delete_provider_key(id: &str) -> Result<(), String> {
    let mut keys = load_store();
    let existed = keys.remove(id).is_some();
    save_store(&keys)?;
    key_cache().lock().unwrap().insert(id.to_string(), None);
    if existed {
        Ok(())
    } else {
        Err("no key stored".into())
    }
}

/// Check if a provider has a key stored.
pub fn has_provider_key(id: &str) -> bool {
    load_provider_key(id).ok().flatten().is_some()
}

/// OpenCode stores Cline-pass under `cline-pass.key` in auth.json.
pub fn cline_api_key_from_opencode_auth(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let key = value.get("cline-pass")?.get("key")?.as_str()?;
    let key = key.trim();
    (!key.is_empty()).then(|| key.to_string())
}

pub fn opencode_auth_path() -> Option<std::path::PathBuf> {
    if let Some(explicit) = std::env::var_os("OPENCODE_AUTH_PATH") {
        return Some(std::path::PathBuf::from(explicit));
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".local").join("share")))?;
    Some(data.join("opencode").join("auth.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_clinepass_aliases() {
        let spec = find("clinepass").expect("id");
        assert_eq!(spec.id, "clinepass");
        assert_eq!(spec.env_vars, &["CLINE_API_KEY"]);
        assert_eq!(find("cline-pass").map(|p| p.id), Some("clinepass"));
        assert_eq!(find("Cline-pass").map(|p| p.id), Some("clinepass"));
    }

    #[test]
    fn normalizes_clinepass_model_slugs() {
        let spec = find("clinepass").unwrap();
        assert_eq!(
            normalize_model(spec, "clinepass/deepseek-v4-flash"),
            "cline-pass/deepseek-v4-flash"
        );
        assert_eq!(
            normalize_model(spec, "deepseek-v4-flash"),
            "cline-pass/deepseek-v4-flash"
        );
        assert_eq!(
            normalize_model(spec, "cline-pass/qwen3.7-max"),
            "cline-pass/qwen3.7-max"
        );
    }

    #[test]
    fn reads_opencode_auth_key_without_other_fields() {
        let json =
            r#"{"cline-pass":{"type":"api","key":"sk-test-not-real"},"other":{"key":"nope"}}"#;
        assert_eq!(
            cline_api_key_from_opencode_auth(json).as_deref(),
            Some("sk-test-not-real")
        );
        assert_eq!(cline_api_key_from_opencode_auth("{}"), None);
        assert_eq!(
            cline_api_key_from_opencode_auth(r#"{"cline-pass":{"type":"api","key":"  "}}"#),
            None
        );
    }

    #[test]
    fn infers_provider_from_model_prefix() {
        assert_eq!(
            infer_from_model("cline-pass/deepseek-v4-flash").map(|p| p.id),
            Some("clinepass")
        );
        assert_eq!(
            infer_from_model("clinepass/glm-5.2").map(|p| p.id),
            Some("clinepass")
        );
        assert!(infer_from_model("deepseek-v4-flash").is_none());
    }
}