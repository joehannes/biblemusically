use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
use tauri::State;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

fn bson_to_value(doc: Document) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in doc {
        if k == "_id" { continue; }
        if let Ok(jv) = bson::from_bson::<Value>(v) { m.insert(k, jv); }
    }
    Value::Object(m)
}

// ────────────────────────────────────────────────────────────────
// Print on demand, through Printify
//
// Printify is the one integration in this app with a real, documented REST API, and the request shapes
// below were taken from its OpenAPI specification rather than from memory:
//
//   POST /v1/uploads/images.json                       { file_name, contents | url } → { id }
//   POST /v1/shops/{shop}/products.json                { title, description, blueprint_id,
//                                                        print_provider_id, variants[], print_areas[] }
//   POST /v1/shops/{shop}/products/{id}/publish.json   { title, description, images, variants, … }
//
// Note the upload endpoint is account-level, not shop-level — a detail worth getting right, because the
// shop-scoped path returns 404 with no hint about why.
//
// Three things shape the design:
//
//   1. The rate limits are real and low where it matters: 600 requests/minute globally, 100/minute on
//      catalogue endpoints, and **200 product publishes per 30 minutes**. A daily run across fifty
//      channels and a dozen products walks into that, so publishing is paced here rather than
//      discovering the ceiling as a wall of 429s.
//   2. Print areas are 3000–4000 pixels. Generated art is 1024–2048. So the effective print resolution
//      is computed and reported before anything is created — an image that prints at 90 DPI is a
//      refund, and no amount of scaling in the API fixes it.
//   3. Nothing publishes without the toggle. A product on a storefront is a public commitment with a
//      price on it.
// ────────────────────────────────────────────────────────────────

const API: &str = "https://api.printify.com/v1";

/// Printify's own publish ceiling: 200 per 30 minutes. Everything else it allows is far above what a
/// daily run needs.
const PUBLISH_CAP: i64 = 200;
const PUBLISH_WINDOW_MIN: i64 = 30;

/// Storefronts a Printify shop can publish to, and what each one costs to list on.
///
/// Checked July 2026. The costs matter more here than anywhere else in the app: a daily generated
/// product on a per-listing storefront is a daily charge whether or not anything sells.
#[tauri::command]
pub async fn printify_storefronts() -> Res<Value> {
    Ok(json!({
        "storefronts": [
            {
                "id": "popup", "name": "Printify Pop-Up Store",
                "cost": "Free",
                "detail": "Printify hosts it; no external account, no listing fee, no monthly fee. The \
                           obvious default for a daily run — and the only one where publishing every \
                           day costs nothing.",
                "recommended": true,
            },
            {
                "id": "etsy", "name": "Etsy",
                "cost": "$0.20 per listing, plus transaction and payment fees",
                "detail": "Real buyer traffic, but every listing is charged and listings expire after \
                           four months. A product a day is about $6 a month in listing fees before a \
                           single sale.",
                "recommended": false,
            },
            {
                "id": "tiktok", "name": "TikTok Shop",
                "cost": "Commission only, no listing fee",
                "detail": "No fee to list, and it sits next to the shorts this app already cuts.",
                "recommended": true,
            },
            {
                "id": "ebay", "name": "eBay",
                "cost": "Commission, with a monthly allowance of free listings",
                "detail": "No monthly fee at low volume.",
                "recommended": false,
            },
            {
                "id": "shopify", "name": "Shopify",
                "cost": "Monthly subscription",
                "detail": "Full control and your own domain, but a fixed monthly cost from day one.",
                "recommended": false,
            },
            {
                "id": "woocommerce", "name": "WooCommerce",
                "cost": "Free plugin, you provide the hosting",
                "detail": "Free software on hosting you already pay for.",
                "recommended": false,
            },
        ],
        "note": "Connect a storefront inside Printify itself — the API lists the shops that result and \
                 publishes to them, but it cannot create the connection.",
        "checked": "July 2026",
    }))
}

// ── HTTP ────────────────────────────────────────────────────────────────────

async fn token(state: &AppState) -> Res<String> {
    let settings = state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let t = settings["printify_api_token"].as_str().unwrap_or("").trim().to_string();
    if t.is_empty() {
        return Err("No Printify token saved. Settings → Print on demand, then create a Personal \
                    Access Token in Printify under My Profile → Connections.".into());
    }
    Ok(t)
}

