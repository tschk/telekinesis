use std::io::{stdin, IsTerminal, Read};
use std::path::PathBuf;
use std::sync::Arc;

use rx4::agent::Event as Rx4Event;
use rx4::provider::Role;
use rx4::ModelRegistry;

use crate::harness::{apply_prewalk_exec_env, sync_prewalk_model};
use crate::host::build_agent;
use crate::models::host_model_info;
use crate::providers::setup_providers;
use crate::tools::discover_mcp_tools;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExecArgs {
    pub prompt: Option<String>,
    pub json: bool,
    pub cwd: Option<PathBuf>,
    pub help: bool,
    pub no_yolo: bool,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub mcp: bool,
    pub prewalk: bool,
    pub smol_model: Option<String>,
    pub investigate_model: Option<String>,
}

pub fn parse_effort_level(value: &str) -> Result<String, String> {
    match value {
        "low" | "medium" | "high" | "xhigh" => Ok(value.to_string()),
        _ => Err(format!(
            "effort must be low, medium, high, or xhigh (got {value})"
        )),
    }
}

/// Flag wins; otherwise `TK_EFFORT` if it is a known level; else `low` for headless.
pub fn resolve_exec_effort(explicit: Option<&str>) -> String {
    if let Some(value) = explicit {
        return value.to_string();
    }
    std::env::var("TK_EFFORT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| parse_effort_level(&value).ok())
        .unwrap_or_else(|| "low".to_string())
}

fn exec_help() {
    eprintln!("tk exec — run one agent turn without a TUI");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  tk exec \"<prompt>\"      Run the prompt and print the final text to stdout");
    eprintln!("  tk exec -               Read the prompt from stdin");
    eprintln!();
    eprintln!("OPTIONS:");
    eprintln!("  --json          Emit {{\"ok\",\"text\",\"error\"}} on stdout instead of prose");
    eprintln!("  --cwd <dir>     Workspace to run against (default: current directory)");
    eprintln!("  --model <name>  Override the first configured provider's default model");
    eprintln!("  --effort <lvl>  Reasoning effort: low (default), medium, high, xhigh");
    eprintln!("  --thinking <lvl>  Alias for --effort (matches Codex/Pi flag names)");
    eprintln!("  --mcp           Discover and register MCP tools (skipped by default)");
    eprintln!("  --no-yolo       Deny Ask-class tools (default non-TTY/exec is AlwaysAllow)");
    eprintln!("  --prewalk       Enable rx4 prewalk (or set RX4_PREWALK=1)");
    eprintln!("  --smol-model <name>  Apply model after the first write (RX4_SMOL_MODEL)");
    eprintln!("  --investigate-model <name>  Plan model (RX4_INVESTIGATE_MODEL)");
    eprintln!("  --help          Show this help");
    eprintln!();
    eprintln!("Only the final text goes to stdout; status and errors go to stderr.");
}

fn exec_failure(json: bool, message: &str) -> ! {
    if json {
        println!(
            "{}",
            serde_json::json!({ "ok": false, "text": "", "error": message })
        );
    }
    eprintln!("error: {message}");
    std::process::exit(1);
}

pub fn run_exec(parsed: ExecArgs) -> anyhow::Result<()> {
    if parsed.help {
        exec_help();
        return Ok(());
    }
    apply_prewalk_exec_env(
        parsed.prewalk,
        parsed.smol_model.as_deref(),
        parsed.investigate_model.as_deref(),
    );
    let json = parsed.json;

    if let Some(dir) = &parsed.cwd {
        if let Err(error) = std::env::set_current_dir(dir) {
            exec_failure(
                json,
                &format!("cannot use --cwd {}: {error}", dir.display()),
            );
        }
    }

    let prompt = match parsed.prompt {
        Some(prompt) => prompt,
        None => {
            if stdin().is_terminal() {
                exec_failure(json, "no prompt given; pass one as an argument or on stdin");
            }
            let mut buffer = String::new();
            if let Err(error) = stdin().read_to_string(&mut buffer) {
                exec_failure(json, &format!("cannot read prompt from stdin: {error}"));
            }
            buffer
        }
    };
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        exec_failure(json, "empty prompt");
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let providers = setup_providers(&rt);
    let Some((configured, default_model)) = providers.into_iter().next() else {
        exec_failure(json, "no provider credentials; run `tk login <provider>`");
    };
    let mcp = if parsed.mcp {
        let (mcp, errors) = rt.block_on(discover_mcp_tools());
        for error in errors {
            eprintln!("· {error}");
        }
        mcp
    } else {
        Vec::new()
    };

    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let configured_id = configured.id.clone();
    let model = parsed.model.unwrap_or(default_model);
    let effort = resolve_exec_effort(parsed.effort.as_deref());
    let (mut agent, _subagent_manager) = build_agent(
        Some(configured.client),
        &model,
        &effort,
        workspace.clone(),
        ModelRegistry::from_models([host_model_info(&configured_id, &model)]),
        &mcp,
    );
    if parsed.no_yolo {
        agent.set_approver(Arc::new(rx4::permissions::AlwaysDeny));
    } else {
        agent.set_approver(Arc::new(rx4::permissions::AlwaysAllow));
    }

    agent.subscribe(move |event: &Rx4Event| match event {
        Rx4Event::ToolExecutionStart(call) => eprintln!("· {}", call.name),
        Rx4Event::Error(message) => eprintln!("· error: {message}"),
        _ => {}
    });

    eprintln!(
        "· {} / {} / {} in {}",
        configured.name,
        model,
        effort,
        workspace.display()
    );

    sync_prewalk_model(&mut agent);
    let result = rt.block_on(agent.prompt(&prompt));
    if let Err(error) = result {
        exec_failure(json, &error.to_string());
    }

    let text = agent
        .messages
        .read()
        .iter()
        .rev()
        .find(|message| {
            matches!(message.role, Role::Assistant) && !message.content.trim().is_empty()
        })
        .map(|message| message.content.trim().to_string())
        .unwrap_or_default();

    if json {
        println!(
            "{}",
            serde_json::json!({ "ok": true, "text": text, "error": serde_json::Value::Null })
        );
    } else {
        println!("{text}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_yolo_is_opt_in_on_exec_args() {
        let yolo = ExecArgs::default();
        assert!(!yolo.no_yolo);
        assert_eq!(yolo.model, None);
        assert_eq!(yolo.effort, None);
        assert!(!yolo.mcp);
        let denied = ExecArgs {
            no_yolo: true,
            ..ExecArgs::default()
        };
        assert!(denied.no_yolo);
    }

    #[test]
    fn exec_effort_defaults_to_low_and_accepts_known_levels() {
        for level in ["low", "medium", "high", "xhigh"] {
            assert_eq!(parse_effort_level(level).unwrap(), level);
        }
        assert!(parse_effort_level("turbo").is_err());
        assert_eq!(resolve_exec_effort(Some("high")), "high");
        assert_eq!(resolve_exec_effort(Some("low")), "low");
    }
}
