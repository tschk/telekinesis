//! Native Crepuscularity workspace; LocalRuntime remains the sole execution host.
mod menu;
use super::editor::{self, Submitted, TextInput};
use crepuscularity_gpui::prelude::*;
use crepuscularity_macros::view_file;
use gpui::*;
use std::{collections::HashSet, io::Read};
use telekinesis::context_import::{
    parse_chat_export, prepare_context, ImportPreview, PreparedContext, MAX_SOURCE_BYTES,
};
use telekinesis::local_runtime::Handoff;
use telekinesis_gui::desktop_host::{AgentRole, DesktopHost, Permission, ReceiptState, Workflow};

pub struct Workspace {
    host: DesktopHost,
    composer: Entity<TextInput>,
    search: Entity<TextInput>,
    budget: Entity<TextInput>,
    show_settings: bool,
    show_context: bool,
    preview: Option<ImportPreview>,
    selected: HashSet<String>,
    queue: bool,
    import_page: usize,
    error: Option<String>,
    handoff: Option<Handoff>,
    renaming: bool,
    saved_composer: String,
    close_pending: bool,
    close_after_settle: bool,
    sidebar_hidden: bool,
    light: bool,
    review_open: bool,
    _subscriptions: Vec<Subscription>,
}
struct ViewMessage {
    role: String,
    content: String,
}
impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| TextInput::new("What would you like to build?", cx));
        let search = cx.new(|cx| {
            let mut e = TextInput::new("Search threads", cx);
            e.compact = true;
            e
        });
        let budget = cx.new(|cx| {
            let mut e = TextInput::new("Character budget", cx);
            e.compact = true;
            e.set_text("12000".into(), cx);
            e
        });
        let submit = cx.subscribe_in(&composer, window, |this, _, _: &Submitted, _, cx| {
            this.send(cx)
        });
        let search_changed = cx.observe(&search, |_, _, cx| cx.notify());
        let find_thread = cx.subscribe_in(&search, window, |this, _, _: &Submitted, window, cx| {
            let query = this.search.read(cx).content.to_lowercase();
            let id = this
                .host
                .snapshot()
                .threads
                .iter()
                .find(|t| t.title.to_lowercase().contains(query.trim()))
                .map(|t| t.id.clone());
            if let Some(id) = id {
                this.select_thread(&id, window, cx);
            }
        });
        let budget_changed = cx.observe(&budget, |this, _, cx| {
            this.host.set_reference_context(None);
            cx.notify();
        });
        let changed = cx.observe(&composer, |this, _, cx| {
            if this.handoff.is_none() && !this.renaming {
                this.host.input = this.composer.read(cx).content.to_string();
            }
        });
        let bounds = cx.observe_window_bounds(window, |_, window, cx| {
            cx.notify();
            // Native resize arrives asynchronously; invalidate after that effect cycle too.
            cx.on_next_frame(window, |_, window, cx| {
                window.refresh();
                cx.notify();
            });
        });
        let activation = cx.observe_window_activation(window, |_, _, cx| cx.notify());
        let appearance = cx.observe_window_appearance(window, |_, _, cx| cx.notify());
        let closing = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = closing.update(cx, |this, cx| this.request_close(window, cx));
            false
        });
        window.focus(&composer.read(cx).focus_handle);
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(80))
                .await;
            if this
                .update_in(cx, |this, window, cx| {
                    if this.close_after_settle && !this.host.has_active_work() {
                        window.remove_window();
                        cx.quit();
                        return;
                    }
                    if this.host.poll().dirty {
                        if this.handoff.is_none()
                            && !this.renaming
                            && this.composer.read(cx).content.as_ref() != this.host.input
                        {
                            this.composer
                                .update(cx, |e, cx| e.set_text(this.host.input.clone(), cx));
                        }
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
        let subscriptions = vec![
            find_thread,
            submit,
            changed,
            search_changed,
            budget_changed,
            activation,
            appearance,
            bounds,
        ];
        Self {
            host: DesktopHost::boot(),
            composer,
            search,
            budget,
            show_settings: false,
            show_context: false,
            preview: None,
            selected: HashSet::new(),
            queue: false,
            import_page: 0,
            error: None,
            handoff: None,
            renaming: false,
            saved_composer: String::new(),
            close_pending: false,
            close_after_settle: false,
            sidebar_hidden: false,
            light: false,
            review_open: false,
            _subscriptions: subscriptions,
        }
    }
    fn result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.error = result.err();
        cx.notify();
    }
    fn sync_draft(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if self.handoff.is_none() && !self.renaming {
            self.host.input = self.composer.read(cx).content.to_string();
        }
        self.host.save_draft()
    }
    fn refresh_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.handoff = None;
        self.renaming = false;
        self.preview = None;
        self.selected.clear();
        self.composer
            .update(cx, |e, cx| e.set_text(self.host.input.clone(), cx));
        window.focus(&self.composer.read(cx).focus_handle);
        cx.notify();
    }
    fn send(&mut self, cx: &mut Context<Self>) {
        if self.renaming {
            let title = self.composer.read(cx).content.to_string();
            let result = self.host.rename_thread(&title);
            if result.is_ok() {
                self.renaming = false;
                self.composer
                    .update(cx, |e, cx| e.set_text(self.saved_composer.clone(), cx));
            }
            self.result(result, cx);
            return;
        }
        if let Some(mut request) = self.handoff.clone() {
            request.text = self.composer.read(cx).content.to_string();
            let result = self.host.handoff(request);
            if result.is_ok() {
                self.handoff = None;
            }
            self.result(result, cx);
        } else {
            self.host.input = self.composer.read(cx).content.to_string();
            if self.queue && self.host.snapshot().worker.is_none() {
                self.host.send_queue();
            } else {
                self.host.send_prompt();
            }
        }
        if self.handoff.is_none() && !self.renaming {
            self.composer
                .update(cx, |e, cx| e.set_text(self.host.input.clone(), cx));
        }
        cx.notify();
    }
    fn send_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.send(cx);
    }
    fn stop(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.host.interrupt();
        cx.notify();
    }
    fn new_thread(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.sync_draft(cx).and_then(|_| {
            self.host.create_thread(&format!(
                "Thread {}",
                self.host.snapshot().threads.len() + 1
            ))
        });
        if result.is_ok() {
            self.refresh_editor(window, cx);
        }
        self.result(result, cx);
    }
    fn select_coordinator(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let result = self
            .sync_draft(cx)
            .and_then(|_| self.host.select_coordinator());
        if result.is_ok() {
            self.refresh_editor(window, cx);
        }
        self.result(result, cx);
    }
    fn open_project(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = self.sync_draft(cx) {
            self.error = Some(e);
            cx.notify();
            return;
        }
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open local project".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = chosen.await {
                if let Some(path) = paths.first() {
                    let _ = this.update(cx, |this, cx| {
                        let result = this.host.add_project(&path.to_string_lossy());
                        if result.is_ok() {
                            this.handoff = None;
                            this.renaming = false;
                            this.preview = None;
                            this.selected.clear();
                            this.composer
                                .update(cx, |e, cx| e.set_text(this.host.input.clone(), cx));
                        }
                        this.result(result, cx);
                    });
                }
            }
        })
        .detach();
    }
    fn toggle_sidebar(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_hidden = !self.sidebar_hidden;
        cx.notify();
    }
    fn toggle_settings(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.show_settings = !self.show_settings;
        self.show_context = false;
        cx.notify();
    }
    fn toggle_context(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.show_context = !self.show_context;
        self.show_settings = false;
        cx.notify();
    }
    fn close_inspector(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.show_context = false;
        self.show_settings = false;
        cx.notify();
    }
    fn toggle_queue(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.host.snapshot().worker.is_some() {
            return;
        }
        self.queue = !self.queue;
        cx.notify();
    }
    fn retry_settlement(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let result = self.host.retry_settlement();
        self.result(result, cx);
    }
    fn cycle_workflow(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let snap = self.host.snapshot();
        if !cfg!(feature = "coordinator-preview")
            || snap.busy
            || snap.worker.is_some()
            || snap
                .coordinator
                .as_ref()
                .is_some_and(|t| Some(&t.id) == snap.active_thread_id.as_ref())
        {
            return;
        }
        let mut config = snap.config;
        config.workflow = match config.workflow {
            Workflow::Direct => Workflow::Coordinator,
            Workflow::Coordinator => Workflow::Direct,
        };
        let result = self.host.update_config(config);
        self.result(result, cx);
    }
    fn cycle_role(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let snap = self.host.snapshot();
        if snap.busy {
            return;
        }
        let mut config = snap.config;
        config.role = match config.role {
            AgentRole::General => AgentRole::Research,
            AgentRole::Research => AgentRole::Review,
            AgentRole::Review => AgentRole::Plan,
            AgentRole::Plan => AgentRole::General,
        };
        let result = self.host.update_config(config);
        self.result(result, cx);
    }
    fn cycle_permission(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let snap = self.host.snapshot();
        if snap.busy {
            return;
        }
        let mut config = snap.config;
        config.permission = if config.permission == Permission::Yolo {
            Permission::Restrictive
        } else {
            Permission::Yolo
        };
        let result = self.host.update_config(config);
        self.result(result, cx);
    }
    fn start_rename(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let snap = self.host.snapshot();
        let Some(thread) = snap
            .threads
            .iter()
            .find(|t| Some(&t.id) == snap.active_thread_id.as_ref())
        else {
            return;
        };
        if self.handoff.is_some() || self.renaming {
            return;
        }
        self.saved_composer = self.composer.read(cx).content.to_string();
        self.renaming = true;
        self.composer
            .update(cx, |e, cx| e.set_text(thread.title.clone(), cx));
        window.focus(&self.composer.read(cx).focus_handle);
        cx.notify();
    }
    fn start_handoff(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.handoff.is_some() || self.renaming {
            return;
        }
        let snap = self.host.snapshot();
        if snap.worker.is_some() {
            self.error = Some("Handoff is available only from a root thread.".into());
            cx.notify();
            return;
        }
        if snap.busy
            || snap
                .receipts
                .iter()
                .any(|r| matches!(r.state, ReceiptState::Accepted | ReceiptState::Queued))
        {
            self.error =
                Some("Stop pending work and wait for settlement before handing off.".into());
            cx.notify();
            return;
        }
        let Some(id) = snap.active_thread_id else {
            return;
        };
        self.saved_composer = self.composer.read(cx).content.to_string();
        self.handoff = Some(Handoff {
            source_thread_id: id,
            source_run_id: snap.receipts.last().map(|r| r.run_id.clone()),
            request_id: format!("native-handoff-{}", uuid::Uuid::new_v4()),
            text: String::new(),
        });
        let transcript = snap
            .messages
            .iter()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|m| {
                format!(
                    "{}: {}",
                    m.role,
                    m.content.chars().take(1500).collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        self.composer.update(cx,|e,cx|e.set_text(format!("Continue this linked thread.\n\nGoal and next action:\n\nRecent reference excerpts:\n{transcript}"),cx));
        window.focus(&self.composer.read(cx).focus_handle);
        cx.notify();
    }
    fn cancel_handoff(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.handoff.is_none() && !self.renaming {
            return;
        }
        self.handoff = None;
        self.renaming = false;
        self.composer
            .update(cx, |e, cx| e.set_text(self.saved_composer.clone(), cx));
        window.focus(&self.composer.read(cx).focus_handle);
        cx.notify();
    }
    fn original_thread(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let snap = self.host.snapshot();
        let parent = snap
            .threads
            .iter()
            .find(|t| Some(&t.id) == snap.active_thread_id.as_ref())
            .and_then(|t| t.parent_thread_id.clone());
        if let Some(id) = parent {
            self.select_thread(&id, window, cx);
        }
    }
    fn select_thread(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let result = self
            .sync_draft(cx)
            .and_then(|_| self.host.select_thread(id));
        if result.is_ok() {
            self.refresh_editor(window, cx);
        }
        self.result(result, cx);
    }
    fn remove_context(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.host.set_reference_context(None);
        self.preview = None;
        self.selected.clear();
        window.focus(&self.composer.read(cx).focus_handle);
        window.refresh();
        cx.notify();
    }
    fn import_chat(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Preview chat export".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = chosen.await {
                if let Some(path) = paths.into_iter().next() {
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
                            let mut bytes = Vec::new();
                            file.take(MAX_SOURCE_BYTES as u64 + 1)
                                .read_to_end(&mut bytes)
                                .map_err(|e| e.to_string())?;
                            if bytes.len() > MAX_SOURCE_BYTES {
                                return Err("Export exceeds 10 MB.".into());
                            }
                            let text = String::from_utf8(bytes)
                                .map_err(|_| "Export must be UTF-8.".to_string())?;
                            parse_chat_export(
                                &text,
                                &path.file_name().unwrap_or_default().to_string_lossy(),
                            )
                            .map_err(|e| e.to_string())
                        })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        match result {
                            Ok(p) => {
                                this.preview = Some(p);
                                this.import_page = 0;
                                this.selected.clear();
                                this.host.set_reference_context(None);
                                this.error = None;
                            }
                            Err(e) => this.error = Some(e),
                        };
                        window.refresh();
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
    fn prepared(&self, cx: &App) -> Result<PreparedContext, String> {
        let budget = context_budget(&self.budget.read(cx).content)?;
        prepare_context(
            self.preview.as_ref().ok_or("Choose an export first.")?,
            &self.selected.iter().cloned().collect::<Vec<_>>(),
            budget,
        )
        .map_err(|e| e.to_string())
    }
    fn attach_context(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.host.snapshot().worker.is_some() {
            self.error = Some("Reference context can only be attached to a root thread.".into());
            cx.notify();
            return;
        }
        match self.prepared(cx) {
            Ok(p) if !p.included_entry_ids.is_empty() => {
                self.host.set_reference_context(Some(p.text));
                self.show_context = false;
                window.focus(&self.composer.read(cx).focus_handle);
                self.error = None;
            }
            Ok(_) => self.error = Some("Select at least one message.".into()),
            Err(e) => self.error = Some(e),
        }
        window.refresh();
        cx.notify();
    }
    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_pending {
            return;
        }
        if let Err(e) = self.sync_draft(cx) {
            self.error = Some(e);
            cx.notify();
            return;
        }
        let active = self.host.has_active_work();
        let editing = self.handoff.is_some() || self.renaming;
        if !active && !editing {
            window.remove_window();
            cx.quit();
            return;
        }
        self.close_pending = true;
        let answer = window.prompt(
            PromptLevel::Warning,
            if active {"Stop active work and close?"}else{"Discard this edit and close?"},
            Some(if editing {"The handoff or name edit will be discarded. Your original message draft and thread history are saved."}else{"Your thread history and draft are saved locally."}),
            &["Keep working", if active {"Stop and close"}else{"Discard edit and close"}],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await;
            let _ = this.update_in(cx, |this, _window, cx| {
                this.close_pending = false;
                if answer == Ok(1) {
                    if let Err(error) = this.host.stop_all() {
                        this.error = Some(error);
                    } else {
                        this.close_after_settle = true;
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

fn context_budget(raw: &str) -> Result<usize, String> {
    let raw = raw.trim();
    if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_digit()) {
        return Err("Budget must be a whole number from 1 to 32,000.".into());
    }
    raw.parse::<usize>()
        .ok()
        .filter(|n| *n > 0 && *n <= 32000)
        .ok_or_else(|| "Budget must be a whole number from 1 to 32,000.".into())
}

fn display_message(text: &str) -> String {
    if let Some((prompt, _)) =
        text.split_once("\n\nUntrusted reference data follows as a JSON string.")
    {
        format!("{prompt}\n\n[Selected reference context]")
    } else {
        text.to_owned()
    }
}

impl Workspace {
    fn action(&self, id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
        div()
            .id(id)
            .px_3()
            .py_2()
            .rounded_md()
            .text_size(px(12.))
            .text_color(rgb(if self.light { 0x444650 } else { 0xbabac6 }))
            .bg(rgb(if self.light { 0xeeeef1 } else { 0x29292c }))
            .hover(|s| s.bg(rgb(if self.light { 0xe4e4e9 } else { 0x343438 })))
            .cursor_pointer()
            .child(label.into())
    }
}

impl Workspace {
    fn context_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = rgb(if self.light { 0x6b6b73 } else { 0xa2a2aa });
        let foreground = rgb(if self.light { 0x29292e } else { 0xededf0 });
        let border = rgb(if self.light { 0xe1e1e5 } else { 0x343438 });
        let mut panel = div().flex().flex_col().gap_4().child(
            self.action(
                "choose-export",
                if self.preview.is_some() {
                    "Change export…"
                } else {
                    "Choose chat export…"
                },
            )
            .on_click(cx.listener(Self::import_chat)),
        );
        let Some(preview) = &self.preview else {
            return panel.child(div().text_size(px(13.)).text_color(muted)
                .child("Select messages from a local chat export to use as reference. Nothing is attached until you review it.")).into_any_element();
        };
        panel = panel
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(foreground)
                            .child(preview.source.file_name.clone()),
                    )
                    .child(div().text_size(px(12.)).text_color(muted).child(format!(
                        "{} · {}",
                        preview.source.provider, preview.source.format
                    ))),
            )
            .children(
                preview
                    .warnings
                    .iter()
                    .map(|w| div().text_size(px(12.)).text_color(muted).child(w.clone())),
            );
        let entries = preview
            .conversations
            .iter()
            .flat_map(|chat| chat.entries.iter().map(move |entry| (chat, entry)))
            .collect::<Vec<_>>();
        let pages = entries.len().div_ceil(30).max(1);
        if pages > 1 {
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.action("previous-context", "←").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.import_page = this.import_page.saturating_sub(1);
                            cx.notify();
                        },
                    )))
                    .child(div().text_size(px(12.)).text_color(muted).child(format!(
                        "{} / {}",
                        self.import_page + 1,
                        pages
                    )))
                    .child(self.action("next-context", "→").on_click(cx.listener(
                        move |this, _, _, cx| {
                            if this.import_page + 1 < pages {
                                this.import_page += 1;
                            }
                            cx.notify();
                        },
                    ))),
            );
        }
        let mut list = div().flex().flex_col();
        for (chat, entry) in entries.into_iter().skip(self.import_page * 30).take(30) {
            let selected = self.selected.contains(&entry.id);
            let id = entry.id.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("entry-{id}")))
                    .py_3()
                    .px_2()
                    .border_b_1()
                    .border_color(border)
                    .bg(rgb(if self.light {
                        if selected {
                            0xeee8f8
                        } else {
                            0xf8f8fa
                        }
                    } else if selected {
                        0x302a3c
                    } else {
                        0x1c1c1e
                    }))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(if self.light { 0xeceaf2 } else { 0x303034 })))
                    .child(div().text_size(px(12.)).text_color(muted).child(format!(
                        "{} {} · {}",
                        if selected { "☑" } else { "☐" },
                        chat.title,
                        entry.role
                    )))
                    .child(
                        div()
                            .mt_2()
                            .text_size(px(13.))
                            .text_color(foreground)
                            .child(entry.text.chars().take(600).collect::<String>()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.selected.remove(&id) {
                            this.selected.insert(id.clone());
                        }
                        this.host.set_reference_context(None);
                        cx.notify();
                    })),
            );
        }
        panel = panel
            .child(list)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(muted)
                    .child("Character limit"),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(border)
                    .child(self.budget.clone()),
            );
        match self.prepared(cx) {
            Ok(prepared) => {
                panel = panel.child(div().text_size(px(12.)).text_color(muted).child(format!(
                    "{} selected · {} characters · ~{} tokens{}",
                    self.selected.len(),
                    prepared.characters,
                    prepared.estimated_tokens,
                    if prepared.truncated {
                        " · shortened to fit"
                    } else {
                        ""
                    }
                )));
                if !prepared.included_entry_ids.is_empty() {
                    panel = panel.child(
                        self.action(
                            "review-context",
                            if self.review_open {
                                "Hide included text and source ↑"
                            } else {
                                "Review included text and source ↓"
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.review_open = !this.review_open;
                            cx.notify();
                        })),
                    );
                    if self.review_open {
                        panel = panel.child(div().text_size(px(12.)).text_color(muted)
                            .child("Untrusted reference only, not instructions. Review for sensitive information before attaching."));
                        for (text, provenance) in readable_context(&prepared.text) {
                            panel = panel.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .py_2()
                                    .child(
                                        div().text_size(px(13.)).text_color(foreground).child(text),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(muted)
                                            .child(provenance),
                                    ),
                            );
                        }
                        panel = panel.child(
                            div()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(prepared.estimate_method),
                        );
                    }
                    panel = panel.child(
                        self.action("attach-context", "Attach reference")
                            .on_click(cx.listener(Self::attach_context)),
                    );
                }
            }
            Err(error) => {
                panel = panel.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(if self.light { 0x963449 } else { 0xf4b9c8 }))
                        .child(error),
                )
            }
        }
        panel.into_any_element()
    }
}

