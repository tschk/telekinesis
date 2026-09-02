//! Host-side Event adapter for current and forthcoming rotary shapes.
//!
//! crates.io rx4 0.7.1 does not emit RetryReason / ProcessId / WriteStdin /
//! RequestPermissions / PatchHunk. Classify serialized Event JSON so a later
//! pin compiles; ignore unknown variants. Forward only — no harness policy.

use rx4::agent::Event as Rx4Event;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSurface {
    RetryReason { reason: String },
    ProcessId { process_id: String },
    WriteStdin { process_id: String, data: String },
    RequestPermissions {
        tool_name: String,
        arguments: String,
        reason: String,
    },
    PatchHunk { path: String, hunk: String },
}

pub trait EventExt {
    fn host_surface(&self) -> Option<HostSurface>;
}

impl EventExt for Rx4Event {
    fn host_surface(&self) -> Option<HostSurface> {
        surface_from_event(self)
    }
}

pub fn surface_from_event(event: &Rx4Event) -> Option<HostSurface> {
    surface_from_json(&serde_json::to_value(event).ok()?)
}

pub fn surface_from_json(value: &Value) -> Option<HostSurface> {
    let ty = event_type(value)?;
    match normalize_type(&ty).as_str() {
        "retryreason" => Some(HostSurface::RetryReason {
            reason: first_str(value, &["reason", "retry_reason", "cause"]).unwrap_or_default(),
        }),
        "processid" => Some(HostSurface::ProcessId {
            process_id: first_str(value, &["process_id", "pid", "id"]).unwrap_or_default(),
        }),
        "writestdin" => Some(HostSurface::WriteStdin {
            process_id: first_str(value, &["process_id", "pid", "id"]).unwrap_or_default(),
            data: first_str(value, &["data", "stdin", "content", "text"]).unwrap_or_default(),
        }),
        "requestpermissions" => Some(HostSurface::RequestPermissions {
            tool_name: first_str(value, &["tool_name", "name", "tool"]).unwrap_or_default(),
            arguments: first_str(value, &["arguments", "args"]).unwrap_or_default(),
            reason: first_str(value, &["reason"]).unwrap_or_default(),
        }),
        "patchhunk" | "streamingpatch" | "streamingpatchhunk" | "filepatch" => {
            Some(HostSurface::PatchHunk {
                path: first_str(value, &["path", "file", "id"]).unwrap_or_default(),
                hunk: first_str(value, &["hunk", "content", "diff", "patch", "delta"])
                    .unwrap_or_default(),
            })
        }
        _ => None,
    }
}

pub fn cli_line(surface: &HostSurface) -> String {
    match surface {
        HostSurface::RetryReason { reason } => {
            if reason.is_empty() {
                "retry".to_string()
            } else {
                format!("retry {reason}")
            }
        }
        HostSurface::ProcessId { process_id } => {
            if process_id.is_empty() {
                "pty".to_string()
            } else {
                format!("pty {process_id}")
            }
        }
        HostSurface::WriteStdin { data, .. } => {
            let preview = truncate_cli(data);
            if preview.is_empty() {
                "stdin".to_string()
            } else {
                format!("stdin {preview}")
            }
        }
        HostSurface::RequestPermissions { tool_name, .. } => {
            if tool_name.is_empty() {
                "approval".to_string()
            } else {
                tool_name.clone()
            }
        }
        HostSurface::PatchHunk { path, .. } => {
            if path.is_empty() {
                "patch".to_string()
            } else {
                format!("patch {path}")
            }
        }
    }
}

fn event_type(value: &Value) -> Option<String> {
    value
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn normalize_type(ty: &str) -> String {
    ty.chars()
        .filter(|ch| *ch != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

fn first_str(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_str()
                .map(str::to_string)
                .or_else(|| item.as_u64().map(|n| n.to_string()))
                .or_else(|| item.as_i64().map(|n| n.to_string()))
        })
    })
}

