use std::io::{stdin, IsTerminal, Read};
use std::path::PathBuf;
use std::sync::Arc;

use rx4::agent::Event as Rx4Event;
use rx4::provider::Role;
use rx4::ModelRegistry;

use crate::host::build_agent;
use crate::models::host_model_info;
use crate::providers::setup_providers;
use crate::roles::ModelRouting;
use crate::tools::discover_mcp_tools;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExecArgs {
    pub prompt: Option<String>,
    pub json: bool,
    pub cwd: Option<PathBuf>,
    pub help: bool,
    pub no_yolo: bool,
    pub model: Option<String>,
    pub smol: Option<String>,
    pub slow: Option<String>,
    pub plan_model: Option<String>,
    pub prewalk: bool,
    pub plan_yolo: bool,
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
    eprintln!("  --model <name>       Override the first configured provider's default model");
    eprintln!("  --smol <name>        Apply/implement model (or TK_SMOL_MODEL); Sol-light is a typical choice");
    eprintln!("  --slow <name>        Slow/reasoning role (or TK_SLOW_MODEL)");
    eprintln!("  --plan-model <name>  Plan role (or TK_PLAN_MODEL)");
    eprintln!("  --prewalk            Investigate on --model; first write/edit switches one-way to --smol");
    eprintln!("  --plan-yolo          Plan on --plan-model/--model, then implement on --smol");
    eprintln!("  --no-yolo            Deny Ask-class tools (default non-TTY/exec is AlwaysAllow)");
    eprintln!("  --help               Show this help");
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
    let json = parsed.json;

    if let Some(dir) = &parsed.cwd {
        if let Err(error) = std::env::set_current_dir(dir) {
            exec_failure(
                json,
                &format!("cannot use --cwd {}: {error}", dir.display()),
            );
        }
    }

    let routing = ModelRouting::from_exec(&parsed);
    let model_override = parsed.model.clone();
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

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let discover = rt.spawn(discover_mcp_tools());
    let providers = setup_providers(&rt);
    let Some((configured, default_model)) = providers.into_iter().next() else {
        exec_failure(json, "no provider credentials; run `tk login <provider>`");
    };
    let (mcp, errors) = match rt.block_on(discover) {
        Ok(result) => result,
        Err(error) => exec_failure(json, &format!("MCP discover failed: {error}")),
    };
    for error in errors {
        eprintln!("· {error}");
    }

    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let configured_id = configured.id.clone();
    let model = routing.start_model(model_override.as_deref().unwrap_or(&default_model));
    let model = model.to_string();
    let (mut agent, _subagent_manager) = build_agent(
        Some(configured.client),
        &model,
        "high",
        workspace.clone(),
        ModelRegistry::from_models([host_model_info(&configured_id, &model)]),
        &mcp,
        routing,
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
        "· {} / {} in {}",
        configured.name,
        model,
        workspace.display()
    );

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
        let denied = ExecArgs {
            no_yolo: true,
            ..ExecArgs::default()
        };
        assert!(denied.no_yolo);
    }
}
