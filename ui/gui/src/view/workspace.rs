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
use telekinesis_gui::desktop_host::{AgentRole, DesktopHost, Permission, ReceiptState};

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
    _subscriptions: Vec<Subscription>,
}
struct ViewMessage {
    role: String,
    content: String,
    is_user: bool,
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
            if self.queue {
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
        cx.notify();
    }
    fn toggle_context(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.show_context = !self.show_context;
        cx.notify();
    }
    fn toggle_queue(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.queue = !self.queue;
        cx.notify();
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
    fn remove_context(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.host.set_reference_context(None);
        self.preview = None;
        self.selected.clear();
        cx.notify();
    }
    fn import_chat(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Preview chat export".into()),
        });
        cx.spawn(async move |this, cx| {
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
                    let _ = this.update(cx, |this, cx| {
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
    fn attach_context(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        match self.prepared(cx) {
            Ok(p) if !p.included_entry_ids.is_empty() => {
                self.host.set_reference_context(Some(p.text));
                self.show_context = false;
                self.error = None;
            }
            Ok(_) => self.error = Some("Select at least one message.".into()),
            Err(e) => self.error = Some(e),
        }
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
            .bg(rgb(if self.light { 0xededf2 } else { 0x1b1d28 }))
            .hover(|s| s.bg(rgb(if self.light { 0xe2e2ea } else { 0x292b3a })))
            .cursor_pointer()
            .child(label.into())
    }
}

impl Workspace {
    fn context_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div().flex().flex_col().gap_3().child(
            self.action("choose-export", "Choose chat export…")
                .on_click(cx.listener(Self::import_chat)),
        );
        let Some(preview) = &self.preview else {
            return panel
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(rgb(if self.light { 0x646570 } else { 0x858591 }))
                        .child("Choose a local export, then select the messages to include."),
                )
                .into_any_element();
        };
        panel = panel
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(if self.light { 0x5f606c } else { 0xaaaab7 }))
                    .child(format!(
                        "{} · {}",
                        preview.source.provider, preview.source.file_name
                    )),
            )
            .children(preview.warnings.iter().map(|w| {
                div()
                    .text_size(px(12.))
                    .text_color(rgb(if self.light { 0x895d18 } else { 0xe6bd81 }))
                    .child(w.clone())
            }))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(if self.light { 0x62636f } else { 0x9999aa }))
                    .child("Character budget"),
            )
            .child(
                div()
                    .rounded_md()
                    .p_2()
                    .bg(rgb(if self.light { 0xf5f5f7 } else { 0x191b23 }))
                    .child(self.budget.clone()),
            );
        let prepared = self.prepared(cx);
        match &prepared {
            Ok(p) => {
                panel = panel.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(if self.light { 0x62636f } else { 0x9696a6 }))
                        .child(format!(
                            "{} selected · {} characters · ~{} tokens{}",
                            self.selected.len(),
                            p.characters,
                            p.estimated_tokens,
                            if p.truncated { " · truncated" } else { "" }
                        )),
                );
            }
            Err(e) => {
                panel = panel.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(if self.light { 0xa43d50 } else { 0xf1b4bc }))
                        .child(e.clone()),
                );
            }
        }
        let mut entries = Vec::new();
        for chat in &preview.conversations {
            for entry in &chat.entries {
                entries.push((chat.title.clone(), entry.clone()));
            }
        }
        let page = self.import_page;
        panel = panel.child(
            div()
                .flex()
                .gap_2()
                .child(self.action("previous-context", "←").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.import_page = this.import_page.saturating_sub(1);
                        cx.notify();
                    },
                )))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(if self.light { 0x62636f } else { 0x858594 }))
                        .child(format!(
                            "{} / {}",
                            page + 1,
                            entries.len().div_ceil(30).max(1)
                        )),
                )
                .child(
                    self.action("next-context", "→")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let count = this
                                .preview
                                .as_ref()
                                .map(|p| {
                                    p.conversations
                                        .iter()
                                        .map(|c| c.entries.len())
                                        .sum::<usize>()
                                })
                                .unwrap_or(0);
                            if (this.import_page + 1) * 30 < count {
                                this.import_page += 1;
                            }
                            cx.notify();
                        })),
                ),
        );
        for (title, entry) in entries.into_iter().skip(page * 30).take(30) {
            let selected = self.selected.contains(&entry.id);
            let id = entry.id.clone();
            panel = panel.child(
                div()
                    .id(SharedString::from(format!("entry-{id}")))
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(if selected { 0x8370c0 } else { 0x292b36 }))
                    .bg(rgb(if self.light {
                        if selected {
                            0xeee8f8
                        } else {
                            0xf5f5f7
                        }
                    } else if selected {
                        0x211d35
                    } else {
                        0x11131d
                    }))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(if self.light { 0xeae6f5 } else { 0x242334 })))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(if self.light { 0x6a5296 } else { 0xb1a5d3 }))
                            .child(format!(
                                "{} {} · {}",
                                if selected { "☑" } else { "☐" },
                                title,
                                entry.role
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(if self.light { 0x393b44 } else { 0xcac9d3 }))
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
        if let Ok(prepared) = prepared {
            if !prepared.included_entry_ids.is_empty() {
                panel = panel
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(if self.light { 0x64616a } else { 0xa49cac }))
                            .child("Review excerpt"),
                    )
                    .child(
                        div()
                            .p_3()
                            .bg(rgb(if self.light { 0xf3f3f6 } else { 0x080a10 }))
                            .text_size(px(11.))
                            .text_color(rgb(if self.light { 0x50515c } else { 0xa9a7b5 }))
                            .child(prepared.text),
                    )
                    .child(
                        self.action("attach-context", "Attach selected context")
                            .on_click(cx.listener(Self::attach_context)),
                    );
            }
        }
        panel.into_any_element()
    }
}
impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snap = self.host.snapshot();
        let wide = window.viewport_size().width >= px(1100.);
        let reference_layout = window.viewport_size().width >= px(1400.);
        let expanded_toolbar = wide && !self.show_context;
        self.light = matches!(
            window.appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        let light = self.light;
        let sidebar_visible = !self.sidebar_hidden && (wide || !self.show_context);
        // macOS needs its native frame: GPUI start_window_move is Linux-only.
        let titlebar = div().h(px(16.)).w_full();
        let project = snap
            .projects
            .iter()
            .find(|p| Some(&p.id) == snap.active_project_id.as_ref());
        let project_name = project
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Open project…".into());
        let project_path = project.map(|p| p.path.clone()).unwrap_or_default();
        let thread = snap
            .threads
            .iter()
            .find(|t| Some(&t.id) == snap.active_thread_id.as_ref());
        let thread_title = thread
            .map(|t| t.title.clone())
            .unwrap_or_else(|| "Local workspace".into());
        window.set_window_title(&thread_title);
        let has_parent = thread.is_some_and(|t| t.parent_thread_id.is_some());
        let query = self.search.read(cx).content.to_lowercase();
        let thread_items = div().flex().flex_col().gap_1().children(
            snap.threads
                .iter()
                .filter(|t| t.title.to_lowercase().contains(query.trim()))
                .map(|t| {
                    let active = Some(&t.id) == snap.active_thread_id.as_ref();
                    let id = t.id.clone();
                    div()
                        .id(SharedString::from(format!("thread-{id}")))
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .text_size(px(13.))
                        .bg(rgb(if self.light {
                            if active {
                                0xe5def5
                            } else {
                                0xf1f1f3
                            }
                        } else if active {
                            0x34313f
                        } else {
                            0x1c1d21
                        }))
                        .text_color(rgb(if self.light {
                            if active {
                                0x4b356f
                            } else {
                                0x555762
                            }
                        } else if active {
                            0xe4ddfa
                        } else {
                            0xa5a5b1
                        }))
                        .hover(|s| s.bg(rgb(if self.light { 0xe4e4ed } else { 0x2b2d39 })))
                        .cursor_pointer()
                        .child(t.title.clone())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_thread(&id, window, cx)
                        }))
                }),
        );
        let search_input = self.search.clone();
        let composer = self.composer.clone();
        let show_settings = self.show_settings;
        let mut settings = div().flex().flex_col().gap_2();
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
            .map(|m| ViewMessage {
                is_user: m.role == "user",
                role: m.role,
                content: display_message(&m.content),
            })
            .collect::<Vec<_>>();
        let messages = messages.iter();
        let receipts = snap
            .receipts
            .iter()
            .rev()
            .take(3)
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
        let model_label = snap
            .config
            .model
            .unwrap_or_else(|| "Choose model".into())
            .chars()
            .take(24)
            .collect::<String>();
        let role_label = format!("{:?}", snap.config.role);
        let permission_label = if snap.config.permission == Permission::Restrictive {
            "Restrictive"
        } else {
            "YOLO"
        };
        let send_mode = if self.queue { "Queue" } else { "Steer" };
        let send_label = if self.renaming {
            "Save name"
        } else if handoff_open {
            "Continue ↗"
        } else {
            "Send ↑"
        };
        let status = snap.status;
        let context_visible = self.show_context;
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
