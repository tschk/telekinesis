//! Provider catalog — re-exported from `rs_ai_providers::catalog`.
//!
//! Telekinesis-specific helpers (opencode auth path, cline-pass key
//! extraction) live here alongside the re-export.

pub use rs_ai_providers::catalog::{
    by_id, env_key, find, infer_from_model, normalize_model, ProviderApi, ProviderSpec,
    API_KEY_PROVIDERS,
};

/// OpenCode stores Cline-pass under `cline-pass.key` in auth.json.
/// The key is never logged; callers treat it like any other env secret.
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

/// Env first, then OpenCode `auth.json` for Cline-pass.
pub fn resolve_key(spec: &ProviderSpec) -> Option<String> {
    env_key(spec).or_else(|| {
        if spec.id != "clinepass" {
            return None;
        }
        let path = opencode_auth_path()?;
        let json = std::fs::read_to_string(path).ok()?;
        cline_api_key_from_opencode_auth(&json)
    })
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
    fn resolve_key_uses_opencode_auth_when_env_missing() {
        let spec = find("clinepass").unwrap();
        let dir = std::env::temp_dir().join(format!(
            "tk-opencode-auth-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&dir);
        let auth = dir.join("auth.json");
        std::fs::write(
            &auth,
            r#"{"cline-pass":{"type":"api","key":"sk-from-opencode"}}"#,
        )
        .unwrap();
        let prev_auth = std::env::var_os("OPENCODE_AUTH_PATH");
        let prev_key = std::env::var_os("CLINE_API_KEY");
        std::env::set_var("OPENCODE_AUTH_PATH", &auth);
        std::env::remove_var("CLINE_API_KEY");
        assert_eq!(resolve_key(spec).as_deref(), Some("sk-from-opencode"));
        std::env::set_var("CLINE_API_KEY", "sk-from-env");
        assert_eq!(resolve_key(spec).as_deref(), Some("sk-from-env"));
        match prev_auth {
            Some(value) => std::env::set_var("OPENCODE_AUTH_PATH", value),
            None => std::env::remove_var("OPENCODE_AUTH_PATH"),
        }
        match prev_key {
            Some(value) => std::env::set_var("CLINE_API_KEY", value),
            None => std::env::remove_var("CLINE_API_KEY"),
        }
        let _ = std::fs::remove_dir_all(&dir);
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
