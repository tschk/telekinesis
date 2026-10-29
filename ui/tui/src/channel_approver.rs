//! Sync host Approver: blocks tool gate until UI sends Allow/Deny.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
    Arc,
};

use rx4::agent::ToolCall;
use rx4::permissions::{Approver, Decision};

pub struct PendingApproval {
    pub tool_name: String,
    pub arguments: String,
    pub respond: SyncSender<Decision>,
}

/// Whether tool calls wait for a y/n answer (`ask`, the default) or are
/// auto-approved (`bypass`). Bypass is only entered explicitly: the
/// `TK_TOOL_APPROVAL=bypass` env var at startup or `/approvals bypass`.
#[derive(Clone)]
pub struct ApprovalMode {
    bypass: Arc<AtomicBool>,
}

impl ApprovalMode {
    pub fn is_bypass(&self) -> bool {
        self.bypass.load(Ordering::Acquire)
    }

    pub fn set_bypass(&self, bypass: bool) {
        self.bypass.store(bypass, Ordering::Release);
    }
}

/// Startup approval mode from `TK_TOOL_APPROVAL`; anything but an explicit
/// `bypass` keeps the safe ask default.
pub fn bypass_requested(value: Option<&str>) -> bool {
    matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("bypass")
    )
}

pub struct ChannelApprover {
    tx: SyncSender<PendingApproval>,
    mode: ApprovalMode,
}

impl ChannelApprover {
    /// A gate that asks before every tool call until bypass is explicitly set.
    pub fn pair() -> (Self, Receiver<PendingApproval>) {
        let (tx, rx) = mpsc::sync_channel(8);
        let mode = ApprovalMode {
            bypass: Arc::new(AtomicBool::new(false)),
        };
        (Self { tx, mode }, rx)
    }

    pub fn mode(&self) -> ApprovalMode {
        self.mode.clone()
    }
}

impl Approver for ChannelApprover {
    fn approve(&self, tool_call: &ToolCall) -> Decision {
        if self.mode.is_bypass() {
            return Decision::Allow;
        }
        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        let pending = PendingApproval {
            tool_name: tool_call.name.clone(),
            arguments: tool_call.arguments.clone(),
            respond: resp_tx,
        };
        if self.tx.send(pending).is_err() {
            return Decision::Deny;
        }
        resp_rx.recv().unwrap_or(Decision::Deny)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call() -> ToolCall {
        ToolCall {
            id: "test".to_string(),
            name: "bash".to_string(),
            arguments: "{\"command\":\"rm -rf build\"}".to_string(),
        }
    }

    #[test]
    fn asks_by_default_and_only_bypasses_when_set() {
        let (approver, pending) = ChannelApprover::pair();
        let mode = approver.mode();
        assert!(!mode.is_bypass());

        let worker = std::thread::spawn(move || {
            let first = approver.approve(&call());
            (approver, first)
        });
        let request = pending.recv().unwrap();
        assert_eq!(request.tool_name, "bash");
        assert_eq!(request.arguments, "{\"command\":\"rm -rf build\"}");
        request.respond.send(Decision::Deny).unwrap();
        let (approver, first) = worker.join().unwrap();
        assert_eq!(first, Decision::Deny);

        mode.set_bypass(true);
        assert_eq!(approver.approve(&call()), Decision::Allow);
        assert!(pending.try_recv().is_err(), "bypass must not prompt");
    }

    #[test]
    fn only_explicit_bypass_env_value_enables_bypass() {
        assert!(bypass_requested(Some("bypass")));
        assert!(bypass_requested(Some(" BYPASS ")));
        for value in [
            None,
            Some(""),
            Some("ask"),
            Some("1"),
            Some("yes"),
            Some("on"),
        ] {
            assert!(!bypass_requested(value), "{value:?}");
        }
    }
}
