use std::io::{stdin, IsTerminal, Read};
use std::path::PathBuf;
use std::sync::Arc;

use rx4::agent::Event as Rx4Event;
use rx4::provider::Role;
use rx4::ModelRegistry;

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
    pub provider: Option<String>,
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
    eprintln!("  --provider <id> Use a configured provider (or TK_PROVIDER / model prefix)");
    eprintln!("  --model <name>  Override that provider's default model");
    eprintln!("  --no-yolo       Deny Ask-class tools (default non-TTY/exec is AlwaysAllow)");
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

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let discover = rt.spawn(discover_mcp_tools());
    let providers = setup_providers(&rt);
    let Some((configured, default_model)) = pick_configured_provider(
        providers,
        parsed.provider.as_deref(),
        parsed.model.as_deref(),
    ) else {
        exec_failure(
            json,
            &missing_provider_message(parsed.provider.as_deref(), parsed.model.as_deref()),
        );
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
    let model = parsed
        .model
        .as_deref()
        .map(
            |model| match crate::provider_catalog::by_id(&configured.id) {
                Some(spec) => crate::provider_catalog::normalize_model(spec, model),
                None => model.to_string(),
            },
        )
        .unwrap_or(default_model);
    let (mut agent, _subagent_manager) = build_agent(
        Some(configured.client),
        &model,
        "high",
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

fn requested_provider(explicit: Option<&str>) -> Option<String> {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("TK_PROVIDER")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn provider_matches(id: &str, name: &str, query: &str) -> bool {
    crate::provider_catalog::find(query).is_some_and(|spec| spec.id == id)
        || id.eq_ignore_ascii_case(query)
        || name.eq_ignore_ascii_case(query)
}

pub(crate) fn pick_configured_provider(
    providers: Vec<(crate::app::ConfiguredProvider, String)>,
    explicit_provider: Option<&str>,
    explicit_model: Option<&str>,
) -> Option<(crate::app::ConfiguredProvider, String)> {
    let requested = requested_provider(explicit_provider).or_else(|| {
        explicit_model
            .and_then(crate::provider_catalog::infer_from_model)
            .map(|spec| spec.id.to_string())
    });
    if let Some(query) = requested {
        return providers
            .into_iter()
            .find(|(provider, _)| provider_matches(&provider.id, &provider.name, &query));
    }
    providers.into_iter().next()
}

fn missing_provider_message(
    explicit_provider: Option<&str>,
    explicit_model: Option<&str>,
) -> String {
    match requested_provider(explicit_provider).or_else(|| {
        explicit_model
            .and_then(crate::provider_catalog::infer_from_model)
            .map(|spec| spec.id.to_string())
    }) {
        Some(id) => {
            let hint = crate::provider_catalog::find(&id)
                .map(|spec| {
                    let env = spec.env_vars.join(" or ");
                    if spec.id == "clinepass" {
                        format!("set {env} or reuse OpenCode auth.json")
                    } else {
                        format!("set {env}")
                    }
                })
                .unwrap_or_else(|| {
                    "run `tk login <provider>` or set that provider's API key".to_string()
                });
            format!("provider {id} is not configured; {hint}")
        }
        None => "no provider credentials; run `tk login <provider>`".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_provider_message_matches_requested_provider() {
        let groq = missing_provider_message(Some("groq"), None);
        assert!(groq.contains("groq"));
        assert!(groq.contains("GROQ_API_KEY"));
        assert!(!groq.contains("CLINE_API_KEY"));
        let cline = missing_provider_message(Some("clinepass"), None);
        assert!(cline.contains("CLINE_API_KEY"));
        assert!(cline.contains("OpenCode"));
        let unknown = missing_provider_message(Some("not-a-provider"), None);
        assert!(unknown.contains("not-a-provider"));
        assert!(!unknown.contains("CLINE_API_KEY"));
        let none = missing_provider_message(None, None);
        assert!(none.contains("tk login"));
    }

    #[test]
    fn no_yolo_is_opt_in_on_exec_args() {
        let yolo = ExecArgs::default();
        assert!(!yolo.no_yolo);
        assert_eq!(yolo.model, None);
        assert_eq!(yolo.provider, None);
        let denied = ExecArgs {
            no_yolo: true,
            ..ExecArgs::default()
        };
        assert!(denied.no_yolo);
    }
}
