//! Devotional imagery: the models, the controls an artist would actually name, and where a picture
//! is going.
//!
//! Not a generic model catalogue. The images this app makes are scripture set to music, on YouTube,
//! and they have to read as clean, reverent and morally unambiguous while still carrying suspense,
//! mystery and glory. Two researched facts decide almost every choice below, and both are easy to
//! get wrong:
//!
//! # Fact 1 — FLUX cannot use a negative prompt at all
//!
//! FLUX.1 Dev was trained with guidance distillation and runs at CFG 1. There is no classifier-free
//! guidance to push away from, so a "things to avoid" field does **nothing** on FLUX. It does not
//! warn and it does not fail — it silently ignores you.
//!
//! That is a trap for exactly this use case, because "without getting dark or dirty" is precisely
//! what someone would type into a negative box. So on a model without negatives the restraint is
//! expressed **positively** ("modestly dressed, fully clothed, reverent, wholesome, serene"), which
//! is the documented substitute, and [`compose`] says so in its notes rather than dropping it in
//! silence. On SDXL and SD 3.5 the same control produces a real negative prompt. One control, two
//! mechanisms, decided per model.
//!
//! # Fact 2 — Pony Diffusion is the wrong model for this app
//!
//! Trained primarily on Derpibooru with acknowledged bias in its training data, it needs
//! `score_9, score_8_up, score_7_up` plus `rating_safe` *plus* a suppressive negative prompt to
//! reliably stay wholesome. For a Christian channel a model whose baseline needs active suppression
//! is a liability: one forgotten tag on one of fifty daily images is a published mistake. It is not
//! in the curated set, and this comment is here so nobody adds it back without reading why.
//!
//! # The two rules that are enforced rather than documented
//!
//! * **A destination that forbids text must not receive text.** Release covers are rejected by the
//!   stores for burnt-in lettering, so [`compose`] refuses a text workflow there rather than
//!   discouraging it.
//! * **A destination with a print resolution is checked before anything is created.** 1024px into a
//!   4000px print area is 77 DPI — a refund, not a product. [`print_check`] runs first, not after.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ────────────────────────────────────────────────────────────────────────────
// Models
// ────────────────────────────────────────────────────────────────────────────

/// A checkpoint, described the way an artist needs rather than the way a model card does.
///
/// "Juggernaut XL" tells an artist nothing. "Photographic — best for faces, hands and fabric;
/// slower; ignores long instructions" tells them everything.
#[derive(Debug, Clone, Serialize)]
pub struct Model {
    pub id: &'static str,
    pub label: &'static str,
    /// What it is for, in one sentence an artist can act on.
    pub good_at: &'static str,
    /// What it is bad at. Every model has something, and hiding it wastes the user's GPU hours.
    pub bad_at: &'static str,
    /// Does a "things to avoid" field do anything at all on this model?
    pub negatives: bool,
    /// Roughly how long one image takes on a free Kaggle T4, in seconds.
    pub seconds_on_a_free_gpu: u32,
    /// The shapes it is happiest at.
    pub sweet_spot: &'static [&'static str],
    /// The checkpoint file to resolve on the server.
    pub checkpoint: &'static str,
    /// FLUX ships UNet + CLIP + VAE as separate files; the loader differs from a single checkpoint.
    pub split_files: bool,
    /// Is text *inside* the image something this model does well?
    pub text_in_image: bool,
}

