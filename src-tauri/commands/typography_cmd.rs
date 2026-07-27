//! Setting words in type, as commands.
//!
//! The rendering and every rule live in `typography.rs`, tested without a window. These commands add
//! the two things that need the outside world: writing an SVG to disk, and rasterising it with
//! ImageMagick — which is already in `ALLOWED_TOOLS`, so nothing new is trusted here.

use crate::typography::{self, Bubble, Decoration, TextArt};
use serde_json::{json, Value};

type Res<T> = Result<T, String>;

/// Faces, layouts, bubble kinds and the licensing note.
#[tauri::command]
pub async fn typography_catalogue() -> Res<Value> {
    Ok(typography::catalogue())
}

/// Can this phrase survive that decoration method?
///
/// Answered before a product is created, because the alternative is finding out from a customer.
#[tauri::command]
pub async fn typography_fit(
    phrase: String, face: String, decoration: String, colours: usize, height: u32,
) -> Res<Value> {
    match typography::fit_for(&phrase, &face, Decoration::parse(&decoration), colours, height) {
        Ok(v) => Ok(v),
        Err(reason) => Ok(json!({ "ok": false, "refused": true, "reason": reason })),
    }
}

#[derive(serde::Deserialize)]
pub struct RenderRequest {
    #[serde(flatten)]
    pub art: TextArt,
    /// Rasterise as well, at the SVG's own size. Without this only the SVG is written.
    #[serde(default)]
    pub raster: bool,
    /// The decoration method the product uses, so the words are checked before they are drawn.
    #[serde(default)]
    pub decoration: Option<String>,
    #[serde(default)]
    pub colours: Option<usize>,
}

/// Render a phrase to SVG (and optionally to a transparent PNG).
///
/// The check comes first. Drawing something that cannot be printed and reporting it afterwards
/// wastes the user's time and teaches them to ignore the answer.
#[tauri::command]
pub async fn render_text_art(payload: RenderRequest) -> Res<Value> {
    if let Some(method) = payload.decoration.as_deref().filter(|s| !s.is_empty()) {
        let height = if payload.art.height == 0 { 4000 } else { payload.art.height };
        let face = if payload.art.face.is_empty() { "inter" } else { &payload.art.face };
        typography::fit_for(&payload.art.phrase, face, Decoration::parse(method),
                            payload.colours.unwrap_or(2), height)?;
    }
    let svg = typography::render(&payload.art)?;
    let svg_path = crate::local_media::store("typography", "svg", svg.as_bytes())
        .map_err(|e| format!("The SVG could not be saved: {e}"))?;

    let mut out = json!({
        "svg": svg,
        "svg_path": svg_path.to_string_lossy(),
        "svg_url": crate::local_media::url_for(&svg_path),
    });

    if payload.raster {
        match rasterise(&svg_path) {
            Ok(png) => {
                out["png_path"] = json!(png.to_string_lossy());
                out["png_url"] = json!(crate::local_media::url_for(&png));
            }
            // Not fatal: the SVG is the real artefact and can be rasterised elsewhere, including on
            // Modal from a phone that has no ImageMagick at all.
            Err(why) => { out["raster_error"] = json!(why); }
        }
    }
    Ok(out)
}

/// A speech bubble over a panel — as SVG for export, as HTML for the EPUB.
///
/// Both are returned because both are wanted: the EPUB wants selectable, translatable, reflowable
/// text, and an exported PNG wants pixels.
#[tauri::command]
pub async fn render_bubble(
    text: String, kind: String, width: u32, height: u32, anchor_x: f64, anchor_y: f64,
) -> Res<Value> {
    let bubble = Bubble::parse(&kind);
    let svg = typography::bubble_svg(&text, bubble, width.max(1), height.max(1), anchor_x, anchor_y)?;
    Ok(json!({
        "svg": svg,
        "html": typography::bubble_html(&text, bubble, anchor_x, anchor_y),
        "css": typography::BUBBLE_CSS,
    }))
}

/// Turn an SVG into a transparent PNG at its own size.
///
/// `magick`/`convert` only — the same two tools the shorts cutter already uses, and already in the
/// allowlist. `-background none` matters: without it ImageMagick fills white, and a white rectangle
/// prints as a white rectangle on the shirt.
fn rasterise(svg_path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let png = svg_path.with_extension("png");
    let tool = ["magick", "convert"].into_iter()
        .find(|t| which::which(t).is_ok())
        .ok_or("Neither `magick` nor `convert` is installed, so the SVG could not be rasterised. \
                The SVG itself is saved and can be converted anywhere.")?;
    let out = std::process::Command::new(tool)
        .args(["-background", "none", "-density", "300"])
        .arg(svg_path)
        .arg(&png)
        .output()
        .map_err(|e| format!("{tool} could not be run: {e}"))?;
    if !out.status.success() {
        return Err(format!("{tool} failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(png)
}
