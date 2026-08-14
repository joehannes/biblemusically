use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

// ────────────────────────────────────────────────────────────────
// Resource file locator (shared by settings.rs and channels.rs)
// ────────────────────────────────────────────────────────────────

/// Locate a resource file (e.g. a Node.js script) by searching
/// common paths: TAURI_RESOURCE_DIR, dev tree, Tauri 2 bundle
/// layouts, deb/snap/flatpak installation paths.
pub fn locate_resource_file(name: &str) -> Option<PathBuf> {
    // 1. TAURI_RESOURCE_DIR env var (used in dev / older Tauri versions)
    if let Ok(rd) = env::var("TAURI_RESOURCE_DIR") {
        let p = PathBuf::from(&rd).join(name);
        if p.exists() { return Some(p); }
        let p2 = PathBuf::from(&rd).join("packaging").join(name);
        if p2.exists() { return Some(p2); }
    }

    // 2. Current working directory (dev mode from project root)
    if let Ok(cwd) = env::current_dir() {
        let p = cwd.join("src-tauri").join("packaging").join(name);
        if p.exists() { return Some(p); }
        let p2 = cwd.join("src-tauri").join(name);
        if p2.exists() { return Some(p2); }
    }

    // 3. Walk up from executable parent directories (Tauri 2 bundle layouts)
    if let Ok(exe) = env::current_exe() {
        let exe_name = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let mut path = exe.parent();
        while let Some(dir) = path {
            let dir_str = dir.to_string_lossy();

            // At /usr/bin/ level (binary installed to /usr/bin/), check /usr/lib/<appname>/
            if dir_str == "/usr/bin" || dir_str == "/usr/local/bin" {
                for lib_subdir in [&exe_name, "AI Music Video Studio"] {
                    let lib_root = PathBuf::from("/usr/lib").join(lib_subdir);
                    let checks = [
                        lib_root.join("_up_").join("src-tauri").join("packaging").join(name),
                        lib_root.join("_up_").join("packaging").join(name),
                        lib_root.join("packaging").join(name),
                        lib_root.join("resources").join("packaging").join(name),
                        lib_root.join("resources").join(name),
                        lib_root.join(name),
                    ];
                    for p in checks.iter() {
                        if p.exists() { return Some(p.clone()); }
                    }
                }
            }

            // Direct checks at this directory level
            let checks = [
                dir.join("resources").join(name),
                dir.join(name),
                dir.join("packaging").join(name),
                dir.join("_up_").join("src-tauri").join("packaging").join(name),
                dir.join("_up_").join("packaging").join(name),
                dir.join("lib").join(name),
                dir.join("share").join(name),
            ];
            for p in checks.iter() {
                if p.exists() { return Some(p.clone()); }
            }
            // src-tauri/packaging/ one level up
            if let Some(parent) = dir.parent() {
                let p2 = parent.join("src-tauri").join("packaging").join(name);
                if p2.exists() { return Some(p2); }
            }
            path = dir.parent();
        }
    }

    // 4. Common absolute paths for installed Tauri 2 deb packages (Linux)
    let app_name = "AI Music Video Studio";
    let deb_candidates = [
        format!("/usr/lib/{}/resources/{}", app_name, name),
        format!("/usr/lib/{}/{}", app_name, name),
        format!("/usr/lib/{}/packaging/{}", app_name, name),
        format!("/usr/lib/{}/_up_/src-tauri/packaging/{}", app_name, name),
    ];
    for c in &deb_candidates {
        if std::path::Path::new(c).exists() {
            return Some(std::path::PathBuf::from(c));
        }
    }

    // 5. Try snap / flatpak paths
    let snap_candidates = [
        format!("/snap/{app_name}/current/usr/lib/{app_name}/resources/{name}"),
        format!("/snap/{app_name}/current/resources/{name}"),
    ];
    for c in &snap_candidates {
        let p = c.replace("{app_name}", app_name).replace("{name}", name);
        if std::path::Path::new(&p).exists() {
            return Some(std::path::PathBuf::from(p));
        }
    }

    None
}

