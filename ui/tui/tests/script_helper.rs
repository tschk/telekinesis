//! Exercise the real packaged helper, never an in-process script substitute.
#![cfg(unix)]
use rx4::agent::{ToolContext, ToolRegistry};
use std::{path::PathBuf, sync::Arc};
fn helper() -> PathBuf {
    std::env::var_os("TK_PACKAGED_HELPER")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_rx4-script-worker")))
}
async fn run(mut context: ToolContext, path: PathBuf) -> rx4::ToolResult {
    context.script_worker = Some(path);
    let registry = ToolRegistry::new();
    rx4::tools::register_builtin_tools(&registry);
    registry
        .execute(
            "script",
            &Arc::new(context),
            r#"{"code":"return 42","timeout_ms":5000}"#,
        )
        .await
        .unwrap()
}
#[tokio::test]
async fn matching_packaged_helper_executes_and_missing_helper_fails_closed() {
    let root = tempfile::tempdir().unwrap();
    let valid = run(ToolContext::new(root.path()), helper()).await;
    assert!(!valid.is_error, "{}", valid.content);
    assert!(valid.content.contains("42"));
    let missing = run(
        ToolContext::new(root.path()),
        root.path().join("absent-helper"),
    )
    .await;
    assert!(missing.is_error);
    assert!(missing.content.contains("missing"));
}
#[cfg(target_os = "macos")]
#[tokio::test]
async fn matching_helper_executes_under_real_macos_seatbelt() {
    use rx4::sandbox::{OsSandbox, OsSandboxConfig, OsSandboxRunner};
    let root = tempfile::tempdir().unwrap();
    let mut context = ToolContext::new(root.path());
    context.os_sandbox = Some(Arc::new(
        OsSandboxRunner::new(OsSandboxConfig::new(
            OsSandbox::MacosSeatbelt,
            root.path().to_path_buf(),
        ))
        .unwrap(),
    ));
    let result = run(context, helper()).await;
    assert!(!result.is_error, "{}", result.content);
    assert!(result.content.contains("42"));
}
