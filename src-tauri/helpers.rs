use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

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
