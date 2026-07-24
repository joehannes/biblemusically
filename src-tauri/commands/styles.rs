// Style presets, preset packs, and per-channel sticky styles.
//
// A **style preset** is a named bundle of ComfyUI image parameters (`params`, using the same
// `comfyui_*` keys as Settings) grouped into a **pack**. A **channel style** pins a set of those
// params to a YouTube channel so every image generated for that channel's language automatically
// inherits the look — persisted cross-session in the `channel_styles` collection.

use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn to_value(d: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in d {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

/// The `comfyui_*` keys a preset / channel style may carry. Kept explicit so a malformed payload
/// can't inject arbitrary settings, and so resolution knows exactly what to overlay.
const STYLE_KEYS: &[&str] = &[
    "comfyui_style", "comfyui_ckpt", "comfyui_steps", "comfyui_cfg",
    "comfyui_width", "comfyui_height", "comfyui_negative", "comfyui_ip_weight",
    "comfyui_prompt_prefix",
];

fn sanitize_params(raw: &Value) -> Document {
    let mut d = Document::new();
    if let Some(obj) = raw.as_object() {
        for k in STYLE_KEYS {
            if let Some(v) = obj.get(*k) {
                if let Ok(b) = bson::to_bson(v) { d.insert(*k, b); }
            }
        }
    }
    d
}

fn builtin_presets() -> Vec<Value> {
    // Two starter packs; users add more from the Style Studio.
    vec![
        json!({ "id":"builtin-photoreal-cinematic", "name":"Photoreal Cinematic", "pack":"Cinematic", "builtin":true,
            "params":{ "comfyui_style":"photoreal", "comfyui_steps":32, "comfyui_cfg":6.5, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"cinematic composition, dramatic film lighting, subtle film grain", "comfyui_negative":"lowres, oversaturated" } }),
        json!({ "id":"builtin-photoreal-soft", "name":"Photoreal Soft Portrait", "pack":"Cinematic", "builtin":true,
            "params":{ "comfyui_style":"photoreal", "comfyui_steps":30, "comfyui_cfg":5.5, "comfyui_width":896, "comfyui_height":1152,
                       "comfyui_prompt_prefix":"soft natural window light, shallow depth of field, gentle tones", "comfyui_ip_weight":0.8 } }),
        json!({ "id":"builtin-comic-bold", "name":"Bold Comic", "pack":"Illustrated", "builtin":true,
            "params":{ "comfyui_style":"comic", "comfyui_steps":28, "comfyui_cfg":7.0, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"bold clean linework, vivid saturated palette, dynamic angle" } }),
        json!({ "id":"builtin-graphic-novel-noir", "name":"Graphic Novel Noir", "pack":"Illustrated", "builtin":true,
            "params":{ "comfyui_style":"graphic_novel", "comfyui_steps":30, "comfyui_cfg":6.5, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"high-contrast noir shading, moody rim light, textured ink" } }),
        json!({ "id":"builtin-watercolor-dream", "name":"Watercolor Dream", "pack":"Illustrated", "builtin":true,
            "params":{ "comfyui_style":"watercolor", "comfyui_steps":30, "comfyui_cfg":6.0, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"loose expressive washes, luminous color bleed" } }),
        json!({ "id":"builtin-anime-key", "name":"Anime Key Visual", "pack":"Illustrated", "builtin":true,
            "params":{ "comfyui_style":"anime", "comfyui_steps":28, "comfyui_cfg":6.5, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"anime key visual, clean cel shading, expressive eyes, studio quality" } }),
        json!({ "id":"builtin-oil-master", "name":"Oil Painting (Old Master)", "pack":"Painterly", "builtin":true,
            "params":{ "comfyui_style":"oil_painting", "comfyui_steps":34, "comfyui_cfg":6.5, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"old master oil painting, chiaroscuro, rich impasto, gilded warmth" } }),
        json!({ "id":"builtin-fresco-sacred", "name":"Sacred Fresco", "pack":"Sacred", "builtin":true,
            "params":{ "comfyui_style":"oil_painting", "comfyui_steps":32, "comfyui_cfg":6.5, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"renaissance fresco, sacred iconography, soft gold leaf, reverent light, muted earth palette" } }),
        json!({ "id":"builtin-stained-glass", "name":"Stained Glass", "pack":"Sacred", "builtin":true,
            "params":{ "comfyui_style":"illustration", "comfyui_steps":30, "comfyui_cfg":7.0, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"stained glass window, bold lead lines, luminous jewel tones, backlit glow" } }),
        json!({ "id":"builtin-concept-epic", "name":"Epic Concept Art", "pack":"Concept", "builtin":true,
            "params":{ "comfyui_style":"photoreal", "comfyui_steps":34, "comfyui_cfg":7.0, "comfyui_width":1152, "comfyui_height":896,
                       "comfyui_prompt_prefix":"epic concept art, vast scale, atmospheric depth, volumetric light, matte painting" } }),
        json!({ "id":"builtin-synthwave-retro", "name":"Synthwave Retro", "pack":"Stylized", "builtin":true,
            "params":{ "comfyui_style":"illustration", "comfyui_steps":28, "comfyui_cfg":6.5, "comfyui_width":1152, "comfyui_height":896,
                       "comfyui_prompt_prefix":"synthwave, neon grid, retro-futuristic sunset, chromatic glow, 80s aesthetic" } }),
        json!({ "id":"builtin-paper-cut", "name":"Layered Paper-Cut", "pack":"Stylized", "builtin":true,
            "params":{ "comfyui_style":"illustration", "comfyui_steps":28, "comfyui_cfg":6.5, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"layered paper cut art, soft depth shadows, tactile craft, warm storybook feel" } }),
        json!({ "id":"builtin-charcoal", "name":"Charcoal Sketch", "pack":"Painterly", "builtin":true,
            "params":{ "comfyui_style":"graphic_novel", "comfyui_steps":30, "comfyui_cfg":6.0, "comfyui_width":1024, "comfyui_height":1024,
                       "comfyui_prompt_prefix":"expressive charcoal sketch, smudged shading, raw gestural lines, monochrome" } }),
        json!({ "id":"builtin-golden-hour", "name":"Golden Hour Film", "pack":"Cinematic", "builtin":true,
            "params":{ "comfyui_style":"photoreal", "comfyui_steps":32, "comfyui_cfg":6.0, "comfyui_width":1152, "comfyui_height":896,
                       "comfyui_prompt_prefix":"golden hour, warm backlight, lens flare, 35mm film, nostalgic tones" } }),
    ]
}

#[tauri::command]
pub async fn list_style_presets(state: State<'_, AppState>) -> Res<Value> {
    let coll = state.db.collection::<Document>("style_presets");
    // Upsert the built-ins every load (keyed by _id) so newly-added packs reach existing users too;
    // user-created presets are untouched. (Previously this only seeded when the collection was
    // empty, so anyone who had opened Style Studio once never saw new built-in packs.)
    for p in builtin_presets() {
        if let Ok(mut d) = bson::to_document(&p) {
            let id = p["id"].as_str().unwrap_or("").to_string();
            d.insert("_id", &id);
            let _ = coll.update_one(doc! { "_id": &id }, doc! { "$set": &d }).upsert(true).await;
        }
    }
    use futures_util::StreamExt;
    let mut cursor = coll.find(doc! {}).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(to_value(d)); }
    Ok(json!({ "presets": out }))
}

#[tauri::command]
pub async fn save_style_preset(state: State<'_, AppState>, payload: Value) -> Res<Value> {
    let coll = state.db.collection::<Document>("style_presets");
    let id = payload["id"].as_str().filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let name = payload["name"].as_str().unwrap_or("Untitled").to_string();
    let pack = payload["pack"].as_str().unwrap_or("Custom").to_string();
    let params = sanitize_params(&payload["params"]);
    let mut d = doc! {
        "id": &id, "name": &name, "pack": &pack, "params": &params, "builtin": false,
    };
    d.insert("_id", &id);
    coll.update_one(doc! { "_id": &id }, doc! { "$set": &d }).upsert(true).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id, "name": name, "pack": pack, "params": to_value(doc!{"params": params})["params"].clone() }))
}

