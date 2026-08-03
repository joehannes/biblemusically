//! The look of a picture, and how two looks combine.
//!
//! # Why this is not just a longer match arm
//!
//! Styles used to be six hardcoded `(prefix, negative)` pairs. That works until somebody wants a
//! look that is *between* two of them — trash polka's red-and-black collage with the wash of
//! watercolour, say — at which point the only options are to add a seventh hardcoded pair for every
//! combination anybody asks for, or to let styles be composed. Twelve styles is sixty-six pairs;
//! composition is the only version of this that stays finite.
//!
//! # How a mix actually works
//!
//! ComfyUI's `CLIPTextEncode` understands `(term:weight)` emphasis, so a mix is not string
//! concatenation — it is a weighted prompt. A 70/30 blend puts the dominant style's terms at a
//! weight above one and the accent's below, which is what makes "mostly trash polka, a little
//! watercolour" a different picture from "both, equally". Concatenating the two prefixes unweighted
//! gives neither, and is what a naive implementation produces.
//!
//! Negatives merge rather than blend. A negative is a list of things to avoid, and avoiding
//! something "70% as much" is not meaningful — so the union is taken and duplicates dropped, which
//! also keeps the negative from doubling in length every time two styles are combined.
//!
//! # Why the negatives matter more than the prefixes
//!
//! Every style here fights the checkpoint's defaults. SDXL wants to make everything a glossy
//! photograph; a watercolour prompt without `photograph, 3d render, sharp edges` in the negative
//! produces a photograph of a watercolour, which is a different and worse picture. So each style's
//! negative names the specific thing the model will otherwise revert to, and they are as
//! deliberately chosen as the prefixes.

use serde::Serialize;

/// Roughly what tradition a look comes from. Used to keep a mix coherent: two styles from opposite
/// families usually produce mud rather than a blend, and `mixes_well_with` encodes the exceptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Family {
    /// Ink, line, print — high contrast, graphic.
    Graphic,
    /// Paint on a surface — brush, wash, texture.
    Painterly,
    /// Lens-like. Wants to be a photograph.
    Photographic,
    /// Design work: production art, matte painting, orthographic sheets.
    Design,
}

#[derive(Debug, Clone, Serialize)]
pub struct Style {
    pub id: &'static str,
    pub label: &'static str,
    pub family: Family,
    /// The positive terms. No trailing comma — the composer adds separators.
    pub prompt: &'static str,
    /// What this look must be told not to become. See the module header.
    pub negative: &'static str,
    /// What it is for, in one sentence, written for somebody choosing rather than implementing.
    pub note: &'static str,
}

