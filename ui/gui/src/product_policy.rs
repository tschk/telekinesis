//! Host product policy defaults for telekinesis GUI (same shell lists as TUI).

use rx4::agent::Agent;
use rx4::Policy;

/// When true, require seatbelt/bwrap. Default: kernel sandbox if a backend
/// exists. `TK_OS_SANDBOX=1` fail-closed; `TK_OS_SANDBOX=0` userspace only.
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

/// Apply after policy/scope: Coding's profile re-enables OS sandbox.
pub fn apply_os_sandbox(agent: &mut Agent) {
    if os_sandbox_requested() {
        let _ = agent.enable_os_sandbox();
        return;
    }
    agent.policy.enable_os_sandbox = false;
    agent.os_sandbox = None;
}
