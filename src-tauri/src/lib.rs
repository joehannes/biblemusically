// Module declarations. The app keeps backend modules at the src-tauri root,
// while Cargo's library entrypoint lives in src-tauri/src.
#[path = "../commands/mod.rs"]
pub mod commands;
#[path = "../epub.rs"]
pub mod epub;

// Desktop only: Android and iOS have no taskbar. The clipboard machinery it exposes is
// cross-platform and lives in commands/clipboard.rs.
#[cfg(desktop)]
#[path = "../tray.rs"]
pub mod tray;

#[path = "../comfy_registry.rs"]
pub mod comfy_registry;
#[path = "../deep_link.rs"]
pub mod deep_link;
#[path = "../git_api.rs"]
pub mod git_api;
#[path = "../helpers.rs"]
pub mod helpers;
#[path = "../image_apis.rs"]
pub mod image_apis;
#[path = "../imagery.rs"]
pub mod imagery;
#[path = "../local_media.rs"]
pub mod local_media;
#[path = "../jobs.rs"]
pub mod jobs;
#[path = "../models.rs"]
pub mod models;
#[path = "../paths.rs"]
pub mod paths;
#[path = "../apk_install.rs"]
pub mod apk_install;
#[path = "../ai_budget.rs"]
pub mod ai_budget;
#[path = "../idle_guard.rs"]
pub mod idle_guard;
#[path = "../mongo_import.rs"]
pub mod mongo_import;
#[path = "../project_sync.rs"]
pub mod project_sync;
#[path = "../state.rs"]
pub mod state;
#[path = "../store.rs"]
pub mod store;
#[path = "../typography.rs"]
pub mod typography;
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
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_deep_link::init())
        // Registers the Android class that hands a downloaded update to the system installer. A
        // no-op on desktop, where the OS package manager already does this.
        .plugin(apk_install::init())
        .setup(move |app| {
            use tauri::Manager;
            let handle = app.handle();

            // Persistence is plain JSON files (see store.rs) — there is no database server to
            // start any more. The old `mongod` sidecar is gone: it was a native x86_64 binary that
            // could never ship to Android, and it made the user's own data unreadable without it.
            let app_data = handle.path().app_data_dir().expect("failed to get app data dir");

            // Teach `paths` where this platform lets the app write, before the store below becomes
            // the first thing to ask. Mobile only: on desktop `dirs` already answers correctly, and
            // answering differently would strand the data every existing install already has.
            //
            // Android has no home directory to hang an XDG layout off, so an un-taught `dirs` sends
            // the store to `/.config` — unwritable, and startup treats that as fatal a few lines
            // below. This block is the whole reason the app opens on a phone at all; see paths.rs.
            #[cfg(mobile)]
            {
                let p = handle.path();
                paths::install(
                    p.app_config_dir().unwrap_or_else(|_| app_data.clone()),
                    p.app_cache_dir().unwrap_or_else(|_| app_data.join("cache")),
                    p.document_dir().unwrap_or_else(|_| app_data.join("Documents")),
                    p.download_dir().unwrap_or_else(|_| app_data.join("Downloads")),
                );
            }

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

            // A sign-in that finished in the system browser comes back here. Two deliveries on
            // purpose: the URL is held in `deep_link` where a poll can always find it, *and*
            // announced as an event. On Android the intent can arrive before the webview has
            // attached its listener, and an event fired at nobody is a sign-in that silently did
            // nothing — so the held value is the one that cannot be missed, and the event is only
            // the fast path.
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let handle_for_links = handle.clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        let Some(cb) = deep_link::parse(url.as_str()) else { continue };
                        let payload = serde_json::json!({
                            "grant": cb.is_grant(), "code": cb.code, "state": cb.state,
                            "error": cb.error, "message": cb.message(),
                        });
                        deep_link::remember(cb);
                        use tauri::Emitter;
                        let _ = handle_for_links.emit("bm:deep-link", payload);
                    }
                });
            }

            // Unlock the sealed project cache from the entitlement already on disk, before any
            // command can read a project. Blocking rather than spawned on purpose: a window that
            // opens and *then* becomes able to read the user's projects would show an empty app
            // for a moment and invite them to make a second copy of work they already have.
            tauri::async_runtime::block_on(commands::subscription::apply_cache_key(&app_state));
            
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

                    // Generated media that arrived as bytes rather than as a URL — ElevenLabs
                    // answers a generation request with the MP3 itself, and every stage downstream
                    // fetches with reqwest, which has no file:// scheme. See local_media.rs; the
                    // containment rule that keeps this from being a file-read primitive lives in
                    // `local_media::resolve` and is tested there.
                    let media_route = warp::get()
                        .and(warp::path("media"))
                        .and(warp::query::<std::collections::HashMap<String, String>>())
                        .and_then(|q: std::collections::HashMap<String, String>| async move {
                            let param = q.get("path").cloned().unwrap_or_default();
                            let Some(file) = local_media::resolve(&param, &local_media::media_root()) else {
                                return Ok::<_, std::convert::Infallible>(warp::reply::with_status(
                                    warp::reply::with_header(Vec::new(), "content-type", "text/plain"),
                                    warp::http::StatusCode::NOT_FOUND));
                            };
                            let ctype = local_media::content_type(&file);
                            match tokio::fs::read(&file).await {
                                Ok(bytes) => Ok(warp::reply::with_status(
                                    warp::reply::with_header(bytes, "content-type", ctype),
                                    warp::http::StatusCode::OK)),
                                Err(_) => Ok(warp::reply::with_status(
                                    warp::reply::with_header(Vec::new(), "content-type", "text/plain"),
                                    warp::http::StatusCode::NOT_FOUND)),
                            }
                        });

                    let cors = warp::cors().allow_any_origin().allow_methods(vec!["POST", "GET"]).allow_headers(vec!["content-type"]);
                    let route = suno_route.or(scrape_route).or(macro_step_route).or(macro_state_route)
                        .or(media_route).with(cors);

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

            // Free GPU hours are the scarcest thing this app spends. A Kaggle session holds its slot
            // whether or not anything is generating, so a server nobody stopped costs next week's
            // rendering. Every two minutes, end whatever has gone quiet — see idle_guard.rs for why
            // the rule is deliberately timid about it.
            {
                let idle_state = state_arc.clone();
                tauri::async_runtime::spawn(async move {
                    let mut tick = tokio::time::interval(std::time::Duration::from_secs(120));
                    tick.tick().await;
                    loop {
                        tick.tick().await;
                        idle_guard::sweep(&idle_state).await;
                    }
                });
            }

            // Workflow runs: the pipeline's sequencing lives here rather than in the /workflow page, so
            // switching project does not abandon a run half way through. Ten seconds, because a run is
            // mostly waiting for jobs and a step that has nothing to do should not sit for minutes.
            {
                let run_state = state_arc.clone();
                tauri::async_runtime::spawn(async move {
                    let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
                    loop {
                        tick.tick().await;
                        commands::workflow_run::tick(&run_state).await;
                    }
                });
            }

            // Check for FFmpeg and show a warning dialog if missing.
            //
            // Desktop only, and the gate is not about tidiness. A phone has no ffmpeg on PATH and no
            // package manager to suggest one with, so `which` always fails there and the advice
            // ("sudo apt install") is advice nobody can take. Worse, `blocking_show` parks the setup
            // hook until someone dismisses the dialog — and on Android setup runs before the activity
            // has a window to draw a dialog in. The app would not warn; it would hang, on every
            // single launch, with the same blank screen as the data-directory bug above.
            #[cfg(desktop)]
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
            // The OS taskbar icon and its clipboard menu. Desktop only — a phone has no tray, and the
            // same actions are in the app's own clipboard view on every platform. The watcher is
            // desktop-only for a second reason: the clipboard plugin cannot notify on change, so
            // history means polling, and polling from an app the OS suspends costs battery for nothing.
            #[cfg(desktop)]
            {
                if let Err(err) = tray::attach(&handle.clone()) {
                    eprintln!("tray icon unavailable: {err}");
                }
                tray::watch(&handle.clone());
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
            commands::list_storage_locations,
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
            commands::list_ai_providers,
            commands::ai_budget_report,
            commands::list_ai_models,
            commands::guide_proposal,
            commands::guide_templates,
            commands::get_workflow_state,
            commands::save_workflow_state,
            commands::reset_workflow_state,
            // Assistant voice (speech out, speech in)
            commands::list_assistant_voices,
            commands::tts_speak,
            commands::stt_transcribe,
            commands::guide_interpret,
            // Publicity: AI-authored articles and posts per platform, with their own covers
            commands::author_publicity_piece,
            commands::author_publicity_set,
            commands::generate_publicity_covers,
            commands::list_publicity_pieces,
            commands::update_publicity_piece,
            commands::delete_publicity_piece,
            // AI-authored browser macros for platforms with no usable posting API
            commands::list_cli_runners,
            commands::remote_exec,
            commands::build_short,
            // Whole-book compilations: chapter videos joined, with a timestamped tracklist
            commands::compilation_plan,
            commands::build_compilation,
            commands::list_compilations,
            commands::delete_compilation,
            // Music-only distribution: releases, artists, distributor matrix, export packages
            commands::list_distributors,
            commands::save_artist_profile,
            commands::list_artist_profiles,
            commands::plan_releases,
            commands::save_release,
            commands::list_releases,
            commands::delete_release,
            commands::export_release_package,
            commands::generate_release_cover,
            // Poetic graphic novels → EPUB 3 (the one ebook format that carries the song)
            commands::list_novel_styles,
            commands::author_edition,
            commands::list_editions,
            commands::update_edition,
            commands::delete_edition,
            commands::generate_edition_art,
            commands::collect_edition_art,
            commands::build_epub,
            commands::list_ebook_stores,
            commands::ebook_pricing,
            // Print on demand: Printify's real API, paced to its real limits
            commands::printify_storefronts,
            commands::printify_connect,
            commands::printify_catalog,
            commands::printify_blueprint_detail,
            commands::printify_pick_products,
            commands::printify_selection,
            commands::printify_make_products,
            commands::list_printify_products,
            // Clipboard: a mirrored system history, an append-only vault, and a paste queue
            commands::clipboard_sync,
            commands::clipboard_history,
            commands::clipboard_use,
            commands::clipboard_add,
            commands::clipboard_pop,
            commands::clipboard_pin,
            commands::clipboard_clear,
            commands::clipboard_queue_set,
            commands::clipboard_queue_next,
            commands::clipboard_queue_take,
            commands::clipboard_queue_status,
            // Autosave: staged on focus shift, committed on view change, pushed on request
            commands::stage_field,
            commands::autosave_commit,
            commands::autosave_status,
            commands::save_and_push,
            // Workflow runs: sequenced on the backend so a project switch cannot abandon one
            commands::start_workflow_run,
            commands::workflow_run_status,
            commands::set_workflow_run_status,
            // When a channel should publish: AI-researched per region, or measured, or chosen
            commands::suggest_publish_time,
            commands::set_publish_time,
            commands::list_publish_times,
            commands::channels_missing_publish_time,
            // Guides for getting tokens and permissions out of other people's dashboards
            commands::list_access_guides,
            commands::access_guide,
            commands::set_access_step,
            commands::save_access_capture,
            // Posting macros: record once with prepared values, replay per song
            commands::list_post_recipes,
            commands::prepare_post_macro,
            commands::link_post_macro,
            commands::unlink_post_macro,
            // Users, shared projects, roles, merging and rewinding
            commands::sign_in,
            commands::current_user,
            commands::sign_out,
            commands::add_project_member,
            commands::list_project_members,
            commands::remove_project_member,
            commands::my_project_role,
            commands::share_project,
            commands::accept_invite,
            commands::team_pull,
            commands::resolve_conflict,
            commands::finish_merge,
            commands::project_history,
            commands::rewind_project,
            // Suno without a browser: cookie → JWT over plain HTTP, wrapper as the fallback
            commands::suno_generate,
            commands::suno_poll,
            commands::save_suno_cookie,
            commands::suno_status,
            // Signing in from a phone: borrow the desktop's tokens, or its own deep-link client
            commands::platform_capabilities,
            commands::export_channel_tokens,
            commands::import_channel_tokens,
            commands::deeplink_setup_steps,
            // Accounts, trial, subscription — the gate is here, not in the interface
            commands::subs_sign_in,
            commands::subs_refresh,
            commands::subs_status,
            commands::subs_sign_out,
            commands::subs_pricing,
            commands::subs_terms,
            commands::subs_redeem,
            commands::subs_referral,
            commands::subs_can,
            // The devotional-imagery catalogue: models, destinations, packaged intents
            commands::imagery_catalogue,
            commands::compose_image_prompt,
            commands::imagery_text_allowed,
            commands::imagery_print_check,
            commands::save_imagery_choice,
            commands::take_deep_link,
            commands::submit_deep_link,
            // Words set in a real font, never generated — products and speech bubbles alike
            commands::typography_catalogue,
            commands::typography_fit,
            commands::render_text_art,
            commands::render_bubble,
            commands::subs_seal_projects,
            commands::subs_cache_state,
            commands::send_report,
            commands::track_events,
            commands::list_sessions,
            commands::end_session,
            commands::end_other_sessions,
            // Updates within a major version; upgrades announced, never applied
            commands::check_update,
            commands::dismiss_upgrade,
            commands::download_update,
            apk_install::update_install_state,
            apk_install::request_install_permission,
            apk_install::install_downloaded_update,
            commands::author_macro,
            commands::list_authored_macros,
            commands::delete_authored_macro,
            commands::setup_recommendation,
            // Remote rendering (ffmpeg + upload on somebody else's computer)
            commands::list_render_providers,
            commands::build_render_spec,
            commands::submit_remote_render,
            commands::list_render_jobs,
            commands::record_render_result,
            commands::write_render_workflow,
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