async fn get_json(token: &str, path: &str) -> Res<Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45)).build().map_err(e)?;
    let r = client.get(format!("{API}{path}"))
        .bearer_auth(token)
        .header("User-Agent", "ai-music-video-studio")
        .send().await.map_err(e)?;
    let status = r.status();
    let body = r.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(printify_error(status.as_u16(), &body));
    }
    serde_json::from_str(&body).map_err(|err| format!("Printify returned unreadable JSON: {err}"))
}

async fn post_json(token: &str, path: &str, payload: &Value) -> Res<Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120)).build().map_err(e)?;
    let r = client.post(format!("{API}{path}"))
        .bearer_auth(token)
        .header("User-Agent", "ai-music-video-studio")
        .json(payload)
        .send().await.map_err(e)?;
    let status = r.status();
    let body = r.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(printify_error(status.as_u16(), &body));
    }
    if body.trim().is_empty() { return Ok(json!({ "ok": true })); }
    // A 2xx whose body is not JSON is still a success — publish answers with an empty body.
    Ok(serde_json::from_str(&body).unwrap_or(json!({ "ok": true, "raw": body })))
}

/// Turn Printify's error bodies into something that says what to do.
///
/// Its 4xx responses carry the reason in `errors.reason` or `message`, and the two that actually happen
/// — a bad token and a rate limit — are worth naming outright rather than showing a status code.
pub fn printify_error(status: u16, body: &str) -> String {
    let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let detail = parsed["errors"]["reason"].as_str()
        .or_else(|| parsed["message"].as_str())
        .or_else(|| parsed["error"].as_str())
        .unwrap_or_else(|| body.trim())
        .chars().take(400).collect::<String>();
    match status {
        401 | 403 => format!(
            "Printify rejected the token ({status}). Create a new Personal Access Token in Printify \
             under My Profile → Connections and paste it into Settings. {detail}"),
        404 => format!("Printify has no such resource ({status}). {detail}"),
        422 => format!("Printify refused the product ({status}): {detail}"),
        429 => format!(
            "Printify rate limit reached ({status}). Publishing is capped at {PUBLISH_CAP} products \
             per {PUBLISH_WINDOW_MIN} minutes — the run will continue when the window clears. {detail}"),
        _ => format!("Printify error {status}: {detail}"),
    }
}

// ── Connect + catalogue ─────────────────────────────────────────────────────

/// Verify the token and list the shops it can reach.
#[tauri::command]
pub async fn printify_connect(state: State<'_, AppState>) -> Res<Value> {
    let t = token(&state).await?;
    let shops = get_json(&t, "/shops.json").await?;
    let list = shops.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        return Err("The token works, but this Printify account has no shop yet. Add a storefront in \
                    Printify first — the free Pop-Up Store needs no external account.".into());
    }
    Ok(json!({ "shops": list }))
}