pub const MODELS: &[Model] = &[
    Model {
        id: "flux_dev",
        label: "FLUX.1 Dev",
        good_at: "The cleanest baseline and the best at doing what the prompt actually says — which \
                  matters most here, because the prompt is doing all the moral work.",
        bad_at: "Cannot use a \"things to avoid\" field at all. Say what you want, not what you don't.",
        negatives: false,
        seconds_on_a_free_gpu: 45,
        sweet_spot: &["16:9", "1:1", "9:16"],
        checkpoint: "flux1-dev.safetensors",
        split_files: true,
        text_in_image: false,
    },
    Model {
        id: "juggernaut_xl",
        label: "Juggernaut XL",
        good_at: "Faces, hands, skin and fabric — human presence. The one place a real negative \
                  prompt earns its keep.",
        bad_at: "Less literal with long prompts than FLUX; it drifts on a paragraph.",
        negatives: true,
        seconds_on_a_free_gpu: 30,
        sweet_spot: &["16:9", "2:3", "1:1"],
        checkpoint: "juggernautXL_v9.safetensors",
        split_files: false,
        text_in_image: false,
    },
    Model {
        id: "sd35",
        label: "SD 3.5",
        good_at: "An all-rounder with the best open text rendering — useful when a verse has to be \
                  legible inside the picture.",
        bad_at: "Heavier than SDXL for what you get back on ordinary scenes.",
        negatives: true,
        seconds_on_a_free_gpu: 60,
        sweet_spot: &["16:9", "1:1", "2:3"],
        checkpoint: "sd3.5_large.safetensors",
        split_files: false,
        text_in_image: true,
    },
    Model {
        id: "krea_turbo",
        label: "Krea 2 Turbo",
        good_at: "Drafts. Fast enough to review fifty ideas without spending the day's GPU hours.",
        bad_at: "Softer than the full model — a draft tool, not a finish.",
        negatives: true,
        seconds_on_a_free_gpu: 12,
        sweet_spot: &["16:9", "1:1"],
        checkpoint: "krea2_turbo.safetensors",
        split_files: false,
        text_in_image: false,
    },
    Model {
        id: "krea_raw",
        label: "Krea 2 RAW",
        good_at: "Finals with more aesthetic variety than FLUX, when everything looks too similar.",
        bad_at: "New and least documented of the set; expect to iterate on the prompt.",
        negatives: true,
        seconds_on_a_free_gpu: 55,
        sweet_spot: &["16:9", "2:3"],
        checkpoint: "krea2_raw.safetensors",
        split_files: false,
        text_in_image: false,
    },
    Model {
        id: "qwen_image",
        label: "Qwen-Image",
        good_at: "Words inside a picture, better than anything else open. Verse cards and study cards.",
        bad_at: "Not the one for a wide landscape; it is a typesetter more than a painter.",
        negatives: true,
        seconds_on_a_free_gpu: 50,
        sweet_spot: &["1:1", "2:3"],
        checkpoint: "qwen-image.safetensors",
        split_files: false,
        text_in_image: true,
    },
];

pub fn model(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.id == id)
}

// ────────────────────────────────────────────────────────────────────────────
// The controls — human grammar, not machine parameters
// ────────────────────────────────────────────────────────────────────────────

/// What an artist would say, mapped to real mechanics per model. Nothing here exposes a sampler
/// name or a CFG number.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Controls {
    /// dawn · shafts · candlelit · golden · storm · starlit
    #[serde(default)]
    pub light: String,
    /// serene · hopeful · solemn · awe · held_breath · glory
    #[serde(default)]
    pub mood: String,
    /// none · some · high — suspense, and see [`tension_phrase`] for why it is not darkness
    #[serde(default)]
    pub tension: String,
    /// ancient · timeless · modern
    #[serde(default)]
    pub era: String,
    /// none · subtle · overt
    #[serde(default)]
    pub symbolism: String,
    /// no_faces · faces · symbolic_only
    #[serde(default)]
    pub figures: String,
    /// Modest clothing, no gore, no occult symbolism, nothing sensual. On by default; turning it
    /// off is a deliberate act.
    #[serde(default = "yes")]
    pub restraint: bool,
    /// Enable CFG on a distilled model so a negative prompt works, at roughly twice the time.
    #[serde(default)]
    pub strict: bool,
}

fn yes() -> bool { true }

fn light_phrase(key: &str) -> &'static str {
    match key {
        "dawn" => "first light of dawn, low warm sun, long soft shadows",
        "shafts" => "shafts of light breaking through cloud, visible light rays, luminous haze",
        "candlelit" => "candlelight, warm pooled light in darkness, gentle falloff",
        "golden" => "golden hour, low amber sun, rim light",
        "storm" => "storm light, dark sky with a bright horizon, dramatic contrast",
        "starlit" => "starlight, deep blue night, faint cool glow",
        _ => "",
    }
}

fn mood_phrase(key: &str) -> &'static str {
    // Note what is deliberately absent: no "dark", no "ominous", no "horror". The vocabulary itself
    // keeps the output clean, which is worth more than any filter downstream.
    match key {
        "serene" => "serene, still, peaceful",
        "hopeful" => "hopeful, gently uplifting",
        "solemn" => "solemn, weighty, reverent",
        "awe" => "awe-struck, overwhelming scale",
        "held_breath" => "quiet anticipation, a held breath",
        "glory" => "glorious, radiant, transcendent",
        _ => "",
    }
}

/// Suspense without darkness.
///
/// This is the specific requirement and it is a *technique*, not a slider toward black. Dramatic
/// chiaroscuro, weather, vast scale, a small figure against something immense, a withheld reveal —
/// those produce tension. Gore, occult imagery and a horror palette produce something this app must
/// never publish, and simply darkening the scene produces neither.
fn tension_phrase(key: &str) -> &'static str {
    match key {
        "some" => "dramatic chiaroscuro, gathering weather, a sense of something approaching",
        "high" => "vast scale with a small solitary figure, towering sky, a withheld reveal just \
                   out of frame, dramatic chiaroscuro, gathering storm",
        _ => "",
    }
}

