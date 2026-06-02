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
            // Attempt to start a bundled Browsh CLI browser sidecar (text/TTY browser)
            if let Ok(sidecar) = handle.shell().sidecar("browsh") {
                // Auto-launch Browsh pointing at Suno and request GUI Firefox so user can
                // perform interactive logins (captcha/2FA) when needed.
                match sidecar.args(["--startup-url", "https://studio.suno.ai", "--firefox.with-gui=true"]).spawn() {
                    Ok(_) => println!("browsh sidecar started"),
                    Err(e) => eprintln!("Failed to start browsh sidecar: {}", e),
                }
            } else {
                eprintln!("Browsh sidecar not configured or binary missing.");
            }
            
            // Give mongod and any sidecars a second to bind
            std::thread::sleep(std::time::Duration::from_millis(1500));
            
            // Tell AppState to use our sidecar port
            std::env::set_var("MONGO_URL", "mongodb://localhost:27018");

            // Auto-detect and auto-start bundled Midjourney proxy if present in workspace
            // Search upward from the executable for a folder named `midjourney-proxy` (dev workspace)
            if let Ok(exe_path) = std::env::current_exe() {
                let mut p = exe_path.parent().map(|s| s.to_path_buf());
                let mut found: Option<std::path::PathBuf> = None;
                for _ in 0..6 {
                    if let Some(dir) = &p {
                        let cand = dir.join("midjourney-proxy");
                        if cand.exists() && cand.is_dir() {
                            found = Some(cand);
                            break;
                        }
                    }
                    p = p.and_then(|d| d.parent().map(|s| s.to_path_buf()));
                }

                if let Some(proxy_dir) = &found {
                    // Try to start via run_app.sh if present
                    // proxy repo places runtime scripts under `scripts/run_app.sh`
                    let run_sh = if proxy_dir.join("run_app.sh").exists() {
                        proxy_dir.join("run_app.sh")
                    } else {
                        proxy_dir.join("scripts").join("run_app.sh")
                    };
                    let mut started = false;
                    if run_sh.exists() {
                        // Spawn the script in background so it's independent of the GUI thread
                        let pd = proxy_dir.clone();
                        std::thread::spawn(move || {
                            let _ = std::process::Command::new("sh")
                                .arg("run_app.sh")
                                .current_dir(pd)
                                .spawn();
                        });
                        started = true;
                    }

                    // If started, probe common ports for the proxy and set MJ_PROXY_URL env var
                    if started {
                        let ports = [8080u16, 8086u16, 8081u16, 8085u16];
                        for port in ports.iter() {
                            let url = format!("http://127.0.0.1:{}", port);
                            let cl = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(2)).build();
                            if let Ok(client) = cl {
                                if client.get(format!("{}/info", url.trim_end_matches('/'))).send().is_ok() {
                                    std::env::set_var("MJ_PROXY_URL", url.clone());
                                    // persist into settings DB if available later during AppState init
                                    break;
                                }
                            }
                        }
                    }
                }
                // If not found via executable tree, check bundled resources (packaged app)
                if found.is_none() {
                    if let Ok(res_dir) = handle.path().resource_dir() {
                        let cand = res_dir.join("midjourney-proxy");
                        let run_sh = if cand.join("run_app.sh").exists() {
                            cand.join("run_app.sh")
                        } else {
                            cand.join("scripts").join("run_app.sh")
                        };
                        if run_sh.exists() {
                            found = Some(cand);
                        }
                    }
                }
            }
            
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

            // Autotrigger authentication helpers at startup:
            // - If Suno cookie missing, ensure Browsh is running (started above) to capture cookies.
            // - If Google OAuth client exists and no stored Google refresh token for Suno, perform loopback OAuth.
            {
                let db_start = db_clone.clone();
                tauri::async_runtime::spawn(async move {
                    // Small delay to allow sidecars and DB to be ready
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                    // Check settings for existing Suno cookie and Google refresh token
                    let settings_coll = db_start.collection::<mongodb::bson::Document>("settings");
                    let sdoc = settings_coll.find_one(mongodb::bson::doc! { "_id": "singleton" }, None).await.ok().flatten();
                    let mut has_suno_cookie = false;
                    let mut has_google_refresh = false;
                    if let Some(doc) = sdoc {
                        if let Some(v) = doc.get_str("suno_cookie").ok() {
                            has_suno_cookie = !v.trim().is_empty();
                        }
                        if let Some(v) = doc.get_str("suno_google_refresh_token").ok() {
                            has_google_refresh = !v.trim().is_empty();
                        }
                    }

                    // If no Suno cookie, Browsh was started above with startup-url to capture cookies.
                    if !has_suno_cookie {
                        eprintln!("Suno cookie missing: awaiting Browsh-assisted capture (startup web page)");
                    }

                    // If no Google refresh token, try performing loopback OAuth using first available OAuth client
                    if !has_google_refresh {
                        let ocoll = db_start.collection::<mongodb::bson::Document>("oauth_clients");
                        if let Ok(doc_opt) = ocoll.find_one(None, None).await {
                            if let Some(doc) = doc_opt {
                                if let Ok(oid) = doc.get_str("id") {
                                    // spawn the loopback OAuth flow (opens system browser)
                                    let oid_str = oid.to_string();
                                    tokio::spawn(async move {
                                        match crate::commands::oauth::perform_oauth_loopback(&db_start, &oid_str, None).await {
                                            Ok(tokens) => {
                                                eprintln!("Loopback OAuth completed on startup: tokens stored");
                                            }
                                            Err(err) => {
                                                eprintln!("Loopback OAuth on startup failed: {}", err);
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    }
                });
            }

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
                            let _ = coll.update_one(filter, update, None).await;
                            Ok::<_, std::convert::Infallible>(warp::reply::with_status(
                                "OK",
                                warp::http::StatusCode::OK,
                            ))
                        });

                    warp::serve(route).run(([127, 0, 0, 1], 3335)).await;
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
            commands::oauth_start_loopback,
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
