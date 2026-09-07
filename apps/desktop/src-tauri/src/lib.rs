use serde::Serialize;
use std::process::Command;

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
#[tauri::command]
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

#[tauri::command]
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![spawn_telekinesis, telekinesis_status])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
