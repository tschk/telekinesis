//! API-key management panel: OS-keychain save/delete with a menu-driven UI.
//!
//! Owns all keychain interaction timing — lookups are IPC, so flags are
//! cached per state change (refresh_cache), never queried per frame.

use crepuscularity_tui::{Template, TemplateContext, TemplateValue};

use crate::provider_catalog;

#[derive(Default)]
pub struct ApikeyPanel {
    pub open: bool,
    provider: Option<&'static provider_catalog::ProviderSpec>,
    /// Typing/pasting a new key.
    pub input_open: bool,
    buffer: String,
    action_choice: usize,
    status: Option<String>,
    /// Keychain roundtrips are expensive; cache both flags on state change.
    configured_cached: bool,
    keychain_cached: bool,
}

impl ApikeyPanel {
    pub fn open(&mut self, provider: &'static provider_catalog::ProviderSpec) {
        self.close();
        self.open = true;
        self.provider = Some(provider);
        self.refresh_cache();
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    pub fn provider_id(&self) -> Option<&'static str> {
        self.provider.map(|p| p.id)
    }

    /// One Keychain roundtrip per state change, not per frame.
    fn refresh_cache(&mut self) {
        if let Some(provider) = self.provider {
            self.configured_cached = provider_catalog::env_key(provider).is_some();
            self.keychain_cached = provider_catalog::has_provider_key(provider.id);
        }
    }

    /// Save + Close always; Delete only when a keychain entry exists.
    fn actions(&self) -> Vec<&'static str> {
        let mut actions = vec!["Save API key", "Close"];
        if self.keychain_cached {
            actions.insert(1, "Delete key");
        }
        actions
    }

    fn move_action(&mut self, delta: isize) {
        let len = self.actions().len();
        if len != 0 {
            self.action_choice =
                (self.action_choice as isize + delta).rem_euclid(len as isize) as usize;
        }
    }

    fn run_action(&mut self) {
        let Some(action) = self.actions().get(self.action_choice).copied() else {
            return;
        };
        match action {
            "Save API key" => {
                self.input_open = true;
                self.buffer.clear();
                self.status = None;
            }
            "Delete key" => {
                if let Some(provider) = self.provider {
                    self.status = Some(match provider_catalog::delete_provider_key(provider.id) {
                        Ok(()) => format!("Deleted {} key from keychain.", provider.name),
                        Err(e) => format!("Delete failed: {e}"),
                    });
                    self.refresh_cache();
                }
            }
            _ => self.close(),
        }
    }

    fn commit_input(&mut self) {
        let key = self.buffer.trim().to_string();
        if key.is_empty() {
            self.status = Some("Key cannot be empty.".into());
            return;
        }
        if let Some(provider) = self.provider {
            match provider_catalog::save_provider_key(provider.id, &key) {
                Ok(()) => {
                    self.status = Some("Saved to OS keychain.".into());
                    self.input_open = false;
                    self.buffer.clear();
                    self.refresh_cache();
                }
                Err(e) => {
                    self.status = Some(format!("Save failed: {e}"));
                    self.input_open = false;
                }
            }
        }
    }

    /// Paste into the key buffer; API keys are single-line, strip control
    /// chars and whitespace so trailing newlines never corrupt a paste.
    pub fn paste(&mut self, pasted: &str) {
        if !self.input_open {
            return;
        }
        let clean: String = pasted
            .chars()
            .filter(|c| !c.is_control() && !c.is_whitespace())
            .collect();
        self.buffer.push_str(&clean);
    }

    /// Handle a key press while the panel is open. Fully self-contained:
    /// status messages render inside the panel.
    pub fn handle_key(&mut self, code: crossterm::event::KeyCode) {
        if self.input_open {
            match code {
                crossterm::event::KeyCode::Enter => self.commit_input(),
                crossterm::event::KeyCode::Esc => {
                    self.input_open = false;
                    self.buffer.clear();
                }
                crossterm::event::KeyCode::Backspace => {
                    self.buffer.pop();
                }
                crossterm::event::KeyCode::Char(c) => self.buffer.push(c),
                _ => {}
            }
        } else if self.status.is_some() {
            match code {
                crossterm::event::KeyCode::Esc => self.close(),
                _ => {
                    self.status = None;
                    self.action_choice = 0;
                }
            }
        } else {
            match code {
                crossterm::event::KeyCode::Esc => self.close(),
                crossterm::event::KeyCode::Enter => self.run_action(),
                crossterm::event::KeyCode::Up => self.move_action(-1),
                crossterm::event::KeyCode::Down => self.move_action(1),
                _ => {}
            }
        }
    }

    pub fn set_template(&self, tpl: &mut Template) {
        tpl.set("apikey_detail_open", self.open);
        tpl.set("apikey_input_open", self.input_open);
        tpl.set("apikey_edit_buffer", self.buffer.clone());
        tpl.set("apikey_configured", self.configured_cached);
        tpl.set("apikey_has_keychain", self.keychain_cached);
        let action_rows: Vec<TemplateContext> =
            if self.open && !self.input_open && self.status.is_none() {
                self.actions()
                    .into_iter()
                    .enumerate()
                    .map(|(index, action)| {
                        let mut row = TemplateContext::new();
                        row.set("action", action);
                        row.set("selected", index == self.action_choice);
                        row
                    })
                    .collect()
            } else {
                Vec::new()
            };
        tpl.set("apikey_action_rows", TemplateValue::List(action_rows));
        tpl.set("apikey_status", self.status.clone().unwrap_or_default());
        if let Some(provider) = self.provider {
            tpl.set("apikey_name", provider.name);
            tpl.set("apikey_id", provider.id);
            tpl.set("apikey_env", provider.env_vars.join(", "));
            tpl.set("apikey_url", provider.base_url);
            tpl.set("apikey_default_model", provider.default_model);
            tpl.set("apikey_models", provider.models.join(", "));
        }
    }
}

/// Text fallback used by tests.
pub fn help_text(provider: &provider_catalog::ProviderSpec) -> String {
    let configured = if provider_catalog::env_key(provider).is_some() {
        "configured in this process"
    } else {
        "not configured"
    };
    format!(
        "{} ({})\n  status: {configured}\n  API key: {}\n  endpoint: {}\n  default model: {}\n  catalog: {}\n\nSet it with /apikey {} → Save API key (stored in your OS keychain), or:\n  export {}='<your-api-key>'\n\nKeys are read from the environment or keychain only and are never written to session files or preferences.",
        provider.name,
        provider.id,
        provider.env_vars.join(", "),
        provider.base_url,
        provider.default_model,
        provider.models.join(", "),
        provider.id,
        provider.env_vars[0],
    )
}
