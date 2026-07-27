//! Words, set in a real font — never generated.
//!
//! One mechanism serves the Printify phrases and the graphic-novel speech bubbles, because it is the
//! same problem twice: text that has to be correct, editable and sharp at print size.
//!
//! # Why not an image model, even a good one
//!
//! Ideogram, Recraft and Qwen-Image are genuinely good at text — and "good" means *occasional*
//! letter errors. On a mug that is a returned order rather than a retry, and at fifty products a day
//! nobody proofreads every one. It compounds with resolution: generated images are 1024–2048px
//! against print areas of 3000–4000px, and upscaled letterforms go soft in a way that reads as cheap
//! where soft scenery does not. And the phrase can never be edited afterwards without regenerating
//! the artwork.
//!
//! So: **AI makes the artwork; typography lays the words on top.** That is how merch and comics are
//! actually made, and it is already how this app draws the link card on shorts.
//!
//! # SVG, rasterised once
//!
//! Everything here produces SVG and is rasterised once at the exact target size — real vector layout,
//! text-on-path for curves, and no resampling of letterforms. `magick`/`convert` are already in
//! `ALLOWED_TOOLS` (see `commands/remote_exec.rs`), so this runs locally on a desktop and on Modal
//! from a phone with no new machinery.
//!
//! # The decoration method changes the design, and Printify tells us which
//!
//! The variants response carries `decoration_method` — `dtg`, `dtf`, `embroidery`. Embroidery cannot
//! do gradients, fine detail, or many colours; a design that looks lovely on DTF becomes an
//! unrecognisable blob stitched. So [`fit_for`] **refuses** rather than warning when a phrase cannot
//! survive the process it is going to.
//!
//! # Fonts, and the part that can actually cost money
//!
//! Google Fonts are almost entirely SIL OFL or Apache 2.0, and both permit commercial use including
//! print on merchandise — that is the safe pool. Selling a product with text *rendered* in a font is
//! not distributing the font and is always fine; bundling the font file **is** distribution, which
//! OFL permits as long as the licence ships alongside. What is not safe is an arbitrary font file
//! from a "free fonts" site: many mean free for *personal* use, and merchandise is precisely what
//! they exclude. [`font_warning`] exists so that is said out loud rather than discovered in a
//! takedown.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ────────────────────────────────────────────────────────────────────────────
// Fonts
// ────────────────────────────────────────────────────────────────────────────

/// A face this app is willing to put on a product, with the licence that makes that safe.
#[derive(Debug, Clone, Serialize)]
pub struct Face {
    pub id: &'static str,
    pub family: &'static str,
    pub licence: &'static str,
    /// What it is for, in an artist's terms.
    pub use_for: &'static str,
    /// Lettering faces are all-caps by convention in comics; body faces are not.
    pub caps_by_convention: bool,
    /// Hairline serifs disappear at small sizes on fabric.
    pub hairline: bool,
}

pub const FACES: &[Face] = &[
    Face { id: "inter", family: "Inter", licence: "SIL OFL 1.1",
           use_for: "Clean and neutral. The default when the words should not have an opinion.",
           caps_by_convention: false, hairline: false },
    Face { id: "playfair", family: "Playfair Display", licence: "SIL OFL 1.1",
           use_for: "High-contrast serif with real elegance. Beautiful large, invisible small.",
           caps_by_convention: false, hairline: true },
    Face { id: "cormorant", family: "Cormorant Garamond", licence: "SIL OFL 1.1",
           use_for: "Devotional and old-world. The closest thing here to a printed prayer book.",
           caps_by_convention: false, hairline: true },
    Face { id: "bebas", family: "Bebas Neue", licence: "SIL OFL 1.1",
           use_for: "Tall condensed capitals. Survives embroidery better than anything else here.",
           caps_by_convention: true, hairline: false },
    Face { id: "comic_neue", family: "Comic Neue", licence: "SIL OFL 1.1",
           use_for: "Speech bubbles and captions — a lettering face rather than a body face.",
           caps_by_convention: true, hairline: false },
    Face { id: "lora", family: "Lora", licence: "SIL OFL 1.1",
           use_for: "A readable serif for a verse and its reference together.",
           caps_by_convention: false, hairline: false },
    Face { id: "montserrat", family: "Montserrat", licence: "SIL OFL 1.1",
           use_for: "Geometric and confident. Good for a single big word.",
           caps_by_convention: false, hairline: false },
];