// ────────────────────────────────────────────────────────────────
// Mood keywords DB
// ────────────────────────────────────────────────────────────────

pub static MOOD_KEYWORDS: Lazy<HashMap<&'static str, Vec<&'static str>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("celestial", vec!["serene", "ethereal", "ascending"]);
    m.insert("divine",    vec!["radiant", "majestic", "warm"]);
    m.insert("darkness",  vec!["tense", "low", "dramatic"]);
    m.insert("victory",   vec!["triumphant", "rising", "bright"]);
    m.insert("wilderness",vec!["sparse", "earthy", "raw"]);
    m.insert("wonder",    vec!["awe", "expanding", "soft"]);
    m.insert("grace",     vec!["tender", "warm", "intimate"]);
    m.insert("glory",     vec!["epic", "soaring", "powerful"]);
    m.insert("redemption",vec!["uplifting", "warm", "hopeful"]);
    m.insert("invitation",vec!["welcoming", "warm", "soft"]);
    m.insert("bridge",    vec!["expansive", "transcendent", "vast"]);
    m
});

// ────────────────────────────────────────────────────────────────
// Effect presets
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub moods: Vec<&'static str>,
    pub ff: &'static str,
}

pub static EFFECT_PRESETS: Lazy<Vec<EffectPreset>> = Lazy::new(|| vec![
    EffectPreset { id: "slow-zoom-in",   label: "Slow Zoom In",       moods: vec!["serene","warm","tender","soft"],           ff: "zoompan=z='min(zoom+0.0008,1.5)':d=125" },
    EffectPreset { id: "ken-burns",      label: "Ken Burns",           moods: vec!["ethereal","expansive","awe"],             ff: "zoompan=z='zoom+0.001':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':d=125" },
    EffectPreset { id: "dramatic-pan",   label: "Dramatic Pan",        moods: vec!["epic","soaring","powerful"],              ff: "zoompan=z='1.2':x='iw*0.3*on/125':d=125" },
    EffectPreset { id: "fade-cross",     label: "Cross Fade",          moods: vec!["transcendent","warm","soft"],             ff: "xfade=transition=fade:duration=0.8" },
    EffectPreset { id: "fade-dissolve",  label: "Dissolve",            moods: vec!["ethereal","soft"],                        ff: "xfade=transition=dissolve:duration=1.0" },
    EffectPreset { id: "glow-bloom",     label: "Glow Bloom",          moods: vec!["radiant","bright","divine"],              ff: "gblur=sigma=2,colorchannelmixer=aa=0.6" },
    EffectPreset { id: "color-warm",     label: "Warm Grade",          moods: vec!["warm","tender","intimate"],               ff: "colorbalance=rs=0.15:gs=0.05:bs=-0.1" },
    EffectPreset { id: "color-cool",     label: "Cool Grade",          moods: vec!["serene","celestial"],                     ff: "colorbalance=rs=-0.1:gs=0.0:bs=0.15" },
    EffectPreset { id: "vignette",       label: "Vignette",            moods: vec!["dramatic","tense","epic"],                ff: "vignette=PI/4" },
    EffectPreset { id: "film-grain",     label: "Film Grain",          moods: vec!["earthy","raw","intimate"],                ff: "noise=alls=8:allf=t" },
    EffectPreset { id: "light-leak",     label: "Light Leak",          moods: vec!["awe","hopeful","uplifting"],              ff: "overlay=format=auto" },
    EffectPreset { id: "shake-subtle",   label: "Subtle Shake",        moods: vec!["powerful","rising","triumphant"],         ff: "crop=iw:ih:0:0" },
    EffectPreset { id: "flash-cut",      label: "Flash Cut",           moods: vec!["dramatic","rising"],                     ff: "fade=t=in:st=0:d=0.1" },
    EffectPreset { id: "slow-fade-out",  label: "Slow Fade Out",       moods: vec!["transcendent","soft"],                   ff: "fade=t=out:st=4:d=1" },
    EffectPreset { id: "particles",      label: "Particles Overlay",   moods: vec!["ethereal","celestial","divine"],          ff: "overlay=format=auto" },
    EffectPreset { id: "godrays",        label: "God Rays",            moods: vec!["radiant","majestic","divine"],            ff: "overlay=format=auto" },
]);

