//! Searchable API-key provider catalog menu (crepuscularity-rendered).
//!
//! Typing filters the catalog live; Enter opens the API-key panel for the
//! highlighted provider.

use crepuscularity_tui::{Template, TemplateContext, TemplateValue};

use crate::app::fuzzy_filter;
use crate::provider_catalog::{self, ProviderSpec};

#[derive(Default)]
pub struct ProviderMenu {
    pub open: bool,
    choice: usize,
}

impl ProviderMenu {
    pub fn filtered(&self, query: &str) -> Vec<&'static ProviderSpec> {
        fuzzy_filter(provider_catalog::API_KEY_PROVIDERS, query, |provider| {
            format!(
                "{} {} {} {} {}",
                provider.id,
                provider.name,
                provider.env_vars.join(" "),
                provider.aliases.join(" "),
                provider.models.join(" ")
            )
        })
    }

    pub fn move_choice(&mut self, delta: isize, query: &str) {
        let len = self.filtered(query).len();
        if len != 0 {
            self.choice = (self.choice as isize + delta).rem_euclid(len as isize) as usize;
        }
    }

    pub fn reset_choice(&mut self, query: &str) {
        let len = self.filtered(query).len();
        self.choice = self.choice.min(len.saturating_sub(1));
    }

    pub fn selected(&self, query: &str) -> Option<&'static ProviderSpec> {
        self.filtered(query).get(self.choice).copied()
    }

    pub fn set_template(&self, tpl: &mut Template, query: &str) {
        tpl.set("provider_menu_open", self.open);
        let rows: Vec<TemplateContext> = if self.open {
            self.filtered(query)
                .into_iter()
                .enumerate()
                .skip(self.choice.saturating_sub(3))
                .take(7)
                .map(|(index, provider)| {
                    let mut row = TemplateContext::new();
                    row.set("name", provider.name);
                    row.set("id", provider.id);
                    row.set("env", provider.env_vars.join(", "));
                    row.set("configured", provider_catalog::env_key(provider).is_some());
                    row.set("selected", index == self.choice);
                    row
                })
                .collect()
        } else {
            Vec::new()
        };
        tpl.set("provider_rows", TemplateValue::List(rows));
    }
}

/// Full key handling while the menu is open. Free function because it needs
/// both menu state and the composer on `App`.
pub(crate) fn handle_key(app: &mut crate::app::App, code: crossterm::event::KeyCode) {
    match code {
        crossterm::event::KeyCode::Enter => {
            if let Some(provider) = app.provider_menu.selected(&app.input) {
                app.close_provider_menu();
                app.apikey.open(provider);
            }
        }
        crossterm::event::KeyCode::Esc => app.close_provider_menu(),
        crossterm::event::KeyCode::Up => app.provider_menu.move_choice(-1, &app.input),
        crossterm::event::KeyCode::Down => app.provider_menu.move_choice(1, &app.input),
        crossterm::event::KeyCode::Backspace => {
            app.delete_back_at_cursor();
            app.provider_menu.reset_choice(&app.input);
        }
        crossterm::event::KeyCode::Delete => {
            app.delete_forward_at_cursor();
            app.provider_menu.reset_choice(&app.input);
        }
        crossterm::event::KeyCode::Char(c) => {
            app.insert_at_cursor(&c.to_string());
            app.provider_menu.reset_choice(&app.input);
        }
        _ => {}
    }
}
