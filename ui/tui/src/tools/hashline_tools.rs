use std::sync::Arc;

use parking_lot::Mutex;
use rx4::agent::{ToolContext, ToolDefinition, ToolEffect, ToolFuture, ToolResult};
use rx4::ToolRegistry;

use crate::roles::ModelRouting;

use super::hashline::{
    apply_patch, format_read, parse_patch, resolve_workspace_path, sloppy_for_model, DiskFs, Op,
    SnapshotStore, TextFs,
};

pub struct HashlineRuntime {
    store: Mutex<SnapshotStore>,
    routing: Arc<ModelRouting>,
}

impl HashlineRuntime {
    pub fn new(routing: Arc<ModelRouting>) -> Arc<Self> {
        Arc::new(Self {
            store: Mutex::new(SnapshotStore::default()),
            routing,
        })
    }
}

pub fn register_hashline_tools(tools: &mut ToolRegistry, routing: Arc<ModelRouting>) {
    let runtime = HashlineRuntime::new(routing);
    let read_runtime = runtime.clone();
    let write_runtime = runtime.clone();
    let edit_runtime = runtime.clone();
    tools.register(
        ToolDefinition::new_boxed(
            "read",
            "Read a file as hashline: [path#TAG] plus N:line rows. Edits must copy the TAG and may only hunk displayed lines.",
            r#"{"type":"object","properties":{"path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}},"required":["path"]}"#,
            Box::new(move |ctx, args| exec_read(read_runtime.clone(), ctx, args)),
        )
        .with_effect(ToolEffect::Read),
    );
    tools.register(
        ToolDefinition::new_boxed(
            "write",
            "Create or overwrite a file. Returns a fresh [path#TAG] snapshot for later hashline edits.",
            r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
            Box::new(move |ctx, args| exec_write(write_runtime.clone(), ctx, args)),
        )
        .with_effect(ToolEffect::Write),
    );
    tools.register(
        ToolDefinition::new_boxed(
            "edit",
            "Apply a hashline patch. Default language is PUT/CUT/MV/REM on [path#TAG] sections, not apply_patch. Stale tags, unseen lines, and no-ops fail closed.",
            r#"{"type":"object","properties":{"input":{"type":"string"}},"required":["input"]}"#,
            Box::new(move |ctx, args| exec_edit(edit_runtime.clone(), ctx, args)),
        )
        .with_effect(ToolEffect::Write),
    );
}

fn exec_read(runtime: Arc<HashlineRuntime>, ctx: Arc<ToolContext>, args: String) -> ToolFuture {
    Box::pin(async move {
        let value: serde_json::Value = match serde_json::from_str(&args) {
            Ok(value) => value,
            Err(error) => return ToolResult::err("read", format!("invalid json: {error}")),
        };
        let Some(path) = value.get("path").and_then(serde_json::Value::as_str) else {
            return ToolResult::err("read", "path required");
        };
        if let Err(error) = check_path(&ctx, path, false) {
            return ToolResult::err("read", error);
        }
        let offset = value
            .get("offset")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        let limit = value
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(2000) as usize;
        let fs = DiskFs::new(&ctx.workspace_root);
        let text = match fs.read(path) {
            Ok(text) => text,
            Err(error) => return ToolResult::err("read", error),
        };
        let (output, snapshot) = format_read(path, &text, offset, limit);
        runtime
            .store
            .lock()
            .record(path, &snapshot.text, snapshot.seen);
        ToolResult::ok("read", output)
    })
}

fn exec_write(runtime: Arc<HashlineRuntime>, ctx: Arc<ToolContext>, args: String) -> ToolFuture {
    Box::pin(async move {
        let value: serde_json::Value = match serde_json::from_str(&args) {
            Ok(value) => value,
            Err(error) => return ToolResult::err("write", format!("invalid json: {error}")),
        };
        let Some(path) = value.get("path").and_then(serde_json::Value::as_str) else {
            return ToolResult::err("write", "path required");
        };
        let Some(content) = value.get("content").and_then(serde_json::Value::as_str) else {
            return ToolResult::err("write", "content required");
        };
        if let Err(error) = check_path(&ctx, path, true) {
            return ToolResult::err("write", error);
        }
        let mut fs = DiskFs::new(&ctx.workspace_root);
        if let Err(error) = fs.write(path, content) {
            return ToolResult::err("write", error);
        }
        runtime.routing.note_mutating_tool();
        let (output, snapshot) = format_read(path, content, 0, usize::MAX);
        runtime
            .store
            .lock()
            .record(path, &snapshot.text, snapshot.seen);
        ToolResult::ok("write", output)
    })
}

fn exec_edit(runtime: Arc<HashlineRuntime>, ctx: Arc<ToolContext>, args: String) -> ToolFuture {
    Box::pin(async move {
        let value: serde_json::Value = match serde_json::from_str(&args) {
            Ok(value) => value,
            Err(error) => return ToolResult::err("edit", format!("invalid json: {error}")),
        };
        let Some(input) = value.get("input").and_then(serde_json::Value::as_str) else {
            return ToolResult::err(
                "edit",
                "input required; emit [path#TAG] plus PUT/CUT/MV/REM, not apply_patch",
            );
        };
        let sloppy = sloppy_for_model(&runtime.routing.apply_model(&runtime.routing.roles.default));
        let sections = match parse_patch(input, sloppy) {
            Ok(sections) => sections,
            Err(error) => return ToolResult::err("edit", error.message),
        };
        for section in &sections {
            if let Err(error) = check_path(&ctx, &section.path, true) {
                return ToolResult::err("edit", error);
            }
            for op in &section.ops {
                if let Op::Mv { dest } = op {
                    if let Err(error) = check_path(&ctx, dest, true) {
                        return ToolResult::err("edit", error);
                    }
                }
            }
        }
        let mut fs = DiskFs::new(&ctx.workspace_root);
        let mut store = runtime.store.lock();
        match apply_patch(&mut store, &mut fs, &sections) {
            Ok(result) => {
                runtime.routing.note_mutating_tool();
                ToolResult::ok("edit", result.outputs.join("\n\n"))
            }
            Err(error) => ToolResult::err("edit", error.message),
        }
    })
}

fn check_path(ctx: &ToolContext, path: &str, creating: bool) -> Result<(), String> {
    let full = resolve_workspace_path(&ctx.workspace_root, path)?;
    if let Some(sandbox) = ctx.sandbox.as_ref() {
        sandbox
            .validate_path(&full, creating)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
