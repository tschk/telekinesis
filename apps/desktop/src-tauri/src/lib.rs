use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use telekinesis::local_runtime::{
    CreateThread, Handoff, LocalRuntime, Project, Receipt, RuntimeEvent, Submit, Thread,
    ThreadConfig, ThreadSnapshot,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnResult {
    pub ok: bool,
    pub binary: Option<String>,
    pub message: String,
}

fn find_telekinesis_binary() -> Option<String> {
    for name in ["tk", "telekinesis"] {
        if which_ok(name) {
            return Some(name.to_string());
        }
    }
    None
}

fn which_ok(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Spawn local telekinesis CLI (`tk` or `telekinesis` on PATH).
/// Safe when missing: returns ok=false with a clear message (build does not require the binary).
#[tauri::command(async)]
fn spawn_telekinesis() -> SpawnResult {
    let Some(binary) = find_telekinesis_binary() else {
        return SpawnResult {
            ok: false,
            binary: None,
            message: "Neither `tk` nor `telekinesis` found on PATH. Install with `cargo install telekinesis` or the project's install.sh.".to_string(),
        };
    };

    match Command::new(&binary).spawn() {
        Ok(child) => SpawnResult {
            ok: true,
            binary: Some(binary.clone()),
            message: format!("Spawned `{binary}` (pid {}).", child.id()),
        },
        Err(err) => SpawnResult {
            ok: false,
            binary: Some(binary.clone()),
            message: format!("Failed to spawn `{binary}`: {err}"),
        },
    }
}

#[tauri::command(async)]
fn telekinesis_status() -> SpawnResult {
    match find_telekinesis_binary() {
        Some(binary) => SpawnResult {
            ok: true,
            binary: Some(binary.clone()),
            message: format!("Found `{binary}` on PATH."),
        },
        None => SpawnResult {
            ok: false,
            binary: None,
            message: "Neither `tk` nor `telekinesis` found on PATH.".to_string(),
        },
    }
}

type Host = Arc<DesktopRuntime>;

/// Provider discovery and storage opening happen on a blocking worker, never
/// during window setup. Concurrent first requests share one runtime instance.
struct DesktopRuntime {
    root: PathBuf,
    runtime: Mutex<Option<Arc<LocalRuntime>>>,
    emit: Arc<dyn Fn(RuntimeEvent) + Send + Sync>,
}
impl DesktopRuntime {
    fn runtime(&self) -> Result<Arc<LocalRuntime>, String> {
        let mut slot = self
            .runtime
            .lock()
            .map_err(|_| "Local runtime state unavailable.".to_string())?;
        if let Some(runtime) = slot.as_ref() {
            return Ok(runtime.clone());
        }
        let emit = self.emit.clone();
        let runtime = LocalRuntime::open(self.root.clone(), move |event| emit(event))?;
        *slot = Some(runtime.clone());
        Ok(runtime)
    }
}
async fn with_runtime<T: Send + 'static>(
    host: Host,
    work: impl FnOnce(Arc<LocalRuntime>) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(host.runtime()?))
        .await
        .map_err(|error| error.to_string())?
}
#[tauri::command]
async fn local_projects(host: tauri::State<'_, Host>) -> Result<Vec<Project>, String> {
    with_runtime(host.inner().clone(), |runtime| Ok(runtime.projects())).await
}
#[tauri::command]
async fn local_add_project(host: tauri::State<'_, Host>, path: String) -> Result<Project, String> {
    with_runtime(host.inner().clone(), move |runtime| {
        runtime.add_project(&path)
    })
    .await
}
#[tauri::command]
async fn local_threads(
    host: tauri::State<'_, Host>,
    project_id: String,
) -> Result<Vec<Thread>, String> {
    with_runtime(host.inner().clone(), move |runtime| {
        Ok(runtime.threads(&project_id))
    })
    .await
}
#[tauri::command]
async fn local_create_thread(
    host: tauri::State<'_, Host>,
    request: CreateThread,
) -> Result<Thread, String> {
    with_runtime(host.inner().clone(), move |runtime| {
        runtime.create_thread(request)
    })
    .await
}
#[tauri::command]
async fn local_load_thread(
    host: tauri::State<'_, Host>,
    thread_id: String,
) -> Result<ThreadSnapshot, String> {
    with_runtime(host.inner().clone(), move |runtime| {
        runtime.load_thread(&thread_id)
    })
    .await
}
#[tauri::command]
async fn local_update_thread(
    host: tauri::State<'_, Host>,
    thread_id: String,
    config: ThreadConfig,
) -> Result<Thread, String> {
    with_runtime(host.inner().clone(), move |runtime| {
        runtime.update_thread(&thread_id, config)
    })
    .await
}
#[tauri::command]
async fn local_submit(host: tauri::State<'_, Host>, request: Submit) -> Result<Receipt, String> {
    with_runtime(host.inner().clone(), move |runtime| runtime.submit(request)).await
}
#[tauri::command]
async fn local_handoff(
    host: tauri::State<'_, Host>,
    request: Handoff,
) -> Result<ThreadSnapshot, String> {
    with_runtime(host.inner().clone(), move |runtime| {
        runtime.handoff(request)
    })
    .await
}
#[tauri::command]
async fn local_cancel(
    host: tauri::State<'_, Host>,
    thread_id: String,
    run_id: String,
) -> Result<bool, String> {
    with_runtime(host.inner().clone(), move |runtime| {
        runtime.cancel(&thread_id, &run_id)
    })
    .await
}
#[tauri::command]
async fn local_runtime_options(
    host: tauri::State<'_, Host>,
) -> Result<Vec<serde_json::Value>, String> {
    with_runtime(host.inner().clone(), |runtime| Ok(runtime.options())).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let root = telekinesis::host::config_home()
                .ok_or_else(|| {
                    std::io::Error::other("Home directory unavailable; cannot open local history.")
                })?
                .join("local-runtime");
            app.manage(Arc::new(DesktopRuntime {
                root,
                runtime: Mutex::new(None),
                emit: Arc::new(move |event| {
                    let _ = handle.emit("local-runtime", event);
                }),
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            spawn_telekinesis,
            telekinesis_status,
            local_projects,
            local_add_project,
            local_threads,
            local_create_thread,
            local_load_thread,
            local_update_thread,
            local_submit,
            local_handoff,
            local_cancel,
            local_runtime_options
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(host) = app.try_state::<Host>() {
                    if let Ok(slot) = host.runtime.try_lock() {
                        if let Some(runtime) = slot.as_ref() {
                            let _ = runtime.shutdown();
                        }
                    }
                }
            }
        });
}