#[derive(serde::Deserialize)]
pub struct CatalogRequest {
    /// Filter blueprints by title, case-insensitive. "shirt", "mug", "poster".
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// The catalogue, filtered — the full list is over a thousand blueprints.
#[tauri::command]
pub async fn printify_catalog(state: State<'_, AppState>, payload: CatalogRequest) -> Res<Value> {
    let t = token(&state).await?;
    let all = get_json(&t, "/catalog/blueprints.json").await?;
    let needle = payload.search.unwrap_or_default().to_lowercase();
    let limit = payload.limit.unwrap_or(40).min(200);
    let items: Vec<Value> = all.as_array().cloned().unwrap_or_default().into_iter()
        .filter(|b| needle.is_empty() || b["title"].as_str().unwrap_or("").to_lowercase().contains(&needle))
        .take(limit)
        .map(|b| json!({
            "id": b["id"], "title": b["title"], "brand": b["brand"], "model": b["model"],
            "images": b["images"],
        }))
        .collect();
    Ok(json!({ "blueprints": items, "total": all.as_array().map(|a| a.len()).unwrap_or(0) }))
}

/// Print providers and their variants for one blueprint, with the print-area sizes.
///
/// The placeholder dimensions are the reason this call matters: they are what decides whether generated
/// art is big enough to print, and they differ per provider for the same product.
#[tauri::command]
pub async fn printify_blueprint_detail(state: State<'_, AppState>, blueprint_id: i64) -> Res<Value> {
    let t = token(&state).await?;
    let providers = get_json(&t, &format!("/catalog/blueprints/{blueprint_id}/print_providers.json")).await?;
    let first = providers.as_array().and_then(|a| a.first()).cloned().unwrap_or(Value::Null);
    let provider_id = first["id"].as_i64().unwrap_or(0);
    let mut variants = Value::Null;
    if provider_id > 0 {
        variants = get_json(&t, &format!(
            "/catalog/blueprints/{blueprint_id}/print_providers/{provider_id}/variants.json")).await?;
    }
    Ok(json!({
        "blueprint_id": blueprint_id,
        "print_providers": providers,
        "print_provider_id": provider_id,
        "variants": variants["variants"],
    }))
}

// ── Print resolution ────────────────────────────────────────────────────────

/// Whether generated art can actually be printed at this size, and at what quality.
///
/// Printify placeholders are given in pixels at 300 DPI, so a 3000×4000 placeholder is a 10×13 inch
/// print. Art smaller than the placeholder is scaled up by the print process, and the effective DPI is
/// what the buyer sees. Under 150 the print is visibly soft; under 100 it is a refund.
pub fn print_quality(placeholder_w: i64, placeholder_h: i64, art_w: i64, art_h: i64) -> Value {
    if placeholder_w <= 0 || placeholder_h <= 0 || art_w <= 0 || art_h <= 0 {
        return json!({ "ok": false, "dpi": 0, "verdict": "unknown",
                       "advice": "The print area or the artwork size is unknown." });
    }
    // Fit the art inside the placeholder without distorting it — the same "contain" the API's
    // scale factor produces.
    let scale = (placeholder_w as f64 / art_w as f64).min(placeholder_h as f64 / art_h as f64);
    let dpi = (300.0 / scale).round() as i64;     // placeholders are quoted at 300 DPI
    let (verdict, advice) = if dpi >= 300 {
        ("excellent", "Prints at full quality.")
    } else if dpi >= 200 {
        ("good", "Slightly soft up close; fine on fabric.")
    } else if dpi >= 150 {
        ("acceptable", "Visibly soft on a hard surface like a mug or a poster. Fine on a t-shirt.")
    } else if dpi >= 100 {
        ("poor", "Generate this art at a higher resolution before selling it — this prints blurry.")
    } else {
        ("unusable", "Far too small to print. Regenerate the art at the product's own size.")
    };
    json!({
        "ok": dpi >= 150,
        "dpi": dpi,
        "scale": (scale * 1000.0).round() / 1000.0,
        "verdict": verdict,
        "advice": advice,
        "placeholder": { "width": placeholder_w, "height": placeholder_h },
        "art": { "width": art_w, "height": art_h },
    })
}

// ── Payloads ────────────────────────────────────────────────────────────────

/// The exact body Printify's product endpoint expects.
///
/// `price` is in **cents** — an integer. Passing dollars creates a product priced at a hundredth of
/// what was meant, which is the kind of mistake that only shows up on an order.
pub fn product_payload(
    title: &str,
    description: &str,
    blueprint_id: i64,
    print_provider_id: i64,
    variant_ids: &[i64],
    price_cents: i64,
    image_id: &str,
    position: &str,
    scale: f64,
) -> Value {
    json!({
        "title": title,
        "description": description,
        "blueprint_id": blueprint_id,
        "print_provider_id": print_provider_id,
        "variants": variant_ids.iter().map(|id| json!({
            "id": id, "price": price_cents, "is_enabled": true,
        })).collect::<Vec<_>>(),
        "print_areas": [{
            "variant_ids": variant_ids,
            "placeholders": [{
                "position": position,
                "images": [{
                    "id": image_id,
                    // Centred: 0.5/0.5 is the middle of the print area, not the top-left.
                    "x": 0.5, "y": 0.5,
                    "scale": scale,
                    "angle": 0,
                }],
            }],
        }],
    })
}

/// What to push to the storefront. Everything, because a product published without its variants is a
/// listing nobody can buy.
pub fn publish_payload() -> Value {
    json!({
        "title": true, "description": true, "images": true,
        "variants": true, "tags": true, "keyFeatures": true, "shipping_template": true,
    })
}

/// A phrase fit to print: short, no trailing punctuation, no quotes that will be set as glyphs.
///
/// The AI writes for a page; a mug takes about six words. Truncation happens on a word boundary,
/// because a phrase cut mid-word is worse than a shorter one.
pub fn print_phrase(raw: &str, max_words: usize) -> String {
    let cleaned = raw.trim()
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '“' || c == '”')
        .replace('\n', " ");
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let kept = if words.len() > max_words { &words[..max_words] } else { &words[..] };
    kept.join(" ").trim_end_matches(|c: char| c == ',' || c == ';' || c == ':' || c == '.').to_string()
}