fn era_phrase(key: &str) -> &'static str {
    match key {
        "ancient" => "ancient Near East, period-accurate robes, stone and olive wood",
        "timeless" => "timeless and abstract, no period markers",
        "modern" => "modern parable, present-day setting",
        _ => "",
    }
}

fn symbolism_phrase(key: &str) -> &'static str {
    match key {
        // Subtle by default: overt reads as clip-art at fifty images a day.
        "subtle" => "quiet symbolism, understated",
        "overt" => "clear symbolism — light, water, bread, vine, dove, lamp, path or door as the \
                    visual centre",
        _ => "",
    }
}

fn figures_phrase(key: &str) -> &'static str {
    match key {
        // Offered first rather than as an afterthought: it is what many Christian channels actually
        // want, it sidesteps depicting Christ's face, and it happens to sidestep the whole
        // face-consistency problem.
        "no_faces" => "no faces shown — seen from behind, in silhouette, or only hands, feet and robes",
        "faces" => "faces visible, expressive, dignified",
        "symbolic_only" => "no people at all, symbolic objects and landscape only",
        _ => "",
    }
}

/// Restraint, said positively. The substitute for a negative prompt on a distilled model, and the
/// phrasing is deliberately concrete: "wholesome" alone does very little.
const RESTRAINT_POSITIVE: &str =
    "modestly dressed, fully clothed, reverent, wholesome, dignified, tasteful";

/// Restraint as a real negative prompt, for models that have one.
const RESTRAINT_NEGATIVE: &str =
    "nudity, nsfw, suggestive, revealing clothing, gore, blood, violence, occult symbols, \
     demonic imagery, horror, grotesque, distorted anatomy, extra limbs, watermark, signature, \
     lowres, deformed";

/// Text is forbidden at some destinations, and asked for at others.
const NO_TEXT_POSITIVE: &str = "no lettering anywhere, no text, no words, no watermark";
const NO_TEXT_NEGATIVE: &str = "text, lettering, words, watermark, signature, caption, logo";

// ────────────────────────────────────────────────────────────────────────────
// Destinations — the applied half
// ────────────────────────────────────────────────────────────────────────────

/// Where a picture is going. The artistic controls are the *voice*; this is the *intent*.
///
/// They are separate because the same prompt at the same aspect ratio is not the same usable image
/// for two destinations, and the differences are hard platform constraints rather than taste. A user
/// picks where it is going, once, and never has to remember a number below.
#[derive(Debug, Clone, Serialize)]
pub struct Destination {
    pub id: &'static str,
    pub label: &'static str,
    pub aspect: &'static str,
    pub width: u32,
    pub height: u32,
    /// May text appear in this image at all?
    pub allows_text: bool,
    /// The composition rule that actually bites here, in the artist's own terms.
    pub constraint: &'static str,
    /// Added to the prompt so the composition survives what happens to it later.
    pub composition: &'static str,
    /// Print destinations refuse rather than warn below this DPI.
    pub min_dpi: u32,
    pub transparent: bool,
}