// ────────────────────────────────────────────────────────────────
// Annotation parsing
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AnnotationPair {
    pub line: String,
    pub image_prompt: String,
}

pub fn parse_annotations(annotations: &str, lyrics: &str) -> Vec<AnnotationPair> {
    let re = regex::Regex::new(r"^\[(.+)\]$").unwrap();
    let mut pairs: Vec<AnnotationPair> = Vec::new();
    let mut current_prompt = String::new();

    for ln in annotations.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        if let Some(cap) = re.captures(ln) {
            current_prompt = cap[1].trim().to_string();
        } else {
            pairs.push(AnnotationPair {
                line: ln.to_string(),
                image_prompt: std::mem::take(&mut current_prompt),
            });
        }
    }

    // Fallback: derive from lyrics if no annotations
    if pairs.is_empty() {
        for ln in lyrics.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            pairs.push(AnnotationPair {
                line: ln.to_string(),
                image_prompt: String::new(),
            });
        }
    }
    pairs
}

pub fn resolve_node_executable() -> Option<String> {
    // 1. Bundled / sidecar node binary next to the executable (Tauri 2 externalBin layout)
    //    Looks for "<exe_dir>/node" or "<lib_dir>/_up_/binaries/node" etc.
    if let Ok(exe) = env::current_exe() {
        let exe_name = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let mut candidates: Vec<PathBuf> = Vec::new();

        if let Some(parent) = exe.parent() {
            // Directly beside the binary
            candidates.push(parent.join("node"));
            candidates.push(parent.join("bin").join("node"));
            candidates.push(parent.join("node.exe"));
            candidates.push(parent.join("bin").join("node.exe"));

            // At /usr/bin/ level, check /usr/lib/<app>/_up_/binaries/node
            let dir_str = parent.to_string_lossy();
            if dir_str == "/usr/bin" || dir_str == "/usr/local/bin" {
                for subdir in [&exe_name, "AI Music Video Studio"] {
                    candidates.push(
                        PathBuf::from("/usr/lib").join(subdir).join("_up_").join("binaries").join("node")
                    );
                    candidates.push(
                        PathBuf::from("/usr/lib").join(subdir).join("_up_").join("binaries").join("bin").join("node")
                    );
                    candidates.push(
                        PathBuf::from("/usr/lib").join(subdir).join("_up_").join("src-tauri").join("packaging").join("node")
                    );
                    candidates.push(
                        PathBuf::from("/usr/lib").join(subdir).join("_up_").join("src-tauri").join("packaging").join("bin").join("node")
                    );
                    // Check resources/ dir
                    candidates.push(
                        PathBuf::from("/usr/lib").join(subdir).join("node")
                    );
                }
            }
        }

        // Walk up from exe parent, checking common resource locations
        let mut path = exe.parent();
        while let Some(dir) = path {
            candidates.push(dir.join("node"));
            candidates.push(dir.join("bin").join("node"));
            candidates.push(dir.join("binaries").join("node"));
            candidates.push(dir.join("binaries").join("bin").join("node"));
            candidates.push(dir.join("_up_").join("binaries").join("node"));
            candidates.push(dir.join("_up_").join("binaries").join("bin").join("node"));
            candidates.push(dir.join("_up_").join("src-tauri").join("packaging").join("node"));
            candidates.push(dir.join("_up_").join("src-tauri").join("packaging").join("bin").join("node"));
            candidates.push(dir.join("resources").join("node"));
            candidates.push(dir.join("resources").join("bin").join("node"));
            candidates.push(dir.join("packaging").join("node"));
            candidates.push(dir.join("packaging").join("bin").join("node"));
            path = dir.parent();
        }

        for candidate in candidates {
            if candidate.exists() && candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    // 2. TAURI_RESOURCE_DIR env var (dev mode / older Tauri)
    if let Some(rd) = env::var_os("TAURI_RESOURCE_DIR") {
        let rd = PathBuf::from(rd);
        let candidates = [
            rd.join("node"),
            rd.join("bin").join("node"),
            rd.join("node.exe"),
            rd.join("bin").join("node.exe"),
            rd.join("_up_").join("binaries").join("node"),
            rd.join("_up_").join("src-tauri").join("packaging").join("node"),
            rd.join("resources").join("node"),
        ];
        for c in &candidates {
            if c.exists() && c.is_file() {
                return Some(c.to_string_lossy().to_string());
            }
        }
    }

    // 3. Source tree / development paths
    if let Ok(cwd) = env::current_dir() {
        let candidates = [
            cwd.join("src-tauri").join("packaging").join("node"),
            cwd.join("src-tauri").join("packaging").join("bin").join("node"),
            cwd.join("src-tauri").join("packaging").join("node.exe"),
            cwd.join("src-tauri").join("packaging").join("bin").join("node.exe"),
            cwd.join("binaries").join("node"),
            cwd.join("binaries").join("bin").join("node"),
        ];
        for c in &candidates {
            if c.exists() && c.is_file() {
                return Some(c.to_string_lossy().to_string());
            }
        }
    }

    // 4. System PATH (standard Node.js installation)
    if let Ok(path) = which::which("node") {
        return Some(path.to_string_lossy().to_string());
    }

    // 5. Common Linux paths
    for cand in ["/usr/bin/node", "/usr/local/bin/node", "/bin/node", "/snap/bin/node"] {
        let path = Path::new(cand);
        if path.exists() && path.is_file() {
            return Some(path.to_string_lossy().to_string());
        }
    }

    // 6. Windows common paths
    if cfg!(target_os = "windows") {
        for cand in [
            "C:\\Program Files\\nodejs\\node.exe",
            "C:\\Program Files (x86)\\nodejs\\node.exe",
        ] {
            let path = Path::new(cand);
            if path.exists() && path.is_file() {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }

    None
}

// ────────────────────────────────────────────────────────────────
// Mood derivation
// ────────────────────────────────────────────────────────────────

pub fn derive_mood(prompt: &str) -> &'static str {
    let p = prompt.to_lowercase();
    for (key, moods) in MOOD_KEYWORDS.iter() {
        if p.contains(*key) {
            return moods[0];
        }
    }
    if ["light", "radiant", "glow"].iter().any(|w| p.contains(w)) { return "radiant"; }
    if ["dark", "night", "shadow"].iter().any(|w| p.contains(w))  { return "dramatic"; }
    if ["heaven", "sky", "angel"].iter().any(|w| p.contains(w))   { return "ethereal"; }
    "warm"
}

// ────────────────────────────────────────────────────────────────
// Effect suggestion
// ────────────────────────────────────────────────────────────────

pub fn suggest_effects(mood: &str) -> Vec<String> {
    let matches: Vec<String> = EFFECT_PRESETS
        .iter()
        .filter(|e| e.moods.contains(&mood))
        .map(|e| e.id.to_string())
        .take(3)
        .collect();
    if matches.is_empty() {
        vec!["fade-cross".into(), "slow-zoom-in".into()]
    } else {
        matches
    }
}

// ────────────────────────────────────────────────────────────────
// OAuth client secret masking
// ────────────────────────────────────────────────────────────────

pub fn mask_secret(mut doc: serde_json::Value) -> serde_json::Value {
    if let Some(secret) = doc.get("client_secret").and_then(|v| v.as_str()) {
        if secret.len() > 4 {
            let masked = format!("{}{}", "•".repeat(8), &secret[secret.len()-4..]);
            doc["client_secret"] = serde_json::Value::String(masked);
        }
    }
    doc
}

// ────────────────────────────────────────────────────────────────
// Folder picking
// ────────────────────────────────────────────────────────────────

/// Ask the user to choose a destination folder.
///
/// `tauri-plugin-dialog` has no `blocking_pick_folder` on mobile — Android exposes documents through
/// the Storage Access Framework, not a filesystem path a picker could return — so mobile callers
/// get a clear error instead of the crate failing to compile. Desktop behaviour is unchanged.
#[cfg(desktop)]
pub fn pick_folder_blocking(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .blocking_pick_folder()
        .ok_or_else(|| "Cancelled".to_string())?
        .into_path()
        .map_err(|err| err.to_string())
}

#[cfg(not(desktop))]
pub fn pick_folder_blocking(_app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Err("Choosing a folder isn't available on mobile — files are written to the app's own storage \
         directory instead."
        .to_string())
}

/// Open a URL in whatever the platform considers the user's browser.
///
/// Not `open::that`. That crate shells out to xdg-open/open/start, none of which exist on Android —
/// so every "open the Kaggle settings page" and "open Google Cloud credentials" button returned an
/// OS error and opened nothing, which is exactly how it was reported from a phone.
///
/// The opener plugin's *standalone* `open_url` is the same trap: it wraps the same crate. Only
/// `OpenerExt` on an AppHandle reaches the mobile plugin, which fires a real ACTION_VIEW intent —
/// hence the handle argument, which is the whole reason this helper exists.
pub fn open_external<R: tauri::Runtime>(app: &tauri::AppHandle<R>, url: &str) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| format!("Could not open {url}: {err}"))
}

/// The ffmpeg (or ffprobe) binary to run, given whatever the settings hold.
///
/// The bug this replaces was a one-liner repeated in five places:
///
/// ```ignore
/// settings.get("ffmpeg_path").and_then(|v| v.as_str()).unwrap_or("ffmpeg")
/// ```
///
/// `ffmpeg_path` is not absent when nobody has set it — it is the empty string, because the
/// Settings form writes every field it renders. `as_str()` on `""` is `Some("")`, so `unwrap_or`
/// never fires, and the app tries to spawn a program named `""`. Every download, every conversion
/// and every render failed with an error about ffmpeg not starting, on machines with a perfectly
/// good ffmpeg on PATH.
///
/// So an empty or whitespace-only setting is treated as "not set", and PATH is consulted properly
/// rather than by handing a bare name to the OS and hoping the process inherited a useful PATH —
/// a desktop-launched app frequently has not.
pub fn resolve_media_binary(configured: Option<&str>, name: &str) -> String {
    if let Some(p) = configured.map(str::trim).filter(|p| !p.is_empty()) {
        return p.to_string();
    }
    which::which(name)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| name.to_string())
}