#[tauri::command]
pub async fn delete_style_preset(state: State<'_, AppState>, id: String) -> Res<Value> {
    // Built-ins are protected (their ids are prefixed "builtin-").
    if id.starts_with("builtin-") {
        return Err("Built-in presets cannot be deleted.".into());
    }
    state.db.collection::<Document>("style_presets")
        .delete_one(doc! { "_id": &id }).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id }))
}

#[tauri::command]
pub async fn get_channel_style(state: State<'_, AppState>, channel_id: String) -> Res<Value> {
    let d = state.db.collection::<Document>("channel_styles")
        .find_one(doc! { "_id": &channel_id }).await.map_err(e)?;
    Ok(match d { Some(d) => to_value(d), None => json!({ "channel_id": channel_id, "params": {} }) })
}

#[tauri::command]
pub async fn set_channel_style(state: State<'_, AppState>, channel_id: String, payload: Value) -> Res<Value> {
    let params = sanitize_params(&payload["params"]);
    let preset_id = payload["preset_id"].as_str().unwrap_or("").to_string();
    let mut d = doc! { "channel_id": &channel_id, "preset_id": &preset_id, "params": &params };
    d.insert("_id", &channel_id);
    state.db.collection::<Document>("channel_styles")
        .update_one(doc! { "_id": &channel_id }, doc! { "$set": &d }).upsert(true).await.map_err(e)?;
    Ok(json!({ "ok": true, "channel_id": channel_id, "preset_id": preset_id }))
}

