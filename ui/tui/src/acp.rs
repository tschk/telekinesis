//! ACP (Agent Client Protocol) host — JSON-RPC over stdin/stdout.
//!
//! Thin telekinesis adapter over the in-process rx4 `Agent`. Wire methods match
//! rotary's historical `AcpHost` (`initialize`, `session/new`, `session/prompt`,
//! `session/cancel`, `session/list`) so IDE clients keep the same handshake.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use rx4::agent::Agent;
use rx4::provider::Role;
use rx4::ModelRegistry;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::exec::{pick_configured_provider, resolve_exec_effort};
use crate::host::build_agent;
use crate::models::host_model_info;
use crate::providers::setup_providers;
use crate::tools::discover_mcp_tools;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AcpArgs {
    pub cwd: Option<PathBuf>,
    pub help: bool,
    pub no_yolo: bool,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub mcp: bool,
}

#[derive(Debug, Clone)]
pub struct AcpSession {
    pub id: String,
    pub cancelled: bool,
    pub turns: usize,
}

pub struct AcpHost {
    agent: Arc<tokio::sync::Mutex<Agent>>,
    sessions: Mutex<HashMap<String, AcpSession>>,
    protocol_version: u32,
}

impl AcpHost {
    pub fn new() -> Self {
        Self::with_agent(Agent::new())
    }

    pub fn with_agent(agent: Agent) -> Self {
        Self {
            agent: Arc::new(tokio::sync::Mutex::new(agent)),
            sessions: Mutex::new(HashMap::new()),
            protocol_version: 1,
        }
    }

    pub fn agent(&self) -> Arc<tokio::sync::Mutex<Agent>> {
        self.agent.clone()
    }

    fn handle_initialize(&self, id: Value) -> Value {
        ok_response(
            id,
            json!({
                "protocolVersion": self.protocol_version,
                "serverInfo": {
                    "name": "telekinesis",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "prompt": true,
                    "tools": true,
                    "sessions": true,
                }
            }),
        )
    }

    fn handle_session_new(&self, id: Value) -> Value {
        let sid = Uuid::new_v4().to_string();
        self.sessions.lock().insert(
            sid.clone(),
            AcpSession {
                id: sid.clone(),
                cancelled: false,
                turns: 0,
            },
        );
        ok_response(id, json!({ "sessionId": sid }))
    }

    fn handle_session_list(&self, id: Value) -> Value {
        let sessions: Vec<Value> = self
            .sessions
            .lock()
            .values()
            .map(|s| {
                json!({
                    "sessionId": s.id,
                    "cancelled": s.cancelled,
                    "turns": s.turns,
                })
            })
            .collect();
        ok_response(id, json!({ "sessions": sessions }))
    }

    fn handle_session_cancel(&self, id: Value, params: &Value) -> Value {
        let sid = params
            .get("sessionId")
            .and_then(|s| s.as_str())
            .unwrap_or("");
        let mut sessions = self.sessions.lock();
        if let Some(s) = sessions.get_mut(sid) {
            s.cancelled = true;
            ok_response(id, json!({ "cancelled": true }))
        } else {
            error_response(id, -32001, &format!("unknown session: {sid}"))
        }
    }

    async fn handle_session_prompt(&self, id: Value, params: &Value) -> Value {
        let sid = params
            .get("sessionId")
            .and_then(|s| s.as_str())
            .unwrap_or("");
        let prompt = params
            .get("prompt")
            .or_else(|| params.get("text"))
            .and_then(|s| s.as_str())
            .unwrap_or("");
        if prompt.is_empty() {
            return error_response(id, -32602, "prompt required");
        }
        {
            let mut sessions = self.sessions.lock();
            let Some(session) = sessions.get_mut(sid) else {
                return error_response(id, -32001, &format!("unknown session: {sid}"));
            };
            if session.cancelled {
                return error_response(id, -32002, "session cancelled");
            }
            session.turns += 1;
        }

        let mut agent = self.agent.lock().await;
        match agent.prompt(prompt).await {
            Ok(()) => {
                if self
                    .sessions
                    .lock()
                    .get(sid)
                    .is_some_and(|session| session.cancelled)
                {
                    return error_response(id, -32002, "session cancelled");
                }
                let msgs = agent.messages.read().clone();
                let content = msgs
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::Assistant)
                    .map(|m| m.content.clone())
                    .unwrap_or_default();
                ok_response(
                    id,
                    json!({
                        "sessionId": sid,
                        "content": content,
                        "messageCount": msgs.len(),
                    }),
                )
            }
            Err(e) => error_response(id, -32000, &e.to_string()),
        }
    }

    pub async fn handle_request(&self, request: &Value) -> Option<Value> {
        let id = request.get("id").cloned()?;
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);

        Some(match method {
            "initialize" => self.handle_initialize(id),
            "session/new" => self.handle_session_new(id),
            "session/list" => self.handle_session_list(id),
            "session/cancel" => self.handle_session_cancel(id, &params),
            "session/prompt" => self.handle_session_prompt(id, &params).await,
            "" => error_response(id, -32600, "method required"),
            other => error_response(id, -32601, &format!("method not found: {other}")),
        })
    }

    pub async fn handle_line(&self, line: &str) -> Option<String> {
        match serde_json::from_str::<Value>(line) {
            Ok(req) => self
                .handle_request(&req)
                .await
                .map(|response| response.to_string()),
            Err(e) => {
                Some(error_response(Value::Null, -32700, &format!("parse error: {e}")).to_string())
            }
        }
    }
}