pub fn face(id: &str) -> Option<&'static Face> {
    FACES.iter().find(|f| f.id == id)
}

/// What to say when somebody points the app at a font file of their own.
///
/// Said before it is used, not after a takedown: "free font" on most sites means free for personal
/// use, and merchandise is exactly the case those licences carve out.
pub fn font_warning(path: &str) -> String {
    format!(
        "Using {path}. Check its licence permits *commercial* use before selling anything set in \
         it — many \"free font\" sites mean free for personal use only, and merchandise is exactly \
         what those licences exclude. Everything in the built-in list is SIL OFL or Apache 2.0, \
         which do permit it."
    )
}

// ────────────────────────────────────────────────────────────────────────────
// Decoration methods
// ────────────────────────────────────────────────────────────────────────────

/// What the printer can actually reproduce. Straight from Printify's `decoration_method`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decoration {
    /// Direct-to-garment: loses thin strokes and light-on-light contrast.
    Dtg,
    /// Direct-to-film: the most forgiving.
    Dtf,
    /// Thread. No gradients, few colours, nothing fine.
    Embroidery,
    /// Mugs, posters, anything hard — the least forgiving about resolution.
    HardSurface,
}

impl Decoration {
    pub fn parse(raw: &str) -> Decoration {
        match raw.trim().to_ascii_lowercase().as_str() {
            "embroidery" => Decoration::Embroidery,
            "dtf" => Decoration::Dtf,
            "sublimation" | "hard" | "mug" => Decoration::HardSurface,
            _ => Decoration::Dtg,
        }
    }

    /// The thinnest stroke that survives, as a fraction of the design's height.
    fn min_stroke_ratio(self) -> f64 {
        match self {
            Decoration::Embroidery => 0.012,
            Decoration::Dtg => 0.004,
            Decoration::Dtf => 0.002,
            Decoration::HardSurface => 0.002,
        }
    }

    /// How many colours it can hold before the result stops resembling the design.
    pub fn colour_cap(self) -> usize {
        match self {
            Decoration::Embroidery => 4,
            Decoration::Dtg => 16,
            _ => 64,
        }
    }

    pub fn min_dpi(self) -> u32 {
        match self {
            // A mug shows every soft edge a shirt hides.
            Decoration::HardSurface => 300,
            _ => 150,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Decoration::Dtg => "direct-to-garment printing",
            Decoration::Dtf => "direct-to-film printing",
            Decoration::Embroidery => "embroidery",
            Decoration::HardSurface => "hard-surface printing",
        }
    }
}

/// Can this phrase survive this process — and if not, why not?
///
/// A refusal rather than a warning, on purpose. A warning at fifty products a day is a thing nobody
/// reads; an unrecognisable stitched blob is a refund and a bad review.
pub fn fit_for(phrase: &str, face_id: &str, dec: Decoration, colours: usize, height_px: u32)
    -> Result<Value, String>
{
    let f = face(face_id).ok_or_else(|| format!("There is no font called {face_id:?}."))?;
    let words = phrase.split_whitespace().count();

    if colours > dec.colour_cap() {
        return Err(format!(
            "{} colours is more than {} can hold — {} at most, or the design stops resembling \
             itself. Reduce it to flat shapes before ordering.",
            colours, dec.label(), dec.colour_cap()
        ));
    }
    if dec == Decoration::Embroidery && f.hairline {
        return Err(format!(
            "{} has hairline strokes that embroidery cannot stitch — they vanish or blob. \
             Bebas Neue or Montserrat hold up.",
            f.family
        ));
    }
    if dec == Decoration::Embroidery && words > 6 {
        return Err(format!(
            "{words} words is too many to embroider legibly. Six or fewer, or move this to a \
             printed product."
        ));
    }
    // The stroke a face this size would actually produce, against what the process can hold.
    let stroke = (height_px as f64 / (words.max(1) as f64 * 9.0)) * 0.08;
    let needed = dec.min_stroke_ratio() * height_px as f64;
    if stroke < needed {
        return Err(format!(
            "At this size the letters come out about {stroke:.1} pixels thick and {} needs at \
             least {needed:.1}. Fewer words, or a bigger print area.",
            dec.label()
        ));
    }
    Ok(json!({ "ok": true, "words": words, "stroke_px": (stroke * 10.0).round() / 10.0,
               "colour_cap": dec.colour_cap(), "min_dpi": dec.min_dpi() }))
}