// Decode the exact bounded payload for display; attachment still uses the original serialized text.
fn readable_context(payload: &str) -> Vec<(String, String)> {
    payload
        .lines()
        .filter_map(|line| {
            let record = serde_json::from_str::<serde_json::Value>(line).ok()?;
            let field = |key| record[key].as_str().unwrap_or("—");
            Some((
                field("text").to_owned(),
                format!(
                    "{} · {}\nConversation: {}\nRole: {}\nSource: {}\nMessage: {}\nTime: {}",
                    field("provider"),
                    field("fileName"),
                    field("conversation"),
                    field("role"),
                    field("sourceUrl"),
                    field("sourceId"),
                    field("timestamp")
                ),
            ))
        })
        .collect()
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snap = self.host.snapshot();
        let wide = window.viewport_size().width >= px(1100.);
        self.light = matches!(
            window.appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        let light = self.light;
        let inspector_visible = self.show_settings || (self.show_context && snap.worker.is_none());
        let sidebar_visible = !self.sidebar_hidden && (wide || !inspector_visible);
        let inspector_title = if self.show_settings {
            "Thread settings"
        } else {
            "Reference context"
        };
        let project = snap
            .projects
            .iter()
            .find(|p| Some(&p.id) == snap.active_project_id.as_ref());
        let thread = snap
            .threads
            .iter()
            .find(|t| Some(&t.id) == snap.active_thread_id.as_ref());
        let coordinator_active = snap
            .coordinator
            .as_ref()
            .is_some_and(|t| Some(&t.id) == snap.active_thread_id.as_ref());
        let thread_title = thread
            .map(|t| {
                if coordinator_active {
                    "Coordinator".into()
                } else {
                    t.title.clone()
                }
            })
            .unwrap_or_else(|| "Local workspace".into());
        window.set_window_title(&thread_title);
        let has_parent = thread.is_some_and(|t| t.parent_thread_id.is_some());
        let is_root = snap.worker.is_none();
        let can_send = snap
            .worker
            .as_ref()
            .is_none_or(|w| w.status == rx4::subagent::SubagentStatus::Running);
        let coordinator_item = div()
            .id("pinned-coordinator")
            .h(px(36.))
            .px_4()
            .mb_3()
            .flex()
            .items_center()
            .text_size(px(13.))
            .bg(rgb(if coordinator_active {
                if light {
                    0xe3e3e8
                } else {
                    0x333338
                }
            } else if light {
                0xf3f3f5
            } else {
                0x1c1c1e
            }))
            .hover(|s| s.bg(rgb(if light { 0xe6e6ea } else { 0x2d2d31 })))
            .cursor_pointer()
            .child("Coordinator")
            .on_click(cx.listener(Self::select_coordinator));
        let can_retry_settlement = snap.can_retry_settlement;
        let query = self.search.read(cx).content.to_lowercase();
        let mut thread_items = div().flex().flex_col().gap_3();
        for group in &snap.project_groups {
            let project = &group.project;
            let matching = group
                .threads
                .iter()
                .filter(|thread| thread.title.to_lowercase().contains(query.trim()))
                .collect::<Vec<_>>();
            if !query.trim().is_empty() && matching.is_empty() {
                continue;
            }
            let project_id = project.id.clone();
            let mut section = div().flex().flex_col().child(
                div()
                    .id(SharedString::from(format!("project-{}", project.id)))
                    .h(px(30.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .text_color(rgb(if light { 0x64646b } else { 0x96969e }))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(if light { 0xe6e6ea } else { 0x2d2d31 })))
                    .child(project.name.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let result = this
                            .sync_draft(cx)
                            .and_then(|_| this.host.select_project(&project_id));
                        if result.is_ok() {
                            this.refresh_editor(window, cx);
                        }
                        this.result(result, cx);
                    })),
            );
            for thread in matching {
                let active = Some(&thread.id) == snap.active_thread_id.as_ref();
                let id = thread.id.clone();
                section = section.child(
                    div()
                        .id(SharedString::from(format!("thread-{id}")))
                        .h(px(32.))
                        .pl(px(30.))
                        .pr_3()
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .truncate()
                        .bg(rgb(if active {
                            if light {
                                0xe3e3e8
                            } else {
                                0x333338
                            }
                        } else if light {
                            0xf3f3f5
                        } else {
                            0x1c1c1e
                        }))
                        .text_color(rgb(if active {
                            if light {
                                0x242426
                            } else {
                                0xededf0
                            }
                        } else if light {
                            0x64646b
                        } else {
                            0xa5a5ad
                        }))
                        .hover(|s| s.bg(rgb(if light { 0xe6e6ea } else { 0x2d2d31 })))
                        .cursor_pointer()
                        .child(if thread.parent_thread_id.is_some() {
                            format!("↳ {}", thread.title)
                        } else {
                            thread.title.clone()
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_thread(&id, window, cx)
                        })),
                );
            }
            thread_items = thread_items.child(section);
        }
        let search_input = self.search.clone();
        let composer = self.composer.clone();
        let show_settings = self.show_settings;
        let mut settings = div().flex().flex_col().gap_2();
        if is_root {
            settings = settings.child(
                self.action("details-handoff", "Handoff to a linked thread…")
                    .on_click(cx.listener(Self::start_handoff)),
            );
            if cfg!(feature = "coordinator-preview") {
                settings = settings.child(div().text_size(px(12.)).child(
                    "Local preview: roots use read tools; one delegate owns workspace writes until settled.",
                ));
            }
            if cfg!(feature = "coordinator-preview") && !coordinator_active {
                settings = settings.child(
                    self.action(
                        "settings-workflow",
                        format!("Workflow: {:?}", snap.config.workflow),
                    )
                    .on_click(cx.listener(Self::cycle_workflow)),
                );
            }
            for option in &snap.options {
                let provider = option["provider"].as_str().unwrap_or("").to_string();
                let model = option["model"].as_str().unwrap_or("").to_string();
                let selected = snap.config.provider.as_ref() == Some(&provider)
                    && snap.config.model.as_ref() == Some(&model);
                settings = settings.child(
                    self.action(
                        SharedString::from(format!("model-{provider}")),
                        format!("{} {provider} / {model}", if selected { "●" } else { "○" }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let snap = this.host.snapshot();
                        if snap.busy {
                            return;
                        }
                        let mut config = snap.config;
                        config.provider = Some(provider.clone());
                        config.model = Some(model.clone());
                        let result = this.host.update_config(config);
                        this.result(result, cx);
                    })),
                );
            }
            settings = settings
                .child(
                    self.action("settings-role", format!("Role: {:?}", snap.config.role))
                        .on_click(cx.listener(Self::cycle_role)),
                )
                .child(
                    self.action(
                        "settings-permission",
                        format!("Permission: {:?}", snap.config.permission),
                    )
                    .on_click(cx.listener(Self::cycle_permission)),
                );
            if snap.options.is_empty() {
                settings = settings.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(if self.light { 0x62636f } else { 0xa29daa }))
                        .child("No configured models. Configure a provider with tk login."),
                );
            }
        } else {
            settings = settings.child(div().text_size(px(12.)).child(
                "Model and permission policy inherit the root. Steering accepts up to 8 KiB of reference text.",
            ));
        }
        let error_text = self.error.clone().or(snap.error).unwrap_or_default();
        let has_error = !error_text.is_empty();
        let empty_history = snap.messages.is_empty();
        let empty_title = if project.is_some() {
            "Start with an idea."
        } else {
            "Open your project."
        };
        let empty_detail = if project.is_some() {
            "Choose a model and send your first message."
        } else {
            "Your threads and history stay on this Mac."
        };
        let messages = snap
            .messages
            .into_iter()
            .filter(|m| !m.content.trim().is_empty())
            .map(|m| ViewMessage {
                role: match m.role.as_str() {
                    "user" => "You".into(),
                    "assistant" => "Telekinesis".into(),
                    "tool" => "Tool result".into(),
                    _ => m.role,
                },
                content: display_message(&m.content),
            })
            .collect::<Vec<_>>();
        let messages = messages.iter();
        let receipts = snap
            .receipts
            .iter()
            .rev()
            .take(3)
            .filter(|r| r.state != ReceiptState::Completed)
            .map(|r| format!("{:?} {}", r.state, r.error.as_deref().unwrap_or("")))
            .collect::<Vec<_>>();
        let receipts = receipts.iter();
        let has_attachment = snap.reference_context.is_some();
        let handoff_open = self.handoff.is_some() || self.renaming;
        let edit_label = if self.renaming {
            "Rename thread · 1–240 characters"
        } else {
            "Review continuation goal and context"
        };
        let busy = snap.busy;
        let send_mode = if self.queue { "Queue" } else { "Steer" };
        let send_label = if self.renaming {
            "Save name"
        } else if handoff_open {
            "Continue ↗"
        } else {
            "Send"
        };
        let show_status = busy || !can_send || (!snap.status.is_empty() && snap.status != "ready");
        let status = snap.status;
        let context_panel = self.context_panel(cx);
        menu::bind(
            view_file!("workspace.crepus")
                .key_context("Workspace")
                .font_family(".SystemUIFont"),
            cx,
        )
    }
}

