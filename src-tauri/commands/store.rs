//! The storefront: what gets printed, how the art sits on it, and what the listing says.
//!
//! The print-on-demand path worked and was thin in three places, each of which shows up as a worse
//! product rather than as an error:
//!
//!   * **The art was whichever one came first.** `make_products` walked this song's sections and
//!     `break`ed on the first file that existed. A picture chosen by iteration order — and the same
//!     one for a 2:3 poster and a square mug, which want different pictures, not a different crop of
//!     one.
//!   * **The scale meant two different things.** `print_quality` returns a *pixel ratio* used to work
//!     out DPI, and it was passed to Printify as `scale`, which is the image's width as a fraction of
//!     the print area's width. They agree only when the art is exactly the placeholder's size.
//!     Everywhere else, art larger than the print area was being shrunk into the middle of it: a
//!     4000px design on a 2000px area went on at half width, centred, with white all around.
//!   * **Every listing said the same thing.** One hard-coded description — "Scripture set to music,
//!     printed on demand" — on every product of every project. Two people selling completely
//!     different things got byte-identical copy.
//!
//! ## Flavours
//!
//! A **flavour** is what kind of shop this is: a devotional gift shop, a gallery of art prints, a
//! children's range, a memorial line. It decides the copy's register, which products are worth
//! carrying, how the art should be framed, and how pricing works. It is one choice that sets a dozen
//! defaults, and every one of them can still be overridden — the point is that a person who has not
//! thought about any of it gets something coherent rather than something generic.
//!
//! ## Other providers
//!
//! Printify is the one with a working integration here, and this module says plainly what the others
//! would take. That is the same honesty `list_ebook_stores` and `list_distributors` already practise:
//! a provider listed as "supported" that has never been called is worse than one listed as "not
//! wired up, and here is what it would need".

use serde_json::{json, Value};

// ────────────────────────────────────────────────────────────────
// How the art sits on the product
// ────────────────────────────────────────────────────────────────

/// Printify's `scale` is the image's width as a fraction of the **print area's width**, not a pixel
/// ratio. 1.0 means "as wide as the printable area".
///
/// Two ways to place art whose shape does not match the area's:
///   * **fit** — the whole image is on the product, with empty area on two sides. Never crops.
///   * **fill** — the area is covered edge to edge and the overflow is lost off two sides.
///
/// Which is right is a property of the product, not a preference: a poster is the art, so it fits; a
/// mug wraps and would show a band of blank, so it fills. The flavour picks a default and the
/// product can override.
pub fn placement_scale(placeholder_w: i64, placeholder_h: i64, art_w: i64, art_h: i64, fill: bool) -> f64 {
    if placeholder_w <= 0 || placeholder_h <= 0 || art_w <= 0 || art_h <= 0 { return 1.0; }
    let art_ratio = art_w as f64 / art_h as f64;
    let area_ratio = placeholder_w as f64 / placeholder_h as f64;
    // Placed at width = scale × area_width, the image's height is that over its own aspect. Setting
    // that equal to the area's height gives exactly art_ratio / area_ratio.
    let to_match_height = art_ratio / area_ratio;
    let scale = if fill { to_match_height.max(1.0) } else { to_match_height.min(1.0) };
    // Printify rejects a scale above 1 on most blueprints; a fill that would need more than the
    // area's width is achieved by cropping the source instead, which `crop_loss` reports.
    (scale.clamp(0.05, 1.0) * 1000.0).round() / 1000.0
}

/// How much of the artwork is lost when it is made to fill an area of a different shape.
///
/// Reported rather than silently accepted: a square design filling a wide banner loses a third of its
/// height, and that is fine for a pattern and fatal for anything with a face in it.
pub fn crop_loss(placeholder_w: i64, placeholder_h: i64, art_w: i64, art_h: i64) -> f64 {
    if placeholder_w <= 0 || placeholder_h <= 0 || art_w <= 0 || art_h <= 0 { return 0.0; }
    let art_ratio = art_w as f64 / art_h as f64;
    let area_ratio = placeholder_w as f64 / placeholder_h as f64;
    let kept = if art_ratio > area_ratio { area_ratio / art_ratio } else { art_ratio / area_ratio };
    ((1.0 - kept) * 1000.0).round() / 1000.0
}