pub const STYLES: &[Style] = &[
    // ── The four asked for ──────────────────────────────────────────────
    Style {
        id: "trash_polka",
        label: "Trash Polka",
        family: Family::Graphic,
        prompt: "trash polka tattoo art, strictly red black and white palette, photorealistic \
                 fragments collaged with abstract brush strokes, ink splatter, torn paper edges, \
                 bold sans-serif type fragments, smeared stamps, high contrast",
        // The palette is the whole identity of this style, so anything that reintroduces colour is
        // the primary thing to suppress — more important here than the usual quality negatives.
        negative: "full color, pastel, muted palette, soft gradients, blue, green, photorealistic \
                   whole scene, 3d render, blurry, low contrast",
        note: "Red, black and white only. Realistic fragments cut into abstract marks — aggressive, \
               graphic, and it reads at thumbnail size, which most painterly looks do not.",
    },
    Style {
        id: "concept_art",
        label: "Concept Art",
        family: Family::Design,
        prompt: "professional concept art, production design, matte painting, dramatic scale, \
                 atmospheric perspective, confident brushwork, cinematic composition, \
                 art-station quality, detailed environment design",
        negative: "flat lighting, amateur, sketchy, unfinished, watermark, text, signature, \
                   oversaturated, cluttered composition",
        note: "The look of film pre-production art: scale, atmosphere, a world implied beyond the \
               frame. The safest choice when a shot has to carry a whole scene.",
    },
    Style {
        id: "watercolor_fx",
        label: "Watercolour + FX",
        family: Family::Painterly,
        prompt: "expressive watercolour painting, wet-on-wet bleeding, granulating pigment, \
                 blooming backruns, splatter and spatter, visible cold-press paper texture, \
                 white paper showing through, loose confident edges",
        // "photograph of a painting" is the exact failure mode here — hence naming it directly.
        negative: "photograph, photo of a painting, digital airbrush, hard edges, vector, 3d render, \
                   uniform flat color, oversaturated, blurry",
        note: "Wet, loose and accidental — bleeds and splatter rather than tidy washes. Leaves \
               white paper, which gives text somewhere to sit.",
    },
    Style {
        id: "pop_art",
        label: "Pop Art",
        family: Family::Graphic,
        prompt: "pop art, ben-day halftone dots, thick black outlines, flat saturated primary \
                 colours, screen-print registration offset, comic panel composition, \
                 Lichtenstein and Warhol influence, bold graphic simplicity",
        negative: "photorealistic, soft shading, gradient mesh, muted palette, painterly texture, \
                   3d render, blurry, fine detail",
        note: "Flat, loud and printed. Halftone dots and heavy outlines — the most legible style \
               here on a phone screen.",
    },

    // ── Neighbours worth having, because a mix needs something to mix with ──
    Style {
        id: "ink_wash",
        label: "Ink Wash (sumi-e)",
        family: Family::Painterly,
        prompt: "sumi-e ink wash painting, sparse confident brush strokes, deep black ink on rice \
                 paper, generous negative space, tonal gradation, meditative restraint",
        negative: "color, busy detail, hard vector edges, 3d render, photograph, cluttered",
        note: "Almost empty. Restraint and negative space — the counterweight to every loud style \
               here, and what makes trash polka legible when blended with it.",
    },
    Style {
        id: "linocut",
        label: "Linocut / Woodblock",
        family: Family::Graphic,
        prompt: "linocut relief print, carved gouge marks, bold black shapes, visible carving \
                 texture, limited two-colour registration, hand-pulled print imperfection",
        negative: "smooth gradients, photorealistic, 3d render, fine airbrush detail, blurry",
        note: "Carved rather than drawn. Strong shapes and a hand-made roughness that survives \
               heavy compression.",
    },
    Style {
        id: "risograph",
        label: "Risograph",
        family: Family::Graphic,
        prompt: "risograph print, fluorescent pink and blue spot colours, visible misregistration, \
                 coarse grain, overprint multiply blending, limited palette, paper stock texture",
        negative: "full colour photography, smooth gradient, 3d render, sharp digital edges, blurry",
        note: "Two or three spot colours, deliberately misaligned. Modern, printed and cheap-looking \
               in the way people currently find attractive.",
    },
    Style {
        id: "dark_baroque",
        label: "Dark Baroque",
        family: Family::Painterly,
        prompt: "baroque oil painting, extreme chiaroscuro, single dramatic light source, deep \
                 shadow, rich glazed colour, visible impasto, old master composition, \
                 Caravaggio influence",
        negative: "flat lighting, pastel, cartoon, 3d render, modern clothing, watermark, blurry",
        note: "One light source, everything else in darkness. The most reverent look here, and the \
               obvious fit for scripture that is about weight rather than joy.",
    },
    Style {
        id: "stained_glass",
        label: "Stained Glass",
        family: Family::Graphic,
        prompt: "stained glass window, heavy black leading between jewel-toned glass panels, \
                 backlit luminosity, geometric tracery, flat saturated segments, cathedral light",
        negative: "photorealistic depth, soft gradients, 3d render, muted palette, blurry, \
                   realistic shadows",
        note: "Leaded segments and backlight. Ecclesiastical without being pastiche, and its heavy \
               black lines hold up at any size.",
    },
    Style {
        id: "collage",
        label: "Mixed-Media Collage",
        family: Family::Graphic,
        prompt: "mixed media collage, torn magazine paper, cut-and-paste layering, visible tape and \
                 staples, xerox texture, handwritten annotation, punk zine aesthetic",
        negative: "clean vector, smooth 3d render, photorealistic single scene, symmetrical, blurry",
        note: "Torn and taped. Sits naturally with trash polka and gives it somewhere to go beyond \
               red and black.",
    },
    Style {
        id: "cinematic_photo",
        label: "Cinematic Photograph",
        family: Family::Photographic,
        prompt: "cinematic photograph, anamorphic lens, shallow depth of field, motivated practical \
                 lighting, filmic colour grade, 35mm grain, natural skin tones",
        negative: "illustration, painting, cartoon, 3d render, oversaturated, flat lighting, \
                   deformed hands, watermark, text",
        note: "Straight photography, graded like film. The default when a shot needs to be believed \
               rather than admired.",
    },
    Style {
        id: "orthographic_sheet",
        label: "Design Sheet",
        family: Family::Design,
        prompt: "character and prop design sheet, orthographic views, neutral grey background, \
                 callout annotations, consistent line weight, turnaround layout, clean presentation",
        negative: "dramatic lighting, cluttered background, painterly texture, photograph, blurry",
        note: "Flat turnarounds on grey. Not a picture to publish — a reference to keep a character \
               consistent across everything that is.",
    },
];

