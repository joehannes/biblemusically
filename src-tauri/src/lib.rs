// Module declarations. The app keeps backend modules at the src-tauri root,
// while Cargo's library entrypoint lives in src-tauri/src.
#[path = "../commands/mod.rs"]
pub mod commands;
#[path = "../helpers.rs"]
pub mod helpers;
#[path = "../jobs.rs"]
pub mod jobs;
#[path = "../models.rs"]
pub mod models;
#[path = "../state.rs"]
pub mod state;

use state::AppState;
use std::sync::Arc;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
    
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            use tauri::Manager;
            use tauri_plugin_shell::ShellExt;
            let handle = app.handle();
            
            // Start mongod sidecar
            let app_data = handle.path().app_data_dir().expect("failed to get app data dir");
            let db_path = app_data.join("db");
            std::fs::create_dir_all(&db_path).unwrap();
            
            if let Ok(sidecar) = handle.shell().sidecar("mongod") {
                match sidecar.args(["--dbpath", db_path.to_str().unwrap(), "--port", "27018"]).spawn() {
                    Ok(_) => println!("mongod sidecar started on port 27018"),
                    Err(e) => eprintln!("Failed to start mongod sidecar: {}", e),
                }
            } else {
                eprintln!("Failed to find mongod sidecar configuration.");
            }
            
            // Give mongod a second to bind
            std::thread::sleep(std::time::Duration::from_millis(1500));
            
            // Tell AppState to use our sidecar port
            std::env::set_var("MONGO_URL", "mongodb://localhost:27018");
            
            let app_state_res = tauri::async_runtime::block_on(async {
                state::AppState::new().await
            });
            
            let app_state = match app_state_res {
                Ok(state) => state,
                Err(e) => {
                    rfd::MessageDialog::new()
                        .set_title("Database Connection Error")
                        .set_description(&format!(
                            "Failed to connect to the local bundled database.\n\nError: {}\n\nThe application will now exit.",
                            e
                        ))
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                    std::process::exit(1);
                }
            };
            
            let state_arc = Arc::new(app_state.clone());
            app.manage(app_state);
            app.manage(state_arc);

            // Check for FFmpeg and show a warning dialog if missing
            if which::which("ffmpeg").is_err() {
                use tauri_plugin_dialog::DialogExt;
                let os = std::env::consts::OS;
                let hint = match os {
                    "linux" => "Please install it using: sudo apt install ffmpeg",
                    "macos" => "Please install it using: brew install ffmpeg",
                    "windows" => "Please download it from https://ffmpeg.org/download.html and add it to your PATH.",
                    _ => "Please install ffmpeg.",
                };
                app.dialog()
                    .message(format!("FFmpeg was not found on your system.\n\n{}\n\nVideo composition features will not work without it.", hint))
                    .title("Missing Dependency: FFmpeg")
                    .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
                    .blocking_show();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            // Settings commands
            commands::get_settings,
            commands::update_settings,
            commands::test_suno,
            commands::test_mj,
            commands::test_ffmpeg,
            // Projects commands
            commands::list_projects,
            commands::create_project,
            commands::get_project,
            commands::update_project,
            commands::delete_project,
            commands::export_project,
            commands::import_project,
            commands::import_lyrics,
            // Songs commands
            commands::list_songs,
            commands::get_song,
            commands::delete_song,
            commands::generate_music,
            commands::analyze_song,
            commands::compose_video,
            commands::download_and_convert_audio,
            commands::download_all_songs,
            commands::select_song_variant,
            // Sections commands
            commands::list_sections,
            commands::update_section,
            commands::generate_section_image,
            commands::batch_generate_images,
            commands::get_effects_presets,
            // Channels commands
            commands::list_channels,
            commands::create_channel,
            commands::delete_channel,
            commands::oauth_complete_channel,
            commands::channels_connect_all_urls,
            // OAuth commands
            commands::list_oauth_clients,
            commands::create_oauth_client,
            commands::update_oauth_client,
            commands::delete_oauth_client,
            commands::channel_picked_client,
            commands::oauth_start,
            commands::oauth_callback,
            // Jobs commands
            commands::list_jobs,
            commands::get_job,
            commands::retry_job,
            commands::cancel_job,
            // Uploads commands
            commands::list_uploads,
            commands::create_upload,
            commands::publish_upload,
            commands::publish_all_uploads,
            commands::bulk_uploads_from_videos,
            commands::uploads_preflight,
            commands::ai_enrich_uploads,
            // Bible commands
            commands::list_translations,
            commands::list_bible_books,
            commands::fetch_chapter,
            commands::list_pasted_chapters,
            commands::save_pasted_chapter,
            commands::delete_pasted_chapter,
            // AI commands
            commands::get_compose_config,
            commands::save_compose_config,
            commands::compose_lyrics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