fn truncate_cli(text: &str) -> String {
    let line = text.lines().find(|row| !row.trim().is_empty()).unwrap_or("");
    let mut out = String::new();
    for ch in line.chars() {
        if out.chars().count() >= 80 {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rx4::agent::{ToolCall, ToolResult};

    #[test]
    fn known_rx4_events_are_not_host_surfaces() {
        let events = [
            Rx4Event::AgentStart,
            Rx4Event::ToolCall(ToolCall {
                id: "1".into(),
                name: "bash".into(),
                arguments: r#"{"command":"pwd"}"#.into(),
            }),
            Rx4Event::ApprovalRequired(rx4::permissions::ApprovalRequest {
                call_id: "1".into(),
                tool_name: "bash".into(),
                arguments: r#"{"command":"pwd"}"#.into(),
                reason: "ask".into(),
                policy_mode: "ask".into(),
                is_process_tool: true,
                is_write_tool: false,
            }),
            Rx4Event::ToolExecutionStart(ToolCall {
                id: "1".into(),
                name: "bash".into(),
                arguments: "{}".into(),
            }),
            Rx4Event::ToolExecutionEnd(ToolResult {
                id: "1".into(),
                content: "ok".into(),
                is_error: false,
                error_kind: None,
            }),
        ];
        for event in events {
            assert_eq!(event.host_surface(), None, "{event:?}");
        }
    }

    #[test]
    fn classifies_forthcoming_rotary_shapes() {
        assert_eq!(
            surface_from_json(&serde_json::json!({
                "type": "retry_reason",
                "reason": "sandbox escalate"
            })),
            Some(HostSurface::RetryReason {
                reason: "sandbox escalate".into()
            })
        );
        assert_eq!(
            surface_from_json(&serde_json::json!({
                "type": "ProcessId",
                "process_id": "pty-9"
            })),
            Some(HostSurface::ProcessId {
                process_id: "pty-9".into()
            })
        );
        assert_eq!(
            surface_from_json(&serde_json::json!({
                "type": "write_stdin",
                "process_id": "pty-9",
                "data": "ls\n"
            })),
            Some(HostSurface::WriteStdin {
                process_id: "pty-9".into(),
                data: "ls\n".into()
            })
        );
        assert_eq!(
            surface_from_json(&serde_json::json!({
                "type": "RequestPermissions",
                "tool_name": "bash",
                "arguments": "{\"command\":\"pwd\"}",
                "reason": "ask"
            })),
            Some(HostSurface::RequestPermissions {
                tool_name: "bash".into(),
                arguments: r#"{"command":"pwd"}"#.into(),
                reason: "ask".into()
            })
        );
        assert_eq!(
            surface_from_json(&serde_json::json!({
                "type": "patch_hunk",
                "path": "src/lib.rs",
                "hunk": "@@ -1,1 +1,2 @@"
            })),
            Some(HostSurface::PatchHunk {
                path: "src/lib.rs".into(),
                hunk: "@@ -1,1 +1,2 @@".into()
            })
        );
        assert_eq!(
            surface_from_json(&serde_json::json!({
                "type": "StreamingPatch",
                "path": "a.rs",
                "content": "+fn main() {}"
            })),
            Some(HostSurface::PatchHunk {
                path: "a.rs".into(),
                hunk: "+fn main() {}".into()
            })
        );
        assert_eq!(
            surface_from_json(&serde_json::json!({ "type": "ToolExecutionStart", "name": "bash" })),
            None
        );
    }

    #[test]
    fn cli_lines_match_tool_execution_style() {
        assert_eq!(
            cli_line(&HostSurface::RetryReason {
                reason: "sandbox escalate".into()
            }),
            "retry sandbox escalate"
        );
        assert_eq!(
            cli_line(&HostSurface::ProcessId {
                process_id: "pty-9".into()
            }),
            "pty pty-9"
        );
        assert_eq!(
            cli_line(&HostSurface::WriteStdin {
                process_id: "pty-9".into(),
                data: "ls\n".into()
            }),
            "stdin ls"
        );
        assert_eq!(
            cli_line(&HostSurface::RequestPermissions {
                tool_name: "bash".into(),
                arguments: "{}".into(),
                reason: "ask".into()
            }),
            "bash"
        );
        assert_eq!(
            cli_line(&HostSurface::PatchHunk {
                path: "src/lib.rs".into(),
                hunk: "@@".into()
            }),
            "patch src/lib.rs"
        );
    }
}