/// Resolve the sticky style overrides for a given song language: the "general + per-channel
/// specialization" hierarchy — first try the channel-specific pin (matched by language, set via
/// Style Studio / Channel Manager), and if the channel has none pinned, fall back to the
/// project-wide default pack (`global_channel_settings.default_style_preset_id`, set in Channel
/// Manager's Global Settings). Returns the `comfyui_*` params object to overlay onto the job
/// settings, or `None` if neither resolves to anything. Used by the image job.
pub async fn channel_style_overrides(db: &mongodb::Database, language: &str, project_id: Option<&str>) -> Option<Value> {
    let language = language.trim();
    if !language.is_empty() {
        // Case-insensitive language match against channels.
        if let Some(ch) = db.collection::<Document>("channels")
            .find_one(doc! { "language": { "$regex": format!("^{}$", regex_escape(language)), "$options": "i" } })
            .await.ok().flatten()
        {
            if let Ok(channel_id) = ch.get_str("id") {
                if let Some(style) = db.collection::<Document>("channel_styles")
                    .find_one(doc! { "_id": channel_id }).await.ok().flatten()
                {
                    if let Ok(params) = style.get_document("params") {
                        if !params.is_empty() {
                            return Some(to_value(doc! { "params": params.clone() })["params"].clone());
                        }
                    }
                }
            }
        }
    }

    // No channel-specific pin (or no language to match on) — fall back to the project-wide
    // default pack, if one's configured.
    let filter = match project_id {
        Some(pid) if !pid.is_empty() => doc! { "project_id": pid },
        _ => doc! { "_id": "singleton" },
    };
    let global = db.collection::<Document>("global_channel_settings").find_one(filter).await.ok().flatten()?;
    let preset_id = global.get_str("default_style_preset_id").ok()?;
    if preset_id.is_empty() { return None; }
    let preset = db.collection::<Document>("style_presets").find_one(doc! { "_id": preset_id }).await.ok().flatten()?;
    let params = preset.get_document("params").ok()?;
    if params.is_empty() { return None; }
    Some(to_value(doc! { "params": params.clone() })["params"].clone())
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\^$.|?*+()[]{}".contains(c) { out.push('\\'); }
        out.push(c);
    }
    out
}

