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
        .plugin(tauri_plugin_opener::init())
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
            // Browsh sidecar and midjourney-proxy autostart have been deprecated
            // in favor of a visible Playwright-driven browser workflow.
            // No automatic browsh or proxy startup is performed.
            
            // Give mongod and any sidecars a second to bind
            std::thread::sleep(std::time::Duration::from_millis(1500));
            
            // Tell AppState to use our sidecar port
            std::env::set_var("MONGO_URL", "mongodb://localhost:27018");

            // Midjourney proxy auto-detection removed. Use direct Playwright
            // driven automation which uses a Playwright profile (stored as `mj_profile_dir`)
            // to interact with midjourney.com.
            
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
            
            // If we auto-detected and set MJ_PROXY_URL earlier, persist it into the settings collection
            if let Ok(proxy) = std::env::var("MJ_PROXY_URL") {
                if !proxy.trim().is_empty() {
                    let db_clone = app_state.db.clone();
                    let proxy_clone = proxy.clone();
                    // perform update on the runtime to ensure DB is available
                    let _ = tauri::async_runtime::block_on(async move {
                        let coll = db_clone.collection::<mongodb::bson::Document>("settings");
                        let filter = mongodb::bson::doc! { "_id": "singleton" };
                        let update = mongodb::bson::doc! { "$set": { "mj_proxy_url": proxy_clone } };
                        let _ = coll.update_one(filter.clone(), update).await;
                    });
                }
            }

            // Persist Suno cookie from environment to settings if provided.
            if let Ok(suno_cookie) = std::env::var("SUNO_COOKIE") {
                if !suno_cookie.trim().is_empty() {
                    let db_clone = app_state.db.clone();
                    let cookie_clone = suno_cookie.clone();
                    let _ = tauri::async_runtime::block_on(async move {
                        let coll = db_clone.collection::<mongodb::bson::Document>("settings");
                        let filter = mongodb::bson::doc! { "_id": "singleton" };
                        let update = mongodb::bson::doc! { "$set": { "suno_cookie": cookie_clone } };
                        let _ = coll.update_one(filter.clone(), update).await;
                    });
                }
            }

            // Persist MJ_DISCORD_TOKEN if provided via environment
            if let Ok(dtoken) = std::env::var("MJ_DISCORD_TOKEN") {
                if !dtoken.trim().is_empty() {
                    let db_clone = app_state.db.clone();
                    let token_clone = dtoken.clone();
                    let _ = tauri::async_runtime::block_on(async move {
                        let coll = db_clone.collection::<mongodb::bson::Document>("settings");
                        let filter = mongodb::bson::doc! { "_id": "singleton" };
                        let update = mongodb::bson::doc! { "$set": { "mj_discord_token": token_clone } };
                            let _ = coll.update_one(filter, update).await;
                        });
                    }
                }

            // Attempt to auto-start the Midjourney proxy on app startup.
            {
                let db_clone = app_state.db.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = commands::ensure_mj_autostart_internal(&db_clone).await;
                });
            }

            let db_clone = app_state.db.clone();
            let state_arc = Arc::new(app_state.clone());
            app.manage(app_state);
            app.manage(state_arc.clone());

            // NOTE: Automatic OAuth loopback on startup has been intentionally removed.
            // It was binding to the redirect port (e.g. 3335) at startup and leaving a
            // warp server running, which caused a panic when the user clicked
            // "Discover channels" — their explicit OAuth flow tried to bind the same port.
            // All OAuth flows are now user-initiated only.

            // Start a small local HTTP endpoint so the Browsh CLI/extension can POST
            // detected Suno cookies to the backend for persistence.
            {
                let db_for_server = db_clone.clone();
                tauri::async_runtime::spawn(async move {
                    use warp::Filter;

                    #[derive(serde::Deserialize)]
                    struct CookiePayload {
                        cookie: String,
                    }

                    let db_filter = warp::any().map(move || db_for_server.clone());

                    let route = warp::post()
                        .and(warp::path("auth")).and(warp::path("suno"))
                        .and(warp::body::json())
                        .and(db_filter)
                        .and_then(|payload: CookiePayload, db: mongodb::Database| async move {
                            let coll = db.collection::<mongodb::bson::Document>("settings");
                            let filter = mongodb::bson::doc! { "_id": "singleton" };
                            let update = mongodb::bson::doc! { "$set": { "suno_cookie": payload.cookie.clone() } };
                            let _ = coll.update_one(filter, update).await;
                            Ok::<_, std::convert::Infallible>(warp::reply::with_status(
                                "OK",
                                warp::http::StatusCode::OK,
                            ))
                        });

                    warp::serve(route).run(([127, 0, 0, 1], 3337)).await;
                });
            }

            // Start background token validation and periodic refresh checks.
            {
                let db_clone = db_clone.clone();
                tauri::async_runtime::spawn(async move {
                    let mut auth_interval = tokio::time::interval(std::time::Duration::from_secs(900));
                    let mut refresh_interval = tokio::time::interval(std::time::Duration::from_secs(3600));
                    auth_interval.tick().await;
                    refresh_interval.tick().await;
                    loop {
                        tokio::select! {
                            _ = auth_interval.tick() => {
                                if let Err(err) = commands::validate_suno_cookie_internal(&db_clone).await {
                                    eprintln!("Token maintenance: Suno cookie check failed: {}", err);
                                }
                                if let Err(err) = commands::validate_mj_token_internal(&db_clone).await {
                                    eprintln!("Token maintenance: MJ proxy/auth check failed: {}", err);
                                }
                            }
                            _ = refresh_interval.tick() => {
                                match commands::validate_google_refresh_tokens_internal(&db_clone).await {
                                    Ok(invalidated) if !invalidated.is_empty() => {
                                        eprintln!("Token maintenance: invalidated YouTube refresh tokens for channels: {:?}", invalidated);
                                    }
                                    Err(err) => {
                                        eprintln!("Token maintenance: Google refresh validation failed: {}", err);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                });
            }

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
            commands::open_suno_login,
            commands::open_midjourney_login,
            commands::capture_suno_session,
            commands::capture_midjourney_session,
            commands::generate_mj_now,
            commands::ensure_mj_autostart,
            commands::mj_auto_login,
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
            commands::bulk_generate_all_songs,
            commands::get_effects_presets,
            // Channels commands
            commands::list_channels,
            commands::create_channel,
            commands::delete_channel,
            commands::oauth_complete_channel,
            commands::channels_connect_all_urls,
            commands::discover_youtube_channels,
            commands::discover_from_channel_switcher,
            commands::connect_all_channels_one_shot,
            commands::import_discovered_channels,
            commands::refresh_all_channel_metadata,
            commands::import_from_google_account,
            // Characters commands
            commands::list_characters,
            commands::create_character,
            commands::update_character,
            commands::delete_character,
            commands::generate_character_image,
            commands::vary_character_image,
            commands::select_character_variant,
            commands::discard_character_variant,
            commands::discard_all_character_variants,
            commands::propose_characters,
            // OAuth commands
            commands::list_oauth_clients,
            commands::create_oauth_client,
            commands::update_oauth_client,
            commands::delete_oauth_client,
            commands::channel_picked_client,
            commands::oauth_start,
            commands::oauth_start_for_channel,
            commands::oauth_start_loopback,
            commands::oauth_callback,
            commands::validate_oauth_client,
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
            commands::compose_assist,
            commands::compose_lyrics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