/// A named blend of two or three styles.
///
/// `weights` are emphasis multipliers, applied through ComfyUI's `(term:weight)` syntax. Above 1.0
/// pushes a style forward, below pulls it back; the dominant one is listed first purely for reading.
#[derive(Debug, Clone, Serialize)]
pub struct StyleMix {
    pub id: &'static str,
    pub label: &'static str,
    /// `(style id, emphasis)`.
    pub parts: &'static [(&'static str, f32)],
    pub note: &'static str,
}

pub const MIXES: &[StyleMix] = &[
    StyleMix {
        id: "trash_polka_gospel",
        label: "Trash Polka Gospel",
        parts: &[("trash_polka", 1.25), ("ink_wash", 0.75)],
        note: "Trash polka's red-and-black aggression, opened up by ink wash's empty space so it \
               stops being solid noise. Room for text, and it still reads at thumbnail size.",
    },
    StyleMix {
        id: "trash_polka_collage",
        label: "Trash Polka Collage",
        parts: &[("trash_polka", 1.15), ("collage", 0.9)],
        note: "The full torn-paper version — tape, staples and xerox on top of the red and black. \
               Busiest thing here; put nothing else on the frame.",
    },
    StyleMix {
        id: "watercolor_light",
        label: "Watercolour Light",
        parts: &[("watercolor_fx", 1.2), ("stained_glass", 0.7)],
        note: "Wet bleeds carrying stained-glass colour and a hint of leading. Luminous rather than \
               heavy — the one for grace rather than judgement.",
    },
    StyleMix {
        id: "watercolor_ink",
        label: "Watercolour & Ink",
        parts: &[("watercolor_fx", 1.1), ("ink_wash", 0.95)],
        note: "Loose wash with a confident black line through it. The most forgiving mix here — it \
               rarely produces anything ugly.",
    },
    StyleMix {
        id: "concept_cinematic",
        label: "Cinematic Concept",
        parts: &[("concept_art", 1.2), ("cinematic_photo", 0.8)],
        note: "Concept art pulled toward a real lens: keeps the scale and atmosphere, loses the \
               illustrated tell. The safest choice for a hero frame.",
    },
    StyleMix {
        id: "concept_baroque",
        label: "Baroque Concept",
        parts: &[("concept_art", 1.0), ("dark_baroque", 1.1)],
        note: "Production design lit like an old master — one light, deep shadow, large scale. \
               Reverent without looking like a church bulletin.",
    },
    StyleMix {
        id: "pop_praise",
        label: "Pop Art Praise",
        parts: &[("pop_art", 1.25), ("risograph", 0.8)],
        note: "Halftone dots with riso's fluorescent misregistration on top. Loud, modern, and the \
               most thumb-stopping thing in the catalogue.",
    },
    StyleMix {
        id: "pop_glass",
        label: "Pop Glass",
        parts: &[("pop_art", 1.0), ("stained_glass", 1.0)],
        note: "Flat saturated panels behind heavy black lines — pop art and stained glass want the \
               same things, so this blends cleanly where most cross-family mixes do not.",
    },
    StyleMix {
        id: "print_shop",
        label: "Print Shop",
        parts: &[("linocut", 1.15), ("risograph", 0.85)],
        note: "Carved shapes printed in spot colour, slightly off-register. Hand-made and cheap in \
               the good way.",
    },
];

// ── Composition ─────────────────────────────────────────────────────────────

pub fn style(id: &str) -> Option<&'static Style> { STYLES.iter().find(|s| s.id == id) }
pub fn mix(id: &str) -> Option<&'static StyleMix> { MIXES.iter().find(|m| m.id == id) }

