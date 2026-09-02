//! OAuth login provider selector menu (crepuscularity-rendered).

use crepuscularity_tui::{Template, TemplateContext, TemplateValue};

#[derive(Default)]
pub struct LoginMenu {
    pub open: bool,
    choice: usize,
}

impl LoginMenu {
    pub fn open(&mut self) {
        self.open = true;
        self.choice = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.choice = 0;
    }

    pub fn move_choice(&mut self, delta: isize) {
        let len = rs_ai_oauth::OAuthProvider::all().len();
        if len != 0 {
            self.choice = (self.choice as isize + delta).rem_euclid(len as isize) as usize;
        }
    }

    pub fn selected(&self) -> Option<rs_ai_oauth::OAuthProvider> {
        rs_ai_oauth::OAuthProvider::all().get(self.choice).copied()
    }

    fn display_name(oauth: rs_ai_oauth::OAuthProvider) -> &'static str {
        match oauth {
            rs_ai_oauth::OAuthProvider::ChatGpt => "ChatGPT Codex",
            rs_ai_oauth::OAuthProvider::Xai => "xAI Grok",
            rs_ai_oauth::OAuthProvider::Claude => "Anthropic Claude",
            rs_ai_oauth::OAuthProvider::Gemini => "Google Gemini",
            rs_ai_oauth::OAuthProvider::Antigravity => "Antigravity",
            rs_ai_oauth::OAuthProvider::Copilot => "GitHub Copilot",
            rs_ai_oauth::OAuthProvider::Kimi => "Kimi",
        }
    }

    pub fn set_template(&self, tpl: &mut Template) {
        tpl.set("login_menu_open", self.open);
        let rows: Vec<TemplateContext> = if self.open {
            rs_ai_oauth::OAuthProvider::all()
                .iter()
                .enumerate()
                .map(|(index, oauth)| {
                    let mut row = TemplateContext::new();
                    let name = oauth.name();
                    row.set("id", name);
                    row.set("display", Self::display_name(*oauth));
                    row.set("configured", crate::providers::provider_is_configured(name));
                    row.set("selected", index == self.choice);
                    row
                })
                .collect()
        } else {
            Vec::new()
        };
        tpl.set("login_rows", TemplateValue::List(rows));
    }
}

/// Key handling while the menu is open. `Some(msg)` after a completed login
/// attempt — caller surfaces it as a system message.
pub(crate) fn handle_key(
    app: &mut crate::app::App,
    code: crossterm::event::KeyCode,
) -> Option<String> {
    match code {
        crossterm::event::KeyCode::Enter => {
            let name = app
                .login_menu
                .selected()
                .map(|oauth| oauth.name().to_string())?;
            app.login_menu.close();
            let result = crate::providers::run_login_from_tui(Some(&name));
            Some(match result {
                Ok(()) => "Login complete. Restart tk to load the new provider.".to_string(),
                Err(error) => format!("Login failed: {error}"),
            })
        }
        crossterm::event::KeyCode::Esc => {
            app.login_menu.close();
            None
        }
        crossterm::event::KeyCode::Up => {
            app.login_menu.move_choice(-1);
            None
        }
        crossterm::event::KeyCode::Down => {
            app.login_menu.move_choice(1);
            None
        }
        _ => None,
    }
}
