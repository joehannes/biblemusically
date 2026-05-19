use crate::{models::now_iso, state::AppState};
use bson::{doc, Document};
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::collections::HashMap;
use tauri::State;
use uuid::Uuid;

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
// Static data
// ────────────────────────────────────────────────────────────────

static BIBLE_BOOKS: Lazy<Value> = Lazy::new(|| json!([
    {"id":"genesis",      "name":"Genesis",       "chapters":50},
    {"id":"exodus",       "name":"Exodus",        "chapters":40},
    {"id":"leviticus",    "name":"Leviticus",     "chapters":27},
    {"id":"numbers",      "name":"Numbers",       "chapters":36},
    {"id":"deuteronomy",  "name":"Deuteronomy",   "chapters":34},
    {"id":"joshua",       "name":"Joshua",        "chapters":24},
    {"id":"psalms",       "name":"Psalms",        "chapters":150},
    {"id":"proverbs",     "name":"Proverbs",      "chapters":31},
    {"id":"isaiah",       "name":"Isaiah",        "chapters":66},
    {"id":"matthew",      "name":"Matthew",       "chapters":28},
    {"id":"mark",         "name":"Mark",          "chapters":16},
    {"id":"luke",         "name":"Luke",          "chapters":24},
    {"id":"john",         "name":"John",          "chapters":21},
    {"id":"acts",         "name":"Acts",          "chapters":28},
    {"id":"romans",       "name":"Romans",        "chapters":16},
    {"id":"1corinthians", "name":"1 Corinthians", "chapters":16},
    {"id":"revelation",   "name":"Revelation",    "chapters":22},
]));

static BIBLE_TRANSLATIONS: Lazy<Value> = Lazy::new(|| json!({
    "English (modern)": [
        {"id":"web",    "name":"World English Bible (WEB) — public domain",      "source":"bible-api"},
        {"id":"BSB",    "name":"Berean Standard Bible — literal modern",         "source":"helloao"},
        {"id":"bbe",    "name":"Bible in Basic English (BBE) — plain English",   "source":"bible-api"},
        {"id":"ENGWEBP","name":"WEB w/ apocrypha (helloao)",                     "source":"helloao"},
        {"id":"oeb-cw", "name":"Open English Bible (OEB-CW)",                    "source":"bible-api"},
    ],
    "English (literal / original-meaning)": [
        {"id":"ylt",    "name":"Young's Literal Translation (YLT)",              "source":"bible-api"},
        {"id":"darby",  "name":"Darby Bible — literal 1890",                    "source":"bible-api"},
        {"id":"eng_dby","name":"Darby (helloao)",                                "source":"helloao"},
        {"id":"GHT",    "name":"Garth's Hyper-literal Translation",              "source":"helloao"},
        {"id":"eng_lxx","name":"Septuagint in American English (LXX2012)",       "source":"helloao"},
        {"id":"eng_lxu","name":"Septuagint in British English (LXX2012)",        "source":"helloao"},
        {"id":"asv",    "name":"American Standard Version (ASV) 1901",           "source":"bible-api"},
        {"id":"kjv",    "name":"King James Version (KJV) 1611",                  "source":"bible-api"},
    ],
    "Greek (Koine / NT)": [
        {"id":"GHTG",   "name":"Garth's Hyper-literal (Greek interlinear-style)","source":"helloao"},
        {"id":"grc_byz","name":"Byzantine Greek New Testament",                  "source":"helloao"},
        {"id":"grc_bre","name":"Septuagint (Greek OT, LXX)",                    "source":"helloao"},
    ],
    "Hebrew (OT)": [
        {"id":"heb_mod","name":"Modern Hebrew Bible",                            "source":"helloao"},
        {"id":"heb_scg","name":"Salkinson-Ginsburg Hebrew NT",                   "source":"helloao"},
    ],
    "German": [
        {"id":"deu_l12","name":"Luther 1912",                                    "source":"helloao"},
        {"id":"deu_sch","name":"Schlachter 1951",                                "source":"helloao"},
        {"id":"deu_elo","name":"Elberfelder (Darby)",                            "source":"helloao"},
    ],
    "Italian": [
        {"id":"ita_dio","name":"Diodati 1885",                                   "source":"helloao"},
        {"id":"ita_riv","name":"Riveduta 1927",                                  "source":"helloao"},
    ],
    "Russian": [
        {"id":"rus_syn","name":"Синодальный перевод (Synodal)",                  "source":"helloao"},
    ],
    "Spanish": [
        {"id":"spa_r09","name":"Reina-Valera 1909",                              "source":"helloao"},
        {"id":"spa_blm","name":"Santa Biblia libre para el mundo",               "source":"helloao"},
    ],
    "French": [
        {"id":"fra_lsg","name":"Louis Segond 1910",                              "source":"helloao"},
        {"id":"fra_ost","name":"Sainte Bible Ostervald",                         "source":"helloao"},
    ],
    "Portuguese": [
        {"id":"por_blj","name":"Bíblia Livre",                                   "source":"helloao"},
        {"id":"por_bsl","name":"Bíblia Portuguesa Mundial",                      "source":"helloao"},
    ],
}));

