//! Host-side Event adapter. Match rotary Event variants directly; ignore unknown.

use rx4::agent::Event as Rx4Event;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSurface {
    RetryReason { retry_reason: String, layer: String },
    ProcessStdin { process_id: String, bytes: usize },
    RequestPermissions { tool: String, paths: Vec<String> },
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
    match event {
        Rx4Event::RetryReason {
            retry_reason,
            layer,
        } => Some(HostSurface::RetryReason {
            retry_reason: retry_reason.clone(),
            layer: layer.clone(),
        }),
        Rx4Event::ProcessStdin { process_id, bytes } => Some(HostSurface::ProcessStdin {
            process_id: process_id.clone(),
            bytes: *bytes,
        }),
        Rx4Event::RequestPermissions { tool, paths } => Some(HostSurface::RequestPermissions {
            tool: tool.clone(),
            paths: paths.clone(),
        }),
        Rx4Event::PatchHunk { path, hunk } => Some(HostSurface::PatchHunk {
            path: path.clone(),
            hunk: hunk.clone(),
        }),
        other => surface_from_json(&serde_json::to_value(other).ok()?),
    }
}

pub fn surface_from_json(value: &Value) -> Option<HostSurface> {
    let ty = event_type(value)?;
    match normalize_type(&ty).as_str() {
        "retryreason" => Some(HostSurface::RetryReason {
            retry_reason: first_str(value, &["retry_reason"]).unwrap_or_default(),
            layer: first_str(value, &["layer"]).unwrap_or_default(),
        }),
        "processstdin" => Some(HostSurface::ProcessStdin {
            process_id: first_str(value, &["process_id"]).unwrap_or_default(),
            bytes: first_usize(value, &["bytes"]).unwrap_or(0),
        }),
        "requestpermissions" => Some(HostSurface::RequestPermissions {
            tool: first_str(value, &["tool"]).unwrap_or_default(),
            paths: string_list(value, "paths"),
        }),
        "patchhunk" => Some(HostSurface::PatchHunk {
            path: first_str(value, &["path"]).unwrap_or_default(),
            hunk: first_str(value, &["hunk"]).unwrap_or_default(),
        }),
        _ => None,
    }
}

pub fn cli_line(surface: &HostSurface) -> String {
    match surface {
        HostSurface::RetryReason { retry_reason, .. } => {
            if retry_reason.is_empty() {
                "retry".to_string()
            } else {
                format!("retry {retry_reason}")
            }
        }
        HostSurface::ProcessStdin { process_id, .. } => {
            if process_id.is_empty() {
                "pty".to_string()
            } else {
                format!("pty {process_id}")
            }
        }
        HostSurface::RequestPermissions { tool, .. } => {
            if tool.is_empty() {
                "approval".to_string()
            } else {
                tool.clone()
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

fn first_usize(value: &Value, keys: &[&str]) -> Option<usize> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_u64()
                .map(|n| n as usize)
                .or_else(|| item.as_i64().and_then(|n| usize::try_from(n).ok()))
                .or_else(|| item.as_str().and_then(|s| s.parse().ok()))
        })
    })
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    match value.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(text)) if !text.is_empty() => vec![text.clone()],
        _ => Vec::new(),
    }
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
    fn matches_rotary_event_variants() {
        assert_eq!(
            Rx4Event::RetryReason {
                retry_reason: "sandbox deny".into(),
                layer: "NestedFs".into(),
            }
            .host_surface(),
            Some(HostSurface::RetryReason {
                retry_reason: "sandbox deny".into(),
                layer: "NestedFs".into(),
            })
        );
        assert_eq!(
            Rx4Event::ProcessStdin {
                process_id: "pty-9".into(),
                bytes: 3,
            }
            .host_surface(),
            Some(HostSurface::ProcessStdin {
                process_id: "pty-9".into(),
                bytes: 3,
            })
        );
        assert_eq!(
            Rx4Event::RequestPermissions {
                tool: "write".into(),
                paths: vec!["src/lib.rs".into()],
            }
            .host_surface(),
            Some(HostSurface::RequestPermissions {
                tool: "write".into(),
                paths: vec!["src/lib.rs".into()],
            })
        );
        assert_eq!(
            Rx4Event::PatchHunk {
                path: "src/lib.rs".into(),
                hunk: "@@ -1,1 +1,2 @@".into(),
            }
            .host_surface(),
            Some(HostSurface::PatchHunk {
                path: "src/lib.rs".into(),
                hunk: "@@ -1,1 +1,2 @@".into(),
            })
        );
    }

    #[test]
    fn classifies_serialized_rotary_shapes() {
        assert_eq!(
            surface_from_json(&serde_json::json!({
                "type": "RetryReason",
                "retry_reason": "sandbox deny",
                "layer": "NestedFs"
            })),
            Some(HostSurface::RetryReason {
                retry_reason: "sandbox deny".into(),
                layer: "NestedFs".into()
            })
        );
        assert_eq!(
            surface_from_json(&serde_json::json!({
                "type": "ProcessStdin",
                "process_id": "pty-9",
                "bytes": 3
            })),
            Some(HostSurface::ProcessStdin {
                process_id: "pty-9".into(),
                bytes: 3
            })
        );
        assert_eq!(
            surface_from_json(&serde_json::json!({
                "type": "RequestPermissions",
                "tool": "write",
                "paths": ["src/lib.rs"]
            })),
            Some(HostSurface::RequestPermissions {
                tool: "write".into(),
                paths: vec!["src/lib.rs".into()]
            })
        );
        assert_eq!(
            surface_from_json(&serde_json::json!({
                "type": "PatchHunk",
                "path": "src/lib.rs",
                "hunk": "@@ -1,1 +1,2 @@"
            })),
            Some(HostSurface::PatchHunk {
                path: "src/lib.rs".into(),
                hunk: "@@ -1,1 +1,2 @@".into()
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
                retry_reason: "sandbox deny".into(),
                layer: "NestedFs".into()
            }),
            "retry sandbox deny"
        );
        assert_eq!(
            cli_line(&HostSurface::ProcessStdin {
                process_id: "pty-9".into(),
                bytes: 3
            }),
            "pty pty-9"
        );
        assert_eq!(
            cli_line(&HostSurface::RequestPermissions {
                tool: "write".into(),
                paths: vec!["src/lib.rs".into()]
            }),
            "write"
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
