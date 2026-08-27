//! Interactive `/config` menu (crepuscularity-rendered).

use crepuscularity_tui::{Template, TemplateContext, TemplateValue};

use crate::app::App;

#[derive(Default)]
pub struct ConfigMenu {
    pub open: bool,
    choice: usize,
}

impl ConfigMenu {
    pub fn open(&mut self) {
        self.open = true;
        self.choice = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.choice = 0;
    }

    pub fn move_choice(&mut self, delta: isize, len: usize) {
        if len == 0 {
            return;
        }
        self.choice = (self.choice as isize + delta).rem_euclid(len as isize) as usize;
    }

    /// Highlighted entry index — used by `App::activate_config`.
    pub fn choice(&self) -> usize {
        self.choice
    }

    /// Rows are derived from live App state, hence the free function.
    pub fn rows(app: &App) -> Vec<(String, &'static str)> {
        vec![
            (format!("model · {}", app.model), "open model selector"),
            (
                format!("scope · {}", app.agent_mode),
                "cycle with the config menu or Alt+Shift+←/→",
            ),
            (
                format!("effort · {}", app.effort),
                "cycle reasoning effort",
            ),
            (
                format!("providers · {}", app.provider_names()),
                "log in with a new provider",
            ),
            (
                "show configuration".to_string(),
                "print the runtime summary",
            ),
        ]
    }

    pub fn set_template(&self, tpl: &mut Template, app: &App) {
        tpl.set("config_open", self.open);
        let rows: Vec<TemplateContext> = if self.open {
            Self::rows(app)
                .into_iter()
                .enumerate()
                .map(|(index, (label, hint))| {
                    let mut row = TemplateContext::new();
                    row.set("label", label);
                    row.set("hint", hint);
                    row.set("selected", index == self.choice);
                    row
                })
                .collect()
        } else {
            Vec::new()
        };
        tpl.set("config_rows", TemplateValue::List(rows));
    }
}