pub const DESTINATIONS: &[Destination] = &[
    Destination {
        id: "video_frame", label: "YouTube video frame",
        aspect: "16:9", width: 1920, height: 1080, allows_text: true,
        constraint: "The video assembler pans and zooms across this, so a subject at the very edge \
                     gets cropped out mid-move.",
        composition: "generous margin on all four sides, subject well inside the frame",
        min_dpi: 0, transparent: false,
    },
    Destination {
        id: "thumbnail", label: "YouTube thumbnail",
        aspect: "16:9", width: 1280, height: 720, allows_text: true,
        constraint: "Has to read at 120 pixels wide in a sidebar. This is a different image from the \
                     video frame, not a resized one.",
        composition: "one clear subject, high contrast, bold simple shapes, no fine detail",
        min_dpi: 0, transparent: false,
    },
    Destination {
        id: "short", label: "Shorts / Reels / TikTok",
        aspect: "9:16", width: 1080, height: 1920, allows_text: true,
        constraint: "Platform interface covers roughly the top 10% and bottom 20%, and the app burns \
                     a link card into the bottom third.",
        composition: "everything that matters in the middle 70% vertically, lower third kept \
                      visually quiet and uncluttered",
        min_dpi: 0, transparent: false,
    },
    Destination {
        id: "instagram", label: "Instagram feed",
        aspect: "4:5", width: 1080, height: 1350, allows_text: true,
        constraint: "Cropped to a square in grid view whatever you upload.",
        composition: "composed to survive a centre square crop",
        min_dpi: 0, transparent: false,
    },
    Destination {
        id: "ebook_page", label: "Ebook page",
        aspect: "2:3", width: 1600, height: 2400, allows_text: true,
        constraint: "The only destination read close up, so fine detail survives — but the inner \
                     edge disappears into the gutter.",
        composition: "fine detail rewarded, inner edge kept clear for the binding",
        min_dpi: 0, transparent: false,
    },
    Destination {
        id: "panel", label: "Comic panel",
        aspect: "1:1", width: 1600, height: 1600, allows_text: true,
        // Composing *for* the bubble is the difference between one that sits in the picture and one
        // that vandalises it.
        composition: "a single clear focal point, with uncluttered negative space in the upper third \
                      where a caption or speech bubble will sit",
        constraint: "Read on a phone in a stack, and a bubble goes on top of it later.",
        min_dpi: 0, transparent: false,
    },
    Destination {
        id: "release_cover", label: "Release cover",
        aspect: "1:1", width: 3000, height: 3000,
        // The stores draw the title themselves and burnt-in text is a common rejection.
        allows_text: false,
        constraint: "No text at all — the stores draw the title themselves and reject burnt-in \
                     lettering. Must also read at 64 pixels.",
        composition: "one bold central image, reads clearly at thumbnail size",
        min_dpi: 0, transparent: false,
    },
    Destination {
        id: "product", label: "Printify product art",
        aspect: "1:1", width: 4000, height: 4000, allows_text: false,
        constraint: "DTG printing loses thin lines and light-on-light contrast, and the real print \
                     area comes from the blueprint rather than a guess.",
        composition: "bold simple shapes, few colours, strong contrast, transparent background, \
                      no thin strokes",
        min_dpi: 150, transparent: true,
    },
    Destination {
        id: "publicity", label: "Publicity cover",
        aspect: "16:9", width: 1920, height: 1080, allows_text: true,
        constraint: "Sized for the platform the article is written for, and about the article's \
                     subject rather than the song's cover.",
        composition: "editorial, clean, room for a headline",
        min_dpi: 0, transparent: false,
    },
];

pub fn destination(id: &str) -> Option<&'static Destination> {
    DESTINATIONS.iter().find(|d| d.id == id)
}

// ────────────────────────────────────────────────────────────────────────────
// Packaged intents — what the user actually picks
// ────────────────────────────────────────────────────────────────────────────

/// One choice that sets a destination *and* a sensible voice, because "a shirt" and "a chapter
/// opener" want different treatment even at the same aspect ratio.
#[derive(Debug, Clone, Serialize)]
pub struct Intent {
    pub id: &'static str,
    pub label: &'static str,
    pub destination: &'static str,
    /// What it is for, in one line.
    pub purpose: &'static str,
    /// What it will refuse. A preset that silently drops a constraint is worse than no preset,
    /// because the user stops checking.
    pub refuses: &'static str,
    pub light: &'static str,
    pub mood: &'static str,
    pub tension: &'static str,
    pub figures: &'static str,
    pub symbolism: &'static str,
    /// Some intents need a specific model — a study card is the one case where text inside the
    /// image is the whole point.
    pub model: Option<&'static str>,
}