/// Choose the artwork for one print area, rather than taking whichever came first.
///
/// `candidates` are `{path, width, height}`. The ranking, in order:
///   1. **Enough resolution to print.** Below 150 DPI at this size nothing else matters.
///   2. **Shape.** The nearer the art's aspect is to the area's, the less is cropped or wasted.
///   3. **Size.** Between two equally well-shaped candidates, the larger one.
///
/// Returns `None` only when there are no candidates at all — a poor-but-printable image is still an
/// answer, and refusing to choose leaves the caller with nothing to offer.
pub fn pick_art<'a>(candidates: &'a [Value], placeholder_w: i64, placeholder_h: i64) -> Option<&'a Value> {
    if candidates.is_empty() { return None; }
    let area_ratio = if placeholder_h > 0 { placeholder_w as f64 / placeholder_h as f64 } else { 1.0 };

    candidates.iter().max_by(|a, b| {
        let score = |v: &Value| {
            let w = v["width"].as_i64().unwrap_or(0);
            let h = v["height"].as_i64().unwrap_or(0);
            if w <= 0 || h <= 0 { return f64::MIN; }
            let dpi = if placeholder_w > 0 { 300.0 * w as f64 / placeholder_w as f64 } else { 300.0 };
            // A hard step rather than a smooth term: 149 DPI and 151 DPI are not nearly the same
            // product, and a beautifully shaped blurry print is still a blurry print.
            let printable = if dpi >= 150.0 { 1000.0 } else { 0.0 };
            let art_ratio = w as f64 / h as f64;
            let shape = 1.0 - (art_ratio.ln() - area_ratio.ln()).abs().min(2.0) / 2.0;
            printable + shape * 100.0 + (w.min(h) as f64 / 10_000.0)
        };
        score(a).partial_cmp(&score(b)).unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// The pixel size of a PNG or JPEG, read from its header.
///
/// Needed before the file is uploaded, because choosing which artwork suits a print area is a
/// decision made locally — Printify only reports dimensions after an upload, by which point the
/// choice has already been made. Two integers is not worth a decoder dependency, and both headers
/// are fixed enough to read directly:
///
///   * **PNG** — the IHDR chunk is always first, so width and height are big-endian u32 at a known
///     offset. There is exactly one place to look.
///   * **JPEG** — no fixed offset. Walk the marker segments to the first SOF (start-of-frame), whose
///     payload carries height then width. Progressive and arithmetic-coded variants use different
///     SOF numbers, hence the range rather than a single value.
///
/// `None` for anything else, which the caller must treat as "unknown size" rather than as zero.
pub fn image_size(bytes: &[u8]) -> Option<(i64, i64)> {
    const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() >= 24 && bytes.starts_with(PNG_MAGIC) && &bytes[12..16] == b"IHDR" {
        let be = |o: usize| u32::from_be_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]) as i64;
        let (w, h) = (be(16), be(20));
        return (w > 0 && h > 0).then_some((w, h));
    }

    if bytes.len() > 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i = 2usize;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF { i += 1; continue; }          // resync over padding bytes
            let marker = bytes[i + 1];
            // Standalone markers carry no length, so stepping over a length here would desynchronise.
            if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) { i += 2; continue; }
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            // SOF0..SOF15, less the four that are not frame headers (DHT, JPGA, DAC, DNL).
            let is_sof = (0xC0..=0xCF).contains(&marker)
                && marker != 0xC4 && marker != 0xC8 && marker != 0xCC;
            if is_sof {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as i64;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as i64;
                return (w > 0 && h > 0).then_some((w, h));
            }
            if len < 2 { return None; }                        // a malformed segment, not a size
            i += 2 + len;
        }
    }
    None
}

/// The size of an image on disk, or `None` when it cannot be read or understood.
///
/// Only the header is read — a 40 MB print file does not need to be loaded to learn it is 4000 wide.
pub fn image_size_of(path: &std::path::Path) -> Option<(i64, i64)> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; 64 * 1024];
    let read = file.read(&mut head).ok()?;
    head.truncate(read);
    image_size(&head)
}

// ────────────────────────────────────────────────────────────────
// Flavours
// ────────────────────────────────────────────────────────────────

pub struct Flavour {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    /// What the listing copy should sound like. Handed to the model verbatim.
    pub copy_direction: &'static str,
    /// Blueprint categories worth carrying, as Printify names them.
    pub categories: &'static [&'static str],
    /// Whether art fills the print area by default or fits inside it.
    pub fill: bool,
    /// Multiplier on the provider's cost to reach a retail price.
    pub markup: f64,
    /// The most words the printed phrase may be. A poster can take a stanza; a mug cannot.
    pub max_phrase_words: usize,
}

