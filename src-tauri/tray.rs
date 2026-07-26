// ────────────────────────────────────────────────────────────────
// The OS taskbar icon
//
// Desktop only, and gated at the call site rather than here — Android and iOS have no tray, and the
// same actions live in the in-app clipboard view on every platform. The clipboard machinery itself is
// cross-platform (see `commands/clipboard.rs`); this file is only the menu on top of it.
//
// The menu is rebuilt whenever the history changes, because a tray menu in Tauri 2 is a static
// structure handed to the OS: there is no "about to open" hook to populate it lazily. Rebuilding is
// cheap (a dozen text items) and it is the only way the entries stay current.
//
// Deliberately small: the last ten entries, the two clipboard actions that are useful without looking
// at anything, and the app itself. Anything that needs to be read before it is clicked belongs in the
// window, not in a menu that closes on the first click.
// ────────────────────────────────────────────────────────────────

use crate::state::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

/// How many clipboard entries the menu shows. Ten is about what fits before a tray menu starts
/// scrolling on a laptop screen.
const MENU_ENTRIES: usize = 10;

/// Menu ids are parsed on click, so they carry the entry id after a prefix.
const CLIP_PREFIX: &str = "clip:";

/// Build (or rebuild) the tray menu from the current clipboard history.
pub async fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let entries = {
        let state = app.state::<AppState>();
        crate::commands::clipboard::tray_entries(&state, MENU_ENTRIES).await
    };

    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "open", "Open Studio", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // The clipboard section. Each entry copies itself back when clicked, which is the one action a
    // person wants from a clipboard menu.
    if entries.is_empty() {
        menu.append(&MenuItem::with_id(app, "clip:none", "Clipboard history is empty", false, None::<&str>)?)?;
    } else {
        let clip = Submenu::new(app, "Clipboard", true)?;
        for (id, preview) in &entries {
            clip.append(&MenuItem::with_id(
                app, format!("{CLIP_PREFIX}{id}"),
                // A leading space keeps the OS from treating a leading & or _ as a mnemonic.
                format!(" {preview}"), true, None::<&str>,
            )?)?;
        }
        clip.append(&PredefinedMenuItem::separator(app)?)?;
        clip.append(&MenuItem::with_id(app, "clip_pop", "Pop the top entry", true, None::<&str>)?)?;
        clip.append(&MenuItem::with_id(app, "clip_clear", "Clear history (keeps pinned)", true, None::<&str>)?)?;
        menu.append(&clip)?;
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "sync", "Capture clipboard now", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
}

/// Attach the tray icon. Call once, from `setup`, behind `#[cfg(desktop)]`.
pub fn attach(app: &AppHandle) -> tauri::Result<()> {
    let handle = app.clone();
    let menu = tauri::async_runtime::block_on(build_menu(app))?;

    TrayIconBuilder::with_id("main")
        .tooltip("AI Music Video Studio")
        .icon(app.default_window_icon().cloned().ok_or_else(||
            tauri::Error::AssetNotFound("default window icon".into()))?)
        .menu(&menu)
        // Left-clicking the icon should show the window; that is what every other tray app does, and
        // `show_menu_on_left_click(false)` is what leaves the left button free for it.
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            let app = app.clone();
            let id = event.id().0.clone();
            tauri::async_runtime::spawn(async move { on_menu(app, id).await; });
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { .. } = event {
                if let Some(win) = tray.app_handle().get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .build(&handle)?;
    Ok(())
}

async fn on_menu(app: AppHandle, id: String) {
    match id.as_str() {
        "open" => {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }
        "quit" => app.exit(0),
        "sync" => { let _ = crate::commands::clipboard::clipboard_sync(app.clone()).await; }
        "clip_pop" => { let _ = crate::commands::clipboard::clipboard_pop(app.clone(), None).await; }
        "clip_clear" => {
            let state = app.state::<AppState>();
            let _ = crate::commands::clipboard::clipboard_clear(state, None).await;
        }
        other if other.starts_with(CLIP_PREFIX) => {
            let entry = other.trim_start_matches(CLIP_PREFIX).to_string();
            if entry != "none" {
                let _ = crate::commands::clipboard::clipboard_use(app.clone(), entry).await;
            }
        }
        _ => {}
    }
    // Whatever just happened changed the history, so the menu is now stale.
    refresh(&app).await;
}

/// Rebuild the menu in place. Cheap, and the only way the entries stay current.
pub async fn refresh(app: &AppHandle) {
    if let Ok(menu) = build_menu(app).await {
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

/// Watch the OS clipboard and record what changes.
///
/// Desktop only, and for a reason: the clipboard plugin has no change notification, so history means
/// polling — and polling from a mobile app that the OS suspends is unreliable and costs battery for
/// nothing. On mobile the history records when the app asks (`clipboard_sync`), which is the moment it
/// is about to be used anyway.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            // Slow enough to be free, fast enough that a copy is in the menu before the hand reaches it.
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            match crate::commands::clipboard::clipboard_sync(app.clone()).await {
                Ok(r) if r["recorded"].as_bool() == Some(true) => refresh(&app).await,
                _ => {}
            }
        }
    });
}
