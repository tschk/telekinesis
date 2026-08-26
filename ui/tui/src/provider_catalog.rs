//! Re-export of the shared inference router catalog.

pub use telekinesis_router::{
    by_id, cline_api_key_from_opencode_auth, env_key, find, infer_from_model, normalize_model,
    opencode_auth_path, resolve_key, ProviderApi, ProviderSpec, API_KEY_PROVIDERS,
};

#[cfg(test)]

mod tests {
    use super::*;

    #[test]
    fn finds_clinepass_aliases() {
        let spec = find("clinepass").expect("id");
        assert_eq!(spec.id, "clinepass");
        assert_eq!(spec.base_url, "https://api.cline.bot/api/v1");
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