/// `resolve_media_binary` for a settings document, which is how every call site holds it.
pub fn ffmpeg_from(settings: &bson::Document) -> String {
    resolve_media_binary(settings.get("ffmpeg_path").and_then(|v| v.as_str()), "ffmpeg")
}

/// The ffprobe beside it. Same empty-string trap, same answer.
pub fn ffprobe_from(settings: &bson::Document) -> String {
    resolve_media_binary(settings.get("ffprobe_path").and_then(|v| v.as_str()), "ffprobe")
}

/// The same, for the merged settings the job runner carries as JSON rather than BSON.
pub fn ffmpeg_from_value(settings: &serde_json::Value) -> String {
    resolve_media_binary(settings.get("ffmpeg_path").and_then(|v| v.as_str()), "ffmpeg")
}

#[cfg(test)]
mod media_binary_tests {
    use super::resolve_media_binary;

    /// The empty string is what the Settings form actually stores, and it must not become the
    /// program name. This is the whole bug: `Some("")` beat `unwrap_or("ffmpeg")` and the app
    /// spawned `""`.
    #[test]
    fn an_empty_setting_is_not_a_program_name() {
        for configured in [Some(""), Some("   "), None] {
            let got = resolve_media_binary(configured, "ffmpeg");
            assert!(!got.trim().is_empty(),
                    "resolved to {got:?} for {configured:?} — spawning that fails with ENOENT");
            assert!(got.ends_with("ffmpeg"), "expected an ffmpeg path, got {got:?}");
        }
    }

