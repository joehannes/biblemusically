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
#[path = "../mongo_import.rs"]
pub mod mongo_import;
#[path = "../project_sync.rs"]
pub mod project_sync;
#[path = "../state.rs"]
pub mod state;
#[path = "../store.rs"]
pub mod store;
#[path = "../tests_logic.rs"]
mod tests_logic;
#[path = "../vault.rs"]
pub mod vault;

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
            let handle = app.handle();

            // Persistence is plain JSON files (see store.rs) — there is no database server to
            // start any more. The old `mongod` sidecar is gone: it was a native x86_64 binary that
            // could never ship to Android, and it made the user's own data unreadable without it.
            let app_data = handle.path().app_data_dir().expect("failed to get app data dir");

            // Browsh sidecar and midjourney-proxy autostart have been deprecated
            // in favor of a visible Playwright-driven browser workflow.
            // No automatic browsh or proxy startup is performed.

            // Midjourney proxy auto-detection removed. Use direct Playwright
            // driven automation which uses a Playwright profile (stored as `mj_profile_dir`)
            // to interact with midjourney.com.

            let app_state_res = tauri::async_runtime::block_on(async {
                state::AppState::new().await
            });

            let app_state = match app_state_res {
                Ok(state) => state,
                Err(e) => {
                    let message = format!(
                        "Failed to open the local data folder.\n\nError: {}\n\nThe application will now exit.",
                        e
                    );
                    // A native dialog on desktop; mobile has no rfd backend, so the message goes to
                    // the log where `adb logcat` will show it.
                    #[cfg(desktop)]
                    rfd::MessageDialog::new()
                        .set_title("Data Folder Error")
                        .set_description(&message)
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                    eprintln!("{message}");
                    std::process::exit(1);
                }
            };

            // One-time carry-over from the retired MongoDB sidecar. Runs before the UI can read
            // anything, so an upgrading user never sees a half-empty app; a no-op (microseconds)
            // once the marker file is in place, and skipped entirely on a fresh install.
            {
                let db = app_state.db.clone();
                let app_data = app_data.clone();
                let report = tauri::async_runtime::block_on(async move {
                    mongo_import::import_if_needed(&db, &app_data).await
                });
                match report.get("status").and_then(|s| s.as_str()) {
                    Some("migrated") => println!(
                        "Legacy database imported into JSON: {} documents",
                        report.get("documents_imported").and_then(|d| d.as_u64()).unwrap_or(0)
                    ),
                    Some("failed") => eprintln!(
                        "Legacy database import failed: {}",
                        report.get("error").and_then(|e| e.as_str()).unwrap_or("unknown")
                    ),
                    _ => {}
                }
                // Keep the report readable from the UI (Settings → Data).
                if let Ok(text) = serde_json::to_string_pretty(&report) {
                    let _ = std::fs::write(app_state.db.global_root().join("migration-report.json"), text);
                }
            }
            
            // If we auto-detected and set MJ_PROXY_URL earlier, persist it into the settings collection
            if let Ok(proxy) = std::env::var("MJ_PROXY_URL") {
                if !proxy.trim().is_empty() {
                    let db_clone = app_state.db.clone();
                    let proxy_clone = proxy.clone();
                    // perform update on the runtime to ensure DB is available
                    let _ = tauri::async_runtime::block_on(async move {
                        let coll = db_clone.collection::<bson::Document>("settings");
                        let filter = bson::doc! { "_id": "singleton" };
                        let update = bson::doc! { "$set": { "mj_proxy_url": proxy_clone } };
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
                        let coll = db_clone.collection::<bson::Document>("settings");
                        let filter = bson::doc! { "_id": "singleton" };
                        let update = bson::doc! { "$set": { "suno_cookie": cookie_clone } };
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
                        let coll = db_clone.collection::<bson::Document>("settings");
                        let filter = bson::doc! { "_id": "singleton" };
                        let update = bson::doc! { "$set": { "mj_discord_token": token_clone } };
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

            // Embedded-browser webview manager. Its scrape store is shared with the local
            // HTTP server below so injected automation scripts can POST results back.
            let webview_mgr = commands::webview::WebviewManager::default();
            let scrape_store = webview_mgr.scrape.clone();
            let macro_state = webview_mgr.macros.clone();
            app.manage(webview_mgr);

            // Live-progress monitors for the Kaggle engine notebooks (one streaming task per engine).
            app.manage(commands::kaggle_monitor::KaggleMonitors::default());

            // Restructure the window so embedded pages can actually be positioned on Linux.
            // Must happen here, at startup: it re-parents the main webview, which re-realizes
            // it — free now, but it would reload the running app if deferred until first use.
            commands::webview::install_layout(&app.handle());

            // NOTE: Automatic OAuth loopback on startup has been intentionally removed.
            // It was binding to the redirect port (e.g. 3335) at startup and leaving a
            // warp server running, which caused a panic when the user clicked
            // "Discover channels" — their explicit OAuth flow tried to bind the same port.
            // All OAuth flows are now user-initiated only.

            // Start a small local HTTP endpoint so the Browsh CLI/extension can POST
            // detected Suno cookies to the backend for persistence.
            {
                let db_for_server = db_clone.clone();
                let scrape_for_server = scrape_store.clone();
                let macros_for_server = macro_state.clone();
                tauri::async_runtime::spawn(async move {
                    use warp::Filter;

                    #[derive(serde::Deserialize)]
                    struct CookiePayload {
                        cookie: String,
                    }

                    #[derive(serde::Deserialize)]
                    struct ScrapePayload {
                        key: String,
                        data: serde_json::Value,
                    }

                    let db_filter = warp::any().map(move || db_for_server.clone());
                    let scrape_filter = warp::any().map(move || scrape_for_server.clone());

                    let suno_route = warp::post()
                        .and(warp::path("auth")).and(warp::path("suno"))
                        .and(warp::body::json())
                        .and(db_filter)
                        .and_then(|payload: CookiePayload, db: crate::store::Db| async move {
                            let coll = db.collection::<bson::Document>("settings");
                            let filter = bson::doc! { "_id": "singleton" };
                            let update = bson::doc! { "$set": { "suno_cookie": payload.cookie.clone() } };
                            let _ = coll.update_one(filter, update).await;
                            Ok::<_, std::convert::Infallible>(warp::reply::with_status(
                                "OK",
                                warp::http::StatusCode::OK,
                            ))
                        });

                    // Injected automation scripts POST scraped results here (see webview.rs).
                    type ScrapeStore = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>>;
                    let scrape_route = warp::post()
                        .and(warp::path("scrape"))
                        .and(warp::body::content_length_limit(1024 * 1024 * 8))
                        .and(warp::body::json())
                        .and(scrape_filter)
                        .and_then(|payload: ScrapePayload, store: ScrapeStore| async move {
                            if let Ok(mut map) = store.lock() {
                                map.insert(payload.key, payload.data);
                            }
                            Ok::<_, std::convert::Infallible>(warp::reply::with_status(
                                "OK",
                                warp::http::StatusCode::OK,
                            ))
                        });

                    // Macro recorder: injected recorder scripts append one step per user action,
                    // and ask on every page load whether a recording is live (see webview.rs).
                    type MacroStateArc = std::sync::Arc<commands::webview::MacroState>;
                    let macro_filter = warp::any().map(move || macros_for_server.clone());
                    let macro_step_route = warp::post()
                        .and(warp::path("macro")).and(warp::path("step"))
                        .and(warp::body::content_length_limit(1024 * 256))
                        .and(warp::body::bytes())
                        .and(macro_filter.clone())
                        .and_then(|body: warp::hyper::body::Bytes, st: MacroStateArc| async move {
                            if st.recording.load(std::sync::atomic::Ordering::SeqCst) {
                                if let Ok(step) = serde_json::from_slice::<serde_json::Value>(&body) {
                                    if let Ok(mut steps) = st.steps.lock() {
                                        if steps.len() < 5000 { steps.push(step); }
                                    }
                                }
                            }
                            Ok::<_, std::convert::Infallible>(warp::reply::with_status(
                                "OK",
                                warp::http::StatusCode::OK,
                            ))
                        });
                    let macro_state_route = warp::get()
                        .and(warp::path("macro")).and(warp::path("state"))
                        .and(macro_filter)
                        .and_then(|st: MacroStateArc| async move {
                            let recording = st.recording.load(std::sync::atomic::Ordering::SeqCst);
                            Ok::<_, std::convert::Infallible>(warp::reply::json(
                                &serde_json::json!({ "recording": recording }),
                            ))
                        });

                    let cors = warp::cors().allow_any_origin().allow_methods(vec!["POST", "GET"]).allow_headers(vec!["content-type"]);
                    let route = suno_route.or(scrape_route).or(macro_step_route).or(macro_state_route).with(cors);

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

            // Per-project git autosave: commit whatever JSON the store changed, every 45s. The
            // store flags projects dirty on write; this turns those flags into commits so each
            // project's folder carries its own history (see project_sync.rs).
            project_sync::spawn_sweeper(db_clone.clone(), 45);

            // Auto-sync: push projects that opted in, every 15 minutes. Skips the network
            // entirely when a project has nothing new (see remote_sync::do_auto_sync).
            {
                let sync_state = state_arc.clone();
                tauri::async_runtime::spawn(async move {
                    let mut tick = tokio::time::interval(std::time::Duration::from_secs(900));
                    tick.tick().await;
                    loop {
                        tick.tick().await;
                        commands::remote_sync::auto_sync_sweep(&sync_state).await;
                    }
                });
            }

            // Scheduler: every 5 minutes, check every project's `schedule_config` and run
            // chapter-generation for any that are due. See commands/scheduler.rs for the full
            // design (Bible-book chapter cursor → AI lyrics → draft song → auto-enqueue music;
            // deliberately stops there, leaving analysis/images/video/upload manual).
            {
                let scheduler_state = state_arc.clone();
                tauri::async_runtime::spawn(async move {
                    let mut tick = tokio::time::interval(std::time::Duration::from_secs(300));
                    loop {
                        tick.tick().await;
                        commands::run_scheduler_tick(&scheduler_state).await;
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
            commands::test_acestep,
            commands::test_heartmula,
            commands::test_flux,
            commands::test_comfy,
            commands::test_mj,
            commands::test_ffmpeg,
            commands::open_suno_login,
            commands::open_midjourney_login,
            commands::open_kaggle_notebook,
            commands::open_kaggle_token_page,
            commands::kaggle_notebook_url,
            commands::open_kaggle_login,
            commands::save_kaggle_token,
            commands::research_project_channels,
            commands::list_style_samples,
            commands::generate_style_sample,
            commands::delete_style_sample,
            commands::get_user_learnings,
            commands::update_user_learnings,
            commands::get_project_learnings,
            commands::update_project_learnings,
            commands::record_learning_signal,
            commands::learnings_locations,
            // Data / storage transparency + legacy import
            commands::service_health,
            // Pipeline overview, performance feedback, quotas
            commands::pipeline_overview,
            commands::refresh_upload_analytics,
            commands::performance_report,
            commands::quota_report,
            // Social presence: accounts, taste profile, derivatives, co-publishing
            commands::list_social_platforms,
            commands::connect_social_account,
            commands::list_social_accounts,
            commands::disconnect_social_account,
            commands::verify_social_account,
            commands::ingest_social_activity,
            commands::refresh_taste_profile,
            commands::ideate_next,
            commands::derive_song_versions,
            commands::list_derivatives,
            commands::update_derivative,
            commands::delete_derivative,
            commands::publish_derivative,
            commands::publish_all_derivatives,
            commands::store_info,
            commands::run_legacy_migration,
            commands::purge_legacy_data,
            commands::commit_project_data,
            commands::commit_all_project_data,
            commands::reveal_in_file_manager,
            // Encrypted credential vault
            vault::vault_status,
            vault::vault_list,
            vault::vault_put,
            vault::vault_delete,
            vault::vault_unlock,
            vault::vault_lock,
            vault::vault_set_passphrase,
            // Remote sync (free git hosts + asset platforms)
            commands::list_sync_providers,
            commands::get_sync_config,
            commands::save_sync_config,
            commands::save_asset_credentials,
            commands::test_sync_connection,
            commands::sync_project_now,
            commands::pull_project_now,
            commands::list_project_assets,
            commands::restore_project_assets,
            commands::list_kaggle_accounts,
            commands::activate_kaggle_account,
            commands::remove_kaggle_account,
            commands::rotate_kaggle_account,
            commands::pick_directory,
            commands::fetch_kaggle_url,
            commands::start_kaggle_server,
            commands::supersede_kaggle_session,
            commands::stop_kaggle_server,
            commands::kaggle_start_monitor,
            commands::kaggle_progress,
            commands::kaggle_stop_monitor,
            commands::capture_suno_session,
            commands::capture_midjourney_session,
            commands::generate_mj_now,
            commands::ensure_mj_autostart,
            commands::mj_auto_login,
            commands::probe_node,
            // Projects commands
            commands::list_projects,
            commands::create_project,
            commands::get_project,
            commands::save_project_file,
            commands::update_project,
            commands::delete_project,
            commands::export_project,
            commands::import_project,
            commands::import_lyrics,
            commands::get_project_git_info,
            commands::save_project_version,
            commands::checkout_project_git_tag,
            commands::checkout_project_git_branch,
            commands::create_project_git_branch,
            commands::authorize_project_gdrive,
            // Scheduler commands
            commands::generate_next_chapter_now,
            // Songs commands
            commands::list_songs,
            commands::get_song,
            commands::update_song,
            commands::delete_song,
            commands::generate_music,
            commands::analyze_song,
            commands::compose_video,
            commands::generate_overlay,
            commands::generate_overlays_bulk,
            commands::download_and_convert_audio,
            commands::download_all_songs,
            commands::save_generated_asset,
            commands::select_song_variant,
            // Sections commands
            commands::list_sections,
            commands::update_section,
            commands::generate_section_image,
            commands::batch_generate_images,
            commands::bulk_generate_all_images,
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
            commands::open_youtube_create_channel,
            commands::import_channel_by_handle,
            commands::import_from_google_account,
            // Channel creation watcher commands
            commands::start_channel_creation_watcher,
            commands::inject_channel_handle,
            // Channel settings commands (AI translation, global settings, overrides)
            commands::get_global_channel_settings,
            commands::save_global_channel_settings,
            commands::translate_and_apply_settings,
            commands::get_channel_settings,
            commands::update_channel_overrides,
            commands::sync_channel_to_youtube,
            commands::ai_flavor_style,
            // Characters commands
            commands::list_characters,
            commands::create_character,
            commands::update_character,
            commands::delete_character,
            commands::generate_character_image,
            commands::vary_character_image,
            commands::generate_character_channel_variant,
            commands::select_character_variant,
            commands::discard_character_variant,
            commands::discard_all_character_variants,
            commands::propose_characters,
            commands::apply_character_to_sections,
            commands::detach_character_from_sections,
            commands::character_section_links,
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
            commands::compose_freeform,
            commands::mix_genres,
            commands::ai_translate_ui,
            commands::take_ai_notices,
            commands::suggest_transitions,
            // Style presets + per-channel sticky styles
            commands::list_style_presets,
            commands::save_style_preset,
            commands::delete_style_preset,
            commands::get_channel_style,
            commands::set_channel_style,
            // Genre-mix presets
            commands::list_genre_presets,
            commands::save_genre_preset,
            commands::delete_genre_preset,
            commands::update_song_styles,
            // Transition presets + per-section transitions
            commands::list_transition_presets,
            commands::save_transition_preset,
            commands::delete_transition_preset,
            commands::set_section_transitions,
            // Embedded browser webview
            commands::webview_open,
            commands::webview_show_page,
            commands::webview_close_page,
            commands::webview_list_pages,
            commands::webview_set_rect,
            commands::webview_hide,
            commands::webview_navigate,
            commands::webview_eval,
            commands::webview_current_url,
            commands::webview_take_scrape,
            commands::webview_arm_download,
            commands::macro_start,
            commands::macro_stop,
            commands::macro_status,
            commands::webview_capture_suno_session,
            commands::webview_capture_mj_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
