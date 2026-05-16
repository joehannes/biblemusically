#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::api::process::{Command, CommandEvent};
use tauri::Manager;

/// Spawn the bundled python-backend sidecar on app launch so the React frontend
/// can hit http://127.0.0.1:8001 the same way it does in dev.
fn spawn_backend(app: &tauri::AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Ok((mut rx, _child)) = Command::new_sidecar("python-backend")
            .expect("python-backend sidecar binary not found in bundle")
            .args(["--host", "127.0.0.1", "--port", "8001"])
            .spawn()
        {
            while let Some(event) = rx.recv().await {
                if let CommandEvent::Stdout(line) | CommandEvent::Stderr(line) = event {
                    let _ = handle.emit_all("backend-log", line);
                }
            }
        }
    });
}

#[tauri::command]
fn ffmpeg_compose(args: Vec<String>) -> Result<String, String> {
    // Use bundled ffmpeg sidecar (no system install needed).
    let (mut rx, _child) = Command::new_sidecar("ffmpeg")
        .map_err(|e| e.to_string())?
        .args(args)
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    tauri::async_runtime::block_on(async {
        while let Some(event) = rx.recv().await {
            if let CommandEvent::Stdout(s) | CommandEvent::Stderr(s) = event {
                out.push_str(&s);
                out.push('\n');
            }
        }
    });
    Ok(out)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| { spawn_backend(&app.handle()); Ok(()) })
        .invoke_handler(tauri::generate_handler![ffmpeg_compose])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
