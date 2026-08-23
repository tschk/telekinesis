use crepuscularity_core::TemplateContext;
use crepuscularity_native::{render_template_to_ir, to_json, ViewIr};

pub const SHELL: &str = include_str!("../shell.crepus");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellVars {
    pub pair_url: String,
    pub input: String,
    pub model: String,
    pub effort: String,
    pub composer_action: String,
    pub messages: Vec<(bool, String)>,
}

impl Default for ShellVars {
    fn default() -> Self {
        Self {
            pair_url: "tkpair://127.0.0.1:8787/ws?token=pending".into(),
            input: "ask the computer...".into(),
            model: "not connected".into(),
            effort: "high".into(),
            composer_action: "pair".into(),
            messages: vec![(
                false,
                "Scan the desktop QR, then this timeline talks to companion.".into(),
            )],
        }
    }
}

pub fn render_shell(vars: &ShellVars) -> Result<ViewIr, String> {
    let mut ctx = TemplateContext::new();
    ctx.set("pair_url", vars.pair_url.as_str());
    ctx.set("input", vars.input.as_str());
    ctx.set("model", vars.model.as_str());
    ctx.set("effort", vars.effort.as_str());
    ctx.set("composer_action", vars.composer_action.as_str());
    let messages: Vec<TemplateContext> = vars
        .messages
        .iter()
        .map(|(is_user, content)| {
            let mut row = TemplateContext::new();
            row.set("is_user", *is_user);
            row.set("content", content.as_str());
            row
        })
        .collect();
    ctx.set("messages", messages);
    render_template_to_ir(SHELL, &ctx).map_err(|error| error.to_string())
}

pub fn render_shell_json(vars: &ShellVars) -> Result<String, String> {
    to_json(&render_shell(vars)?).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_ir_has_pair_and_timeline() {
        let json = render_shell_json(&ShellVars::default()).expect("ir");
        assert!(json.contains("tkpair://127.0.0.1:8787/ws?token=pending"));
        assert!(json.contains("ask the computer..."));
        assert!(json.contains("high"));
        assert!(json.contains("pair"));
        let ir = render_shell(&ShellVars::default()).expect("nodes");
        assert!(!ir.root.is_empty());
    }
}