// ────────────────────────────────────────────────────────────────
// Genre-mix presets (music). Each preset is a named, packaged fused style string plus the source
// genres that produced it, so a mix can be re-loaded, tweaked, and re-applied to songs.
// ────────────────────────────────────────────────────────────────

fn builtin_genre_presets() -> Vec<Value> {
    vec![
        json!({ "id":"builtin-playful-edm-bounce", "name":"Playful EDM Bounce", "pack":"Playful EDM", "builtin":true,
            "genres":["future bounce","playful melodic house","bright supersaw"],
            "styles":"future bounce, playful melodic house, bright supersaw leads, bouncy plucks, vocal chops, sunny festival energy, cheeky rhythm, 126 bpm" }),
        json!({ "id":"builtin-playful-edm-swing", "name":"Playful Electro Swing", "pack":"Playful EDM", "builtin":true,
            "genres":["electro swing","house","vintage brass"],
            "styles":"electro swing, four-on-the-floor house groove, vintage brass stabs, cheeky clarinet, hand claps, playful bounce, retro-future fun, 122 bpm" }),
        json!({ "id":"builtin-playful-edm-tropical", "name":"Playful Tropical Pop-EDM", "pack":"Playful EDM", "builtin":true,
            "genres":["tropical house","playful pop","steel drums"],
            "styles":"tropical house, playful pop toplines, steel drum plucks, marimba, warm sub bass, breezy carefree mood, whistle hook, 108 bpm" }),
        json!({ "id":"builtin-playful-chiptune-dance", "name":"Playful Chiptune Dance", "pack":"Playful", "builtin":true,
            "genres":["chiptune","dance pop","8-bit"],
            "styles":"chiptune, dance pop, 8-bit arpeggios, punchy square leads, cheerful melody, arcade energy, hand claps, 128 bpm" }),
        json!({ "id":"builtin-playful-funk", "name":"Playful Funk Pop", "pack":"Playful", "builtin":true,
            "genres":["funk","pop","brass"],
            "styles":"funk pop, slap bass, bright brass, wah guitar, upbeat groove, feel-good vibe, playful vocal ad-libs, 112 bpm" }),
        json!({ "id":"builtin-cinematic-uplift", "name":"Cinematic Uplift", "pack":"Cinematic", "builtin":true,
            "genres":["cinematic","orchestral","epic"],
            "styles":"cinematic orchestral, soaring strings, epic brass, big percussion, triumphant build, emotional swell, 90 bpm" }),

        // ── EDM mixes ─────────────────────────────────────────────────────
        json!({ "id":"builtin-edm-melodic-progressive", "name":"Melodic Progressive House", "pack":"EDM", "builtin":true,
            "genres":["progressive house","melodic techno","anjunadeep"],
            "styles":"melodic progressive house, deep rolling bassline, lush pad chords, arpeggiated plucks, euphoric breakdown, warm analog synths, emotional drop, 124 bpm" }),
        json!({ "id":"builtin-edm-future-bass-trap", "name":"Future Bass × Trap", "pack":"EDM", "builtin":true,
            "genres":["future bass","trap","edm"],
            "styles":"future bass, hybrid trap, detuned supersaw chords, pitched vocal chops, 808 sub, snappy hats, big festival drop, 150 bpm" }),
        json!({ "id":"builtin-edm-techno-dnb", "name":"Techno × Drum'n'Bass", "pack":"EDM", "builtin":true,
            "genres":["techno","liquid dnb","neurofunk"],
            "styles":"driving techno intro morphing into liquid drum and bass, rolling reese bass, crisp breakbeats, hypnotic stabs, dark energy, 174 bpm" }),
        json!({ "id":"builtin-edm-synthwave-house", "name":"Synthwave × House", "pack":"EDM", "builtin":true,
            "genres":["synthwave","nu-disco","house"],
            "styles":"synthwave meets nu-disco house, retro analog bass, gated reverb drums, neon arps, saxophone lead, 80s nostalgia, 118 bpm" }),

        // ── Latino mixes ──────────────────────────────────────────────────
        json!({ "id":"builtin-latino-reggaeton-pop", "name":"Reggaeton Pop", "pack":"Latino", "builtin":true,
            "genres":["reggaeton","latin pop","dembow"],
            "styles":"reggaeton, latin pop, dembow rhythm, warm dembow kick, catchy spanish hooks, bright synth plucks, party energy, 95 bpm" }),
        json!({ "id":"builtin-latino-cumbia-electro", "name":"Electro Cumbia", "pack":"Latino", "builtin":true,
            "genres":["cumbia","electronica","tropical"],
            "styles":"electro cumbia, accordion melody, tropical percussion, güira, digital cumbia synths, danceable groove, festive, 100 bpm" }),
        json!({ "id":"builtin-latino-salsa-jazz", "name":"Salsa × Latin Jazz", "pack":"Latino", "builtin":true,
            "genres":["salsa","latin jazz","montuno"],
            "styles":"salsa dura with latin jazz, montuno piano, tight brass section, congas and timbales, walking bass, joyful clave groove, 190 bpm" }),
        json!({ "id":"builtin-latino-bachata-acoustic", "name":"Acoustic Bachata", "pack":"Latino", "builtin":true,
            "genres":["bachata","acoustic","bolero"],
            "styles":"modern bachata, requinto guitar lead, syncopated bongo, romantic bolero feel, tender spanish vocals, warm and intimate, 130 bpm" }),

        // ── US American ───────────────────────────────────────────────────
        json!({ "id":"builtin-us-country-pop", "name":"Country Pop", "pack":"US American", "builtin":true,
            "genres":["country","pop","americana"],
            "styles":"modern country pop, acoustic guitar, slide guitar licks, foot-stomp percussion, heartfelt storytelling vocals, anthemic chorus, 100 bpm" }),
        json!({ "id":"builtin-us-gospel-soul", "name":"Gospel Soul", "pack":"US American", "builtin":true,
            "genres":["gospel","soul","r&b"],
            "styles":"gospel soul, hammond organ, powerful choir stacks, warm electric piano, hand claps, uplifting call-and-response, 76 bpm" }),
        json!({ "id":"builtin-us-hiphop-boombap", "name":"Boom-Bap Hip-Hop", "pack":"US American", "builtin":true,
            "genres":["hip-hop","boom bap","soul sample"],
            "styles":"boom-bap hip-hop, dusty soul sample, punchy kick and snare, vinyl crackle, jazzy keys, head-nod groove, 90 bpm" }),

        // ── European ──────────────────────────────────────────────────────
        json!({ "id":"builtin-eu-celtic-folk", "name":"Celtic Folk", "pack":"European", "builtin":true,
            "genres":["celtic","folk","irish"],
            "styles":"celtic folk, tin whistle, fiddle, bodhrán drum, acoustic guitar, uilleann pipes, rousing jig energy, 110 bpm" }),
        json!({ "id":"builtin-eu-italo-disco", "name":"Italo Disco", "pack":"European", "builtin":true,
            "genres":["italo disco","synth-pop","hi-nrg"],
            "styles":"italo disco, pulsing octave bass, glittering arpeggios, gated claps, dreamy 80s synth leads, romantic european flair, 122 bpm" }),
        json!({ "id":"builtin-eu-balkan-brass", "name":"Balkan Brass Fusion", "pack":"European", "builtin":true,
            "genres":["balkan brass","gypsy","electro"],
            "styles":"balkan brass band fused with electro, frantic tuba bassline, blazing trumpets, odd-meter groove, hand percussion, wild celebration, 140 bpm" }),

        // ── Jewish / Ancient Jewish ───────────────────────────────────────
        json!({ "id":"builtin-jewish-klezmer", "name":"Klezmer Celebration", "pack":"Jewish", "builtin":true,
            "genres":["klezmer","folk","freylekh"],
            "styles":"klezmer, soaring clarinet, freylekh dance groove, violin, accordion, tsimbl hammered dulcimer, festive freylekhs, minor-key joy, 128 bpm" }),
        json!({ "id":"builtin-jewish-ancient-temple", "name":"Ancient Temple Chant", "pack":"Ancient Jewish", "builtin":true,
            "genres":["ancient","middle eastern","sacred"],
            "styles":"ancient hebrew temple music, oud, ney flute, frame drum, kinnor lyre, shofar calls, modal middle-eastern melody, reverent and timeless, 70 bpm" }),
        json!({ "id":"builtin-jewish-sephardic", "name":"Sephardic Ladino", "pack":"Ancient Jewish", "builtin":true,
            "genres":["sephardic","ladino","mediterranean"],
            "styles":"sephardic ladino song, spanish-arabic guitar, oud, hand percussion, mediterranean modal vocals, ornamented melody, warm and ancient, 96 bpm" }),

        // ── Messianic Jewish ──────────────────────────────────────────────
        json!({ "id":"builtin-messianic-worship", "name":"Messianic Worship", "pack":"Messianic Jewish", "builtin":true,
            "genres":["messianic","worship","hebraic"],
            "styles":"messianic worship, hebraic melody, acoustic guitar, violin, davidic dance groove, hebrew and english praise vocals, joyful and reverent, 100 bpm" }),
        json!({ "id":"builtin-messianic-davidic-dance", "name":"Davidic Dance", "pack":"Messianic Jewish", "builtin":true,
            "genres":["messianic","klezmer","dance"],
            "styles":"messianic davidic dance, upbeat hebraic klezmer groove, clarinet and violin, hand claps, tambourine, celebratory circle-dance energy, accelerating tempo, 132 bpm" }),
        json!({ "id":"builtin-messianic-ballad", "name":"Messianic Ballad", "pack":"Messianic Jewish", "builtin":true,
            "genres":["messianic","worship ballad","cinematic"],
            "styles":"messianic worship ballad, gentle piano, strings, hebraic modal melody, intimate prayer vocals, building cinematic swell, tender and holy, 68 bpm" }),
        // ── Regional × EDM experimental fusions — currently-popular regional sounds crossed with
        //    club/festival electronics, one per world region.
        json!({ "id":"builtin-redm-amapiano", "name":"Amapiano Deep Fusion", "pack":"Regional EDM Fusion", "builtin":true,
            "genres":["amapiano","deep house","afro-electronic"],
            "styles":"amapiano, log drum bassline, deep house shuffle, soulful South African vocal chops, airy piano stabs, spacious groove, late-night warmth, 112 bpm" }),
        json!({ "id":"builtin-redm-afrohouse", "name":"Afro House Voltage", "pack":"Regional EDM Fusion", "builtin":true,
            "genres":["afro house","tribal percussion","festival EDM"],
            "styles":"afro house, driving tribal percussion, west african call vocals, hypnotic synth arps, festival-size drops, earthy and electric, 122 bpm" }),
        json!({ "id":"builtin-redm-latintech", "name":"Reggaeton Tech-House", "pack":"Regional EDM Fusion", "builtin":true,
            "genres":["reggaeton","tech house","latin club"],
            "styles":"reggaeton dembow groove fused with tech house, rolling latin percussion, spanish vocal hooks, dark club bass, sweaty dancefloor energy, 100 bpm" }),
        json!({ "id":"builtin-redm-brazilianbass", "name":"Brazilian Bass Carnival", "pack":"Regional EDM Fusion", "builtin":true,
            "genres":["brazilian bass","baile funk","big room"],
            "styles":"brazilian bass, baile funk drums, carnival whistle accents, gritty low-end house bounce, portuguese chant hooks, big room lift, 126 bpm" }),
        json!({ "id":"builtin-redm-arabictrap", "name":"Arabic Trap Mirage", "pack":"Regional EDM Fusion", "builtin":true,
            "genres":["arabic trap","oriental scale","EDM"],
            "styles":"arabic trap, maqam-inflected oud and ney melodies, 808 slides, desert-wide reverb, oriental strings over festival drops, mysterious heat, 140 bpm" }),
        json!({ "id":"builtin-redm-desibass", "name":"Desi Bass Bhangra-EDM", "pack":"Regional EDM Fusion", "builtin":true,
            "genres":["bhangra","bollywood","bass house"],
            "styles":"bhangra dhol rhythms fused with bass house, punjabi vocal hooks, tumbi riffs, bollywood string swells, festival drops, jubilant energy, 128 bpm" }),
        json!({ "id":"builtin-redm-kwave", "name":"K-Wave Hyper-Pop EDM", "pack":"Regional EDM Fusion", "builtin":true,
            "genres":["k-pop","future bass","hyperpop"],
            "styles":"k-pop hooks over future bass, glossy hyperpop synths, tight vocal stacks, kinetic drops, neon-bright polish, addictive chorus, 128 bpm" }),
        json!({ "id":"builtin-redm-balkanbass", "name":"Balkan Brass Bass", "pack":"Regional EDM Fusion", "builtin":true,
            "genres":["balkan brass","electro swing","bass music"],
            "styles":"balkan brass fanfares over heavy electro bass, odd-meter breaks, accordion riffs, wild wedding-party energy crossed with club drops, 130 bpm" }),
        json!({ "id":"builtin-redm-celtictrance", "name":"Celtic Trance Voyage", "pack":"Regional EDM Fusion", "builtin":true,
            "genres":["celtic folk","uplifting trance"],
            "styles":"celtic fiddle and whistle themes over uplifting trance, supersaw swells, bodhran-driven builds, misty coastal atmosphere, euphoric release, 138 bpm" }),
        json!({ "id":"builtin-redm-nordicmelodic", "name":"Nordic Melodic Techno", "pack":"Regional EDM Fusion", "builtin":true,
            "genres":["melodic techno","nordic folk"],
            "styles":"melodic techno pulse with nordic folk vocal drones, icy pads, hardanger fiddle echoes, slow-burning hypnotic build, aurora-cold beauty, 122 bpm" }),
    ]
}