impl Default for AcpHost {
    fn default() -> Self {
        Self::new()
    }
}

fn ok_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

pub fn parse_acp_args(args: &[String]) -> Result<AcpArgs, String> {
    let mut parsed = AcpArgs::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "--help" | "-h" => parsed.help = true,
            "--no-yolo" => parsed.no_yolo = true,
            "--mcp" => parsed.mcp = true,
            "--model" => {
                index += 1;
                let model = args
                    .get(index)
                    .ok_or_else(|| "--model requires a name".to_string())?;
                if model.is_empty() {
                    return Err("--model requires a name".to_string());
                }
                parsed.model = Some(model.clone());
            }
            "--provider" => {
                index += 1;
                let provider = args
                    .get(index)
                    .ok_or_else(|| "--provider requires a name".to_string())?;
                if provider.is_empty() {
                    return Err("--provider requires a name".to_string());
                }
                parsed.provider = Some(provider.clone());
            }
            "--cwd" => {
                index += 1;
                let dir = args
                    .get(index)
                    .ok_or_else(|| "--cwd requires a directory".to_string())?;
                parsed.cwd = Some(PathBuf::from(dir));
            }
            _ if arg.starts_with("--") => return Err(format!("Unknown option: {arg}")),
            _ => return Err(format!("Unexpected extra argument: {arg}")),
        }
        index += 1;
    }
    Ok(parsed)
}

fn acp_help() {
    eprintln!("tk acp — Agent Client Protocol JSON-RPC server on stdin/stdout");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  tk acp                 Serve JSON-RPC (one request per line)");
    eprintln!();
    eprintln!("OPTIONS:");
    eprintln!("  --cwd <dir>     Workspace to run against (default: current directory)");
    eprintln!("  --provider <id> Use a configured provider (or TK_PROVIDER / model prefix)");
    eprintln!("  --model <name>  Override that provider's default model");
    eprintln!("  --mcp           Discover and register MCP tools (skipped by default)");
    eprintln!("  --no-yolo       Deny Ask-class tools (default is AlwaysAllow)");
    eprintln!("  --help          Show this help");
    eprintln!();
    eprintln!("METHODS:");
    eprintln!("  initialize, session/new, session/list, session/prompt, session/cancel");
}