/// How much room the publish window still has.
pub fn publish_pacing(recent: &[String], now: &str) -> Value {
    let cutoff = chrono::DateTime::parse_from_rfc3339(now)
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
        - chrono::Duration::minutes(PUBLISH_WINDOW_MIN);
    let cutoff = cutoff.to_rfc3339();
    let used = recent.iter().filter(|t| t.as_str() >= cutoff.as_str()).count() as i64;
    json!({
        "used": used, "cap": PUBLISH_CAP, "window_minutes": PUBLISH_WINDOW_MIN,
        "room": used < PUBLISH_CAP, "remaining": (PUBLISH_CAP - used).max(0),
    })
}

// ── Product preselection ────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct PickRequest {
    /// `[{ blueprint_id, print_provider_id, title, variant_ids[], position, price_cents,
    ///     placeholder_width, placeholder_height }]`
    pub products: Vec<Value>,
    #[serde(default)]
    pub shop_id: Option<String>,
}

/// Remember which products a daily run should apply art and wording to.
///
/// Picked once, deliberately: a daily run must not have to make product decisions, and a run that
/// silently chose different products every day would be impossible to reason about.
#[tauri::command]
pub async fn printify_pick_products(state: State<'_, AppState>, payload: PickRequest) -> Res<Value> {
    let doc_out = json!({
        "_id": "singleton",
        "products": payload.products,
        "shop_id": payload.shop_id.unwrap_or_default(),
        "updated_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&doc_out).map_err(e)?;
    state.db.collection::<Document>("printify_selection")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": d }).upsert(true)
        .await.map_err(e)?;
    Ok(doc_out)
}

#[tauri::command]
pub async fn printify_selection(state: State<'_, AppState>) -> Res<Value> {
    let sel = state.db.collection::<Document>("printify_selection")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or(json!({ "products": [] }));
    Ok(sel)
}

// ── Making one product ──────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct MakeRequest {
    pub shop_id: String,
    /// A song, for the wording and the art.
    pub song_id: String,
    /// Index into the saved selection; omitted means every picked product.
    #[serde(default)]
    pub product_index: Option<usize>,
    /// Overrides the AI-written phrase.
    #[serde(default)]
    pub phrase: Option<String>,
    /// Local image path to print. Omitted: the song's first finished image.
    #[serde(default)]
    pub art_path: Option<String>,
    /// Push it to the connected storefront. Off unless asked — a listing is public and priced.
    #[serde(default)]
    pub publish: Option<bool>,
}

/// Apply a phrase and artwork to the picked products and create them in Printify.
#[tauri::command]
pub async fn printify_make_products(state: State<'_, AppState>, payload: MakeRequest) -> Res<Value> {
    make_products(&state, &payload.shop_id, &payload.song_id, payload.phrase.as_deref(),
                  payload.publish.unwrap_or(false)).await
}

/// The work behind the command, callable from the scheduler.
///
/// Takes `&AppState` rather than Tauri's `State`, which is what lets the daily sweep run from a
/// background tick with no window involved.
pub async fn make_products(
    state: &AppState,
    shop_id: &str,
    song_id: &str,
    phrase_override: Option<&str>,
    publish: bool,
) -> Res<Value> {
    use futures_util::StreamExt;
    let t = token(state).await?;

    let song = state.db.collection::<Document>("songs")
        .find_one(doc! { "id": song_id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "song not found".to_string())?;

    let selection = state.db.collection::<Document>("printify_selection")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value).unwrap_or(json!({ "products": [] }));
    let picked = selection["products"].as_array().cloned().unwrap_or_default();
    if picked.is_empty() {
        return Err("No products picked yet. Choose them once in Print on Demand → Products.".into());
    }
    let targets: Vec<&Value> = picked.iter().collect();

    // The wording. Written by the AI from the song unless the user supplied it.
    let phrase = match phrase_override.filter(|p| !p.trim().is_empty()) {
        Some(p) => print_phrase(p, 12),
        None => {
            let settings = state.db.collection::<Document>("settings")
                .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
                .map(bson_to_value).unwrap_or_default();
            let system = "You write the single line of text that goes on a printed product.\n\
                          Rules: at most eight words; no quotation marks; no hashtags; no trailing \
                          punctuation; it must stand alone on a mug or a shirt without the song for \
                          context. Return ONLY the line.";
            let user = format!(
                "Song: {}\n\nLyrics:\n{}\n\nWrite the line.",
                song["title"].as_str().unwrap_or(""),
                song["lyrics"].as_str().unwrap_or("").chars().take(1200).collect::<String>(),
            );
            let (text, _) = crate::commands::ai::provider_chat(&settings, system, &user, 0.9, false).await?;
            print_phrase(&text, 8)
        }
    };

    // The art. Whatever the image pipeline already produced for this song, unless overridden.
    let art = {
            let mut found = String::new();
            let mut cursor = state.db.collection::<Document>("sections")
                .find(doc! { "song_id": song_id }).await.map_err(e)?;
            while let Some(Ok(d)) = cursor.next().await {
                let s = bson_to_value(d);
                let url = s["image_url"].as_str().unwrap_or("").trim_start_matches("file://");
                if !url.is_empty() && std::path::Path::new(url).exists() {
                    found = url.to_string();
                    break;
                }
            }
            if found.is_empty() {
                return Err("This song has no finished artwork to print. Generate images first.".into());
            }
            found
    };

    // Upload once, reuse for every product — Printify keeps uploads at account level.
    let bytes = std::fs::read(&art).map_err(e)?;
    let file_name = std::path::Path::new(&art).file_name()
        .map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "art.png".into());
    let uploaded = post_json(&t, "/uploads/images.json", &json!({
        "file_name": file_name,
        "contents": base64_encode(&bytes),
    })).await?;
    let image_id = uploaded["id"].as_str().unwrap_or("").to_string();
    if image_id.is_empty() {
        return Err("Printify accepted the upload but returned no image id.".into());
    }
    let (art_w, art_h) = (uploaded["width"].as_i64().unwrap_or(0), uploaded["height"].as_i64().unwrap_or(0));

    // Publishing is the capped operation, so check the window before making anything.
    let mut recent: Vec<String> = Vec::new();
    let mut pc = state.db.collection::<Document>("printify_products")
        .find(doc! {}).await.map_err(e)?;
    while let Some(Ok(d)) = pc.next().await {
        if let Some(ts) = bson_to_value(d)["published_at"].as_str() { recent.push(ts.to_string()); }
    }
    let pacing = publish_pacing(&recent, &crate::models::now_iso());
    let may_publish = publish && pacing["room"].as_bool() == Some(true);

    let mut created = Vec::new();
    let mut errors = Vec::new();
    for target in targets {
        let blueprint_id = target["blueprint_id"].as_i64().unwrap_or(0);
        let provider_id = target["print_provider_id"].as_i64().unwrap_or(0);
        let variant_ids: Vec<i64> = target["variant_ids"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_i64()).collect()).unwrap_or_default();
        let price = target["price_cents"].as_i64().unwrap_or(0);
        if blueprint_id == 0 || provider_id == 0 || variant_ids.is_empty() || price <= 0 {
            errors.push(format!("{} is missing a blueprint, provider, variants or a price",
                target["title"].as_str().unwrap_or("a picked product")));
            continue;
        }
        let quality = print_quality(
            target["placeholder_width"].as_i64().unwrap_or(0),
            target["placeholder_height"].as_i64().unwrap_or(0),
            art_w, art_h,
        );
        let title = format!("{} — {}", phrase, target["title"].as_str().unwrap_or("print"));
        let description = format!(
            "{phrase}\n\nFrom \"{song}\". Scripture set to music, printed on demand.",
            song = song["title"].as_str().unwrap_or(""),
        );
        let body = product_payload(
            &title, &description, blueprint_id, provider_id, &variant_ids, price,
            &image_id, target["position"].as_str().unwrap_or("front"),
            quality["scale"].as_f64().unwrap_or(1.0).min(1.0),
        );
        match post_json(&t, &format!("/shops/{shop_id}/products.json"), &body).await {
            Ok(product) => {
                let product_id = product["id"].as_str().unwrap_or("").to_string();
                let mut published_at = Value::Null;
                if may_publish && !product_id.is_empty() {
                    match post_json(&t, &format!("/shops/{shop_id}/products/{product_id}/publish.json"),
                                    &publish_payload()).await {
                        Ok(_) => published_at = json!(crate::models::now_iso()),
                        Err(err) => errors.push(format!("{title}: created but not published — {err}")),
                    }
                }
                let record = json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "printify_product_id": product_id,
                    "shop_id": shop_id,
                    "song_id": song_id,
                    "project_id": song["project_id"].as_str().unwrap_or(""),
                    "phrase": phrase,
                    "title": title,
                    "blueprint_id": blueprint_id,
                    "price_cents": price,
                    "art_path": art,
                    "print_quality": quality,
                    "published_at": published_at,
                    "created_at": crate::models::now_iso(),
                });
                if let Ok(d) = bson::to_document(&record) {
                    let _ = state.db.collection::<Document>("printify_products").insert_one(d).await;
                }
                created.push(record);
            }
            Err(err) => errors.push(format!("{title}: {err}")),
        }
    }

    Ok(json!({
        "phrase": phrase,
        "art": art,
        "image_id": image_id,
        "created": created,
        "errors": errors,
        "published": may_publish,
        "pacing": pacing,
        "note": if publish && !may_publish {
            "Created as drafts: Printify's publish window is full (200 per 30 minutes). Publish them \
             when it clears."
        } else { "" },
    }))
}