// ────────────────────────────────────────────────────────────────────────────
// Layout
// ────────────────────────────────────────────────────────────────────────────

/// How the words are arranged. Typography without layout looks like a document.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    StackedCentred,
    BigWordSubline,
    TwoLinesWithRule,
    Arc,
    CircularBadge,
    LeftBlock,
    /// "Be still and know" / *Psalm 46:10* — the shape most of this app's phrases actually want.
    VerseAndReference,
}

impl Layout {
    pub fn parse(raw: &str) -> Layout {
        match raw.trim().to_ascii_lowercase().as_str() {
            "big_word_subline" => Layout::BigWordSubline,
            "two_lines_with_rule" => Layout::TwoLinesWithRule,
            "arc" => Layout::Arc,
            "circular_badge" => Layout::CircularBadge,
            "left_block" => Layout::LeftBlock,
            "verse_and_reference" => Layout::VerseAndReference,
            _ => Layout::StackedCentred,
        }
    }
}

fn escape(raw: &str) -> String {
    raw.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Break a phrase into lines of roughly equal length, which reads better than filling greedily.
pub fn balance(phrase: &str, lines: usize) -> Vec<String> {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    if lines <= 1 || words.len() <= 1 { return vec![words.join(" ")]; }
    let per = (words.len() as f64 / lines as f64).ceil() as usize;
    words.chunks(per.max(1)).map(|c| c.join(" ")).collect()
}

/// What to draw, and where.
#[derive(Debug, Clone, Deserialize)]
pub struct TextArt {
    pub phrase: String,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub face: String,
    #[serde(default)]
    pub colour: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

/// Render the words as SVG, at the exact target size.
///
/// Transparent by default — a product design that arrives with a white rectangle behind it is a
/// white rectangle printed onto the shirt.
pub fn render(art: &TextArt) -> Result<String, String> {
    let phrase = art.phrase.trim();
    if phrase.is_empty() { return Err("There are no words to set.".into()); }
    let w = if art.width == 0 { 4000 } else { art.width };
    let h = if art.height == 0 { 4000 } else { art.height };
    let face_id = if art.face.is_empty() { "inter" } else { &art.face };
    let f = face(face_id).ok_or_else(|| format!("There is no font called {face_id:?}."))?;
    let colour = if art.colour.trim().is_empty() { "#111111" } else { art.colour.trim() };
    let layout = Layout::parse(&art.layout);

    // Safe margin: nothing important within 8% of an edge, because print areas are trimmed.
    let margin = (w.min(h) as f64 * 0.08).round();
    let inner_w = w as f64 - margin * 2.0;
    let cx = w as f64 / 2.0;

    let text = if f.caps_by_convention { phrase.to_uppercase() } else { phrase.to_string() };
    let body = match layout {
        Layout::StackedCentred => {
            let lines = balance(&text, ((text.split_whitespace().count() + 2) / 3).max(1));
            stacked(&lines, cx, h as f64 / 2.0, inner_w, colour, &f.family, "middle")
        }
        Layout::BigWordSubline => {
            let words: Vec<&str> = text.split_whitespace().collect();
            let (big, rest) = words.split_first().ok_or("no words")?;
            let size = fit_size(big, inner_w, 1);
            format!(
                "{}\n{}",
                line(big, cx, h as f64 * 0.45, size, colour, &f.family, "middle", 700),
                if rest.is_empty() { String::new() } else {
                    line(&rest.join(" "), cx, h as f64 * 0.45 + size * 0.75,
                         fit_size(&rest.join(" "), inner_w, 1) * 0.45, colour, &f.family, "middle", 400)
                }
            )
        }
        Layout::TwoLinesWithRule => {
            let lines = balance(&text, 2);
            let size = fit_size(lines.iter().max_by_key(|l| l.len()).unwrap(), inner_w, 1);
            let mid = h as f64 / 2.0;
            format!(
                "{}\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{colour}\"/>\n{}",
                line(&lines[0], cx, mid - size * 0.35, size, colour, &f.family, "middle", 700),
                cx - inner_w * 0.3, mid, inner_w * 0.6, (size * 0.05).max(2.0),
                line(lines.get(1).map(String::as_str).unwrap_or(""), cx, mid + size * 0.95,
                     size, colour, &f.family, "middle", 700),
            )
        }
        Layout::Arc => {
            let r = inner_w / 2.0;
            let size = fit_size(&text, inner_w * 1.4, 1);
            format!(
                "<defs><path id=\"arc\" d=\"M {} {} A {r} {r} 0 0 1 {} {}\" fill=\"none\"/></defs>\
                 <text font-family=\"{}\" font-size=\"{size}\" font-weight=\"700\" fill=\"{colour}\">\
                 <textPath href=\"#arc\" startOffset=\"50%\" text-anchor=\"middle\">{}</textPath></text>",
                cx - r, h as f64 * 0.62, cx + r, h as f64 * 0.62, f.family, escape(&text)
            )
        }
        Layout::CircularBadge => {
            let r = inner_w / 2.0;
            let size = fit_size(&text, inner_w, 2);
            format!(
                "<circle cx=\"{cx}\" cy=\"{}\" r=\"{r}\" fill=\"none\" stroke=\"{colour}\" \
                 stroke-width=\"{}\"/>\n{}",
                h as f64 / 2.0, (r * 0.04).max(3.0),
                stacked(&balance(&text, 2), cx, h as f64 / 2.0, inner_w * 0.8, colour, &f.family, "middle")
            )
        }
        Layout::LeftBlock => {
            let lines = balance(&text, ((text.split_whitespace().count() + 2) / 3).max(1));
            stacked(&lines, margin, h as f64 / 2.0, inner_w, colour, &f.family, "start")
        }
        Layout::VerseAndReference => {
            let lines = balance(&text, ((text.split_whitespace().count() + 3) / 4).max(1));
            let block = stacked(&lines, cx, h as f64 * 0.45, inner_w, colour, &f.family, "middle");
            let reference = art.reference.trim();
            if reference.is_empty() { block } else {
                // The reference is deliberately small and italic: it is a citation, not a headline.
                format!(
                    "{block}\n<text x=\"{cx}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" \
                     font-style=\"italic\" fill=\"{colour}\" text-anchor=\"middle\">{}</text>",
                    h as f64 * 0.45 + (lines.len() as f64 + 1.0) * fit_size(&lines[0], inner_w, 1) * 0.9,
                    f.family, fit_size(&lines[0], inner_w, 1) * 0.4, escape(reference)
                )
            }
        }
    };

    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\">\n\
         <!-- Set in {} ({}). Transparent by default: a white rectangle here prints as a white \
         rectangle on the product. -->\n{body}\n</svg>",
        f.family, f.licence
    ))
}