pub const INTENTS: &[Intent] = &[
    Intent {
        id: "chapter_opener", label: "Chapter opener", destination: "video_frame",
        purpose: "The workhorse: opens a passage calmly and leaves room for the camera move.",
        refuses: "Nothing — this is the safe default.",
        light: "dawn", mood: "serene", tension: "none", figures: "no_faces", symbolism: "subtle",
        model: None,
    },
    Intent {
        id: "the_turn", label: "The turn", destination: "video_frame",
        purpose: "The moment a passage changes. Proves tension does not need darkness.",
        refuses: "Never darkens the palette or adds menace — tension comes from scale and weather.",
        light: "storm", mood: "held_breath", tension: "high", figures: "no_faces", symbolism: "subtle",
        model: None,
    },
    Intent {
        id: "glory", label: "Glory", destination: "video_frame",
        purpose: "For the passage that earns it.",
        refuses: "Nothing, but it is easy to overuse — at fifty a day this stops meaning anything.",
        light: "shafts", mood: "glory", tension: "none", figures: "symbolic_only", symbolism: "overt",
        model: None,
    },
    Intent {
        id: "thumbnail", label: "Thumbnail", destination: "thumbnail",
        purpose: "One subject, high contrast, legible at 120 pixels in a sidebar.",
        refuses: "No scripture text baked in — it is unreadable at that size and dates the video.",
        light: "golden", mood: "hopeful", tension: "some", figures: "faces", symbolism: "subtle",
        model: None,
    },
    Intent {
        id: "short_hook", label: "Short hook", destination: "short",
        purpose: "Vertical, with the subject where the platform interface is not.",
        refuses: "Keeps the bottom third quiet, because the link card is burned in there.",
        light: "golden", mood: "hopeful", tension: "some", figures: "no_faces", symbolism: "subtle",
        model: None,
    },
    Intent {
        id: "wearable_phrase", label: "Wearable phrase", destination: "product",
        purpose: "Art for a garment, to sit under a phrase the typography layer renders.",
        refuses: "Refuses low resolution outright, and generates no lettering — the words are set \
                  in a real font afterwards, never generated.",
        light: "", mood: "hopeful", tension: "none", figures: "symbolic_only", symbolism: "overt",
        model: None,
    },
    Intent {
        id: "hard_surface", label: "Mug or hard surface", destination: "product",
        purpose: "The same as a wearable, but a mug shows every soft edge a shirt hides.",
        refuses: "Refuses anything under 300 DPI rather than warning about it.",
        light: "", mood: "serene", tension: "none", figures: "symbolic_only", symbolism: "overt",
        model: None,
    },
    Intent {
        id: "album_cover", label: "Album cover", destination: "release_cover",
        purpose: "One bold image that still reads as a 64-pixel thumbnail.",
        refuses: "No text of any kind — burnt-in lettering is a common store rejection.",
        light: "shafts", mood: "awe", tension: "none", figures: "symbolic_only", symbolism: "overt",
        model: None,
    },
    Intent {
        id: "book_page", label: "Book page", destination: "ebook_page",
        purpose: "Read close up, so detail is finally worth spending on.",
        refuses: "Keeps the inner edge clear for the gutter.",
        light: "candlelit", mood: "solemn", tension: "none", figures: "no_faces", symbolism: "subtle",
        model: None,
    },
    Intent {
        id: "study_card", label: "Study card", destination: "instagram",
        purpose: "The one case where words inside the image are the point.",
        refuses: "Runs only on a model that can actually render text — SD 3.5 or Qwen-Image.",
        light: "dawn", mood: "serene", tension: "none", figures: "symbolic_only", symbolism: "subtle",
        model: Some("sd35"),
    },
];

pub fn intent(id: &str) -> Option<&'static Intent> {
    INTENTS.iter().find(|i| i.id == id)
}

