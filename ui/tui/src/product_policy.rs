//! Host product policy defaults for telekinesis (pi coding-agent layer).
//! Engine (rx4) only matches; this module fills host-owned shell lists.

use rx4::agent::Agent;
use rx4::Policy;

/// When true, require seatbelt/bwrap. Harbor images and this orb often have
/// neither — fail-closed bash then burns the whole SWE/TB budget on blocked
/// shells. Default: kernel sandbox when a backend exists, else userspace.
/// Set `TK_OS_SANDBOX=1` to keep fail-closed. `TK_OS_SANDBOX=0` forces off.
pub fn os_sandbox_requested() -> bool {
    match std::env::var("TK_OS_SANDBOX") {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => !matches!(rx4::detect_sandbox(), rx4::OsSandbox::UserspaceOnly),
    }
}

/// Tele default coding policy: workspace write + OS sandbox + safe shell allows.
pub fn tele_coding_policy() -> Policy {
    Policy::workspace_write()
        .with_os_sandbox(os_sandbox_requested())
        .with_shell_allow([
            "git *",
            "cargo test*",
            "cargo check*",
            "cargo build*",
            "cargo clippy*",
            "cargo fmt*",
            "rg *",
            "fd *",
            "ls *",
            "pwd",
            "cat *",
            "head *",
            "tail *",
            "wc *",
        ])
        .with_shell_deny(["sudo *", "rm -rf /*", "rm -rf /"])
}

/// Apply after `apply_scope`: Coding's profile re-enables OS sandbox.
/// Without this, Harbor/orb hosts with no bwrap fail-close bash.
pub fn apply_os_sandbox(agent: &mut Agent) {
    if os_sandbox_requested() {
        let _ = agent.enable_os_sandbox();
        return;
    }
    agent.policy.enable_os_sandbox = false;
    agent.os_sandbox = None;
}

#[cfg(test)]
mod tests {
    use super::os_sandbox_requested;
    use rx4::OsSandbox;

    #[test]
    fn os_sandbox_env_on_forces_requested() {
        let previous = std::env::var("TK_OS_SANDBOX").ok();
        std::env::set_var("TK_OS_SANDBOX", "1");
        assert!(os_sandbox_requested());
        std::env::set_var("TK_OS_SANDBOX", "0");
        assert!(!os_sandbox_requested());
        match previous {
            Some(value) => std::env::set_var("TK_OS_SANDBOX", value),
            None => std::env::remove_var("TK_OS_SANDBOX"),
        }
    }

    #[test]
    fn os_sandbox_default_follows_backend() {
        let previous = std::env::var("TK_OS_SANDBOX").ok();
        std::env::remove_var("TK_OS_SANDBOX");
        let expected = !matches!(rx4::detect_sandbox(), OsSandbox::UserspaceOnly);
        assert_eq!(os_sandbox_requested(), expected);
        match previous {
            Some(value) => std::env::set_var("TK_OS_SANDBOX", value),
            None => std::env::remove_var("TK_OS_SANDBOX"),
        }
    }

    #[test]
    fn apply_os_sandbox_clears_requirement_when_disabled() {
        let previous = std::env::var("TK_OS_SANDBOX").ok();
        std::env::set_var("TK_OS_SANDBOX", "0");
        let mut agent = rx4::agent::Agent::new();
        agent.policy.enable_os_sandbox = true;
        super::apply_os_sandbox(&mut agent);
        assert!(!agent.policy.enable_os_sandbox);
        assert!(agent.os_sandbox.is_none());
        match previous {
            Some(value) => std::env::set_var("TK_OS_SANDBOX", value),
            None => std::env::remove_var("TK_OS_SANDBOX"),
        }
    }
}