/// A font size that fits `text` into `width`, assuming roughly 0.55em average glyph advance.
fn fit_size(text: &str, width: f64, lines: usize) -> f64 {
    let chars = text.chars().count().max(1) as f64 / lines.max(1) as f64;
    (width / (chars * 0.55)).max(12.0)
}

fn line(text: &str, x: f64, y: f64, size: f64, colour: &str, family: &str, anchor: &str, weight: u32) -> String {
    format!(
        "<text x=\"{x:.1}\" y=\"{y:.1}\" font-family=\"{family}\" font-size=\"{size:.1}\" \
         font-weight=\"{weight}\" fill=\"{colour}\" text-anchor=\"{anchor}\">{}</text>",
        escape(text)
    )
}

fn stacked(lines: &[String], x: f64, centre_y: f64, width: f64, colour: &str, family: &str, anchor: &str) -> String {
    let longest = lines.iter().max_by_key(|l| l.chars().count()).cloned().unwrap_or_default();
    let size = fit_size(&longest, width, 1);
    let step = size * 1.15;
    let start = centre_y - (lines.len() as f64 - 1.0) * step / 2.0;
    lines.iter().enumerate()
        .map(|(i, l)| line(l, x, start + i as f64 * step, size, colour, family, anchor, 700))
        .collect::<Vec<_>>()
        .join("\n")
}

// ────────────────────────────────────────────────────────────────────────────
// Speech bubbles
// ────────────────────────────────────────────────────────────────────────────

/// The four shapes a comic actually uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bubble {
    Speech,
    Thought,
    Shout,
    /// A plain rectangle — narration or a caption box.
    Caption,
}

impl Bubble {
    pub fn parse(raw: &str) -> Bubble {
        match raw.trim().to_ascii_lowercase().as_str() {
            "thought" => Bubble::Thought,
            "shout" => Bubble::Shout,
            "caption" | "narration" => Bubble::Caption,
            _ => Bubble::Speech,
        }
    }
}