/// A resolved look: the two strings a graph actually needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Look {
    pub prompt: String,
    pub negative: String,
}

/// Wrap terms in ComfyUI emphasis, unless the weight is neutral.
///
/// Skipping the wrapper at 1.0 matters: `(term:1.0)` is not identical to `term` in every parser, and
/// an unweighted prompt is the one people have already tuned their expectations against.
fn emphasise(terms: &str, weight: f32) -> String {
    let terms = terms.trim();
    if (weight - 1.0).abs() < 0.01 { return terms.to_string(); }
    // Parentheses inside the terms would nest and change the weight in ways nobody intends.
    let safe = terms.replace('(', "").replace(')', "");
    format!("({safe}:{weight:.2})")
}

/// Merge negative-prompt lists, keeping order and dropping repeats.
///
/// Union rather than blend — see the module header. Order is kept because the first terms in a
/// negative carry the most weight, so a style's own priorities survive being combined with another's.
fn merge_negatives(parts: &[&str]) -> String {
    let mut seen: Vec<String> = Vec::new();
    for part in parts {
        for term in part.split(',') {
            let t = term.trim();
            if t.is_empty() { continue }
            let key = t.to_ascii_lowercase();
            if !seen.iter().any(|s| s.to_ascii_lowercase() == key) { seen.push(t.to_string()); }
        }
    }
    seen.join(", ")
}

/// Resolve a style id, a mix id, or a comma-separated ad-hoc blend into one look.
///
/// Accepting all three in one function is deliberate: the stored setting is a single string, and the
/// caller should not have to know which kind it is holding.
pub fn resolve(id: &str) -> Option<Look> {
    let id = id.trim();
    if id.is_empty() { return None }

    if let Some(s) = style(id) {
        return Some(Look { prompt: s.prompt.trim().to_string(), negative: s.negative.trim().to_string() });
    }
    if let Some(m) = mix(id) {
        let mut prompts = Vec::new();
        let mut negs = Vec::new();
        for (sid, w) in m.parts {
            let Some(s) = style(sid) else { continue };
            prompts.push(emphasise(s.prompt, *w));
            negs.push(s.negative);
        }
        if prompts.is_empty() { return None }
        return Some(Look { prompt: prompts.join(", "), negative: merge_negatives(&negs) });
    }
    // Ad-hoc: "trash_polka+watercolor_fx" — an unnamed blend a user built in the picker. Equal
    // weights, because there is no way to express emphasis in this form and guessing would be worse.
    if id.contains('+') {
        let ids: Vec<&str> = id.split('+').map(str::trim).filter(|s| !s.is_empty()).collect();
        let found: Vec<&Style> = ids.iter().filter_map(|i| style(i)).collect();
        if found.is_empty() { return None }
        return Some(Look {
            prompt: found.iter().map(|s| s.prompt.trim().to_string()).collect::<Vec<_>>().join(", "),
            negative: merge_negatives(&found.iter().map(|s| s.negative).collect::<Vec<_>>()),
        });
    }
    None
}