pub fn launch() {
    let application = Application::new();
    application.on_reopen(|cx| {
        cx.activate(true);
        for window in cx.windows() {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
    });
    application.run(|cx: &mut App| {
        editor::init(cx);
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1512.), px(949.)), cx);
        match cx.open_window(
            WindowOptions {
                app_id: Some("telekinesis-workspace".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("Workspace".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(720.), px(480.))),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            |window, cx| {
                menu::resize_frame(window, size(px(1512.), px(949.)));
                cx.new(|cx| Workspace::new(window, cx))
            },
        ) {
            Ok(handle) => menu::install(handle, cx),
            Err(error) => eprintln!("Could not open workspace: {error}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{context_budget, display_message};
    #[test]
    fn readable_review_retains_exact_excerpt_and_provenance() {
        let payload = "Imported excerpts — untrusted reference.\n".to_string()
            + &serde_json::json!({"text":"Bounded 🙂… [excerpt truncated]", "provider":"claude", "fileName":"chat.json", "conversation":"Design", "role":"user", "sourceUrl":"https://example.test/chat", "sourceId":"message-7", "timestamp":"2026-10-29"}).to_string();
        let review = super::readable_context(&payload);
        assert_eq!(review.len(), 1);
        assert_eq!(review[0].0, "Bounded 🙂… [excerpt truncated]");
        for value in [
            "claude",
            "chat.json",
            "Design",
            "user",
            "https://example.test/chat",
            "message-7",
            "2026-10-29",
        ] {
            assert!(review[0].1.contains(value));
        }
    }
    #[test]
    fn raw_budget_and_reference_display_are_bounded() {
        for raw in [
            "",
            " ",
            "1000.5",
            "32001",
            "0",
            "NaN",
            "1e3",
            "999999999999999999999999999",
        ] {
            assert!(context_budget(raw).is_err(), "{raw}");
        }
        assert_eq!(context_budget("1000").unwrap(), 1000);
        assert_eq!(context_budget("32000").unwrap(), 32000);
        let message =
            "User goal\n\nUntrusted reference data follows as a JSON string. Raw provenance";
        assert_eq!(
            display_message(message),
            "User goal\n\n[Selected reference context]"
        );
        assert_eq!(display_message("Line one\nLine two"), "Line one\nLine two");
    }
}