/// A bubble over a panel, as SVG — auto-sized to the words, with the tail pointing at the speaker.
///
/// `anchor_x`/`anchor_y` are fractions of the panel (0–1): where the speaker is. The bubble is
/// placed away from that point and the tail reaches back toward it, which is the whole reason a
/// bubble is composited *after* the art exists rather than baked into it — placement is a decision
/// you can only make once you can see the picture, and a bubble must not cover a face.
pub fn bubble_svg(text: &str, kind: Bubble, panel_w: u32, panel_h: u32,
                  anchor_x: f64, anchor_y: f64) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() { return Err("A bubble with no words is just a shape.".into()); }

    let w = panel_w as f64;
    let h = panel_h as f64;
    // Auto-size to the text: roughly 22 characters a line, and never wider than 70% of the panel.
    // Ceiling division, not `/22 + 1` — a four-word line does not need breaking, and breaking it
    // makes a bubble twice the height it should be, which is exactly what covers a face.
    let lines = balance(text, text.chars().count().div_ceil(22).max(1));
    let size = (w * 0.035).max(12.0);
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
    let box_w = (longest * size * 0.55 + size * 1.6).min(w * 0.7);
    let box_h = lines.len() as f64 * size * 1.25 + size * 1.2;

    // Placed opposite the speaker, vertically, so the tail has somewhere to travel and the face
    // stays uncovered.
    let bx = (w * anchor_x.clamp(0.1, 0.9) - box_w / 2.0).clamp(w * 0.02, w * 0.98 - box_w);
    let by = if anchor_y > 0.5 { h * 0.06 } else { h * 0.94 - box_h };
    let tail_y = if anchor_y > 0.5 { by + box_h } else { by };
    let tip_y = if anchor_y > 0.5 { (h * anchor_y).min(h * 0.9) } else { (h * anchor_y).max(h * 0.1) };
    let tail_x = bx + box_w * 0.5;
    let tip_x = w * anchor_x;

    let shape = match kind {
        Bubble::Speech => format!(
            "<rect x=\"{bx:.1}\" y=\"{by:.1}\" width=\"{box_w:.1}\" height=\"{box_h:.1}\" \
             rx=\"{:.1}\" fill=\"#ffffff\" stroke=\"#111111\" stroke-width=\"{:.1}\"/>\
             <polygon points=\"{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}\" fill=\"#ffffff\" \
             stroke=\"#111111\" stroke-width=\"{:.1}\"/>",
            box_h * 0.22, size * 0.12,
            tail_x - box_w * 0.09, tail_y, tail_x + box_w * 0.09, tail_y, tip_x, tip_y,
            size * 0.12),
        Bubble::Thought => {
            // A scalloped cloud, plus the trail of shrinking circles that says "thought".
            let bumps: String = (0..9).map(|i| {
                let t = i as f64 / 8.0;
                format!("<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{:.1}\" fill=\"#ffffff\" \
                         stroke=\"#111111\" stroke-width=\"{:.1}\"/>",
                        bx + t * box_w, by + (i % 2) as f64 * box_h * 0.12,
                        box_w * 0.09, size * 0.1)
            }).collect();
            format!(
                "{bumps}<rect x=\"{bx:.1}\" y=\"{:.1}\" width=\"{box_w:.1}\" height=\"{:.1}\" \
                 rx=\"{:.1}\" fill=\"#ffffff\" stroke=\"#111111\" stroke-width=\"{:.1}\"/>\
                 <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{:.1}\" fill=\"#ffffff\" stroke=\"#111111\" \
                 stroke-width=\"{:.1}\"/><circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{:.1}\" \
                 fill=\"#ffffff\" stroke=\"#111111\" stroke-width=\"{:.1}\"/>",
                by + box_h * 0.1, box_h * 0.85, box_h * 0.3, size * 0.1,
                tail_x, tail_y + box_h * 0.12, box_w * 0.05, size * 0.1,
                (tail_x + tip_x) / 2.0, (tail_y + tip_y) / 2.0, box_w * 0.03, size * 0.1)
        }
        Bubble::Shout => {
            // A jagged star — the outline says "shouted" before a single word is read.
            let pts: String = (0..20).map(|i| {
                let angle = i as f64 * std::f64::consts::PI / 10.0;
                let r = if i % 2 == 0 { 1.0 } else { 0.78 };
                format!("{:.1},{:.1} ", bx + box_w / 2.0 + angle.cos() * box_w / 2.0 * r,
                        by + box_h / 2.0 + angle.sin() * box_h / 2.0 * r * 1.35)
            }).collect();
            format!("<polygon points=\"{pts}\" fill=\"#ffffff\" stroke=\"#111111\" \
                     stroke-width=\"{:.1}\"/>", size * 0.14)
        }
        Bubble::Caption => format!(
            "<rect x=\"{bx:.1}\" y=\"{by:.1}\" width=\"{box_w:.1}\" height=\"{box_h:.1}\" \
             fill=\"#fdf6e3\" stroke=\"#111111\" stroke-width=\"{:.1}\"/>", size * 0.1),
    };

    // All-caps in a lettering face: the convention, and it is a convention for a reason — mixed case
    // in a body face reads as a subtitle rather than as speech.
    let words = text.to_uppercase();
    let laid = balance(&words, lines.len());
    let start = by + size * 1.4;
    let text_svg: String = laid.iter().enumerate().map(|(i, l)| {
        format!("<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Comic Neue\" font-size=\"{size:.1}\" \
                 fill=\"#111111\" text-anchor=\"middle\">{}</text>",
                bx + box_w / 2.0, start + i as f64 * size * 1.25, escape(l))
    }).collect::<Vec<_>>().join("\n");

    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{panel_w}\" height=\"{panel_h}\" \
         viewBox=\"0 0 {panel_w} {panel_h}\">\n{shape}\n{text_svg}\n</svg>"
    ))
}