/// Five, because they are the shops people actually open, and each one changes more than a tone.
pub const FLAVOURS: &[Flavour] = &[
    Flavour {
        id: "devotional", label: "Devotional gifts", fill: false, markup: 2.2, max_phrase_words: 8,
        hint: "A verse worth keeping, on something somebody gives away.",
        categories: &["Mugs", "Home Decor", "Accessories"],
        copy_direction: "Write for somebody choosing a gift for a person they love. Warm, plain, \
                         unhurried. Say what is on it and who it might suit. No urgency, no \
                         marketing verbs, no exclamation marks — this is a quiet purchase and the \
                         copy should sound like one.",
    },
    Flavour {
        id: "art_print", label: "Art prints", fill: false, markup: 2.8, max_phrase_words: 4,
        hint: "The image is the product. Wall pieces, framed and unframed.",
        categories: &["Wall Art", "Home Decor"],
        copy_direction: "Write as a gallery would: what the piece depicts, its palette, the room it \
                         suits. One sentence on how it was made. Never explain the meaning of the \
                         image — a buyer is looking at it and does not need it described back.",
    },
    Flavour {
        id: "wearable", label: "Things people wear", fill: true, markup: 2.4, max_phrase_words: 6,
        hint: "Shirts, hoodies, caps. The line has to work with no context.",
        categories: &["T-shirts", "Hoodies", "Hats"],
        copy_direction: "Write for somebody deciding whether they would wear this in front of \
                         strangers. Say in two sentences what the line means and where it comes from, \
                         without preaching. Mention the fit and the fabric — on clothing that is what \
                         the questions are actually about.",
    },
    Flavour {
        id: "children", label: "For children", fill: true, markup: 2.0, max_phrase_words: 5,
        hint: "Bright, simple, and written for the adult who is buying.",
        categories: &["Kids' Clothing", "Baby", "Home Decor"],
        copy_direction: "Write for the adult buying for a child. Warm and simple, naming the age it \
                         suits and what the picture shows. Say something concrete about washing or \
                         durability. Never write in a baby voice — the reader is a grown-up.",
    },
    Flavour {
        id: "memorial", label: "Memorial and keepsake", fill: false, markup: 2.6, max_phrase_words: 10,
        hint: "For a loss. The most careful copy in the shop.",
        categories: &["Home Decor", "Accessories", "Wall Art"],
        copy_direction: "Write for somebody buying in grief, or for somebody in it. Be plain and \
                         short. Do not console, do not promise comfort, and do not use the word \
                         'journey'. State what the object is and what can be put on it. Restraint is \
                         the whole register.",
    },
];

pub fn flavour(id: &str) -> &'static Flavour {
    FLAVOURS.iter().find(|f| f.id == id).unwrap_or(&FLAVOURS[0])
}

/// Whether a blueprint's title belongs to a flavour's categories.
///
/// Printify has over a thousand blueprints and the catalogue call filters by a substring of the
/// title. A flavour already knows which categories are worth carrying, so a shop that has said it
/// sells art prints should not have to type "poster" to find one — but it must still be able to,
/// because a category list is a starting point and not a fence.
///
/// Matched on words rather than on a raw substring: "Hat" as a substring also matches "Hatchback"
/// and, less absurdly, "Whatnot" — and a category filter that quietly drops the product somebody
/// wanted is worse than no filter.
pub fn suits_flavour(title: &str, categories: &[&str]) -> bool {
    if categories.is_empty() { return true; }
    let words: Vec<String> = title.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.trim_end_matches('s').to_string())
        .collect();
    categories.iter().any(|cat| {
        cat.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(|w| w.trim_end_matches('s').to_string())
            .any(|needle| words.iter().any(|w| *w == needle))
    })
}

/// The search terms a flavour would use, for a catalogue that filters by one substring at a time.
pub fn flavour_searches(id: &str) -> Vec<&'static str> {
    flavour(id).categories.to_vec()
}