/// Every look a picker can offer, styles and mixes together.
pub fn catalogue() -> serde_json::Value {
    serde_json::json!({
        "styles": STYLES.iter().map(|s| serde_json::json!({
            "id": s.id, "label": s.label, "family": s.family, "note": s.note,
            "prompt": s.prompt, "negative": s.negative,
        })).collect::<Vec<_>>(),
        "mixes": MIXES.iter().map(|m| serde_json::json!({
            "id": m.id, "label": m.label, "note": m.note,
            "parts": m.parts.iter().map(|(sid, w)| serde_json::json!({
                "style": sid, "weight": w,
                "label": style(sid).map(|s| s.label).unwrap_or(sid),
            })).collect::<Vec<_>>(),
            "prompt": resolve(m.id).map(|l| l.prompt).unwrap_or_default(),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_resolves_to_a_usable_look() {
        for s in STYLES {
            let look = resolve(s.id).unwrap_or_else(|| panic!("{} did not resolve", s.id));
            assert!(look.prompt.len() > 20, "{} has a thin prompt", s.id);
            // See the module header: a style that does not say what to avoid gets the checkpoint's
            // defaults instead of itself.
            assert!(look.negative.len() > 10, "{} has no negative", s.id);
        }
    }

    #[test]
    fn every_mix_names_real_styles_and_resolves() {
        for m in MIXES {
            assert!(m.parts.len() >= 2, "{} is not a mix", m.id);
            for (sid, w) in m.parts {
                assert!(style(sid).is_some(), "{} names unknown style {sid}", m.id);
                assert!(*w > 0.3 && *w < 2.0, "{} weights {sid} at {w}, outside a useful range", m.id);
            }
            // Every part has to survive into the prompt. Not "carries a weight" — an evenly
            // balanced mix is legitimately unweighted, and asserting otherwise would contradict
            // `a_neutral_weight_is_left_unwrapped` below.
            let look = resolve(m.id).unwrap();
            for (sid, _) in m.parts {
                let first = style(sid).unwrap().prompt.split(',').next().unwrap().trim();
                let head = first.split_whitespace().next().unwrap();
                assert!(look.prompt.contains(head),
                        "{} lost its {sid} half: {}", m.id, look.prompt);
            }
        }
    }

    /// The point of a mix is that emphasis survives into the prompt. Plain concatenation would pass
    /// a "contains both styles" test while producing a different picture.
    #[test]
    fn a_mix_carries_its_weights() {
        let look = resolve("trash_polka_gospel").unwrap();
        assert!(look.prompt.contains("1.25"), "dominant weight missing: {}", look.prompt);
        assert!(look.prompt.contains("0.75"), "accent weight missing: {}", look.prompt);
    }

    #[test]
    fn a_neutral_weight_is_left_unwrapped() {
        // pop_glass is 1.0/1.0 — it must read as a plain prompt, not as (…:1.00).
        let look = resolve("pop_glass").unwrap();
        assert!(!look.prompt.contains(":1.00"), "neutral weights should not be wrapped: {}", look.prompt);
    }

    #[test]
    fn negatives_merge_without_repeating() {
        let look = resolve("watercolor_ink").unwrap();
        let blurry = look.negative.matches("blurry").count();
        assert_eq!(blurry, 1, "'blurry' appears {blurry} times: {}", look.negative);
        // Both parents' distinctive terms have to survive the merge.
        assert!(look.negative.contains("photograph"));
        assert!(look.negative.contains("color") || look.negative.contains("colour"));
    }

    #[test]
    fn an_ad_hoc_blend_works_without_being_a_named_mix() {
        let look = resolve("trash_polka+pop_art").unwrap();
        assert!(look.prompt.contains("trash polka"));
        assert!(look.prompt.contains("ben-day"));
        assert!(resolve("trash_polka+nonsense").is_some(), "an unknown half should not void the blend");
        assert!(resolve("nonsense+rubbish").is_none());
    }

    #[test]
    fn nothing_resolves_from_nothing() {
        assert!(resolve("").is_none());
        assert!(resolve("   ").is_none());
        assert!(resolve("no_such_style").is_none());
    }

    /// The four the catalogue was built around must exist under stable ids.
    #[test]
    fn the_requested_styles_are_present() {
        for id in ["trash_polka", "concept_art", "watercolor_fx", "pop_art"] {
            assert!(style(id).is_some(), "{id} is missing");
        }
    }

    /// Ids have to be unique or the lookups silently pick the first.
    #[test]
    fn ids_are_unique_across_styles_and_mixes() {
        let mut all: Vec<&str> = STYLES.iter().map(|s| s.id).collect();
        all.extend(MIXES.iter().map(|m| m.id));
        let before = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(before, all.len(), "duplicate id between styles and mixes");
    }

    /// Emphasis syntax must not nest — `((a:1.2):0.8)` weights in a way nobody intends.
    #[test]
    fn emphasis_never_nests() {
        let out = emphasise("a (parenthetical) term", 1.3);
        assert_eq!(out.matches('(').count(), 1, "nested parens in {out}");
        assert_eq!(out.matches(')').count(), 1, "nested parens in {out}");
    }

    #[test]
    fn the_catalogue_serialises_for_the_picker() {
        let c = catalogue();
        assert_eq!(c["styles"].as_array().unwrap().len(), STYLES.len());
        assert_eq!(c["mixes"].as_array().unwrap().len(), MIXES.len());
        // The picker previews a mix by its resolved prompt, so it must be there.
        assert!(!c["mixes"][0]["prompt"].as_str().unwrap().is_empty());
    }
}