pub fn run_acp(parsed: AcpArgs) -> anyhow::Result<()> {
    if parsed.help {
        acp_help();
        return Ok(());
    }
    if let Some(dir) = &parsed.cwd {
        std::env::set_current_dir(dir)
            .map_err(|error| anyhow::anyhow!("cannot use --cwd {}: {error}", dir.display()))?;
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let host = build_stdio_host(&rt, &parsed);
    eprintln!("tk acp — JSON-RPC on stdin/stdout (initialize, session/*)");

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = rt.block_on(host.handle_line(&line)) {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn build_stdio_host(rt: &tokio::runtime::Runtime, parsed: &AcpArgs) -> AcpHost {
    let providers = setup_providers(rt);
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
    let effort = resolve_exec_effort(None);
    let picked = pick_configured_provider(
        providers,
        parsed.provider.as_deref(),
        parsed.model.as_deref(),
    );

    let (mut agent, _subagent_manager) = if let Some((configured, default_model)) = picked {
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
        eprintln!(
            "· {} / {} / {} in {}",
            configured.name,
            model,
            effort,
            workspace.display()
        );
        build_agent(
            Some(configured.client),
            &model,
            &effort,
            workspace,
            ModelRegistry::from_models([host_model_info(&configured_id, &model)]),
            &mcp,
        )
    } else {
        eprintln!("· no provider credentials; initialize still works, session/prompt will error");
        build_agent(
            None,
            "default",
            &effort,
            workspace,
            ModelRegistry::new(),
            &mcp,
        )
    };

    if parsed.no_yolo {
        agent.set_approver(Arc::new(rx4::permissions::AlwaysDeny));
    } else {
        agent.set_approver(Arc::new(rx4::permissions::AlwaysAllow));
    }

    AcpHost::with_agent(agent)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn result_of(host: &AcpHost, request: Value) -> Value {
        host.handle_request(&request)
            .await
            .expect("request with id should produce a response")
    }

    #[tokio::test]
    async fn initialize_and_session_new() {
        let host = AcpHost::new();
        let init = result_of(&host, json!({"jsonrpc":"2.0","id":1,"method":"initialize"})).await;
        let result = init.get("result").expect("initialize result");
        assert_eq!(result["serverInfo"]["name"], "telekinesis");
        assert_eq!(result["protocolVersion"], 1);
        assert_eq!(result["capabilities"]["prompt"], json!(true));
        let created = result_of(
            &host,
            json!({"jsonrpc":"2.0","id":2,"method":"session/new"}),
        )
        .await;
        let sid = created["result"]["sessionId"].as_str().unwrap();
        assert!(!sid.is_empty());
    }

    #[tokio::test]
    async fn prompt_without_provider_errors() {
        let host = AcpHost::new();
        let created = result_of(
            &host,
            json!({"jsonrpc":"2.0","id":1,"method":"session/new"}),
        )
        .await;
        let sid = created["result"]["sessionId"].as_str().unwrap().to_string();
        let resp = result_of(
            &host,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"session/prompt",
                "params":{"sessionId": sid, "prompt":"hi"}
            }),
        )
        .await;
        assert_eq!(resp["error"]["code"], -32000);
    }

    #[tokio::test]
    async fn cancel_unknown_session() {
        let host = AcpHost::new();
        let resp = result_of(
            &host,
            json!({
                "jsonrpc":"2.0","id":1,
                "method":"session/cancel",
                "params":{"sessionId":"nope"}
            }),
        )
        .await;
        assert_eq!(resp["error"]["code"], -32001);
    }

    #[tokio::test]
    async fn prompt_after_cancel_is_rejected() {
        let host = AcpHost::new();
        let created = result_of(
            &host,
            json!({"jsonrpc":"2.0","id":1,"method":"session/new"}),
        )
        .await;
        let sid = created["result"]["sessionId"].as_str().unwrap().to_string();
        let cancelled = result_of(
            &host,
            json!({
                "jsonrpc":"2.0","id":2,
                "method":"session/cancel",
                "params":{"sessionId": sid}
            }),
        )
        .await;
        assert_eq!(cancelled["result"]["cancelled"], json!(true));
        let resp = result_of(
            &host,
            json!({
                "jsonrpc":"2.0",
                "id":3,
                "method":"session/prompt",
                "params":{"sessionId": sid, "prompt":"hi"}
            }),
        )
        .await;
        assert_eq!(resp["error"]["code"], -32002);
        let listed = result_of(
            &host,
            json!({"jsonrpc":"2.0","id":4,"method":"session/list"}),
        )
        .await;
        assert_eq!(listed["result"]["sessions"][0]["cancelled"], json!(true));
    }

    #[tokio::test]
    async fn prompt_requires_text_and_unknown_method_errors() {
        let host = AcpHost::new();
        let created = result_of(
            &host,
            json!({"jsonrpc":"2.0","id":1,"method":"session/new"}),
        )
        .await;
        let sid = created["result"]["sessionId"].as_str().unwrap().to_string();
        let missing = result_of(
            &host,
            json!({
                "jsonrpc":"2.0","id":2,
                "method":"session/prompt",
                "params":{"sessionId": sid}
            }),
        )
        .await;
        assert_eq!(missing["error"]["code"], -32602);
        let unknown = result_of(&host, json!({"jsonrpc":"2.0","id":3,"method":"nope"})).await;
        assert_eq!(unknown["error"]["code"], -32601);
        assert!(host
            .handle_request(&json!({"jsonrpc":"2.0","method":"initialize"}))
            .await
            .is_none());
    }

    #[test]
    fn parse_acp_args_flags() {
        let parsed = parse_acp_args(&[
            "--cwd".into(),
            "/tmp".into(),
            "--no-yolo".into(),
            "--mcp".into(),
            "--provider".into(),
            "xai".into(),
            "--model".into(),
            "grok-4.5".into(),
        ])
        .unwrap();
        assert_eq!(parsed.cwd, Some(PathBuf::from("/tmp")));
        assert!(parsed.no_yolo);
        assert!(parsed.mcp);
        assert_eq!(parsed.provider.as_deref(), Some("xai"));
        assert_eq!(parsed.model.as_deref(), Some("grok-4.5"));
        assert!(parse_acp_args(&["--cwd".into()]).is_err());
        assert!(parse_acp_args(&["--nope".into()]).is_err());
        assert!(parse_acp_args(&["extra".into()]).is_err());
    }
}
