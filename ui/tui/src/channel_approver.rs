//! Sync host Approver: blocks tool gate until UI sends Allow/Deny.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
    Arc,
};

use rx4::agent::{ToolCall, ToolContext, ToolDefinition, ToolExecuteBox, ToolRegistry, ToolResult};
use rx4::permissions::{
    is_read_only_tool, Approver, Authorizer, Decision, Policy, PolicyAuthorizer,
};

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

/// Policy gate that makes ask mode cover every tool call that can change
/// something. rx4's workspace-write policy auto-allows workspace writes and
/// allowlisted shell commands without consulting the approver; in ask mode
/// those become `Ask` so they reach the y/n prompt. Read-only tools and the
/// in-memory `todo` list still run directly, and a policy `Deny` is never
/// weakened. Bypass keeps the policy decision unchanged.
pub struct ApprovalAuthorizer {
    mode: ApprovalMode,
}

impl Authorizer for ApprovalAuthorizer {
    // Model-issued plan-mode changes also reset scope inside rx4's loop.
    // This gate evaluates the supplied policy/root, not a stale snapshot.
    fn uses_live_policy(&self) -> bool {
        true
    }

    fn authorize(
        &self,
        policy: &Policy,
        tool_name: &str,
        arguments: &str,
        approver: Option<&dyn Approver>,
        workspace_root: Option<&std::path::Path>,
    ) -> Decision {
        // No approver here: the engine asks once on our `Ask`. Passing it
        // would let a policy `Ask` prompt inside the policy check and again
        // after it.
        let _ = approver;
        let decision =
            PolicyAuthorizer::new().authorize(policy, tool_name, arguments, None, workspace_root);
        let unattended = is_read_only_tool(tool_name) || tool_name == "todo";
        if decision == Decision::Allow && !unattended && !self.mode.is_bypass() {
            Decision::Ask
        } else {
            decision
        }
    }
}