#[tauri::command]
pub async fn list_printify_products(state: State<'_, AppState>, project_id: Option<String>) -> Res<Value> {
    use futures_util::StreamExt;
    let mut filter = Document::new();
    if let Some(p) = project_id.filter(|s| !s.is_empty()) { filter.insert("project_id", p); }
    let mut out = Vec::new();
    let mut cursor = state.db.collection::<Document>("printify_products")
        .find(filter).await.map_err(e)?;
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    let recent: Vec<String> = out.iter()
        .filter_map(|p| p["published_at"].as_str().map(|s| s.to_string())).collect();
    Ok(json!({
        "products": out,
        "pacing": publish_pacing(&recent, &crate::models::now_iso()),
    }))
}

/// The daily sweep: today's song onto the picked products.
///
/// Called by the scheduler and by hand. Publishing follows `printify_auto_publish`, off by default —
/// a shop that filled itself overnight is not something an apology fixes.
pub async fn daily_run(state: &AppState, project_id: &str) -> Value {
    let settings = match state.db.collection::<Document>("settings")
        .find_one(doc! { "_id": "singleton" }).await {
        Ok(Some(d)) => bson_to_value(d),
        _ => return json!({ "skipped": "no settings" }),
    };
    if settings["printify_daily_run"].as_bool() != Some(true) {
        return json!({ "skipped": "printify_daily_run is off" });
    }
    let shop_id = settings["printify_shop_id"].as_str().unwrap_or("").to_string();
    if shop_id.is_empty() {
        return json!({ "skipped": "no Printify shop chosen" });
    }
    // The newest song that has artwork — the same thing a person would pick.
    let mut newest: Option<Value> = None;
    if let Ok(mut cursor) = state.db.collection::<Document>("songs")
        .find(doc! { "project_id": project_id }).await {
        use futures_util::StreamExt;
        while let Some(Ok(d)) = cursor.next().await {
            let s = bson_to_value(d);
            let created = s["created_at"].as_str().unwrap_or("").to_string();
            if newest.as_ref().map_or(true, |n| created > n["created_at"].as_str().unwrap_or("").to_string()) {
                newest = Some(s);
            }
        }
    }
    let Some(song) = newest else { return json!({ "skipped": "no songs" }) };
    json!({
        "song_id": song["id"],
        "shop_id": shop_id,
        "auto_publish": settings["printify_auto_publish"].as_bool() == Some(true),
        "note": "Call printify_make_products with these to run it.",
    })
}