static HELLOAO_BOOK_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("genesis","GEN"); m.insert("exodus","EXO"); m.insert("leviticus","LEV");
    m.insert("numbers","NUM"); m.insert("deuteronomy","DEU"); m.insert("joshua","JOS");
    m.insert("psalms","PSA"); m.insert("proverbs","PRO"); m.insert("isaiah","ISA");
    m.insert("matthew","MAT"); m.insert("mark","MRK"); m.insert("luke","LUK");
    m.insert("john","JHN"); m.insert("acts","ACT"); m.insert("romans","ROM");
    m.insert("1corinthians","1CO"); m.insert("revelation","REV");
    m
});

static BIBLE_API_IDS: Lazy<std::collections::HashSet<&'static str>> = Lazy::new(|| {
    ["web","kjv","bbe","asv","ylt","darby","oeb-cw"].into_iter().collect()
});

// ────────────────────────────────────────────────────────────────
// Commands
// ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_translations() -> Res<Value> {
    Ok(BIBLE_TRANSLATIONS.clone())
}

#[tauri::command]
pub async fn list_bible_books() -> Res<Value> {
    Ok(BIBLE_BOOKS.clone())
}

#[tauri::command]
pub async fn fetch_chapter(translation: String, book: String, chapter: u32) -> Res<Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build().map_err(e)?;

    if BIBLE_API_IDS.contains(translation.as_str()) {
        let book_name = BIBLE_BOOKS.as_array()
            .and_then(|a| a.iter().find(|b| b["id"].as_str() == Some(book.to_lowercase().as_str())))
            .and_then(|b| b["name"].as_str())
            .unwrap_or(&book)
            .to_string();
        let reference = format!("{book_name} {chapter}").replace(' ', "+");
        let r = client.get(format!("https://bible-api.com/{reference}"))
            .query(&[("translation", &translation)])
            .send().await.map_err(e)?;
        if !r.status().is_success() {
            return Err(format!("bible-api error {}", r.status()));
        }
        let d: Value = r.json().await.map_err(e)?;
        let verses: Vec<Value> = d["verses"].as_array().unwrap_or(&vec![])
            .iter()
            .map(|v| json!({ "verse": v["verse"], "text": v["text"].as_str().unwrap_or("").trim() }))
            .collect();
        return Ok(json!({ "reference": d["reference"], "translation": translation, "verses": verses }));
    }

    // helloao path
    let fallback_book = book.to_uppercase();
    let book_usfm = HELLOAO_BOOK_MAP.get(book.to_lowercase().as_str())
        .copied()
        .unwrap_or(fallback_book.as_str());
    let url = format!("https://bible.helloao.org/api/{translation}/{book_usfm}/{chapter}.json");
    let r = client.get(&url).send().await.map_err(e)?;
    if !r.status().is_success() {
        return Err(format!("helloao HTTP {}", r.status()));
    }
    let d: Value = r.json().await.map_err(e)?;
    let content = d["chapter"]["content"].as_array().cloned().unwrap_or_default();
    let verses: Vec<Value> = content.iter()
        .filter(|item| item["type"].as_str() == Some("verse"))
        .map(|item| {
            let txt = item["content"].as_array().unwrap_or(&vec![])
                .iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();
            json!({ "verse": item["number"].as_i64().unwrap_or(0), "text": txt })
        })
        .collect();
    let reference = d["thisChapterReference"].as_str()
        .unwrap_or(&format!("{book} {chapter}"))
        .to_string();
    Ok(json!({ "reference": reference, "translation": translation, "verses": verses }))
}

// ── Pasted chapters ───────────────────────────────────────────────

#[tauri::command]
pub async fn list_pasted_chapters(state: State<'_, AppState>) -> Res<Vec<Value>> {
    use futures_util::StreamExt;
    let mut cursor = state.db.collection::<Document>("pasted_chapters")
        .find(doc! {}).sort(doc! { "created_at": -1 }).await.map_err(e)?;
    let mut out = Vec::new();
    while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    Ok(out)
}

#[tauri::command]
pub async fn save_pasted_chapter(state: State<'_, AppState>, body: Value) -> Res<Value> {
    let id = body["id"].as_str().map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let ts = now_iso();
    let doc = json!({
        "id": &id,
        "name": body["name"].as_str().unwrap_or("Pasted chapter"),
        "language": body["language"].as_str().unwrap_or("English"),
        "translation": body["translation"].as_str().unwrap_or("manual"),
        "book": body["book"].as_str().unwrap_or(""),
        "chapter": body["chapter"].as_i64().unwrap_or(0),
        "text": body["text"].as_str().unwrap_or(""),
        "created_at": &ts,
        "updated_at": &ts,
    });
    let bson = bson::to_document(&doc).map_err(e)?;
    state.db.collection::<Document>("pasted_chapters")
        .update_one(doc! { "id": &id }, doc! { "$set": bson })
        .with_options(mongodb::options::UpdateOptions::builder().upsert(true).build())
        .await.map_err(e)?;
    Ok(doc)
}

#[tauri::command]
pub async fn delete_pasted_chapter(state: State<'_, AppState>, pid: String) -> Res<Value> {
    state.db.collection::<Document>("pasted_chapters")
        .delete_one(doc! { "id": &pid }).await.map_err(e)?;
    Ok(json!({ "ok": true }))
}
