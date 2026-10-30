//! In-process adapter for the durable local workspace. The cursor companion stays separate.
use crate::{
    host::{ComposerInput, HostTick},
    session::MessageItem,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{mpsc, Arc},
};
pub use telekinesis::local_runtime::{
    AgentRole, Permission, Project, Receipt, ReceiptState, SendMode, Thread, ThreadConfig,
};
use telekinesis::local_runtime::{CreateThread, Handoff, LocalRuntime, RuntimeEvent, Submit};

#[derive(Clone)]
pub struct DesktopSnapshot {
    pub input: String,
    pub projects: Vec<Project>,
    pub threads: Vec<Thread>,
    pub active_project_id: Option<String>,
    pub active_thread_id: Option<String>,
    pub messages: Vec<MessageItem>,
    pub receipts: Vec<Receipt>,
    pub busy: bool,
    pub status: String,
    pub error: Option<String>,
    pub config: ThreadConfig,
    pub context_pct: usize,
    pub options: Vec<Value>,
    pub reference_context: Option<String>,
}
pub struct DesktopHost {
    pub input: String,
    runtime: Option<Arc<LocalRuntime>>,
    events: mpsc::Receiver<RuntimeEvent>,
    ready: mpsc::Receiver<Result<Arc<LocalRuntime>, String>>,
    active_project_id: Option<String>,
    active_thread_id: Option<String>,
    streaming: HashMap<String, String>,
    context: HashMap<String, usize>,
    error: Option<String>,
    reference_context: Option<String>,
    generation: u64,
    drafts: HashMap<String, String>,
}
impl DesktopHost {
    pub fn boot() -> Self {
        let (tx, events) = mpsc::channel();
        let root = dirs::home_dir().map(|p| p.join(".telekinesis/local-runtime"));
        let (ready_tx, ready) = mpsc::channel();
        std::thread::spawn(move || {
            let result = root
                .ok_or_else(|| "Home directory unavailable; cannot open local history.".to_string())
                .and_then(|root| {
                    LocalRuntime::open(root, move |event| {
                        let _ = tx.send(event);
                    })
                });
            let _ = ready_tx.send(result);
        });
        Self {
            input: String::new(),
            runtime: None,
            events,
            ready,
            active_project_id: None,
            active_thread_id: None,
            streaming: HashMap::new(),
            context: HashMap::new(),
            error: None,
            reference_context: None,
            generation: 0,
            drafts: draft_path()
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|data| serde_json::from_slice(&data).ok())
                .unwrap_or_default(),
        }
    }
    fn initialize(&mut self, result: Result<Arc<LocalRuntime>, String>) {
        match result {
            Ok(runtime) => self.runtime = Some(runtime),
            Err(error) => self.error = Some(error),
        }
        if let Some(runtime) = &self.runtime {
            // LaunchServices can start an app in / or the user's home. Projects
            // are registered only by an explicit picker action, never by cwd.
            let home = dirs::home_dir();
            let selected = runtime.projects().into_iter().find(|project| {
                let path = std::path::Path::new(&project.path);
                path.parent().is_some() && home.as_deref() != Some(path)
            });
            if let Some(project) = selected {
                if let Err(error) = self.select_project(&project.id) {
                    self.error = Some(error);
                }
            }
        }
    }
    fn runtime(&self) -> Result<&Arc<LocalRuntime>, String> {
        self.runtime.as_ref().ok_or_else(|| {
            self.error
                .clone()
                .unwrap_or_else(|| "Local runtime unavailable.".into())
        })
    }
    pub fn snapshot(&self) -> DesktopSnapshot {
        let projects = self
            .runtime
            .as_ref()
            .map(|r| r.projects())
            .unwrap_or_default();
        let threads = self
            .runtime
            .as_ref()
            .zip(self.active_project_id.as_ref())
            .map(|(r, id)| r.threads(id))
            .unwrap_or_default();
        let loaded = self
            .runtime
            .as_ref()
            .zip(self.active_thread_id.as_ref())
            .and_then(|(r, id)| r.load_thread(id).ok());
        let mut messages: Vec<MessageItem> = loaded
            .as_ref()
            .map(|s| {
                s.messages
                    .iter()
                    .map(|m| MessageItem::new(&m.role, &m.text))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(stream) = self
            .active_thread_id
            .as_ref()
            .and_then(|id| self.streaming.get(id))
            .filter(|s| !s.is_empty())
        {
            if messages
                .last()
                .is_none_or(|message| message.role != "assistant" || message.content != *stream)
            {
                messages.push(MessageItem::new("assistant", stream));
            }
        }
        let busy = loaded.as_ref().is_some_and(|s| s.active_run_id.is_some());
        DesktopSnapshot {
            input: self.input.clone(),
            projects,
            threads,
            active_project_id: self.active_project_id.clone(),
            active_thread_id: self.active_thread_id.clone(),
            messages,
            receipts: loaded
                .as_ref()
                .map(|s| s.receipts.clone())
                .unwrap_or_default(),
            busy,
            status: if self.runtime.is_none() && self.error.is_none() {
                "opening local history"
            } else if self.error.is_some() {
                "needs attention"
            } else if self.active_project_id.is_none() {
                "choose a project"
            } else if busy {
                "working"
            } else {
                "ready"
            }
            .into(),
            error: self.error.clone(),
            config: loaded.map(|s| s.thread.config).unwrap_or_default(),
            context_pct: self
                .active_thread_id
                .as_ref()
                .and_then(|id| self.context.get(id))
                .copied()
                .unwrap_or(0),
            options: self
                .runtime
                .as_ref()
                .map(|r| r.options())
                .unwrap_or_default(),
            reference_context: self.reference_context.clone(),
        }
    }
    pub fn add_project(&mut self, path: &str) -> Result<(), String> {
        let project = self.runtime()?.add_project(path)?;
        self.select_project(&project.id)
    }
    pub fn select_project(&mut self, id: &str) -> Result<(), String> {
        if !self.runtime()?.projects().iter().any(|p| p.id == id) {
            return Err("Unknown project.".into());
        }
        self.save_draft()?;
        self.active_project_id = Some(id.into());
        self.active_thread_id = self.runtime()?.threads(id).first().map(|t| t.id.clone());
        self.reference_context = None;
        self.input = self
            .active_thread_id
            .as_ref()
            .and_then(|id| self.drafts.get(id))
            .cloned()
            .unwrap_or_default();
        self.error = None;
        if self.active_thread_id.is_none() {
            self.create_thread("New thread")?;
        }
        Ok(())
    }
    pub fn create_thread(&mut self, title: &str) -> Result<(), String> {
        let project_id = self
            .active_project_id
            .clone()
            .ok_or("Select a project first.")?;
        let thread = self.runtime()?.create_thread(CreateThread {
            project_id,
            title: title.into(),
            config: None,
            parent_thread_id: None,
            request_id: None,
        })?;
        self.select_thread(&thread.id)
    }
    pub fn select_thread(&mut self, id: &str) -> Result<(), String> {
        let thread = self.runtime()?.load_thread(id)?.thread;
        if self.active_project_id.as_ref() != Some(&thread.project_id) {
            return Err("Thread belongs to a different project.".into());
        }
        self.save_draft()?;
        self.active_thread_id = Some(id.into());
        self.reference_context = None;
        self.input = self.drafts.get(id).cloned().unwrap_or_default();
        self.error = None;
        Ok(())
    }
    pub fn rename_thread(&mut self, title: &str) -> Result<(), String> {
        let id = self
            .active_thread_id
            .as_ref()
            .ok_or("Select a thread first.")?;
        self.runtime()?.rename_thread(id, title)?;
        Ok(())
    }
    pub fn update_config(&mut self, config: ThreadConfig) -> Result<(), String> {
        let id = self
            .active_thread_id
            .as_ref()
            .ok_or("Select a thread first.")?;
        self.runtime()?.update_thread(id, config)?;
        self.error = None;
        Ok(())
    }
    pub fn set_reference_context(&mut self, context: Option<String>) {
        if context.as_ref().is_some_and(|s| s.len() > 128_000) {
            self.error = Some("Selected context exceeds 128 KB.".into());
            return;
        }
        self.reference_context = context;
    }
    pub fn send_prompt(&mut self) {
        self.send(SendMode::Steer);
    }
    pub fn send_queue(&mut self) {
        self.send(SendMode::Queue);
    }
    fn send(&mut self, mode: SendMode) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        let result = self
            .active_thread_id
            .clone()
            .ok_or_else(|| "Select a thread first.".into())
            .and_then(|thread_id| {
                self.runtime()?.submit(Submit {
                    thread_id,
                    client_message_id: format!("desktop-{}", uuid::Uuid::new_v4()),
                    text,
                    mode,
                    reference_context: self.reference_context.clone(),
                })
            });
        match result {
            Ok(_) => {
                self.input.clear();
                self.error = self.save_draft().err();
                self.reference_context = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub fn has_active_work(&self) -> bool {
        self.runtime.as_ref().is_some_and(|runtime| {
            runtime.projects().iter().any(|project| {
                runtime.threads(&project.id).iter().any(|thread| {
                    runtime
                        .load_thread(&thread.id)
                        .is_ok_and(|s| s.active_run_id.is_some())
                })
            })
        })
    }
    pub fn stop_all(&mut self) -> Result<(), String> {
        self.runtime()?.shutdown()
    }
    pub fn interrupt(&mut self) {
        let result = (|| {
            let id = self
                .active_thread_id
                .as_ref()
                .ok_or("Select a thread first.")?;
            let runtime = self.runtime()?;
            if let Some(run) = runtime.load_thread(id)?.active_run_id {
                runtime.cancel(id, &run)?;
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
        }
    }
    pub fn handoff(&mut self, request: Handoff) -> Result<(), String> {
        let child = self.runtime()?.handoff(request)?;
        self.select_thread(&child.thread.id)
    }
    pub fn poll(&mut self) -> HostTick {
        let mut tick = HostTick::default();
        if let Ok(ready) = self.ready.try_recv() {
            self.initialize(ready);
            tick.dirty = true;
        }
        while let Ok(event) = self.events.try_recv() {
            tick.dirty = true;
            match event.kind.as_str() {
                "engine" => match event.payload["type"].as_str() {
                    Some("MessageDelta") => {
                        if let Some(delta) = event.payload["delta"].as_str() {
                            self.streaming
                                .entry(event.thread_id.clone())
                                .or_default()
                                .push_str(delta);
                        }
                    }
                    Some("MessageEnd") | Some("AgentEnd") => {
                        self.streaming.remove(&event.thread_id);
                    }
                    Some("ContextUsage") => {
                        let used = event.payload["used_tokens"].as_u64().unwrap_or(0);
                        let total = event.payload["context_window"].as_u64().unwrap_or(0);
                        self.context.insert(
                            event.thread_id.clone(),
                            used.saturating_mul(100).checked_div(total).unwrap_or(0) as usize,
                        );
                    }
                    _ => {}
                },
                "settled" => {
                    self.streaming.remove(&event.thread_id);
                    if event.payload["state"] == "failed" {
                        self.error = Some(
                            event.payload["error"]
                                .as_str()
                                .unwrap_or("Run failed.")
                                .into(),
                        );
                    }
                }
                "storageError" => {
                    self.error = Some("History could not be saved; the run was stopped.".into())
                }
                _ => {}
            }
        }
        if tick.dirty {
            self.generation = self.generation.saturating_add(1);
        }
        tick
    }
    pub fn poll_generation(&self) -> u64 {
        self.generation
    }
    pub fn save_draft(&mut self) -> Result<(), String> {
        let Some(id) = &self.active_thread_id else {
            return Ok(());
        };
        self.drafts.insert(id.clone(), self.input.clone());
        let path = draft_path().ok_or("Home directory unavailable; draft cannot be saved.")?;
        let bytes = serde_json::to_vec(&self.drafts).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.tmp");
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .map_err(|e| format!("Draft could not be saved: {e}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .and_then(|_| std::fs::rename(temp, path))
            .map_err(|e| format!("Draft could not be saved: {e}"))
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

impl Drop for DesktopHost {
    fn drop(&mut self) {
        let _ = self.save_draft();
        if let Some(runtime) = &self.runtime {
            let _ = runtime.shutdown();
        }
    }
}

fn draft_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".telekinesis/native-drafts.json"))
}