    /// An explicit path is still obeyed — people do point this at a custom build.
    #[test]
    fn an_explicit_path_wins() {
        assert_eq!(resolve_media_binary(Some("/opt/ffmpeg/bin/ffmpeg"), "ffmpeg"),
                   "/opt/ffmpeg/bin/ffmpeg");
        assert_eq!(resolve_media_binary(Some("  /opt/ff  "), "ffmpeg"), "/opt/ff");
    }
}

/// Read a settings number that may have been stored as a string.
///
/// The Settings form's number inputs hand back `e.target.value`, which is a *string*, and that is
/// what reaches the store: `comfyui_steps: "40"`, not `40`. Every reader used `as_i64()` / `as_f64()`,
/// which return `None` for a string and therefore fell through to the default — so typing 45 steps
/// or a 1344-wide image changed the form, was saved correctly, and then generated at 30 steps and
/// 1024 wide with nothing reported anywhere. Observed after a look pack wrote 40 steps and the
/// stored document came back holding `'40'`.
///
/// Parsing here rather than coercing on save, deliberately: the store already contains years of
/// string-shaped numbers, so the reader has to tolerate them regardless.
pub fn setting_i64(settings: &serde_json::Value, key: &str, default: i64) -> i64 {
    match settings.get(key) {
        Some(v) if v.is_i64() || v.is_u64() => v.as_i64().unwrap_or(default),
        Some(v) if v.is_f64() => v.as_f64().map(|f| f.round() as i64).unwrap_or(default),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok()
            .map(|f| f.round() as i64).unwrap_or(default),
        _ => default,
    }
}