#[tauri::command]
pub async fn list_genre_presets(state: State<'_, AppState>) -> Res<Value> {
    let coll = state.db.collection::<Document>("genre_presets");
    // Upsert the builtins every load (not only when the collection is empty) so newly-added packs
    // reach existing users too. Keyed by `_id`, so a user's own saved mixes are untouched and a
    // builtin the user edited is refreshed to the current definition.
    for p in builtin_genre_presets() {
        if let Ok(mut d) = bson::to_document(&p) {
            let id = p["id"].as_str().unwrap_or("").to_string();
            d.insert("_id", &id);
            let _ = coll.update_one(doc! { "_id": &id }, doc! { "$set": &d }).upsert(true).await;
        }
    }
    use futures_util::StreamExt;
    let mut cursor = coll.find(doc! {}).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(to_value(d)); }
    Ok(json!({ "presets": out }))
}

#[tauri::command]
pub async fn save_genre_preset(state: State<'_, AppState>, payload: Value) -> Res<Value> {
    let coll = state.db.collection::<Document>("genre_presets");
    let id = payload["id"].as_str().filter(|s| !s.is_empty())
        .map(|s| s.to_string()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let name = payload["name"].as_str().unwrap_or("Untitled Mix").to_string();
    let pack = payload["pack"].as_str().unwrap_or("Custom").to_string();
    let styles = payload["styles"].as_str().unwrap_or("").to_string();
    let genres = bson::to_bson(&payload["genres"]).unwrap_or(bson::Bson::Array(vec![]));
    let mut d = doc! { "id": &id, "name": &name, "pack": &pack, "styles": &styles, "genres": genres, "builtin": false };
    d.insert("_id", &id);
    coll.update_one(doc! { "_id": &id }, doc! { "$set": &d }).upsert(true).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id, "name": name, "pack": pack, "styles": styles }))
}