/// A retail price from a provider's cost, rounded the way shops actually round.
///
/// Charm pricing is not superstition here: a price ending in 00 reads as provisional next to one
/// ending in 99, and every storefront in this category prices that way — one that does not looks
/// like a mistake. The floor exists because a markup on a very cheap item can produce a price that
/// does not cover the platform's own cut.
pub fn retail_price(cost_cents: i64, markup: f64, floor_cents: i64) -> i64 {
    let floor = floor_cents.max(0);
    if cost_cents <= 0 { return floor; }
    let raw = (cost_cents as f64 * markup).round() as i64;
    let raised = raw.max(floor);
    // Round *up* to the next .99 — never down, which would eat the margin the markup was chosen for.
    let hundreds = raised.div_euclid(100);
    if raised.rem_euclid(100) <= 99 { hundreds * 100 + 99 } else { (hundreds + 1) * 100 + 99 }
}

// ────────────────────────────────────────────────────────────────
// Providers
// ────────────────────────────────────────────────────────────────

pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub region: &'static str,
    /// Whether this app can actually drive it today.
    pub wired: bool,
    pub strength: &'static str,
    /// What it would take to wire up, for the ones that are not. Written for somebody deciding
    /// whether it is worth asking for, rather than as a task description.
    pub what_it_needs: &'static str,
}

/// The four fulfilment APIs worth naming, and what each is actually for.
///
/// Listed with `wired` rather than silently omitted, because "which of these can I use" is the
/// question somebody has, and an app that shows only the one it implemented answers a different one.
pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "printify", label: "Printify", region: "global, many print partners", wired: true,
        strength: "The widest catalogue of any single supplier, and several competing print partners \
                   per product, so a blueprint can be sourced where it is cheapest or nearest.",
        what_it_needs: "",
    },
    Provider {
        id: "printful", label: "Printful", region: "global, own facilities", wired: false,
        strength: "Its own factories rather than a partner network, so quality varies less between \
                   orders, and the strongest branding options in the category — custom labels, \
                   packaging inserts, branded packing slips.",
        what_it_needs: "A personal access token, and its own product and variant model — close \
                        enough to Printify's that the placement maths here carries over. The \
                        catalogue call and the order webhook are the two pieces that differ.",
    },
    Provider {
        id: "gelato", label: "Gelato", region: "local production in around 32 countries", wired: false,
        strength: "Prints near the buyer rather than shipping across the world, which is the whole \
                   argument for it: shorter delivery and lower freight for an audience spread over \
                   several countries — which is exactly what a multi-language channel has.",
        what_it_needs: "An API key and a store connection. Its product identifiers are its own, so \
                        the picked-products list would need a second mapping rather than a rename.",
    },
    Provider {
        id: "gooten", label: "Gooten", region: "70+ regions", wired: false,
        strength: "Automated routing across a fulfilment network rather than a single supplier, which \
                   matters at volume more than at the first hundred orders.",
        what_it_needs: "An API key. Worth doing once the volume exists rather than before — its \
                        advantage is routing, and routing one order a day is the same as not routing \
                        it.",
    },
];

#[tauri::command]
pub async fn store_flavours() -> Result<Value, String> {
    Ok(json!({
        "flavours": FLAVOURS.iter().map(|f| json!({
            "id": f.id, "label": f.label, "hint": f.hint,
            "categories": f.categories, "fill": f.fill,
            "markup": f.markup, "max_phrase_words": f.max_phrase_words,
        })).collect::<Vec<_>>(),
        "providers": PROVIDERS.iter().map(|p| json!({
            "id": p.id, "label": p.label, "region": p.region, "wired": p.wired,
            "strength": p.strength, "what_it_needs": p.what_it_needs,
        })).collect::<Vec<_>>(),
    }))
}