/// The same, for a fractional setting (CFG scale, IP-Adapter weight, transition seconds).
pub fn setting_f64(settings: &serde_json::Value, key: &str, default: f64) -> f64 {
    match settings.get(key) {
        Some(v) if v.is_number() => v.as_f64().unwrap_or(default),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().unwrap_or(default),
        _ => default,
    }
}

#[cfg(test)]
mod setting_number_tests {
    use super::{setting_f64, setting_i64};
    use serde_json::json;

    /// A number typed into the form arrives as a string, and must still be the number.
    #[test]
    fn string_shaped_numbers_are_read_as_numbers() {
        let s = json!({
            "comfyui_steps": "40", "comfyui_cfg": "7", "comfyui_width": "1152",
            "comfyui_ip_weight": "0.65",
        });
        assert_eq!(setting_i64(&s, "comfyui_steps", 30), 40);
        assert_eq!(setting_i64(&s, "comfyui_width", 1024), 1152);
        assert_eq!(setting_f64(&s, "comfyui_cfg", 6.5), 7.0);
        assert_eq!(setting_f64(&s, "comfyui_ip_weight", 0.7), 0.65);
    }

    /// Real numbers keep working — this is a widening, not a replacement.
    #[test]
    fn real_numbers_still_work() {
        let s = json!({ "comfyui_steps": 45, "comfyui_cfg": 7.5 });
        assert_eq!(setting_i64(&s, "comfyui_steps", 30), 45);
        assert_eq!(setting_f64(&s, "comfyui_cfg", 6.5), 7.5);
    }

    /// Absent, empty and unparseable all fall back rather than producing a zero, because a zero
    /// here is a black image or a divide-by-nothing rather than a visible error.
    #[test]
    fn junk_falls_back_to_the_default_not_to_zero() {
        let s = json!({ "comfyui_steps": "", "comfyui_cfg": "abc", "comfyui_width": null });
        assert_eq!(setting_i64(&s, "comfyui_steps", 30), 30);
        assert_eq!(setting_f64(&s, "comfyui_cfg", 6.5), 6.5);
        assert_eq!(setting_i64(&s, "comfyui_width", 1024), 1024);
        assert_eq!(setting_i64(&s, "missing_entirely", 7), 7);
    }
}