/// The controls an intent implies, before the user adjusts anything.
pub fn controls_for(i: &Intent) -> Controls {
    Controls {
        light: i.light.into(),
        mood: i.mood.into(),
        tension: i.tension.into(),
        era: "timeless".into(),
        symbolism: i.symbolism.into(),
        figures: i.figures.into(),
        restraint: true,
        strict: false,
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Composition
// ────────────────────────────────────────────────────────────────────────────

/// A prompt, its shape, and everything the user should be told about what was and was not applied.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Composed {
    pub positive: String,
    /// `None` on a model with no classifier-free guidance — and then `notes` says why.
    pub negative: Option<String>,
    pub width: u32,
    pub height: u32,
    pub aspect: String,
    /// Things that were decided for the user, in plain sentences.
    pub notes: Vec<String>,
    /// Roughly how long this will take on a free GPU.
    pub seconds: u32,
    pub transparent: bool,
}

/// Build the prompt for one image.
///
/// The order is deliberate: subject first (models weight the opening most heavily), then light,
/// mood, tension, era, symbolism, figures, then the composition rule the destination imposes, then
/// restraint last as a qualifier.
pub fn compose(model_id: &str, dest_id: &str, controls: &Controls, subject: &str) -> Result<Composed, String> {
    let m = model(model_id).ok_or_else(|| format!("There is no model called {model_id:?}."))?;
    let d = destination(dest_id).ok_or_else(|| format!("There is nowhere called {dest_id:?} to send this."))?;
    let mut notes: Vec<String> = Vec::new();

    let mut parts: Vec<String> = Vec::new();
    let subject = subject.trim();
    if subject.is_empty() {
        return Err("An image needs a subject — what is actually in it?".into());
    }
    parts.push(subject.to_string());
    for phrase in [
        light_phrase(&controls.light),
        mood_phrase(&controls.mood),
        tension_phrase(&controls.tension),
        era_phrase(&controls.era),
        symbolism_phrase(&controls.symbolism),
        figures_phrase(&controls.figures),
        d.composition,
    ] {
        if !phrase.is_empty() { parts.push(phrase.to_string()); }
    }

    // Text: forbidden at some destinations, and that is enforced rather than suggested.
    let mut negatives: Vec<&str> = Vec::new();
    if !d.allows_text {
        parts.push(NO_TEXT_POSITIVE.to_string());
        negatives.push(NO_TEXT_NEGATIVE);
        notes.push(format!(
            "{} takes no lettering at all, so the prompt asks for none. {}",
            d.label, d.constraint
        ));
    }

    // Restraint: a real negative prompt where one works, positive phrasing where it does not.
    let negatives_work = m.negatives || controls.strict;
    if controls.restraint {
        if negatives_work {
            negatives.push(RESTRAINT_NEGATIVE);
        } else {
            parts.push(RESTRAINT_POSITIVE.to_string());
            notes.push(format!(
                "{} runs without classifier-free guidance, so a \"things to avoid\" list would be \
                 silently ignored. The restraint is written into the prompt instead, which is the \
                 documented substitute.",
                m.label
            ));
        }
    } else {
        notes.push("Restraint is switched off for this image. That is a deliberate choice, not a \
                    default.".into());
    }

    let mut seconds = m.seconds_on_a_free_gpu;
    if controls.strict && !m.negatives {
        // DynamicThresholdingFull enables CFG on a distilled model, at roughly twice the time.
        seconds *= 2;
        notes.push(format!(
            "Strict mode turns on guidance so \"things to avoid\" works on {}. It roughly doubles \
             the time — about {seconds} seconds an image on a free GPU.",
            m.label
        ));
    }

    if !d.sweet_spot_ok(m) {
        notes.push(format!(
            "{} is happiest at {} and this is {}. It will work; expect to iterate more.",
            m.label, m.sweet_spot.join(" or "), d.aspect
        ));
    }

    Ok(Composed {
        positive: parts.join(", "),
        negative: if negatives.is_empty() || !negatives_work { None } else { Some(negatives.join(", ")) },
        width: d.width,
        height: d.height,
        aspect: d.aspect.to_string(),
        notes,
        seconds,
        transparent: d.transparent,
    })
}

impl Destination {
    fn sweet_spot_ok(&self, m: &Model) -> bool {
        m.sweet_spot.contains(&self.aspect)
    }
}

/// Is this destination allowed to carry words inside the picture?
///
/// Asked before a text workflow runs, so "no text here" is a refusal rather than a preference.
pub fn text_allowed(dest_id: &str, model_id: &str) -> Result<(), String> {
    let d = destination(dest_id).ok_or("unknown destination")?;
    if !d.allows_text {
        return Err(format!(
            "A {} cannot carry text. {} Put the words on afterwards with the typography layer, or \
             choose a different destination.",
            d.label.to_lowercase(), d.constraint
        ));
    }
    let m = model(model_id).ok_or("unknown model")?;
    if !m.text_in_image {
        return Err(format!(
            "{} does not render readable words. Use SD 3.5 or Qwen-Image for text inside a picture — \
             or set the words in a real font on top, which never misspells anything.",
            m.label
        ));
    }
    Ok(())
}

/// Refuse a print job that would arrive as a refund.
///
/// Runs *before* anything is created. 1024px into a 4000px print area is 77 DPI, and finding that
/// out afterwards means the art, the GPU time and the product listing are all already spent.
pub fn print_check(dest_id: &str, art_w: u32, art_h: u32, area_w: u32, area_h: u32) -> Result<Value, String> {
    let d = destination(dest_id).ok_or("unknown destination")?;
    if d.min_dpi == 0 {
        return Ok(json!({ "ok": true, "checked": false, "reason": "not a print destination" }));
    }
    let report = crate::commands::printify::print_quality(
        area_w as i64, area_h as i64, art_w as i64, art_h as i64);
    let dpi = report["dpi"].as_i64().unwrap_or(0) as u32;
    if dpi < d.min_dpi {
        return Err(format!(
            "This would print at {dpi} DPI, and {} needs at least {}. {} Generate the art at the \
             product's own size — {}×{} — rather than upscaling it afterwards.",
            d.label, d.min_dpi,
            report["advice"].as_str().unwrap_or(""),
            area_w, area_h
        ));
    }
    Ok(report)
}

/// The whole catalogue, for the interface.
pub fn catalogue() -> Value {
    json!({
        "models": MODELS.iter().map(|m| json!({
            "id": m.id, "label": m.label, "good_at": m.good_at, "bad_at": m.bad_at,
            "negatives": m.negatives, "seconds": m.seconds_on_a_free_gpu,
            "sweet_spot": m.sweet_spot, "checkpoint": m.checkpoint,
            "split_files": m.split_files, "text_in_image": m.text_in_image,
        })).collect::<Vec<_>>(),
        "destinations": DESTINATIONS.iter().map(|d| json!({
            "id": d.id, "label": d.label, "aspect": d.aspect,
            "width": d.width, "height": d.height, "allows_text": d.allows_text,
            "constraint": d.constraint, "min_dpi": d.min_dpi, "transparent": d.transparent,
        })).collect::<Vec<_>>(),
        "intents": INTENTS.iter().map(|i| json!({
            "id": i.id, "label": i.label, "destination": i.destination,
            "purpose": i.purpose, "refuses": i.refuses, "model": i.model,
            "controls": { "light": i.light, "mood": i.mood, "tension": i.tension,
                          "figures": i.figures, "symbolism": i.symbolism },
        })).collect::<Vec<_>>(),
        "controls": {
            "light": ["dawn", "shafts", "candlelit", "golden", "storm", "starlit"],
            "mood": ["serene", "hopeful", "solemn", "awe", "held_breath", "glory"],
            "tension": ["none", "some", "high"],
            "era": ["ancient", "timeless", "modern"],
            "symbolism": ["none", "subtle", "overt"],
            "figures": ["no_faces", "faces", "symbolic_only"],
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controls() -> Controls {
        Controls { light: "dawn".into(), mood: "serene".into(), tension: "none".into(),
                   era: "ancient".into(), symbolism: "subtle".into(), figures: "no_faces".into(),
                   restraint: true, strict: false }
    }

    #[test]
    fn flux_gets_its_restraint_in_the_prompt_because_a_negative_would_be_ignored() {
        // The trap this exists to close: "without getting dark or dirty" in a negative box does
        // nothing on FLUX, silently. It does not warn and it does not fail.
        let out = compose("flux_dev", "video_frame", &controls(), "a shepherd on a hillside").unwrap();
        assert!(out.negative.is_none(), "FLUX runs at CFG 1 — there is nothing to push away from");
        assert!(out.positive.contains("modestly dressed"), "{}", out.positive);
        assert!(out.notes.iter().any(|n| n.contains("silently ignored")),
                "the user must be told, not left to assume it worked");

        // The same control on a model with guidance produces a real negative prompt.
        let sdxl = compose("juggernaut_xl", "video_frame", &controls(), "a shepherd on a hillside").unwrap();
        let neg = sdxl.negative.expect("SDXL has classifier-free guidance");
        assert!(neg.contains("nudity") && neg.contains("occult"));
        assert!(!sdxl.positive.contains("modestly dressed"),
                "one control, two mechanisms — not both at once");
    }

    #[test]
    fn strict_mode_buys_a_negative_prompt_on_flux_and_says_what_it_costs() {
        let mut c = controls();
        c.strict = true;
        let out = compose("flux_dev", "video_frame", &c, "a lamp in a doorway").unwrap();
        assert!(out.negative.is_some(), "strict mode enables guidance");
        assert_eq!(out.seconds, model("flux_dev").unwrap().seconds_on_a_free_gpu * 2);
        assert!(out.notes.iter().any(|n| n.contains("doubles")), "{:?}", out.notes);
    }

    #[test]
    fn tension_is_scale_and_weather_rather_than_darkness() {
        // The specific requirement: suspense without going dark. A palette slider toward black is
        // exactly what this must not be.
        let mut c = controls();
        c.tension = "high".into();
        let out = compose("juggernaut_xl", "video_frame", &c, "a road into the hills").unwrap();
        assert!(out.positive.contains("vast scale") || out.positive.contains("withheld reveal"),
                "{}", out.positive);
        for forbidden in ["dark", "ominous", "horror", "sinister", "evil"] {
            assert!(!out.positive.contains(forbidden), "tension must not reach for {forbidden:?}");
        }
    }

    #[test]
    fn a_release_cover_is_never_asked_for_lettering() {
        // Burnt-in text is a common store rejection, so this is enforced rather than discouraged.
        let out = compose("flux_dev", "release_cover", &controls(), "a lamp").unwrap();
        assert!(out.positive.contains("no lettering anywhere"));
        assert!(out.notes.iter().any(|n| n.contains("no lettering")));
        assert!(text_allowed("release_cover", "sd35").is_err());
        let why = text_allowed("release_cover", "sd35").unwrap_err();
        assert!(why.contains("typography layer"), "refusing must point somewhere: {why}");

        // And a model that cannot spell is refused even where text is allowed.
        assert!(text_allowed("ebook_page", "flux_dev").is_err());
        assert!(text_allowed("ebook_page", "sd35").is_ok());
        assert!(text_allowed("ebook_page", "qwen_image").is_ok());
    }

    #[test]
    fn a_print_that_would_arrive_as_a_refund_is_refused_before_it_is_made() {
        // 1024px into a 4000px print area is 77 DPI. Finding that out afterwards means the art, the
        // GPU time and the listing are already spent.
        let err = print_check("product", 1024, 1024, 4000, 4000).unwrap_err();
        assert!(err.contains("77 DPI"), "{err}");
        assert!(err.contains("4000×4000"), "it must say what size to generate instead: {err}");
        assert!(print_check("product", 4000, 4000, 4000, 4000).is_ok());
        // Screens have no DPI question, so the check does not invent one.
        let skipped = print_check("video_frame", 100, 100, 4000, 4000).unwrap();
        assert_eq!(skipped["checked"], json!(false));
    }

    #[test]
    fn every_destination_that_is_read_small_says_so_in_the_prompt() {
        let thumb = compose("juggernaut_xl", "thumbnail", &controls(), "a face").unwrap();
        assert!(thumb.positive.contains("high contrast"));
        assert_eq!((thumb.width, thumb.height), (1280, 720));

        // The platform covers the top and bottom of a short, and the app burns a link card into the
        // bottom third — so that band has to stay quiet.
        let short = compose("juggernaut_xl", "short", &controls(), "a door").unwrap();
        assert!(short.positive.contains("middle 70%"), "{}", short.positive);
        assert!(short.positive.contains("lower third"), "{}", short.positive);
        assert_eq!((short.width, short.height), (1080, 1920));

        // A panel is composed *for* the bubble that lands on it later.
        let panel = compose("juggernaut_xl", "panel", &controls(), "two figures").unwrap();
        assert!(panel.positive.contains("negative space"), "{}", panel.positive);
    }

    #[test]
    fn every_preset_states_what_it_is_for_and_what_it_refuses() {
        // A preset that silently drops a constraint is worse than no preset, because the user stops
        // checking.
        for i in INTENTS {
            assert!(!i.purpose.is_empty(), "{} has no purpose", i.id);
            assert!(!i.refuses.is_empty(), "{} does not say what it refuses", i.id);
            assert!(destination(i.destination).is_some(), "{} points nowhere", i.id);
            if let Some(m) = i.model { assert!(model(m).is_some(), "{} names a model that is gone", i.id); }
            // Every preset must compose without error, with its own controls.
            let model_id = i.model.unwrap_or("flux_dev");
            compose(model_id, i.destination, &controls_for(i), "a test subject")
                .unwrap_or_else(|e| panic!("{} does not compose: {e}", i.id));
        }
        // The study card is the one case where text inside the image is the point, so it must name
        // a model that can actually spell.
        let card = intent("study_card").unwrap();
        assert!(model(card.model.unwrap()).unwrap().text_in_image);
    }

    #[test]
    fn pony_is_not_in_the_curated_set() {
        // Its baseline needs active suppression to stay wholesome, and one forgotten tag on one of
        // fifty daily images is a published mistake.
        assert!(MODELS.iter().all(|m| !m.id.contains("pony") && !m.label.to_lowercase().contains("pony")));
    }

    #[test]
    fn every_model_tells_an_artist_what_it_is_bad_at() {
        // "Juggernaut XL" tells an artist nothing. Every model owes them the rest.
        for m in MODELS {
            assert!(!m.good_at.is_empty(), "{} has no use", m.id);
            assert!(!m.bad_at.is_empty(), "{} claims to be bad at nothing", m.id);
            assert!(m.seconds_on_a_free_gpu > 0, "{} has no time estimate", m.id);
            assert!(!m.sweet_spot.is_empty(), "{} has no aspect guidance", m.id);
            assert!(m.checkpoint.ends_with(".safetensors"),
                    "{} must be safetensors — it cannot contain executable code", m.id);
        }
    }

    #[test]
    fn a_subject_is_required_and_an_unknown_name_is_refused_in_words() {
        assert!(compose("flux_dev", "video_frame", &controls(), "   ").is_err());
        let err = compose("nonexistent", "video_frame", &controls(), "a lamp").unwrap_err();
        assert!(err.contains("nonexistent"), "{err}");
        assert!(compose("flux_dev", "nowhere", &controls(), "a lamp").is_err());
    }

    #[test]
    fn turning_restraint_off_is_recorded_rather_than_quiet() {
        let mut c = controls();
        c.restraint = false;
        let out = compose("juggernaut_xl", "video_frame", &c, "a figure").unwrap();
        assert!(out.notes.iter().any(|n| n.contains("deliberate choice")), "{:?}", out.notes);
        assert!(out.negative.is_none() || !out.negative.as_ref().unwrap().contains("nudity"));
    }

    #[test]
    fn the_catalogue_carries_everything_the_interface_needs() {
        let c = catalogue();
        assert_eq!(c["models"].as_array().unwrap().len(), MODELS.len());
        assert_eq!(c["intents"].as_array().unwrap().len(), INTENTS.len());
        assert_eq!(c["destinations"].as_array().unwrap().len(), DESTINATIONS.len());
        // The mood vocabulary itself keeps the output clean — nothing dark is even offerable.
        let moods = c["controls"]["mood"].as_array().unwrap();
        for m in moods {
            let word = m.as_str().unwrap();
            assert!(!["dark", "ominous", "horror", "grim"].contains(&word), "{word} must not be offered");
        }
    }
}
