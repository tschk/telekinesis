//! Thin host surface for rotary harness APIs.
//!
//! Hashline, prewalk, and AVO live in rx4. This crate only enables them and
//! applies host environment / scope policy.

use std::sync::Arc;

use parking_lot::Mutex;
use rx4::hooks::{HookDecision, HookEvent, HookRegistry};
use rx4::mode::Scope;
use rx4::Agent;

pub use rx4::avo::{
    commit_if_better, is_protected_branch, lineage_p_t, objective_f, CommitDecision, LineageScore,
    StallDetector,
};
pub use rx4::hashline::{
    format_read as format_hashline_read, HashlineSight, ReadOptions as HashlineReadOptions,
};
pub use rx4::prewalk::{is_mutating_call, Prewalk};

/// Build prewalk from `RX4_PREWALK` / `RX4_SMOL_MODEL` / `RX4_INVESTIGATE_MODEL`.
/// When the investigate model is unset, keep the host's current model.
pub fn prewalk_from_host_env(fallback_model: &str) -> Prewalk {
    let mut prewalk = Prewalk::from_env();
    if std::env::var("RX4_INVESTIGATE_MODEL")
        .ok()
        .filter(|value| !value.is_empty())
        .is_none()
    {
        prewalk.set_investigate_model(fallback_model);
    }
    prewalk
}

/// Apply exec flags onto the process env so `Prewalk::from_env` sees them.
pub fn apply_prewalk_exec_env(
    enabled: bool,
    smol_model: Option<&str>,
    investigate_model: Option<&str>,
) {
    if enabled {
        std::env::set_var("RX4_PREWALK", "1");
    }
    if let Some(model) = smol_model.filter(|value| !value.is_empty()) {
        std::env::set_var("RX4_SMOL_MODEL", model);
    }
    if let Some(model) = investigate_model.filter(|value| !value.is_empty()) {
        std::env::set_var("RX4_INVESTIGATE_MODEL", model);
    }
}

pub fn apply_prewalk_model(agent: &mut Agent, prewalk: &Prewalk) {
    if prewalk.is_enabled() {
        agent.set_model(prewalk.current_model());
    }
}

pub fn session_prewalk(fallback_model: &str) -> Arc<Mutex<Prewalk>> {
    static CELL: std::sync::OnceLock<Arc<Mutex<Prewalk>>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| Arc::new(Mutex::new(prewalk_from_host_env(fallback_model))))
        .clone()
}

pub fn sync_prewalk_model(agent: &mut Agent) {
    let prewalk = session_prewalk(&agent.model);
    apply_prewalk_model(agent, &prewalk.lock());
}

/// Force `"hashline": true` on builtin `read` so `hashline_edit` has visibility.
pub fn hashline_read_arguments(name: &str, arguments: &str) -> Option<String> {
    if name != "read" && name != "read_file" {
        return None;
    }
    let mut value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    let obj = value.as_object_mut()?;
    if obj.get("hashline").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }
    obj.insert("hashline".into(), serde_json::Value::Bool(true));
    Some(value.to_string())
}

pub fn install_host_hooks(agent: &mut Agent, scope: Scope, prewalk: Arc<Mutex<Prewalk>>) {
    let hooks = HookRegistry::new();
    hooks.add(move |event: &HookEvent| match event {
        HookEvent::BeforeTool { tool } => {
            if !crate::host::host_tool_allowed(scope, &tool.name) {
                return HookDecision::Deny {
                    reason: format!("tool not in scope {}: {}", scope.name(), tool.name),
                };
            }
            // Agent only dispatches BeforeTool; record here so the first write switches.
            prewalk
                .lock()
                .record_tool(&tool.name, Some(tool.arguments.as_str()));
            match hashline_read_arguments(&tool.name, &tool.arguments) {
                Some(arguments) => HookDecision::ModifyArgs { arguments },
                None => HookDecision::Allow,
            }
        }
        HookEvent::AfterTool { tool, .. } => {
            let switched = prewalk
                .lock()
                .record_tool(&tool.name, Some(tool.arguments.as_str()));
            if switched {
                tracing::info!("prewalk switched to apply model after {}", tool.name);
            }
            HookDecision::Allow
        }
        _ => HookDecision::Allow,
    });
    agent.set_hooks(hooks);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashline_read_is_tagged() {
        let tagged =
            format_hashline_read("demo.rs", "fn main() {}\n", HashlineReadOptions::default());
        assert!(tagged.text.contains("#"));
        assert!(!tagged.tag.is_empty());
    }

    #[test]
    fn read_arguments_enable_hashline() {
        let patched =
            hashline_read_arguments("read", r#"{"path":"src/lib.rs","limit":20}"#).unwrap();
        let value: serde_json::Value = serde_json::from_str(&patched).unwrap();
        assert_eq!(value["hashline"], true);
        assert_eq!(value["path"], "src/lib.rs");
        assert_eq!(value["limit"], 20);
        assert!(hashline_read_arguments("read", r#"{"path":"x","hashline":true}"#).is_none());
        assert!(hashline_read_arguments("write", r#"{"path":"x"}"#).is_none());
    }

    #[test]
    fn commit_if_better_refuses_main() {
        let best = LineageScore {
            id: "a".into(),
            p_t: 0.2,
            incorrect: false,
            quality: 0.1,
        };
        let candidate = LineageScore {
            id: "b".into(),
            p_t: 0.8,
            incorrect: false,
            quality: 0.9,
        };
        assert!(matches!(
            commit_if_better("main", &best, &candidate),
            CommitDecision::RefuseMain { .. }
        ));
        assert!(matches!(
            commit_if_better("feat/consume-rx4-harness", &best, &candidate),
            CommitDecision::Accept { .. }
        ));
    }

    #[test]
    fn prewalk_env_uses_fallback_model() {
        let prewalk = prewalk_from_host_env("host-model");
        if std::env::var("RX4_INVESTIGATE_MODEL")
            .ok()
            .filter(|value| !value.is_empty())
            .is_none()
        {
            assert_eq!(prewalk.current_model(), "host-model");
        }
    }
}