/// Choose a target song length inside a configured range.
///
/// A single fixed number made every track in a series exactly the same length, which is both
/// audibly monotonous and a poor fit for the material: a psalm of six lines and one of forty do not
/// want the same four minutes. The setting is now a range, and the length is placed inside it
/// according to how much there is to sing.
///
/// Two properties matter more than the exact curve:
///
///   * it is **stable per song** — the same song asks for the same length on every retry, so a
///     regenerated track is comparable with the one before it. A random pick would make each retry
///     a different song, which is indistinguishable from the engine behaving inconsistently.
///   * it **degrades to the old behaviour** — an unset or inverted range collapses to the single
///     value, so nothing changes for anyone who has not set one.
///
/// The result is a *request*. Engines are not obliged to honour it: HeartMuLa has been observed
/// returning 120 s for a 500 s ask, which is why analysis measures the audio rather than trusting
/// this number.
pub fn pick_duration(min_s: f64, max_s: f64, lyrics: &str, song_id: &str) -> f64 {
    let lo = min_s.max(10.0);
    let hi = max_s.max(lo);
    if hi - lo < 1.0 { return lo.clamp(10.0, 600.0); }

    // How much there is to sing. Non-empty lines rather than characters: a line is roughly a
    // musical phrase, while character count mostly measures how wordy the translation is.
    let lines = lyrics.lines().filter(|l| !l.trim().is_empty()).count();
    // 8 lines or fewer sits at the bottom of the range, 64 or more at the top, linear between.
    let fill = ((lines.clamp(8, 64) - 8) as f64) / 56.0;

    // A little jitter so a batch of similarly-sized lyrics does not come out identical, derived
    // from the song id so it is the same on every run for the same song.
    let hash = song_id.bytes().fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64));
    let jitter = ((hash % 1000) as f64 / 1000.0 - 0.5) * 0.12; // ±6% of the range

    (lo + (hi - lo) * (fill + jitter).clamp(0.0, 1.0)).clamp(10.0, 600.0)
}

#[cfg(test)]
mod pick_duration_tests {
    use super::pick_duration;

    fn lyrics(n: usize) -> String { vec!["a line"; n].join("\n") }

    #[test]
    fn the_result_stays_inside_the_range() {
        for n in [0, 1, 8, 20, 64, 200] {
            let d = pick_duration(400.0, 600.0, &lyrics(n), "song-1");
            assert!((400.0..=600.0).contains(&d), "{n} lines produced {d}s, outside 400-600");
        }
    }

    /// More to sing, more time to sing it — the whole point of a range.
    #[test]
    fn longer_lyrics_ask_for_a_longer_song() {
        let short = pick_duration(400.0, 600.0, &lyrics(8), "same-song");
        let long = pick_duration(400.0, 600.0, &lyrics(64), "same-song");
        assert!(long > short + 50.0, "8 lines -> {short}s but 64 lines -> {long}s");
    }

    /// A retry must ask for the same length, or two takes of one song are not comparable.
    #[test]
    fn the_same_song_always_asks_for_the_same_length() {
        let a = pick_duration(400.0, 600.0, &lyrics(30), "song-abc");
        let b = pick_duration(400.0, 600.0, &lyrics(30), "song-abc");
        assert_eq!(a, b);
        // ...and different songs of the same size are not all identical.
        let c = pick_duration(400.0, 600.0, &lyrics(30), "song-xyz");
        assert!((a - c).abs() > 0.01, "two different songs both got exactly {a}s");
    }

    /// No range set, or one entered backwards, behaves like the single value it replaces.
    #[test]
    fn a_missing_or_inverted_range_collapses_to_one_value() {
        assert_eq!(pick_duration(240.0, 240.0, &lyrics(30), "s"), 240.0);
        assert_eq!(pick_duration(240.0, 0.0, &lyrics(30), "s"), 240.0);
        // And the hard bounds still hold.
        assert_eq!(pick_duration(1.0, 1.0, &lyrics(30), "s"), 10.0);
        assert!(pick_duration(500.0, 9000.0, &lyrics(64), "s") <= 600.0);
    }
}