#[tauri::command]
pub async fn delete_genre_preset(state: State<'_, AppState>, id: String) -> Res<Value> {
    if id.starts_with("builtin-") { return Err("Built-in presets cannot be deleted.".into()); }
    state.db.collection::<Document>("genre_presets").delete_one(doc! { "_id": &id }).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id }))
}

// ────────────────────────────────────────────────────────────────
// Transition preset packs — each defines a vocabulary of ffmpeg xfade transitions plus a default,
// used by the AI transition suggester and the manual editor.
// ────────────────────────────────────────────────────────────────

fn builtin_transition_presets() -> Vec<Value> {
    vec![
        json!({ "id":"builtin-tr-gentle", "name":"Gentle", "builtin":true, "default":"fade",
            "transitions":["fade","dissolve","fadeblack","smoothleft","smoothright"] }),
        json!({ "id":"builtin-tr-dynamic", "name":"Dynamic", "builtin":true, "default":"slideleft",
            "transitions":["slideleft","slideright","slideup","wipeleft","wipeup","pixelize","zoomin"] }),
        json!({ "id":"builtin-tr-dreamy", "name":"Dreamy", "builtin":true, "default":"dissolve",
            "transitions":["dissolve","circleopen","circleclose","radial","fadewhite","distance"] }),
        json!({ "id":"builtin-tr-cinematic", "name":"Cinematic", "builtin":true, "default":"fadeblack",
            "transitions":["fadeblack","fade","smoothup","wipedown","diagtl"] }),
    ]
}