/// The same bubble as HTML and CSS, for the EPUB.
///
/// Not a compromise — better. The pages are already XHTML, so a bubble positioned over the image
/// with CSS keeps the text **selectable**, **readable by a screen reader**, and **able to reflow**,
/// all of which a rasterised PNG destroys. And because this app ships sixteen languages, one artwork
/// serves every one of them: re-render the words, keep the picture. Baked-in text would mean
/// regenerating every panel per language.
pub fn bubble_html(text: &str, kind: Bubble, anchor_x: f64, anchor_y: f64) -> String {
    let class = match kind {
        Bubble::Speech => "bubble speech",
        Bubble::Thought => "bubble thought",
        Bubble::Shout => "bubble shout",
        Bubble::Caption => "bubble caption",
    };
    let left = (anchor_x.clamp(0.05, 0.95) * 100.0).round();
    let top = if anchor_y > 0.5 { 6.0 } else { 62.0 };
    format!(
        "<div class=\"{class}\" style=\"left:{left}%;top:{top}%\" role=\"note\">{}</div>",
        escape(text)
    )
}

/// The stylesheet those bubbles need. One place, so every page agrees.
pub const BUBBLE_CSS: &str = r#"
/* Speech bubbles are HTML over the image rather than pixels inside it, so the words stay
   selectable, reachable by a screen reader, and translatable without touching the artwork. */
.panel { position: relative; }
.panel img { width: 100%; height: auto; display: block; }
.bubble {
  position: absolute; transform: translateX(-50%);
  max-width: 46%; padding: 0.6em 0.9em;
  font-family: "Comic Neue", "Comic Sans MS", cursive; text-transform: uppercase;
  font-size: 0.82em; line-height: 1.25; text-align: center;
  background: #fff; color: #111; border: 2px solid #111;
}
.bubble.speech { border-radius: 1.2em; }
.bubble.thought { border-radius: 50%; padding: 1.1em 1.2em; }
.bubble.shout { border-radius: 0.2em; clip-path: polygon(
  0% 12%, 8% 0%, 22% 9%, 38% 0%, 54% 8%, 70% 0%, 86% 9%, 100% 0%,
  96% 24%, 100% 44%, 94% 62%, 100% 82%, 84% 92%, 68% 100%, 50% 92%,
  32% 100%, 16% 91%, 0% 100%, 5% 76%, 0% 56%, 6% 34%); padding: 1.2em; }
.bubble.caption { border-radius: 0; background: #fdf6e3; text-transform: none;
  font-family: inherit; text-align: left; }