#[derive(Clone)]
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

    /// Whether a tool call reaches the y/n prompt in the current mode.
    fn gates(&self, tool_name: &str) -> bool {
        !self.mode.is_bypass() && !is_read_only_tool(tool_name) && tool_name != "todo"
    }

    /// Tool registry for subagents. rx4 builds child agents with no approver
    /// and a workspace-write policy, so their writes would run unprompted.
    /// Each tool here forwards to `parent` only after the same y/n prompt the
    /// main agent uses (labelled `subagent <tool>`); a denial returns an
    /// error result. Child policy still runs first, so its denials hold and
    /// its Ask-class calls (shell) fail closed for lack of an approver.
    pub fn gate_subagent_tools(&self, parent: Arc<ToolRegistry>) -> Arc<ToolRegistry> {
        let gated = ToolRegistry::new();
        let definitions = parent.definitions();
        for name in parent.names() {
            let definition = definitions
                .iter()
                .find(|definition| definition["name"] == name.as_str());
            let field = |key: &str| {
                definition
                    .and_then(|definition| definition.get(key))
                    .map(|value| match value {
                        serde_json::Value::String(text) => text.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
            };
            let (description, parameters) = (field("description"), field("parameters"));
            let effect = parent.effect_of(&name);
            let parent = Arc::clone(&parent);
            let gate = self.clone();
            let tool = name.clone();
            let execute: ToolExecuteBox = Box::new(move |ctx: Arc<ToolContext>, args: String| {
                let parent = Arc::clone(&parent);
                let gate = gate.clone();
                let tool = tool.clone();
                Box::pin(async move {
                    if gate.gates(&tool) {
                        let call = ToolCall {
                            id: String::new(),
                            name: format!("subagent {tool}"),
                            arguments: args.clone(),
                        };
                        let decision = tokio::task::spawn_blocking(move || {
                            gate.approve_unconditionally(&call)
                        })
                        .await
                        .unwrap_or(Decision::Deny);
                        if decision != Decision::Allow {
                            return ToolResult::err("", "denied by user");
                        }
                    }
                    parent
                        .execute(&tool, &ctx, &args)
                        .await
                        .unwrap_or_else(|| ToolResult::err("", format!("unknown tool: {tool}")))
                })
            });
            gated.register(
                ToolDefinition::new_boxed(name, description, parameters, execute)
                    .with_effect(effect),
            );
        }
        Arc::new(gated)
    }

    /// Send the call to the UI and wait for its answer, ignoring the mode.
    fn approve_unconditionally(&self, tool_call: &ToolCall) -> Decision {
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

    /// The authorizer to install next to this approver; see [`ApprovalAuthorizer`].
    pub fn authorizer(&self) -> Arc<ApprovalAuthorizer> {
        Arc::new(ApprovalAuthorizer {
            mode: self.mode.clone(),
        })
    }
}

impl Approver for ChannelApprover {
    fn approve(&self, tool_call: &ToolCall) -> Decision {
        if self.mode.is_bypass() {
            return Decision::Allow;
        }
        self.approve_unconditionally(tool_call)
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
    fn ask_mode_gates_policy_allowed_writes_and_shell_but_keeps_denials() {
        let (approver, _pending) = ChannelApprover::pair();
        let mode = approver.mode();
        let authorizer = approver.authorizer();
        let root = tempfile::tempdir().unwrap();
        let policy = crate::product_policy::tele_coding_policy();
        let decide = |tool: &str, args: &str| {
            authorizer.authorize(&policy, tool, args, None, Some(root.path()))
        };
        let inside = format!(
            "{{\"path\":\"{}\",\"content\":\"x\"}}",
            root.path().join("a.txt").display()
        );
        let outside = r#"{"path":"/etc/tk-approval-test","content":"x"}"#;

        // rx4's workspace-write policy allows these without asking.
        assert_eq!(
            PolicyAuthorizer::new().authorize(&policy, "write", &inside, None, Some(root.path())),
            Decision::Allow
        );
        assert_eq!(decide("write", &inside), Decision::Ask);
        assert_eq!(decide("edit", &inside), Decision::Ask);
        assert_eq!(decide("bash", r#"{"command":"git status"}"#), Decision::Ask);
        assert_eq!(decide("read", &inside), Decision::Allow);
        assert_eq!(decide("todo", "{}"), Decision::Allow);
        assert_eq!(decide("write", outside), Decision::Deny);

        mode.set_bypass(true);
        assert_eq!(decide("write", &inside), Decision::Allow);
        assert_eq!(
            decide("bash", r#"{"command":"git status"}"#),
            Decision::Allow
        );
        assert_eq!(
            decide("write", outside),
            Decision::Deny,
            "bypass never overrides deny"
        );
    }

    #[test]
    fn authorizer_never_prompts_itself() {
        use std::sync::atomic::AtomicUsize;
        struct Counting(AtomicUsize);
        impl Approver for Counting {
            fn approve(&self, _call: &ToolCall) -> Decision {
                self.0.fetch_add(1, Ordering::SeqCst);
                Decision::Allow
            }
        }
        let (approver, _pending) = ChannelApprover::pair();
        let counting = Counting(AtomicUsize::new(0));
        let root = tempfile::tempdir().unwrap();
        let policy = crate::product_policy::tele_coding_policy();
        // Policy Ask (non-allowlisted shell): returned as Ask so the engine
        // prompts exactly once, not resolved by an extra prompt here.
        let decision = approver.authorizer().authorize(
            &policy,
            "bash",
            r#"{"command":"make deploy"}"#,
            Some(&counting),
            Some(root.path()),
        );
        assert_eq!(decision, Decision::Ask);
        assert_eq!(counting.0.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn subagent_tools_wait_for_approval_before_running() {
        use std::sync::atomic::AtomicUsize;
        static RUNS: AtomicUsize = AtomicUsize::new(0);
        fn write_tool(_ctx: Arc<ToolContext>, _args: String) -> rx4::agent::ToolFuture {
            RUNS.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { ToolResult::ok("", "written") })
        }
        fn read_tool(_ctx: Arc<ToolContext>, _args: String) -> rx4::agent::ToolFuture {
            Box::pin(async { ToolResult::ok("", "read") })
        }
        let parent = ToolRegistry::new();
        parent.register(
            ToolDefinition::new_fn("write", "write a file", "{}", write_tool)
                .with_effect(rx4::agent::ToolEffect::Write),
        );
        parent.register(ToolDefinition::new_fn(
            "read",
            "read a file",
            "{}",
            read_tool,
        ));
        let (approver, pending) = ChannelApprover::pair();
        let gated = approver.gate_subagent_tools(Arc::new(parent));
        assert_eq!(gated.names(), ["read", "write"]);
        assert_eq!(gated.effect_of("write"), rx4::agent::ToolEffect::Write);
        let ctx = Arc::new(ToolContext::new(std::env::temp_dir()));

        let read = gated.execute("read", &ctx, "{}").await.unwrap();
        assert_eq!(read.content, "read");
        assert!(pending.try_recv().is_err(), "reads do not prompt");

        let pending = Arc::new(std::sync::Mutex::new(pending));
        for (decision, runs_after) in [(Decision::Deny, 0), (Decision::Allow, 1)] {
            let run = tokio::spawn({
                let gated = Arc::clone(&gated);
                let ctx = Arc::clone(&ctx);
                async move { gated.execute("write", &ctx, r#"{"path":"a"}"#).await }
            });
            let pending = Arc::clone(&pending);
            let request = tokio::task::spawn_blocking(move || {
                let pending = pending.lock().unwrap();
                pending
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap()
            })
            .await
            .unwrap();
            assert_eq!(request.tool_name, "subagent write");
            assert_eq!(request.arguments, r#"{"path":"a"}"#);
            assert_eq!(RUNS.load(Ordering::SeqCst), 0, "ran before approval");
            request.respond.send(decision).unwrap();
            let result = run.await.unwrap().unwrap();
            assert_eq!(result.is_error, decision == Decision::Deny);
            assert_eq!(RUNS.load(Ordering::SeqCst), runs_after);
        }

        approver.mode().set_bypass(true);
        gated.execute("write", &ctx, "{}").await.unwrap();
        assert_eq!(RUNS.load(Ordering::SeqCst), 2, "bypass runs without asking");
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

    /// Security regression: the model must not remove its own approval gate
    /// through plan-mode tools inside the public engine loop.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn model_plan_mode_round_trip_cannot_bypass_tui_approval_prompt() {
        use rx4::provider::{Provider, StreamEvent, StreamResult};
        use std::collections::VecDeque;
        use std::sync::{atomic::AtomicUsize, Mutex};

        struct Script(Mutex<VecDeque<ToolCall>>);
        #[async_trait::async_trait]
        impl Provider for Script {
            fn id(&self) -> &str {
                "approval-test"
            }
            fn name(&self) -> &str {
                "approval-test"
            }
            async fn stream(
                &self,
                _: &[rx4::provider::Message],
                _: &Option<String>,
                _: &str,
                _: &[serde_json::Value],
                _: Option<&str>,
            ) -> Result<StreamResult, rx4::provider::ProviderError> {
                let mut events = Vec::new();
                if let Some(call) = self.0.lock().unwrap().pop_front() {
                    events.push(Ok(StreamEvent::ToolCall(call)));
                }
                events.push(Ok(StreamEvent::Done));
                Ok(Box::new(futures::stream::iter(events)))
            }
        }

        for change_scope in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let runs = Arc::new(AtomicUsize::new(0));
            let tools = ToolRegistry::new();
            rx4::register_builtin_tools(&tools);
            // Harmless executors retain real tool names and policy decisions.
            for tool in ["write", "bash"] {
                let runs = runs.clone();
                tools.register(
                    ToolDefinition::new_boxed(
                        tool,
                        "",
                        "{}",
                        Box::new(move |_, _| {
                            runs.fetch_add(1, Ordering::SeqCst);
                            Box::pin(async { ToolResult::ok("", "executed") })
                        }),
                    )
                    .with_effect(rx4::agent::ToolEffect::Write),
                );
            }
            let mut calls = VecDeque::new();
            if change_scope {
                for name in ["enter_plan_mode", "exit_plan_mode"] {
                    calls.push_back(ToolCall {
                        id: name.into(),
                        name: name.into(),
                        arguments: "{}".into(),
                    });
                }
            }
            for (name, arguments) in [
                ("write", r#"{"path":"approval.txt","content":"x"}"#),
                ("bash", r#"{"command":"git status"}"#),
                ("write", r#"{"path":"/etc/tk-approval-test","content":"x"}"#),
                ("bash", r#"{"command":"sudo true"}"#),
            ] {
                calls.push_back(ToolCall {
                    id: format!("call-{}", calls.len()),
                    name: name.into(),
                    arguments: arguments.into(),
                });
            }
            let (approver, pending) = ChannelApprover::pair();
            let responder = std::thread::spawn(move || {
                let mut prompted = Vec::new();
                while let Ok(request) = pending.recv_timeout(std::time::Duration::from_secs(10)) {
                    prompted.push(request.tool_name);
                    request.respond.send(Decision::Deny).unwrap();
                }
                prompted
            });
            let mut agent = rx4::agent::Agent::new();
            agent.set_workspace_root(root.path());
            agent.set_tools(tools);
            agent.set_policy(
                rx4::Policy::workspace_write()
                    .with_os_sandbox(false)
                    .with_shell_allow(["git *"])
                    .with_shell_deny(["sudo *"]),
            );
            agent.set_provider(Arc::new(Script(Mutex::new(calls))));
            agent.set_authorizer(approver.authorizer());
            agent.set_approver(Arc::new(approver));
            agent.prompt("exercise approvals").await.unwrap();
            drop(agent);
            let prompted = responder.join().unwrap();
            assert_eq!(
                prompted,
                ["write", "bash"],
                "scope transitions: {change_scope}"
            );
            assert_eq!(runs.load(Ordering::SeqCst), 0, "denied tools executed");
        }
    }
}