/// Base64, standard alphabet with padding — what Printify's `contents` field expects.
fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_known_encodings() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"any carnal pleasure."), "YW55IGNhcm5hbCBwbGVhc3VyZS4=");
        // Bytes above 127 must not be mangled — this is a PNG header.
        assert_eq!(base64_encode(&[0x89, 0x50, 0x4E, 0x47]), "iVBORw==");
    }

    #[test]
    fn the_product_payload_matches_printifys_contract() {
        let p = product_payload("Be still", "desc", 384, 1, &[45740, 45742], 2499,
                                "5d15ca551163cde90d7b2203", "front", 0.9);
        assert_eq!(p["blueprint_id"], json!(384));
        assert_eq!(p["print_provider_id"], json!(1));
        // Price is in cents, as an integer. Dollars here would list the product at 1% of its price.
        assert_eq!(p["variants"][0]["price"], json!(2499));
        assert!(p["variants"][0]["price"].is_i64(), "price must be an integer number of cents");
        assert_eq!(p["variants"][0]["is_enabled"], json!(true));
        // Every enabled variant must appear in the print area, or it prints blank.
        assert_eq!(p["print_areas"][0]["variant_ids"], json!([45740, 45742]));
        let img = &p["print_areas"][0]["placeholders"][0]["images"][0];
        assert_eq!(img["id"], json!("5d15ca551163cde90d7b2203"));
        assert_eq!(img["x"], json!(0.5), "0.5/0.5 is centred; 0/0 would pin it to a corner");
        assert_eq!(img["y"], json!(0.5));
    }

    #[test]
    fn publishing_pushes_the_variants_too() {
        // A product published without its variants is a listing nobody can buy.
        let p = publish_payload();
        for key in ["title", "description", "images", "variants"] {
            assert_eq!(p[key], json!(true), "{key} must be published");
        }
    }

    #[test]
    fn print_quality_reports_the_dpi_a_buyer_would_see() {
        // A 4000×4000 print area with 4000×4000 art prints at full 300 DPI.
        assert_eq!(print_quality(4000, 4000, 4000, 4000)["dpi"], json!(300));
        // Generated art is usually 1024: a quarter of the area means a quarter of the DPI.
        let small = print_quality(4000, 4000, 1024, 1024);
        assert_eq!(small["dpi"], json!(77));
        assert_eq!(small["ok"], json!(false), "77 DPI is a refund, not a product");
        assert!(small["advice"].as_str().unwrap().contains("Regenerate"));
        // 2048 into a 3000 area lands at 205 DPI — soft up close, sellable on fabric.
        let mid = print_quality(3000, 3000, 2048, 2048);
        assert_eq!(mid["ok"], json!(true));
        assert_eq!(mid["dpi"], json!(205));
        assert_eq!(mid["verdict"], json!("good"));
        // Unknown sizes must not be reported as fine.
        assert_eq!(print_quality(0, 0, 1024, 1024)["ok"], json!(false));
    }

    #[test]
    fn a_printed_phrase_is_short_clean_and_cut_on_a_word() {
        assert_eq!(print_phrase("\"Be still, and know that I am God,\"", 4), "Be still, and know");
        assert_eq!(print_phrase("  Be still\nand know  ", 8), "Be still and know");
        // No trailing punctuation, and never a half word.
        let cut = print_phrase("The heavens declare the glory of God above all things", 5);
        assert_eq!(cut, "The heavens declare the glory");
        assert!(!cut.ends_with(','));
    }

    #[test]
    fn publishing_is_paced_to_printifys_own_ceiling() {
        let now = "2026-07-26T12:00:00+00:00";
        let inside: Vec<String> = (0..PUBLISH_CAP).map(|_| "2026-07-26T11:45:00+00:00".to_string()).collect();
        assert_eq!(publish_pacing(&inside, now)["room"], json!(false));
        // The same volume an hour ago leaves the window clear.
        let outside: Vec<String> = (0..PUBLISH_CAP).map(|_| "2026-07-26T11:00:00+00:00".to_string()).collect();
        let p = publish_pacing(&outside, now);
        assert_eq!(p["room"], json!(true));
        assert_eq!(p["remaining"], json!(PUBLISH_CAP));
    }

    #[test]
    fn a_rejected_token_says_where_to_get_a_new_one() {
        let msg = printify_error(401, r#"{"message":"Unauthenticated."}"#);
        assert!(msg.contains("Personal Access Token"), "{msg}");
        let limited = printify_error(429, "{}");
        assert!(limited.contains("200"), "the actual cap belongs in the message: {limited}");
        let refused = printify_error(422, r#"{"errors":{"reason":"variant 1 is not available"}}"#);
        assert!(refused.contains("variant 1 is not available"), "{refused}");
    }
}
