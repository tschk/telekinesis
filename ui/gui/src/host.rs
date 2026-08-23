use rx4::agent::ToolCall;
use rx4::permissions::Decision;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::agent::{oauth_provider, runtime, setup_agents, AgentSetup};
use crate::bridge::{
    encode_bridge_response, parse_bridge_request, BridgeInbox, BridgeRequest, BridgeResponse,
    BridgeSnapshot,
};
use crate::layout::{composer_action, next_effort, session_rows, SessionRow};
use crate::session::{AgentSession, CompanionEvent, MessageItem, PointTarget, SessionKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostCommand {
    Login(Option<String>),
    Clear,
    ComputerUse,
    Coding,
    CycleEffort,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerInput {
    Submit,
    Newline,
    Backspace,
    Paste,
    Text(String),
}

pub fn composer_input_from_key(
    key: &str,
    shift: bool,
    secondary: bool,
    control: bool,
    alt: bool,
    key_char: Option<&str>,
) -> Option<ComposerInput> {
    if key == "enter" {
        return Some(if shift {
            ComposerInput::Newline
        } else {
            ComposerInput::Submit
        });
    }
    if key == "j" && control && !alt && !secondary {
        return Some(ComposerInput::Newline);
    }
    if key == "v" && secondary && !alt {
        return Some(ComposerInput::Paste);
    }
    if key == "backspace" {
        return Some(ComposerInput::Backspace);
    }
    if let Some(ch) = key_char {
        if !control && !alt && !secondary && !ch.is_empty() {
            return Some(ComposerInput::Text(ch.to_string()));
        }
    }
    None
}

pub fn parse_host_command(text: &str) -> Option<HostCommand> {
    let text = text.trim();
    if !text.starts_with('/') {
        return None;
    }
    let mut parts = text.splitn(2, char::is_whitespace);
    let cmd = parts.next()?.to_ascii_lowercase();
    let arg = parts.next().map(str::trim).filter(|s| !s.is_empty());
    match cmd.as_str() {
        "/login" => Some(HostCommand::Login(arg.map(str::to_string))),
        "/clear" => Some(HostCommand::Clear),
        "/computer" | "/computer_use" => Some(HostCommand::ComputerUse),
        "/coding" => Some(HostCommand::Coding),
        "/effort" => Some(HostCommand::CycleEffort),
        "/scope" => match arg {
            Some("computer_use") | Some("computer") => Some(HostCommand::ComputerUse),
            Some("coding") => Some(HostCommand::Coding),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct HostSnapshot {
    pub input: String,
    pub model: String,
    pub busy: bool,
    pub connected: bool,
    pub context_pct: usize,
    pub status: String,
    pub provider: String,
    pub messages: Vec<MessageItem>,
    pub login_busy: bool,
    pub permission_pending: bool,
    pub session_name: String,
    pub session_kind: SessionKind,
    pub computer_active: bool,
    pub coding_active: bool,
    pub effort: String,
    pub queued: usize,
    pub composer_action: String,
    pub sessions: Vec<SessionRow>,
}

#[derive(Default)]
pub struct HostTick {
    pub dirty: bool,
    pub point: Option<PointTarget>,
}

pub struct CompanionHost {
    pub input: String,
    sessions: Vec<AgentSession>,
    active_session: usize,
    event_rx: UnboundedReceiver<CompanionEvent>,
    event_tx: UnboundedSender<CompanionEvent>,
    approval_rx: Option<std::sync::mpsc::Receiver<(ToolCall, std::sync::mpsc::Sender<Decision>)>>,
    permission_respond: Option<std::sync::mpsc::Sender<Decision>>,
    permission_pending: bool,
    login_busy: bool,
    poll_generation: u64,
    effort: String,
    bridge_rx: UnboundedReceiver<(String, UnboundedSender<String>)>,
    bridge_tx: UnboundedSender<(String, UnboundedSender<String>)>,
}

impl CompanionHost {
    pub fn boot() -> Self {
        let (event_tx, event_rx) = unbounded_channel();
        let (bridge_tx, bridge_rx) = unbounded_channel();
        let mut host = Self {
            input: String::new(),
            sessions: Vec::new(),
            active_session: 0,
            event_rx,
            event_tx: event_tx.clone(),
            approval_rx: None,
            permission_respond: None,
            permission_pending: false,
            login_busy: false,
            poll_generation: 0,
            effort: "high".into(),
            bridge_rx,
            bridge_tx,
        };
        if let Some(setup) = setup_agents(runtime(), event_tx) {
            host.apply_setup(setup);
        } else {
            host.sessions.push(Self::login_session());
        }
        host
    }

    fn login_session() -> AgentSession {
        let mut session =
            AgentSession::new("login required", SessionKind::Coding, None, "not connected");
        session.messages.push(MessageItem::new(
            "system",
            "Log in to start. Click login or run `tk login openai`, then send a prompt.",
        ));
        session
    }

    fn apply_setup(&mut self, setup: AgentSetup) {
        let AgentSetup {
            computer_use,
            computer_use_cancel,
            coding,
            coding_cancel,
            model,
            approval_rx,
        } = setup;
        let mut computer = AgentSession::new(
            "computer use",
            SessionKind::ComputerUse,
            Some(computer_use),
            &model,
        );
        computer.cancellation = Some(computer_use_cancel);
        let mut code = AgentSession::new("coding", SessionKind::Coding, Some(coding), &model);
        code.cancellation = Some(coding_cancel);
        self.sessions = vec![computer, code];
        self.active_session = 0;
        self.approval_rx = Some(approval_rx);
        self.permission_pending = false;
        self.permission_respond = None;
    }

    fn active_session(&self) -> Option<&AgentSession> {
        self.sessions.get(self.active_session)
    }

    fn active_session_mut(&mut self) -> Option<&mut AgentSession> {
        self.sessions.get_mut(self.active_session)
    }

    fn push_system(&mut self, content: impl Into<String>) {
        if let Some(session) = self.active_session_mut() {
            session.messages.push(MessageItem::new("system", content));
        }
    }

    pub fn snapshot(&self) -> HostSnapshot {
        let session = self.active_session();
        let model = session
            .map(|s| s.model.clone())
            .unwrap_or_else(|| "not connected".into());
        let busy = session.map(|s| s.busy).unwrap_or(false);
        let context_pct = session.map(|s| s.context_pct).unwrap_or(0);
        let connected = session.and_then(|s| s.agent.as_ref()).is_some();
        let status = if self.login_busy {
            "logging in".to_string()
        } else if connected {
            if busy {
                "working".into()
            } else {
                "ready".into()
            }
        } else {
            "login required".into()
        };
        let provider = if connected {
            match model.as_str() {
                "gpt-5.5" => "ChatGPT Codex",
                "gpt-5.4" => "OpenAI",
                "claude-sonnet-4-5" => "Claude",
                "grok-4.5" => "xAI",
                "gemini-2.0-flash" => "Google Gemini",
                "kimi-k2.5" => "Kimi",
                _ => "AI",
            }
        } else {
            "none"
        };
        let queued = session.map(|s| s.follow_ups.len()).unwrap_or(0);
        let sessions = session_rows(
            &self
                .sessions
                .iter()
                .map(|s| (s.name.clone(), s.kind, s.busy))
                .collect::<Vec<_>>(),
            self.active_session,
        );
        HostSnapshot {
            input: self.input.clone(),
            model,
            busy,
            connected,
            context_pct,
            status,
            provider: provider.into(),
            messages: session
                .map(AgentSession::render_messages)
                .unwrap_or_default(),
            login_busy: self.login_busy,
            permission_pending: self.permission_pending,
            session_name: session
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "none".into()),
            session_kind: session.map(|s| s.kind).unwrap_or(SessionKind::Coding),
            computer_active: session
                .map(|s| s.kind == SessionKind::ComputerUse)
                .unwrap_or(false),
            coding_active: session
                .map(|s| s.kind == SessionKind::Coding)
                .unwrap_or(false),
            effort: self.effort.clone(),
            queued,
            composer_action: composer_action(busy, connected).to_string(),
            sessions,
        }
    }

    pub fn poll(&mut self) -> HostTick {
        let mut tick = HostTick::default();
        while let Ok(event) = self.event_rx.try_recv() {
            tick.dirty = true;
            match event {
                CompanionEvent::Session(idx, e) => {
                    if let Some(session) = self.sessions.get_mut(idx) {
                        if let Some(point) = session.handle_rx4_event(e) {
                            tick.point = Some(point);
                        }
                    }
                }
                CompanionEvent::SessionError(idx, msg) => {
                    if let Some(session) = self.sessions.get_mut(idx) {
                        session
                            .messages
                            .push(MessageItem::new("error", format!("Error: {msg}")));
                        session.busy = false;
                    }
                }
                CompanionEvent::PromptFinished(idx) => {
                    if let Some(session) = self.sessions.get_mut(idx) {
                        session.busy = false;
                    }
                    self.drain_follow_up(idx);
                }
                CompanionEvent::LoginSucceeded => {
                    self.login_busy = false;
                    if let Some(setup) = setup_agents(runtime(), self.event_tx.clone()) {
                        self.apply_setup(setup);
                        self.push_system("Logged in. Ready.");
                    } else {
                        self.push_system(
                            "Login saved, but no provider is ready. Check `tk login` or API keys.",
                        );
                    }
                }
                CompanionEvent::LoginFailed(msg) => {
                    self.login_busy = false;
                    self.push_system(format!("Login failed: {msg}"));
                }
            }
        }
        if self.poll_approvals() {
            tick.dirty = true;
        }
        if self.poll_bridge() {
            tick.dirty = true;
        }
        if tick.dirty {
            self.poll_generation = self.poll_generation.saturating_add(1);
        }
        tick
    }

    pub fn bridge_sender(&self) -> BridgeInbox {
        self.bridge_tx.clone()
    }

    fn poll_bridge(&mut self) -> bool {
        let mut dirty = false;
        for _ in 0..32 {
            let Ok((raw, reply)) = self.bridge_rx.try_recv() else {
                break;
            };
            dirty = true;
            let response = self.apply_bridge_json(&raw);
            let _ = reply.send(response);
        }
        dirty
    }

    pub fn apply_bridge_json(&mut self, raw: &str) -> String {
        match parse_bridge_request(raw) {
            Ok(request) => encode_bridge_response(&self.apply_bridge_request(request)),
            Err(message) => encode_bridge_response(&BridgeResponse::Error { message }),
        }
    }

    fn apply_bridge_request(&mut self, request: BridgeRequest) -> BridgeResponse {
        match request {
            BridgeRequest::Snapshot => BridgeResponse::Snapshot {
                data: BridgeSnapshot::from(&self.snapshot()),
            },
            BridgeRequest::Prompt { text } => {
                self.dispatch_prompt(text, false);
                BridgeResponse::Ok
            }
            BridgeRequest::Queue { text } => {
                let busy = self.active_session().map(|s| s.busy).unwrap_or(false);
                if busy {
                    if let Some(session) = self.active_session_mut() {
                        session.follow_ups.push_back(text);
                    }
                } else {
                    self.dispatch_prompt(text, false);
                }
                BridgeResponse::Ok
            }
            BridgeRequest::Interrupt => {
                self.interrupt();
                BridgeResponse::Ok
            }
            BridgeRequest::Select { session } => {
                self.set_active_session(session);
                BridgeResponse::Ok
            }
            BridgeRequest::Effort => {
                self.cycle_effort();
                BridgeResponse::Ok
            }
        }
    }

    pub fn poll_generation(&self) -> u64 {
        self.poll_generation
    }

    pub fn poll_approvals(&mut self) -> bool {
        let mut pending = Vec::new();
        if let Some(rx) = self.approval_rx.as_ref() {
            while let Ok(item) = rx.try_recv() {
                pending.push(item);
            }
        }
        let dirty = !pending.is_empty();
        for (call, respond) in pending {
            self.permission_pending = true;
            self.permission_respond = Some(respond);
            self.push_system(format!(
                "Approval required: {}\nargs: {}\n[y] allow  [n] deny",
                call.name,
                call.arguments.chars().take(200).collect::<String>()
            ));
        }
        dirty
    }

    pub fn resolve_permission(&mut self, allow: bool) {
        if let Some(tx) = self.permission_respond.take() {
            let _ = tx.send(if allow {
                Decision::Allow
            } else {
                Decision::Deny
            });
        }
        self.permission_pending = false;
    }

    pub fn set_active_session(&mut self, idx: usize) {
        if idx < self.sessions.len() {
            self.active_session = idx;
        }
    }

    pub fn use_computer(&mut self) {
        if let Some((idx, _)) = self
            .sessions
            .iter()
            .enumerate()
            .find(|(_, session)| session.kind == SessionKind::ComputerUse)
        {
            self.active_session = idx;
        }
    }

    pub fn use_coding(&mut self) {
        if let Some((idx, _)) = self
            .sessions
            .iter()
            .enumerate()
            .find(|(_, session)| session.kind == SessionKind::Coding && session.agent.is_some())
        {
            self.active_session = idx;
        }
    }

    pub fn start_login(&mut self, provider: Option<&str>) {
        if self.login_busy {
            return;
        }
        let name = provider.unwrap_or("openai");
        let Some(oauth) = oauth_provider(name) else {
            self.push_system(format!(
                "Unknown provider: {name}. Try openai, grok, claude, or gemini."
            ));
            return;
        };
        self.login_busy = true;
        self.push_system(format!("Starting OAuth login for {name}..."));
        let tx = self.event_tx.clone();
        std::thread::spawn(move || match rs_ai_oauth::start_oauth_flow(oauth) {
            Ok(tokens) => {
                if let Err(error) = rs_ai_oauth::credentials::save(&oauth, &tokens) {
                    let _ = tx.send(CompanionEvent::LoginFailed(error.to_string()));
                    return;
                }
                let _ = tx.send(CompanionEvent::LoginSucceeded);
            }
            Err(error) => {
                let _ = tx.send(CompanionEvent::LoginFailed(error.to_string()));
            }
        });
    }

    pub fn send_prompt(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        if let Some(command) = parse_host_command(&text) {
            self.input.clear();
            self.dispatch_command(command);
            return;
        }
        if text.starts_with('/') {
            self.input.clear();
            self.push_system(format!(
                "unknown command: {text}. Try /login, /clear, /coding, /computer, /effort, /scope."
            ));
            return;
        }

        self.dispatch_prompt(text, true);
    }

    fn dispatch_prompt(&mut self, text: String, clear_composer: bool) {
        let agent = {
            let Some(session) = self.active_session_mut() else {
                return;
            };
            if session.busy {
                session.follow_ups.push_back(text);
                if clear_composer {
                    self.input.clear();
                }
                return;
            }
            let Some(agent) = session.agent.clone() else {
                session.messages.push(MessageItem::new(
                    "system",
                    "Log in first. Click login or send `/login openai`.",
                ));
                return;
            };
            session
                .messages
                .push(MessageItem::new("user", text.clone()));
            session.busy = true;
            agent
        };
        if clear_composer {
            self.input.clear();
        }
        let session_idx = self.active_session;
        let tx = self.event_tx.clone();
        runtime().handle().spawn(async move {
            let mut agent = agent.lock().await;
            if let Err(error) = agent.prompt(&text).await {
                let _ = tx.send(CompanionEvent::SessionError(session_idx, error.to_string()));
            }
            let _ = tx.send(CompanionEvent::PromptFinished(session_idx));
        });
    }

    fn drain_follow_up(&mut self, idx: usize) {
        let next = self
            .sessions
            .get_mut(idx)
            .and_then(|session| session.follow_ups.pop_front());
        let Some(text) = next else {
            return;
        };
        let previous = self.active_session;
        self.active_session = idx;
        self.dispatch_prompt(text, false);
        self.active_session = previous;
    }

    pub fn cycle_effort(&mut self) {
        self.effort = next_effort(&self.effort);
        for session in &self.sessions {
            if let Some(agent) = &session.agent {
                if let Ok(mut agent) = agent.try_lock() {
                    agent.set_reasoning_effort(Some(self.effort.clone()));
                }
            }
        }
    }

    fn dispatch_command(&mut self, command: HostCommand) {
        match command {
            HostCommand::Login(provider) => {
                self.start_login(provider.as_deref());
            }
            HostCommand::Clear => {
                if let Some(session) = self.active_session_mut() {
                    session.messages.clear();
                    session.streaming_role = None;
                    session.streaming_content.clear();
                    session.busy = false;
                    session.context_pct = 0;
                    session.follow_ups.clear();
                }
            }
            HostCommand::ComputerUse => self.use_computer(),
            HostCommand::Coding => self.use_coding(),
            HostCommand::CycleEffort => self.cycle_effort(),
        }
    }

    pub fn capture_screen(&mut self) {
        let session_idx = self.active_session;
        let agent = {
            let Some(session) = self.active_session_mut() else {
                return;
            };
            if session.busy {
                return;
            }
            let Some(agent) = session.agent.clone() else {
                session.messages.push(MessageItem::new(
                    "system",
                    "Log in first. Click login or send `/login openai`.",
                ));
                return;
            };
            session
                .messages
                .push(MessageItem::new("user", "see screen"));
            session.busy = true;
            agent
        };
        let tx = self.event_tx.clone();
        runtime().handle().spawn(async move {
            let mut agent = agent.lock().await;
            if let Err(error) = agent
                .prompt("Use cu_see to capture my screen, then tell me what you see. Wait for my next instruction.")
                .await
            {
                let _ = tx.send(CompanionEvent::SessionError(session_idx, error.to_string()));
            }
            let _ = tx.send(CompanionEvent::PromptFinished(session_idx));
        });
    }

    pub fn interrupt(&mut self) {
        if let Some(session) = self.active_session_mut() {
            if let Some(cancellation) = &session.cancellation {
                cancellation.cancel();
            }
            session.busy = false;
            session.streaming_role = None;
            session.streaming_content.clear();
        }
    }

    pub fn push_char(&mut self, ch: &str) {
        self.insert_text(ch);
    }

    pub fn insert_text(&mut self, text: &str) {
        self.input.push_str(text);
    }

    pub fn pop_char(&mut self) {
        self.input.pop();
    }

    pub fn apply_composer_input(&mut self, input: ComposerInput, clipboard: Option<&str>) {
        match input {
            ComposerInput::Submit => self.send_prompt(),
            ComposerInput::Newline => self.input.push('\n'),
            ComposerInput::Backspace => self.pop_char(),
            ComposerInput::Paste => {
                if let Some(text) = clipboard {
                    self.insert_text(text);
                }
            }
            ComposerInput::Text(text) => self.insert_text(&text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_commands() {
        assert_eq!(
            parse_host_command("/login grok"),
            Some(HostCommand::Login(Some("grok".into())))
        );
        assert_eq!(parse_host_command("/clear"), Some(HostCommand::Clear));
        assert_eq!(
            parse_host_command("/scope computer_use"),
            Some(HostCommand::ComputerUse)
        );
        assert_eq!(parse_host_command("hello"), None);
        assert_eq!(
            parse_host_command("/effort"),
            Some(HostCommand::CycleEffort)
        );
    }

    #[test]
    fn snapshot_lists_sessions_and_effort() {
        let host = CompanionHost::boot();
        let snap = host.snapshot();
        assert_eq!(snap.effort, "high");
        assert_eq!(snap.queued, 0);
        assert!(!snap.sessions.is_empty());
        assert_eq!(snap.composer_action, "login");
        assert!(snap.sessions.iter().any(|row| row.active));
    }

    #[test]
    fn queues_follow_up_while_busy() {
        let mut host = CompanionHost::boot();
        if let Some(session) = host.active_session_mut() {
            session.busy = true;
        }
        host.input = "later".into();
        host.send_prompt();
        let snap = host.snapshot();
        assert!(host.input.is_empty());
        assert_eq!(snap.queued, 1);
        assert_eq!(snap.composer_action, "queue");
        assert_eq!(
            host.active_session()
                .map(|s| s.follow_ups.iter().cloned().collect::<Vec<_>>()),
            Some(vec!["later".to_string()])
        );
    }

    #[test]
    fn prompt_finished_drains_queued_follow_up() {
        let mut host = CompanionHost::boot();
        if let Some(session) = host.active_session_mut() {
            session.busy = true;
            session.follow_ups.push_back("next turn".into());
        }
        let idx = host.active_session;
        let _ = host.event_tx.send(CompanionEvent::PromptFinished(idx));
        host.poll();
        let session = host.active_session().expect("session");
        assert!(session.follow_ups.is_empty());
        assert!(session
            .messages
            .iter()
            .any(|message| message.content.contains("Log in first")
                || message.content.contains("next turn")));
    }

    #[test]
    fn cycle_effort_and_bridge_snapshot() {
        let mut host = CompanionHost::boot();
        host.cycle_effort();
        assert_eq!(host.snapshot().effort, "xhigh");
        host.input = "/effort".into();
        host.send_prompt();
        assert_eq!(host.snapshot().effort, "low");
        let reply = host.apply_bridge_json(r#"{"v":1,"op":"snapshot"}"#);
        assert!(reply.contains(r#""op":"snapshot""#));
        assert!(reply.contains(r#""effort":"low""#));
        assert!(reply.contains(r#""v":1"#));
    }

    #[test]
    fn empty_send_is_noop() {
        let mut host = CompanionHost::boot();
        let before = host.snapshot().messages.len();
        host.send_prompt();
        assert_eq!(host.snapshot().messages.len(), before);
    }

    #[test]
    fn unknown_login_provider_is_reported() {
        let mut host = CompanionHost::boot();
        host.input = "/login not-a-provider".into();
        host.send_prompt();
        let snap = host.snapshot();
        assert!(snap
            .messages
            .iter()
            .any(|message| message.content.contains("Unknown provider")));
        assert!(host.input.is_empty());
    }

    #[test]
    fn unknown_slash_is_not_sent_as_prompt() {
        let mut host = CompanionHost::boot();
        host.input = "/nope".into();
        host.send_prompt();
        assert!(host
            .snapshot()
            .messages
            .iter()
            .any(|message| message.content.contains("unknown command")));
    }

    #[test]
    fn poll_applies_stream_deltas() {
        let mut host = CompanionHost::boot();
        if host.sessions.is_empty() {
            host.sessions
                .push(AgentSession::new("coding", SessionKind::Coding, None, "m"));
        }
        let idx = host.active_session;
        let before = host.poll_generation();
        let _ = host.event_tx.send(CompanionEvent::Session(
            idx,
            rx4::agent::Event::MessageDelta {
                delta: "streamed".into(),
            },
        ));
        let tick = host.poll();
        assert!(tick.dirty);
        assert!(host.poll_generation() > before);
        assert!(host
            .snapshot()
            .messages
            .iter()
            .any(|message| message.role == "assistant" && message.content.contains("streamed")));
        let after = host.poll_generation();
        assert!(!host.poll().dirty);
        assert_eq!(host.poll_generation(), after);
    }

    #[test]
    fn composer_maps_enter_shift_enter_and_paste() {
        assert_eq!(
            composer_input_from_key("enter", false, false, false, false, None),
            Some(ComposerInput::Submit)
        );
        assert_eq!(
            composer_input_from_key("enter", true, false, false, false, None),
            Some(ComposerInput::Newline)
        );
        assert_eq!(
            composer_input_from_key("j", false, false, true, false, None),
            Some(ComposerInput::Newline)
        );
        assert_eq!(
            composer_input_from_key("v", false, true, false, false, None),
            Some(ComposerInput::Paste)
        );
        assert_eq!(
            composer_input_from_key("v", false, false, false, false, Some("v")),
            Some(ComposerInput::Text("v".into()))
        );
    }

    #[test]
    fn paste_and_newline_stay_in_composer_until_enter() {
        let mut host = CompanionHost::boot();
        host.apply_composer_input(ComposerInput::Text("hello".into()), None);
        host.apply_composer_input(ComposerInput::Newline, None);
        host.apply_composer_input(ComposerInput::Paste, Some("world\nmore"));
        assert_eq!(host.input, "hello\nworld\nmore");
        host.apply_composer_input(ComposerInput::Backspace, None);
        assert_eq!(host.input, "hello\nworld\nmor");
        host.apply_composer_input(ComposerInput::Submit, None);
        let snap = host.snapshot();
        assert!(
            host.input.is_empty()
                || snap
                    .messages
                    .iter()
                    .any(|message| message.content.contains("Log in first"))
        );
        if host.input.is_empty() {
            assert!(snap
                .messages
                .iter()
                .any(|message| message.content.contains("hello\nworld\nmor")));
        }
    }
    #[test]
    fn remote_prompt_keeps_desktop_draft() {
        let mut host = CompanionHost::boot();
        host.input = "desktop draft".into();
        let _ = host.apply_bridge_json(r#"{"v":1,"op":"prompt","text":"from phone"}"#);
        assert_eq!(host.input, "desktop draft");
    }

    #[test]
    fn drain_follow_up_keeps_composer() {
        let mut host = CompanionHost::boot();
        host.input = "still typing".into();
        if let Some(session) = host.active_session_mut() {
            session.busy = true;
            session.follow_ups.push_back("queued".into());
        }
        let idx = host.active_session;
        let _ = host.event_tx.send(CompanionEvent::PromptFinished(idx));
        host.poll();
        assert_eq!(host.input, "still typing");
    }
}