#[tauri::command]
pub async fn list_transition_presets(state: State<'_, AppState>) -> Res<Value> {
    let coll = state.db.collection::<Document>("transition_presets");
    if coll.count_documents(doc! {}).await.map_err(e)? == 0 {
        for p in builtin_transition_presets() {
            if let Ok(mut d) = bson::to_document(&p) {
                let id = p["id"].as_str().unwrap_or("").to_string();
                d.insert("_id", &id);
                let _ = coll.insert_one(d).await;
            }
        }
    }
    use futures_util::StreamExt;
    let mut cursor = coll.find(doc! {}).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(to_value(d)); }
    Ok(json!({ "presets": out }))
}

#[tauri::command]
pub async fn save_transition_preset(state: State<'_, AppState>, payload: Value) -> Res<Value> {
    let coll = state.db.collection::<Document>("transition_presets");
    let id = payload["id"].as_str().filter(|s| !s.is_empty())
        .map(|s| s.to_string()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let name = payload["name"].as_str().unwrap_or("Custom Pack").to_string();
    let default_t = payload["default"].as_str().unwrap_or("fade").to_string();
    let transitions = bson::to_bson(&payload["transitions"]).unwrap_or(bson::Bson::Array(vec![]));
    let mut d = doc! { "id": &id, "name": &name, "default": &default_t, "transitions": transitions, "builtin": false };
    d.insert("_id", &id);
    coll.update_one(doc! { "_id": &id }, doc! { "$set": &d }).upsert(true).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id, "name": name }))
}

#[tauri::command]
pub async fn delete_transition_preset(state: State<'_, AppState>, id: String) -> Res<Value> {
    if id.starts_with("builtin-") { return Err("Built-in packs cannot be deleted.".into()); }
    state.db.collection::<Document>("transition_presets").delete_one(doc! { "_id": &id }).await.map_err(e)?;
    Ok(json!({ "ok": true, "id": id }))
}