@media (prefers-color-scheme: dark) {
  .bubble { background: #f4f4f4; }
}
"#;

/// The catalogue, for the interface.
pub fn catalogue() -> Value {
    json!({
        "faces": FACES.iter().map(|f| json!({
            "id": f.id, "family": f.family, "licence": f.licence, "use_for": f.use_for,
            "caps_by_convention": f.caps_by_convention, "hairline": f.hairline,
        })).collect::<Vec<_>>(),
        "layouts": [
            { "id": "stacked_centred", "label": "Stacked, centred" },
            { "id": "big_word_subline", "label": "Big word with a subline" },
            { "id": "two_lines_with_rule", "label": "Two lines with a rule" },
            { "id": "arc", "label": "Arc" },
            { "id": "circular_badge", "label": "Circular badge" },
            { "id": "left_block", "label": "Left-aligned block" },
            { "id": "verse_and_reference", "label": "Verse and reference" },
        ],
        "bubbles": [
            { "id": "speech", "label": "Speech — rounded, with a tail" },
            { "id": "thought", "label": "Thought — scalloped cloud" },
            { "id": "shout", "label": "Shout — jagged" },
            { "id": "caption", "label": "Caption — plain box for narration" },
        ],
        "licensing": "Everything listed is SIL OFL or Apache 2.0, which permit commercial use \
                      including print on merchandise. Selling a product with text rendered in a font \
                      is not distributing the font; bundling the font file is, and OFL permits that \
                      as long as the licence ships alongside.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(phrase: &str, layout: &str) -> TextArt {
        TextArt { phrase: phrase.into(), reference: String::new(), layout: layout.into(),
                  face: "inter".into(), colour: String::new(), width: 4000, height: 4000 }
    }

    #[test]
    fn embroidery_refuses_what_it_cannot_stitch_rather_than_warning_about_it() {
        // A warning at fifty products a day is a thing nobody reads. An unrecognisable stitched
        // blob is a refund and a bad review.
        let err = fit_for("Be still and know that I am God", "inter", Decoration::Embroidery, 2, 3000)
            .unwrap_err();
        assert!(err.contains("too many to embroider"), "{err}");

        // Hairline serifs vanish in thread, and the refusal names something that works.
        let err = fit_for("Be still", "playfair", Decoration::Embroidery, 2, 3000).unwrap_err();
        assert!(err.contains("hairline"), "{err}");
        assert!(err.contains("Bebas") || err.contains("Montserrat"), "point somewhere: {err}");

        // Colours: four is the cap in thread, and the refusal says so.
        let err = fit_for("Be still", "bebas", Decoration::Embroidery, 9, 3000).unwrap_err();
        assert!(err.contains("4 at most") || err.contains("4,"), "{err}");

        // The same phrase and face on film is fine.
        assert!(fit_for("Be still", "bebas", Decoration::Dtf, 9, 3000).is_ok());
    }

    #[test]
    fn a_mug_demands_more_than_a_shirt() {
        assert_eq!(Decoration::HardSurface.min_dpi(), 300);
        assert_eq!(Decoration::Dtg.min_dpi(), 150);
        assert!(Decoration::Embroidery.colour_cap() < Decoration::Dtg.colour_cap());
        assert_eq!(Decoration::parse("embroidery"), Decoration::Embroidery);
        assert_eq!(Decoration::parse("DTF"), Decoration::Dtf);
        // An unknown method is treated as the common one rather than as permission.
        assert_eq!(Decoration::parse("something-new"), Decoration::Dtg);
    }

    #[test]
    fn the_words_come_out_exactly_as_written() {
        // The entire reason typography beats a text-capable image model: no letter is ever guessed.
        let svg = render(&art("Be still and know", "stacked_centred")).unwrap();
        let words: String = svg.chars().collect();
        assert!(words.contains("Be still") || words.contains("Be"), "{svg}");
        for w in ["Be", "still", "and", "know"] {
            assert!(svg.contains(w), "{w} is missing from the SVG");
        }
    }

    #[test]
    fn a_verse_carries_its_reference_smaller_and_italic() {
        // "Be still and know" / Psalm 46:10 is the shape most of this app's phrases want.
        let mut a = art("Be still and know", "verse_and_reference");
        a.reference = "Psalm 46:10".into();
        let svg = render(&a).unwrap();
        assert!(svg.contains("Psalm 46:10"));
        assert!(svg.contains("font-style=\"italic\""), "a citation is not a headline");
    }

    #[test]
    fn markup_in_a_phrase_cannot_break_out_of_the_svg() {
        // Phrases come from an AI and from users. One of them will contain an ampersand.
        let svg = render(&art("Salt & Light <of> the \"world\"", "stacked_centred")).unwrap();
        assert!(svg.contains("&amp;"));
        assert!(!svg.contains("<of>"));
        assert!(svg.contains("&lt;of&gt;"));
        assert!(svg.contains("&quot;"));
    }

    #[test]
    fn every_layout_produces_a_valid_svg_of_the_asked_for_size() {
        for layout in ["stacked_centred", "big_word_subline", "two_lines_with_rule", "arc",
                       "circular_badge", "left_block", "verse_and_reference"] {
            let mut a = art("Be still and know that I am God", layout);
            a.width = 3000;
            a.height = 3000;
            let svg = render(&a).unwrap_or_else(|e| panic!("{layout}: {e}"));
            assert!(svg.starts_with("<svg"), "{layout}");
            assert!(svg.ends_with("</svg>"), "{layout}");
            assert!(svg.contains("width=\"3000\""), "{layout} ignored the target size");
            // Transparent: a white rectangle here prints as a white rectangle on the product.
            assert!(!svg.contains("<rect x=\"0\" y=\"0\" width=\"3000\" height=\"3000\" fill=\"#fff"),
                    "{layout} painted a background");
        }
    }

    #[test]
    fn a_caps_by_convention_face_gets_capitals_and_a_body_face_does_not() {
        let mut a = art("be still", "stacked_centred");
        a.face = "bebas".into();
        assert!(render(&a).unwrap().contains("BE STILL"));
        a.face = "lora".into();
        assert!(render(&a).unwrap().contains("be still"));
    }

    #[test]
    fn every_bundled_face_is_licensed_for_merchandise() {
        // The pool is deliberately narrow: OFL and Apache both permit commercial use including
        // print. Anything else is a licence question nobody wants to answer at sale time.
        for f in FACES {
            assert!(f.licence.contains("OFL") || f.licence.contains("Apache"),
                    "{} ships under {}", f.family, f.licence);
            assert!(!f.use_for.is_empty(), "{} says nothing about itself", f.family);
        }
        // And a font of the user's own gets the sentence that matters.
        let warning = font_warning("/home/me/Downloads/free-font.ttf");
        assert!(warning.contains("commercial"));
        assert!(warning.contains("personal use"));
    }

    #[test]
    fn a_bubble_is_placed_away_from_the_speaker_so_it_cannot_cover_a_face() {
        // Placement is a decision made *after* seeing the art. That is the whole reason bubbles are
        // composited rather than baked in.
        let low = bubble_svg("Do not be afraid", Bubble::Speech, 1600, 1600, 0.5, 0.8).unwrap();
        let high = bubble_svg("Do not be afraid", Bubble::Speech, 1600, 1600, 0.5, 0.2).unwrap();
        assert!(low.contains("<polygon"), "a speech bubble needs a tail");
        assert_ne!(low, high, "a speaker at the bottom and one at the top cannot share a layout");

        // All four shapes render, and each is visibly a different shape.
        for kind in [Bubble::Speech, Bubble::Thought, Bubble::Shout, Bubble::Caption] {
            let svg = bubble_svg("Peace be with you", kind, 1600, 1600, 0.4, 0.7).unwrap();
            assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
            assert!(svg.contains("PEACE BE WITH YOU"), "comic lettering is all-caps by convention");
        }
        assert!(bubble_svg("   ", Bubble::Speech, 100, 100, 0.5, 0.5).is_err());
    }

    #[test]
    fn in_the_epub_a_bubble_is_html_so_it_stays_selectable_and_translatable() {
        // A PNG destroys selection, screen-reader access and reflow — and would mean regenerating
        // every panel for each of the sixteen languages this app ships.
        let html = bubble_html("Do not be afraid", Bubble::Speech, 0.3, 0.8);
        assert!(html.contains("Do not be afraid"), "the words must be real text");
        assert!(html.contains("class=\"bubble speech\""));
        assert!(html.contains("role=\"note\""));
        assert!(BUBBLE_CSS.contains(".bubble.thought"));
        assert!(BUBBLE_CSS.contains("position: absolute"));
        // Escaped there too.
        assert!(bubble_html("Salt & Light", Bubble::Caption, 0.5, 0.5).contains("&amp;"));
    }

    #[test]
    fn lines_are_balanced_rather_than_filled_greedily() {
        assert_eq!(balance("a b c d", 2), vec!["a b", "c d"]);
        assert_eq!(balance("one", 3), vec!["one"]);
        assert_eq!(balance("", 2), vec![""]);
    }

    #[test]
    fn nothing_renders_without_words() {
        assert!(render(&art("   ", "stacked_centred")).is_err());
        let mut a = art("hello", "stacked_centred");
        a.face = "nonexistent".into();
        assert!(render(&a).unwrap_err().contains("nonexistent"));
    }
}