/// What the listing copy should sound like, for this shop and this project.
///
/// Pure, so the interface and the generator cannot disagree about the register a memorial line is
/// written in. Empty when nothing has been chosen — an unchosen flavour must not silently become
/// the devotional one inside a prompt.
pub fn store_prompt_block(profile: &Value) -> String {
    let id = profile.get("flavour").and_then(|v| v.as_str()).unwrap_or("").trim();
    if id.is_empty() { return String::new(); }
    let Some(f) = FLAVOURS.iter().find(|f| f.id == id) else { return String::new() };

    let mut lines = vec![
        format!("THIS SHOP — {}: {}", f.label, f.hint),
        format!("  {}", f.copy_direction),
    ];
    for (key, label) in [("brand", "Shop name"), ("audience", "Who buys here"), ("note", "House rules")] {
        let v = profile.get(key).and_then(|x| x.as_str()).unwrap_or("").trim();
        if !v.is_empty() { lines.push(format!("  {label}: {v}")); }
    }
    lines.push(format!(
        "  The printed line is at most {} words and has to stand alone on the object, with no song \
         and no context beside it.", f.max_phrase_words));
    lines.push(String::new());
    format!("{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(w: i64, h: i64) -> Value { json!({ "path": format!("{w}x{h}.png"), "width": w, "height": h }) }

    // ── placement ───────────────────────────────────────────────────────

    #[test]
    fn matching_shapes_use_the_whole_print_area() {
        // The commonest case, and the one the old code got right by accident.
        assert_eq!(placement_scale(2000, 2000, 4000, 4000, false), 1.0);
        assert_eq!(placement_scale(2000, 3000, 2000, 3000, true), 1.0);
    }

    #[test]
    fn art_larger_than_the_area_still_fills_it() {
        // The bug this replaces: the old scale was a pixel ratio, so a 4000px design on a 2000px
        // area went on at half width, centred, with white all around.
        assert_eq!(placement_scale(2000, 2000, 4000, 4000, false), 1.0);
        assert_eq!(placement_scale(1000, 1000, 8000, 8000, false), 1.0);
    }

    #[test]
    fn a_tall_image_in_a_wide_area_is_narrowed_to_fit_rather_than_cropped() {
        // 1:2 art in a 2:1 area. To get the whole height on, it may only be a quarter of the width.
        assert_eq!(placement_scale(2000, 1000, 1000, 2000, false), 0.25);
        // Filling instead would need more width than there is, so it clamps and the loss is reported.
        assert_eq!(placement_scale(2000, 1000, 1000, 2000, true), 1.0);
        assert_eq!(crop_loss(2000, 1000, 1000, 2000), 0.75);
    }

    #[test]
    fn a_wide_image_in_a_tall_area_is_the_mirror_case() {
        assert_eq!(placement_scale(1000, 2000, 2000, 1000, false), 1.0);
        assert_eq!(crop_loss(1000, 2000, 2000, 1000), 0.75);
    }

    #[test]
    fn nothing_lost_when_the_shapes_agree() {
        assert_eq!(crop_loss(1200, 1800, 2400, 3600), 0.0);
        // And a square into a slightly-wide area loses a little, not nothing.
        let loss = crop_loss(1100, 1000, 1000, 1000);
        assert!(loss > 0.0 && loss < 0.15, "{loss}");
    }

    #[test]
    fn an_unknown_size_places_the_art_across_the_whole_area_rather_than_vanishing() {
        // A zero would make the product blank; 1.0 is the recoverable wrong answer.
        assert_eq!(placement_scale(0, 0, 1000, 1000, false), 1.0);
        assert_eq!(placement_scale(1000, 1000, 0, 500, true), 1.0);
        assert_eq!(crop_loss(0, 0, 10, 10), 0.0);
    }

    // ── choosing the art ────────────────────────────────────────────────

    #[test]
    fn resolution_beats_shape_because_a_blurry_print_is_a_blurry_print() {
        // A perfectly shaped image too small to print loses to a worse-shaped one that can.
        let candidates = vec![art(400, 400), art(3000, 1500)];
        let picked = pick_art(&candidates, 2000, 2000).unwrap();
        assert_eq!(picked["width"], 3000);
    }

    #[test]
    fn among_printable_candidates_the_best_shaped_one_wins() {
        // All three print fine at this size; only one is square like the area.
        let candidates = vec![art(3000, 1000), art(2400, 2400), art(1000, 3000)];
        let picked = pick_art(&candidates, 2000, 2000).unwrap();
        assert_eq!(picked["width"], 2400);
    }

    #[test]
    fn size_only_decides_between_equally_well_shaped_candidates() {
        let candidates = vec![art(2000, 2000), art(4000, 4000)];
        assert_eq!(pick_art(&candidates, 2000, 2000).unwrap()["width"], 4000);
    }

    #[test]
    fn a_different_print_area_gets_a_different_picture_from_the_same_set() {
        // The whole point: a 2:3 poster and a square mug want different images, not different crops
        // of one. The old code took whichever came first, for both.
        let candidates = vec![art(2400, 3600), art(2400, 2400), art(3600, 2400)];
        let poster = pick_art(&candidates, 1600, 2400).unwrap();
        let mug = pick_art(&candidates, 2000, 2000).unwrap();
        let banner = pick_art(&candidates, 2400, 1600).unwrap();
        assert_eq!(poster["height"], 3600);
        assert_eq!(mug["height"], 2400);
        assert_eq!(banner["width"], 3600);
        assert_ne!(poster["path"], mug["path"]);
    }

    #[test]
    fn nothing_to_choose_from_is_the_only_case_that_returns_nothing() {
        assert!(pick_art(&[], 2000, 2000).is_none());
        // A poor candidate is still an answer — refusing to choose leaves nothing to offer.
        let poor = vec![art(200, 200)];
        assert!(pick_art(&poor, 4000, 4000).is_some());
        // And an entry with no dimensions never wins over one that has them.
        let mixed = vec![json!({ "path": "unknown.png" }), art(2000, 2000)];
        assert_eq!(pick_art(&mixed, 2000, 2000).unwrap()["width"], 2000);
    }

    // ── reading a size off disk ─────────────────────────────────────────

    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v
    }

    /// A JPEG with `before` filler segments ahead of the SOF0, which is where the size lives.
    fn jpeg_header(w: u16, h: u16, before: usize) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8];
        for _ in 0..before {
            v.extend_from_slice(&[0xFF, 0xE0]);            // APP0
            v.extend_from_slice(&10u16.to_be_bytes());
            v.extend_from_slice(&[0u8; 8]);
        }
        v.extend_from_slice(&[0xFF, 0xC0]);                 // SOF0
        v.extend_from_slice(&17u16.to_be_bytes());
        v.push(8);
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&[3u8; 10]);
        v
    }

    #[test]
    fn a_png_size_comes_from_its_one_fixed_place() {
        assert_eq!(image_size(&png_header(4000, 2500)), Some((4000, 2500)));
        assert_eq!(image_size(&png_header(1, 1)), Some((1, 1)));
        // Truncated before IHDR is unknown, not zero.
        assert_eq!(image_size(&png_header(100, 100)[..20]), None);
    }

    #[test]
    fn a_jpeg_size_is_found_however_many_segments_precede_it() {
        // The reason this cannot be a fixed offset: EXIF, ICC and thumbnails all sit before the SOF.
        assert_eq!(image_size(&jpeg_header(3000, 2000, 0)), Some((3000, 2000)));
        assert_eq!(image_size(&jpeg_header(3000, 2000, 1)), Some((3000, 2000)));
        assert_eq!(image_size(&jpeg_header(3000, 2000, 12)), Some((3000, 2000)));
    }

    #[test]
    fn a_jpeg_marker_that_carries_no_length_does_not_desynchronise_the_walk() {
        // A restart marker mid-stream has no length field; treating it as though it did would step
        // into the middle of the next segment and report a plausible wrong size.
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xD0, 0xFF, 0x01];
        v.extend_from_slice(&jpeg_header(1234, 567, 0)[2..]);
        assert_eq!(image_size(&v), Some((1234, 567)));
    }

    #[test]
    fn anything_that_is_not_a_readable_image_is_unknown_rather_than_wrong() {
        assert_eq!(image_size(&[]), None);
        assert_eq!(image_size(b"not an image at all, just some bytes here"), None);
        // A JPEG that starts correctly and then lies about a segment length stops rather than
        // wandering off the end.
        assert_eq!(image_size(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00, 0, 0, 0, 0, 0]), None);
        // A PNG magic with a zero dimension is not a usable size.
        assert_eq!(image_size(&png_header(0, 100)), None);
    }

    // ── pricing ─────────────────────────────────────────────────────────

    #[test]
    fn a_price_lands_on_a_charm_ending_and_never_below_the_markup() {
        // $8.40 cost at 2.2× is $18.48, which becomes $18.99 — up, never down.
        assert_eq!(retail_price(840, 2.2, 0), 1899);
        assert!(retail_price(840, 2.2, 0) as f64 >= 840.0 * 2.2);
        assert_eq!(retail_price(1000, 2.8, 0) % 100, 99);
        assert_eq!(retail_price(355, 2.0, 0), 799);
    }

    #[test]
    fn a_cheap_item_is_lifted_to_the_floor_because_a_markup_can_undercut_the_platform() {
        assert_eq!(retail_price(100, 2.0, 1200), 1299);
        assert!(retail_price(50, 2.0, 999) >= 999);
        // No cost known at all still yields the floor rather than a free product.
        assert_eq!(retail_price(0, 2.5, 1500), 1500);
        assert_eq!(retail_price(-10, 2.5, 0), 0);
    }

    #[test]
    fn an_exact_hundred_does_not_round_down_a_whole_pound() {
        // 2000 must not become 1999 — that is a penny of margin thrown away every sale, and worse,
        // it is the case a naive `- 1` gets wrong.
        assert!(retail_price(1000, 2.0, 0) >= 2000);
        assert_eq!(retail_price(1000, 2.0, 0), 2099);
    }

    // ── flavours ────────────────────────────────────────────────────────

    #[test]
    fn every_flavour_changes_more_than_a_tone() {
        for f in FLAVOURS {
            assert!(f.copy_direction.len() > 150, "{} is an adjective", f.id);
            assert!(!f.categories.is_empty(), "{} carries nothing", f.id);
            assert!(f.markup > 1.0 && f.markup < 5.0, "{} prices absurdly", f.id);
            assert!(f.max_phrase_words >= 3 && f.max_phrase_words <= 12, "{}", f.id);
        }
        // They differ on the things that matter, not only in wording.
        assert!(FLAVOURS.iter().any(|f| f.fill) && FLAVOURS.iter().any(|f| !f.fill));
        let markups: Vec<u64> = FLAVOURS.iter().map(|f| (f.markup * 100.0) as u64).collect();
        assert!(markups.iter().collect::<std::collections::HashSet<_>>().len() > 1);
    }

    #[test]
    fn the_memorial_flavour_is_the_one_that_says_what_not_to_write() {
        // The register that most needs stating, because the default marketing voice is unbearable here.
        let m = flavour("memorial");
        assert!(m.copy_direction.contains("Do not console"));
        assert!(m.copy_direction.contains("journey"), "the word to avoid is named");
    }

    #[test]
    fn an_unknown_flavour_falls_back_rather_than_panicking_but_says_nothing_in_a_prompt() {
        assert_eq!(flavour("nonsense").id, FLAVOURS[0].id);
        // In a prompt it must be silent: an unchosen flavour silently becoming the devotional one is
        // how a memorial line ends up written like a gift shop.
        assert_eq!(store_prompt_block(&json!({})), "");
        assert_eq!(store_prompt_block(&json!({ "flavour": "" })), "");
        assert_eq!(store_prompt_block(&json!({ "flavour": "nonsense" })), "");
    }

    #[test]
    fn the_block_carries_the_shops_own_words_when_it_has_any() {
        let block = store_prompt_block(&json!({
            "flavour": "art_print", "brand": "Lightkid Editions",
            "audience": "people who already own one", "note": "never say 'stunning'",
        }));
        assert!(block.contains("Art prints"));
        assert!(block.contains("gallery"));
        assert!(block.contains("Lightkid Editions"));
        assert!(block.contains("never say 'stunning'"));
        assert!(block.contains("at most 4 words"));
    }

    #[test]
    fn a_flavour_narrows_the_catalogue_by_word_and_not_by_substring() {
        let wear = flavour("wearable").categories;
        assert!(suits_flavour("Unisex Heavy Cotton Tee T-shirt", wear));
        assert!(suits_flavour("Embroidered Dad Hat", wear));
        assert!(suits_flavour("Unisex Hoodie", wear));
        assert!(!suits_flavour("Ceramic Mug 11oz", wear));
        // The reason it is word-based: a substring match on "hat" also catches these.
        assert!(!suits_flavour("Whatnot Shelf", wear));
        assert!(!suits_flavour("Hatchback Decal", wear));
    }

    #[test]
    fn plurals_match_in_both_directions() {
        // The catalogue says "T-shirt" and the category says "T-shirts", or the reverse.
        assert!(suits_flavour("Classic T-shirt", &["T-shirts"]));
        assert!(suits_flavour("Coffee Mugs Set", &["Mugs"]));
        assert!(suits_flavour("Framed Wall Art Poster", flavour("art_print").categories));
    }

    #[test]
    fn no_categories_means_no_filtering_rather_than_nothing_matching() {
        // A category list is a starting point, not a fence — an empty one must not empty the shop.
        assert!(suits_flavour("Anything At All", &[]));
        assert_eq!(flavour_searches("children").len(), flavour("children").categories.len());
    }

    // ── providers ───────────────────────────────────────────────────────

    #[test]
    fn exactly_one_provider_is_claimed_as_working_and_the_rest_say_what_they_need() {
        let wired: Vec<&Provider> = PROVIDERS.iter().filter(|p| p.wired).collect();
        assert_eq!(wired.len(), 1, "claiming an integration that has never been called is a lie");
        assert_eq!(wired[0].id, "printify");
        for p in PROVIDERS.iter().filter(|p| !p.wired) {
            assert!(p.what_it_needs.len() > 80, "{} does not say what it would take", p.id);
            assert!(p.strength.len() > 80, "{} does not say why anybody would want it", p.id);
        }
        // And the one that is wired has nothing outstanding to say.
        assert!(wired[0].what_it_needs.is_empty());
    }
}

