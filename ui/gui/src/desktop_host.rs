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
    AgentRole, Permission, Project, ProjectThreads, Receipt, ReceiptState, SendMode, Thread,
    ThreadConfig, WorkerInfo, Workflow,
};
use telekinesis::local_runtime::{CreateThread, Handoff, LocalRuntime, RuntimeEvent, Submit};

#[derive(Clone)]
pub struct DesktopSnapshot {
    pub input: String,
    pub projects: Vec<Project>,
    pub project_groups: Vec<ProjectThreads>,
    pub coordinator: Option<Thread>,
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
    pub worker: Option<WorkerInfo>,
    pub worker_states: HashMap<String, WorkerInfo>,
    pub can_retry_settlement: bool,
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
        let root = std::env::var_os("TK_LOCAL_RUNTIME_ROOT")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                dirs::home_dir().map(|home| {
                    home.join(if cfg!(feature = "coordinator-preview") {
                        ".telekinesis/local-runtime-preview"
                    } else {
                        ".telekinesis/local-runtime"
                    })
                })
            });
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
        let project_groups = self
            .runtime
            .as_ref()
            .map(|r| r.project_groups())
            .unwrap_or_default();
        let coordinator = self.runtime.as_ref().and_then(|r| r.coordinator_thread());
        let threads: Vec<Thread> = project_groups
            .iter()
            .flat_map(|group| group.threads.iter().cloned())
            .chain(coordinator.iter().cloned())
            .collect();
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
        let busy = loaded
            .as_ref()
            .is_some_and(|s| s.active_run_id.is_some() || s.family_busy);
        let worker = loaded.as_ref().and_then(|s| s.worker.clone());
        let worker_states = self
            .runtime
            .as_ref()
            .map(|runtime| {
                projects
                    .iter()
                    .flat_map(|project| runtime.worker_states(&project.id))
                    .collect()
            })
            .unwrap_or_default();
        let can_retry_settlement = loaded.as_ref().is_some_and(|s| s.can_retry_settlement);
        DesktopSnapshot {
            input: self.input.clone(),
            projects,
            project_groups,
            coordinator,
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
            } else if loaded
                .as_ref()
                .is_some_and(|s| s.settlement_error.is_some())
            {
                "cleanup needs attention"
            } else if worker
                .as_ref()
                .is_some_and(|w| w.status == rx4::subagent::SubagentStatus::Interrupted)
            {
                "interrupted; review workspace ownership"
            } else if worker
                .as_ref()
                .is_some_and(|w| w.status == rx4::subagent::SubagentStatus::Stopping)
            {
                "stopping; waiting for settlement"
            } else if let Some(worker) = &worker {
                match worker.status {
                    rx4::subagent::SubagentStatus::Completed => "completed",
                    rx4::subagent::SubagentStatus::Failed => "failed",
                    rx4::subagent::SubagentStatus::Cancelled => "cancelled",
                    _ => "working",
                }
            } else if busy {
                "working"
            } else {
                "ready"
            }
            .into(),
            error: self.error.clone().or_else(|| {
                loaded.as_ref().and_then(|s| {
                    s.settlement_error.clone().or_else(|| {
                        s.worker
                            .as_ref()
                            .filter(|w| w.status == rx4::subagent::SubagentStatus::Failed)
                            .and_then(|w| w.result.as_ref()?.error.clone())
                    })
                })
            }),
            config: loaded
                .as_ref()
                .map(|s| s.thread.config.clone())
                .unwrap_or_default(),
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
            worker: worker.clone(),
            worker_states,
            can_retry_settlement,
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
        self.save_draft()?;
        self.active_project_id = Some(thread.project_id);
        self.active_thread_id = Some(id.into());
        self.reference_context = None;
        self.input = self.drafts.get(id).cloned().unwrap_or_default();
        self.error = None;
        Ok(())
    }
    pub fn select_coordinator(&mut self) -> Result<(), String> {
        let thread = if let Some(thread) = self.runtime()?.coordinator_thread() {
            thread
        } else {
            let project = self
                .active_project_id
                .as_deref()
                .ok_or("Open a project before starting Coordinator.")?;
            self.runtime()?.ensure_coordinator(project)?
        };
        self.select_thread(&thread.id)
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
            let snapshot = runtime.load_thread(id)?;
            if let Some(run) = snapshot
                .active_run_id
                .or_else(|| snapshot.receipts.last().map(|r| r.run_id.clone()))
            {
                runtime.cancel(id, &run)?;
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
        }
    }
    pub fn handoff(&mut self, request: Handoff) -> Result<(), String> {
        if self.snapshot().worker.is_some() {
            return Err("Handoff is available only from a root thread.".into());
        }
        let child = self.runtime()?.handoff(request)?;
        self.select_thread(&child.thread.id)
    }
    pub fn retry_settlement(&mut self) -> Result<(), String> {
        let id = self
            .active_thread_id
            .as_ref()
            .ok_or("Select a thread first.")?;
        self.runtime()?.retry_settlement(id)
    }
    pub fn poll(&mut self) -> HostTick {
        let mut tick = HostTick::default();
        if let Some(runtime) = &self.runtime {
            match runtime.refresh_workers() {
                Ok(changed) => tick.dirty |= changed,
                Err(e) => self.error = Some(e),
            }
        }
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
    std::env::var_os("TK_LOCAL_RUNTIME_ROOT")
        .map(|root| std::path::PathBuf::from(root).join("native-drafts.json"))
        .or_else(|| {
            dirs::home_dir().map(|home| {
                home.join(if cfg!(feature = "coordinator-preview") {
                    ".telekinesis/native-drafts-preview.json"
                } else {
                    ".telekinesis/native-drafts.json"
                })
            })
        })
}
