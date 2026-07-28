//! Installing a downloaded update on Android.
//!
//! `download_update` in commands/updater.rs ends the same way on every platform: the file is on disk
//! and the operating system is asked to take it from there. On desktop that is a package manager. On
//! Android there was no second half at all — the APK landed in app-specific storage, which is not a
//! folder anybody can browse to, and nothing ever opened it. The Update button downloaded and then
//! silently did nothing.
//!
//! The Android half lives in Kotlin (gen/android/.../InstallPlugin.kt) because handing a file to the
//! system installer needs an Activity, a FileProvider URI and an intent — none of which exist in
//! Rust. This module is the bridge: it registers that class as a Tauri mobile plugin and exposes
//! three commands to the interface.
//!
//! Everything here is a no-op with an honest answer on desktop, rather than a compile-time absence,
//! so the frontend can ask "can this platform install its own updates?" without knowing where it is
//! running.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Runtime};

type Res<T> = Result<T, String>;

#[derive(Serialize)]
struct PathArg {
    path: String,
}

#[derive(Deserialize, Default)]
struct CanInstall {
    #[serde(default)]
    allowed: bool,
    #[serde(default, rename = "needsSettings")]
    needs_settings: bool,
}

#[derive(Deserialize, Default)]
struct Opened {
    #[serde(default)]
    opened: bool,
}

#[derive(Deserialize, Default)]
struct HandedOff {
    #[serde(default, rename = "handedOff")]
    handed_off: bool,
}

#[cfg(target_os = "android")]
struct Installer<R: Runtime>(tauri::plugin::PluginHandle<R>);

/// Register the Android side. Harmless everywhere else.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("apkinstall")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                // The identifier is the Java package and the second argument the class inside it;
                // together they become studio/lightkid/app/InstallPlugin, which is why the class must
                // keep that exact name and survive R8 (see proguard-rules.pro).
                let handle = _api.register_android_plugin("studio.lightkid.app", "InstallPlugin")?;
                _app.manage(Installer(handle));
            }
            Ok(())
        })
        .build()
}

/// Can this platform install its own updates, and is anything in the way?
///
/// `supported` is about the platform, `allowed` about the user's decision — the interface needs both
/// to know whether to offer an Install button, a "let me allow it" button, or neither.
#[tauri::command]
pub async fn update_install_state<R: Runtime>(app: AppHandle<R>) -> Res<Value> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let installer = app.state::<Installer<R>>();
        let r: CanInstall = installer
            .0
            .run_mobile_plugin("canInstall", ())
            .map_err(|e| e.to_string())?;
        return Ok(json!({
            "supported": true,
            "allowed": r.allowed,
            "needs_settings": r.needs_settings,
        }));
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        // Desktop installs through the OS package manager, which needs no permission from us.
        Ok(json!({ "supported": false, "allowed": true, "needs_settings": false }))
    }
}

/// Send the user to the one Settings screen that can grant "install unknown apps".
///
/// Android returns no result for this, so the caller must re-ask `update_install_state` afterwards
/// rather than treat a successful return as consent.
#[tauri::command]
pub async fn request_install_permission<R: Runtime>(app: AppHandle<R>) -> Res<Value> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let installer = app.state::<Installer<R>>();
        let r: Opened = installer
            .0
            .run_mobile_plugin("requestInstallPermission", ())
            .map_err(|e| e.to_string())?;
        return Ok(json!({ "opened": r.opened }));
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(json!({ "opened": false }))
    }
}

/// Hand an already-downloaded APK to the system installer.
#[tauri::command]
pub async fn install_downloaded_update<R: Runtime>(app: AppHandle<R>, path: String) -> Res<Value> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let installer = app.state::<Installer<R>>();
        let r: HandedOff = installer
            .0
            .run_mobile_plugin("installApk", PathArg { path })
            .map_err(|e| e.to_string())?;
        return Ok(json!({ "handed_off": r.handed_off }));
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, path);
        Err("This platform installs updates through its own package manager.".to_string())
    }
}