// ────────────────────────────────────────────────────────────────
// The shop's own settings
// ────────────────────────────────────────────────────────────────

use crate::state::AppState;
use bson::{doc, Document};
use tauri::State;

fn bson_to_value(d: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in d {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

/// The fields a person edits. A patch is not a licence to rewrite the record.
const EDITABLE: &[&str] = &[
    "flavour", "brand", "audience", "note", "blurb", "markup", "price_floor_cents", "fill",
];

#[derive(serde::Deserialize)]
pub struct ProfileSave {
    pub project_id: String,
    #[serde(default)]
    pub patch: Value,
}

/// One shop per project, because two projects in one Printify account are two different shops.
///
/// Returns the flavour's own defaults merged under whatever has been set, so a caller never has to
/// know which fields were filled in — an unset markup is the flavour's markup, not zero.
#[tauri::command]
pub async fn store_profile(state: State<'_, AppState>, project_id: String) -> Result<Value, String> {
    let stored = state.db.collection::<Document>("store_profiles")
        .find_one(doc! { "project_id": &project_id }).await
        .map_err(|e| e.to_string())?
        .map(bson_to_value)
        .unwrap_or_else(|| json!({ "project_id": project_id.clone() }));

    let f = flavour(stored["flavour"].as_str().unwrap_or(""));
    let mut out = stored;
    if let Some(o) = out.as_object_mut() {
        o.entry("markup").or_insert(json!(f.markup));
        o.entry("fill").or_insert(json!(f.fill));
        o.entry("price_floor_cents").or_insert(json!(0));
        o.insert("max_phrase_words".into(), json!(f.max_phrase_words));
        o.insert("categories".into(), json!(f.categories));
        o.insert("flavour_label".into(), json!(f.label));
    }
    Ok(out)
}

#[tauri::command]
pub async fn save_store_profile(state: State<'_, AppState>, payload: ProfileSave) -> Result<Value, String> {
    let mut set = doc! { "project_id": &payload.project_id, "updated_at": crate::models::now_iso() };
    for key in EDITABLE {
        if let Some(v) = payload.patch.get(*key) {
            if let Ok(b) = bson::to_bson(v) { set.insert(*key, b); }
        }
    }
    state.db.collection::<Document>("store_profiles")
        .update_one(doc! { "project_id": &payload.project_id }, doc! { "$set": set })
        .with_options(crate::store::UpdateOptions::builder().upsert(true).build())
        .await.map_err(|e| e.to_string())?;
    store_profile(state, payload.project_id).await
}

#[derive(serde::Deserialize)]
pub struct PriceRequest {
    pub cost_cents: i64,
    #[serde(default)]
    pub project_id: String,
}

/// What a product should cost, and what is left after the platform takes its share.
///
/// Printify's own cut is already inside the cost it quotes, so the margin here is the whole of what
/// reaches the seller — which is worth stating plainly, because a 2.2× markup sounds like 120% profit
/// and is not.
#[tauri::command]
pub async fn store_price(state: State<'_, AppState>, payload: PriceRequest) -> Result<Value, String> {
    let profile = store_profile(state, payload.project_id).await.unwrap_or_default();
    let markup = profile["markup"].as_f64().unwrap_or(2.2);
    let floor = profile["price_floor_cents"].as_i64().unwrap_or(0);
    let retail = retail_price(payload.cost_cents, markup, floor);
    let margin = retail - payload.cost_cents.max(0);
    Ok(json!({
        "cost_cents": payload.cost_cents,
        "retail_cents": retail,
        "margin_cents": margin,
        "markup": markup,
        "margin_pct": if retail > 0 { (margin as f64 / retail as f64 * 100.0).round() } else { 0.0 },
        "note": if payload.cost_cents > 0 && margin < 300 {
            "Under three currency units of margin. Shipping and returns come out of this."
        } else { "" },
    }))
}
