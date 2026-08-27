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

/// Save a provider's API key to the OS keychain.
pub fn save_provider_key(id: &str, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("telekinesis", id)
        .map_err(|e| format!("keyring entry: {e}"))?;
    entry.set_password(key).map_err(|e| format!("keyring set: {e}"))
}

/// Load a provider's API key from the OS keychain.
pub fn load_provider_key(id: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new("telekinesis", id)
        .map_err(|e| format!("keyring entry: {e}"))?;
    match entry.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("keyring get: {e}")),
    }
}

/// Delete a provider's API key from the OS keychain.
pub fn delete_provider_key(id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("telekinesis", id)
        .map_err(|e| format!("keyring entry: {e}"))?;
    entry.delete_credential().map_err(|e| format!("keyring delete: {e}"))
}

/// Check if a provider has a key in the OS keychain.
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