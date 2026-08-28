//! First-start discovery: import API keys from other agent harnesses into
//! the OS keychain. Runs once, guarded by a marker file.

use std::path::PathBuf;

use crate::catalog::{by_id, find, opencode_auth_path, save_provider_key};

/// Import API-key entries from OpenCode's `auth.json` into the OS keychain.
/// Returns the catalog ids that received a key. OAuth-style entries are
/// skipped — those belong to rs_ai_oauth's credential store.
pub fn import_from_opencode() -> Result<Vec<String>, String> {
    let path = opencode_auth_path().ok_or_else(|| "no home directory".to_string())?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| format!("no auth.json at {}", path.display()))?;
    import_from_opencode_json(&raw)
}

/// Parse the auth.json body. Split out for tests — never logs key material.
pub fn import_from_opencode_json(json: &str) -> Result<Vec<String>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("parse auth.json: {e}"))?;
    let mut imported = Vec::new();
    let Some(entries) = value.as_object() else {
        return Ok(imported);
    };
    for (id, entry) in entries {
        if entry.get("type").and_then(|t| t.as_str()) != Some("api") {
            continue;
        }
        let Some(key) = entry.get("key").and_then(|k| k.as_str()) else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        // OpenCode ids follow models.dev; aliases cover the rest (e.g.
        // "cline-pass" -> clinepass).
        let Some(spec) = by_id(id).or_else(|| find(id)) else {
            continue;
        };
        if save_provider_key(spec.id, key).is_ok() {
            imported.push(spec.id.to_string());
        }
    }
    Ok(imported)
}

fn marker_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".telekinesis").join(".keys-imported"))
}

/// Import is due when it never ran, or when `auth.json` changed after the
/// last import — so keys re-saved by the harness (or a TUI test value that
/// clobbered one) are re-discovered automatically.
pub fn already_imported() -> bool {
    let (Some(marker), Some(auth)) = (marker_path(), opencode_auth_path()) else {
        return true;
    };
    if !marker.exists() {
        return false;
    }
    let auth_mtime = std::fs::metadata(&auth).and_then(|m| m.modified()).ok();
    let marker_mtime = std::fs::metadata(&marker).and_then(|m| m.modified()).ok();
    match (auth_mtime, marker_mtime) {
        (Some(a), Some(m)) => a <= m,
        (Some(_), None) => false,
        _ => true,
    }
}

pub fn mark_imported() {
    if let Some(path) = marker_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, b"1");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_api_entries_only() {
        let json = r#"{
            "openai": {"type":"api","key":"sk-openai-test"},
            "anthropic": {"type":"oauth","key":"not-an-api-key"},
            "cline-pass": {"type":"api","key":"sk-cline-test"},
            "unknown-provider": {"type":"api","key":"sk-nope"},
            "groq": {"type":"api","key":"  "}
        }"#;
        let ids = import_from_opencode_json(json).unwrap();
        assert!(ids.contains(&"openai".to_string()));
        assert!(ids.contains(&"clinepass".to_string()));
        assert!(!ids.contains(&"groq".to_string()));
        assert!(!ids.contains(&"unknown-provider".to_string()));
        assert!(!ids.iter().any(|id| id == "anthropic"));
    }

    #[test]
    fn malformed_json_is_an_error_not_a_panic() {
        assert!(import_from_opencode_json("not json").is_err());
    }
}
