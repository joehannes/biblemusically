use crate::state::AppState;
use bson::{doc, Document};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

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
// Clipboard: two of them, and a queue
//
// The OS clipboard holds one thing and forgets the rest. That is fine for a person and useless for a
// macro that has to paste five prepared values into five fields in order. So this keeps a history, and
// keeps it in two forms that behave differently on purpose:
//
//   • `system` mirrors the OS clipboard and supports POP — consuming the top item so the next paste is
//     the one underneath. That is the destructive, stack-like behaviour that makes sequential pasting
//     work.
//   • `vault` never loses anything. A "pop" here does not delete: it APPENDS a new entry equal to what
//     the system clipboard would look like after the pop. The newest item is therefore always what will
//     paste next, and every earlier state is still there to look at. From the user's point of view
//     nothing is ever destroyed, while the top of the list still behaves like a clipboard.
//
// The queue is the part macros need: given N prepared items, the system clipboard is set to the first,
// and each `clipboard_queue_next` advances. The cursor lives in the store rather than in a JS closure,
// because a page navigation inside the embedded webview would silently lose a closure — the same class
// of bug as the self-perpetuating setTimeout loops that died on navigation.
//
// PLATFORMS. Everything here works on mobile: the clipboard plugin covers Android and iOS, and the
// history, the vault and the queue are ordinary store documents. What mobile does not have is a tray,
// so the OS taskbar menu is desktop-only (see `tray.rs`) and the same actions live in the in-app view
// on every platform. Mobile also does not get the background poll — polling the clipboard from a
// suspended app is unreliable and costs battery for nothing, so there the history records when the app
// asks (`clipboard_sync`), which is exactly when it is about to be used.
// ────────────────────────────────────────────────────────────────

/// Longest clipboard entry worth keeping. Above this it is a file's worth of text, and a history of
/// those would grow the store without ever being useful in a menu.
const MAX_TEXT: usize = 100_000;
/// How many entries to keep per clipboard. Pinned items are never trimmed.
const KEEP: usize = 200;

/// A single-line, menu-sized rendering of an entry.
///
/// Tray menus are one line each; a multi-line snippet would either be truncated by the OS at an
/// arbitrary point or break the menu's layout. Whitespace is collapsed first so an indented code
/// snippet does not become a line of spaces.
pub fn preview(text: &str, width: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= width { return collapsed; }
    let kept: String = collapsed.chars().take(width.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Should this clipboard content be recorded as a new entry?
///
/// The poll fires several times a second, so the same content arrives over and over: recording it each
/// time would fill the history with duplicates of whatever is currently copied. Blank content is
/// skipped too — some applications clear the clipboard before writing to it, and that momentary empty
/// state is not something anyone wants in their history.
pub fn should_record(text: &str, last: Option<&str>) -> bool {
    if text.trim().is_empty() { return false; }
    if text.len() > MAX_TEXT { return false; }
    match last {
        Some(prev) => prev != text,
        None => true,
    }
}

/// What a pop means, given a history newest-first.
///
/// Returns `(text_the_system_clipboard_becomes, index_to_remove)`. The index is only acted on for the
/// system clipboard; the vault appends instead, which is the whole point of having two.
///
/// Popping the last item leaves the clipboard holding it rather than emptying it: a pop that silently
/// blanks the clipboard loses data that was there a moment ago, and there is nothing to paste next
/// anyway.
pub fn after_pop(history_newest_first: &[String]) -> (Option<String>, Option<usize>) {
    match history_newest_first.len() {
        0 => (None, None),
        1 => (Some(history_newest_first[0].clone()), None),
        _ => (Some(history_newest_first[1].clone()), Some(0)),
    }
}

/// Where a queue cursor lands next, and whether it is finished.
pub fn queue_advance(cursor: usize, len: usize) -> (usize, bool) {
    let next = cursor.saturating_add(1);
    (next.min(len), next >= len)
}

// ── Store access ────────────────────────────────────────────────────────────

async fn newest(state: &AppState, kind: &str) -> Option<Value> {
    use futures_util::StreamExt;
    let mut best: Option<Value> = None;
    let mut cursor = state.db.collection::<Document>("clipboard_items")
        .find(doc! { "kind": kind }).await.ok()?;
    while let Some(Ok(d)) = cursor.next().await {
        let v = bson_to_value(d);
        let seq = v["seq"].as_i64().unwrap_or(0);
        if best.as_ref().map_or(true, |b| seq > b["seq"].as_i64().unwrap_or(0)) {
            best = Some(v);
        }
    }
    best
}

async fn history(state: &AppState, kind: &str) -> Vec<Value> {
    use futures_util::StreamExt;
    let mut out = Vec::new();
    if let Ok(mut cursor) = state.db.collection::<Document>("clipboard_items")
        .find(doc! { "kind": kind }).await {
        while let Some(Ok(d)) = cursor.next().await { out.push(bson_to_value(d)); }
    }
    // Newest first — the order both the tray menu and a pop want.
    out.sort_by_key(|v| -v["seq"].as_i64().unwrap_or(0));
    out
}

async fn next_seq(state: &AppState) -> i64 {
    use futures_util::StreamExt;
    let mut max = 0;
    if let Ok(mut cursor) = state.db.collection::<Document>("clipboard_items").find(doc! {}).await {
        while let Some(Ok(d)) = cursor.next().await {
            max = max.max(bson_to_value(d)["seq"].as_i64().unwrap_or(0));
        }
    }
    max + 1
}

/// Append an entry. Shared by the poll, by the vault and by explicit adds.
pub async fn record(state: &AppState, kind: &str, text: &str, source: &str) -> Res<Value> {
    let seq = next_seq(state).await;
    let item = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "kind": kind,
        "seq": seq,
        "text": text,
        "preview": preview(text, 64),
        "chars": text.chars().count() as i64,
        "source": source,
        "pinned": false,
        "created_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&item).map_err(e)?;
    state.db.collection::<Document>("clipboard_items").insert_one(d).await.map_err(e)?;
    trim(state, kind).await;
    Ok(item)
}

/// Drop the oldest unpinned entries past `KEEP`.
async fn trim(state: &AppState, kind: &str) {
    let items = history(state, kind).await;
    if items.len() <= KEEP { return; }
    for old in items.iter().skip(KEEP) {
        if old["pinned"].as_bool() == Some(true) { continue; }
        if let Some(id) = old["id"].as_str() {
            let _ = state.db.collection::<Document>("clipboard_items")
                .delete_one(doc! { "id": id }).await;
        }
    }
}

// ── Commands ────────────────────────────────────────────────────────────────

/// Read the OS clipboard and record it if it is new.
///
/// Called by the desktop poll, and directly by the UI and the macro player — on mobile this is the only
/// way history grows, which is fine because it runs at the moment the history is about to be used.
#[tauri::command]
pub async fn clipboard_sync(app: AppHandle) -> Res<Value> {
    let state = app.state::<AppState>();
    let text = app.clipboard().read_text().unwrap_or_default();
    let last = newest(&state, "system").await;
    let last_text = last.as_ref().and_then(|l| l["text"].as_str());
    if !should_record(&text, last_text) {
        return Ok(json!({ "recorded": false, "text": text }));
    }
    let item = record(&state, "system", &text, "os").await?;
    Ok(json!({ "recorded": true, "item": item }))
}

#[tauri::command]
pub async fn clipboard_history(state: State<'_, AppState>, kind: Option<String>, limit: Option<usize>) -> Res<Value> {
    let k = kind.unwrap_or_else(|| "system".into());
    let mut items = history(&state, &k).await;
    items.truncate(limit.unwrap_or(50));
    Ok(json!({ "kind": k, "items": items }))
}

/// Put an entry back on the OS clipboard.
///
/// This records a new system entry as well, because it genuinely is the newest thing on the clipboard —
/// leaving the history untouched would make the top of the list disagree with what will paste.
#[tauri::command]
pub async fn clipboard_use(app: AppHandle, id: String) -> Res<Value> {
    let state = app.state::<AppState>();
    let item = state.db.collection::<Document>("clipboard_items")
        .find_one(doc! { "id": &id }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "no such clipboard entry".to_string())?;
    let text = item["text"].as_str().unwrap_or("").to_string();
    app.clipboard().write_text(text.clone()).map_err(e)?;
    let recorded = record(&state, "system", &text, "reuse").await?;
    Ok(json!({ "ok": true, "item": recorded }))
}

#[derive(serde::Deserialize)]
pub struct AddRequest {
    pub text: String,
    /// "system" writes to the OS clipboard too; "vault" only remembers.
    #[serde(default)]
    pub kind: Option<String>,
}

#[tauri::command]
pub async fn clipboard_add(app: AppHandle, payload: AddRequest) -> Res<Value> {
    let state = app.state::<AppState>();
    let kind = payload.kind.unwrap_or_else(|| "vault".into());
    if kind == "system" {
        app.clipboard().write_text(payload.text.clone()).map_err(e)?;
    }
    let item = record(&state, &kind, &payload.text, "app").await?;
    Ok(json!({ "ok": true, "item": item }))
}

/// Consume the top entry.
///
/// On `system` this is a real pop: the entry is removed and the OS clipboard becomes the one underneath,
/// so the next paste is the next value. On `vault` nothing is removed — a new entry is appended holding
/// what the system clipboard would now contain. Same visible effect at the top of the list, no history
/// destroyed, which is the point of keeping both.
#[tauri::command]
pub async fn clipboard_pop(app: AppHandle, kind: Option<String>) -> Res<Value> {
    let state = app.state::<AppState>();
    let k = kind.unwrap_or_else(|| "system".into());
    let items = history(&state, &k).await;
    let texts: Vec<String> = items.iter()
        .map(|i| i["text"].as_str().unwrap_or("").to_string()).collect();
    let (next_text, remove_at) = after_pop(&texts);
    let Some(next_text) = next_text else {
        return Ok(json!({ "ok": false, "reason": "the clipboard history is empty" }));
    };

    app.clipboard().write_text(next_text.clone()).map_err(e)?;

    if k == "vault" {
        // Append rather than delete: the simulated post-pop state.
        let item = record(&state, "vault", &next_text, "pop-simulated").await?;
        return Ok(json!({ "ok": true, "popped": false, "simulated": true, "item": item }));
    }
    if let Some(idx) = remove_at {
        if let Some(id) = items[idx]["id"].as_str() {
            state.db.collection::<Document>("clipboard_items")
                .delete_one(doc! { "id": id }).await.map_err(e)?;
        }
    }
    Ok(json!({ "ok": true, "popped": remove_at.is_some(), "now": preview(&next_text, 64) }))
}

#[tauri::command]
pub async fn clipboard_pin(state: State<'_, AppState>, id: String, pinned: bool) -> Res<Value> {
    state.db.collection::<Document>("clipboard_items")
        .update_one(doc! { "id": &id }, doc! { "$set": { "pinned": pinned } })
        .await.map_err(e)?;
    Ok(json!({ "id": id, "pinned": pinned }))
}

#[tauri::command]
pub async fn clipboard_clear(state: State<'_, AppState>, kind: Option<String>) -> Res<Value> {
    let k = kind.unwrap_or_else(|| "system".into());
    let items = history(&state, &k).await;
    let mut removed = 0;
    for i in items {
        // Pinned entries survive a clear — that is what pinning is for.
        if i["pinned"].as_bool() == Some(true) { continue; }
        if let Some(id) = i["id"].as_str() {
            if state.db.collection::<Document>("clipboard_items")
                .delete_one(doc! { "id": id }).await.is_ok() { removed += 1; }
        }
    }
    Ok(json!({ "kind": k, "removed": removed }))
}

// ── The paste queue ─────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct QueueRequest {
    /// The values to paste, in the order they should be pasted.
    pub items: Vec<String>,
    /// Also keep them in the vault, so the run can be inspected afterwards.
    #[serde(default)]
    pub remember: Option<bool>,
}

/// Load a sequence of values and put the first one on the clipboard.
///
/// This is what lets a browser macro fill several fields from prepared values: set the queue, then each
/// paste step advances it. The cursor is a stored document, so a navigation inside the webview cannot
/// lose the sequence half way through.
#[tauri::command]
pub async fn clipboard_queue_set(app: AppHandle, payload: QueueRequest) -> Res<Value> {
    let state = app.state::<AppState>();
    let items: Vec<String> = payload.items.into_iter().filter(|i| !i.is_empty()).collect();
    if items.is_empty() { return Err("the queue needs at least one value".into()); }

    if payload.remember.unwrap_or(false) {
        for i in &items { let _ = record(&state, "vault", i, "queued").await; }
    }
    app.clipboard().write_text(items[0].clone()).map_err(e)?;

    let doc_out = json!({
        "_id": "singleton",
        "items": items,
        "cursor": 0,
        "started_at": crate::models::now_iso(),
    });
    let d = bson::to_document(&doc_out).map_err(e)?;
    state.db.collection::<Document>("clipboard_queue")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": d }).upsert(true)
        .await.map_err(e)?;

    Ok(json!({
        "loaded": doc_out["items"].as_array().map(|a| a.len()).unwrap_or(0),
        "cursor": 0,
        "on_clipboard": preview(doc_out["items"][0].as_str().unwrap_or(""), 64),
    }))
}

/// Advance to the next queued value and put it on the clipboard.
#[tauri::command]
pub async fn clipboard_queue_next(app: AppHandle) -> Res<Value> {
    let state = app.state::<AppState>();
    let q = state.db.collection::<Document>("clipboard_queue")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "no queue is loaded".to_string())?;
    let items: Vec<String> = q["items"].as_array().map(|a| a.iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
    let cursor = q["cursor"].as_i64().unwrap_or(0).max(0) as usize;
    let (next, done) = queue_advance(cursor, items.len());

    if done {
        return Ok(json!({ "done": true, "cursor": cursor, "of": items.len() }));
    }
    app.clipboard().write_text(items[next].clone()).map_err(e)?;
    state.db.collection::<Document>("clipboard_queue")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "cursor": next as i64 } })
        .await.map_err(e)?;
    Ok(json!({
        "done": false, "cursor": next, "of": items.len(),
        "on_clipboard": preview(&items[next], 64),
    }))
}

/// Take the current queued value and advance.
///
/// This is what a macro step calls: it returns the text to paste *now* and leaves the clipboard holding
/// the value for the next step, so both an automated paste and a manual Ctrl+V do the right thing. One
/// command rather than read-then-advance, because two calls can interleave with a second macro and
/// paste the same value twice.
#[tauri::command]
pub async fn clipboard_queue_take(app: AppHandle) -> Res<Value> {
    let state = app.state::<AppState>();
    let q = state.db.collection::<Document>("clipboard_queue")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value)
        .ok_or_else(|| "no queue is loaded — set one before a macro asks for it".to_string())?;
    let items: Vec<String> = q["items"].as_array().map(|a| a.iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
    let cursor = q["cursor"].as_i64().unwrap_or(0).max(0) as usize;
    if cursor >= items.len() {
        return Ok(json!({ "done": true, "text": Value::Null, "index": cursor, "of": items.len() }));
    }
    let text = items[cursor].clone();
    let (next, done) = queue_advance(cursor, items.len());
    // Leave the clipboard on the value the *next* step wants, so a manual paste stays in step.
    if !done {
        app.clipboard().write_text(items[next].clone()).map_err(e)?;
    }
    state.db.collection::<Document>("clipboard_queue")
        .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "cursor": next as i64 } })
        .await.map_err(e)?;
    Ok(json!({ "done": false, "text": text, "index": cursor, "of": items.len(), "finished": done }))
}

#[tauri::command]
pub async fn clipboard_queue_status(state: State<'_, AppState>) -> Res<Value> {
    let q = state.db.collection::<Document>("clipboard_queue")
        .find_one(doc! { "_id": "singleton" }).await.map_err(e)?
        .map(bson_to_value);
    match q {
        Some(q) => {
            let items = q["items"].as_array().cloned().unwrap_or_default();
            let cursor = q["cursor"].as_i64().unwrap_or(0);
            Ok(json!({
                "loaded": true,
                "cursor": cursor,
                "of": items.len(),
                "remaining": (items.len() as i64 - cursor - 1).max(0),
                "previews": items.iter().map(|i| preview(i.as_str().unwrap_or(""), 40)).collect::<Vec<_>>(),
            }))
        }
        None => Ok(json!({ "loaded": false })),
    }
}

/// The entries a tray menu should show, already previewed and capped.
pub async fn tray_entries(state: &AppState, limit: usize) -> Vec<(String, String)> {
    history(state, "system").await.into_iter().take(limit)
        .map(|i| (
            i["id"].as_str().unwrap_or("").to_string(),
            i["preview"].as_str().unwrap_or("").to_string(),
        ))
        .filter(|(id, _)| !id.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_is_one_line_and_fits_a_menu() {
        assert_eq!(preview("hello", 20), "hello");
        // Multi-line and indented content collapses rather than breaking the menu.
        assert_eq!(preview("  line one\n    line two  ", 40), "line one line two");
        let long = preview(&"a".repeat(100), 10);
        assert_eq!(long.chars().count(), 10, "including the ellipsis");
        assert!(long.ends_with('…'));
    }

    #[test]
    fn the_same_content_polled_twice_is_recorded_once() {
        // The poll fires several times a second; without this the history is nothing but duplicates
        // of whatever is currently copied.
        assert!(should_record("hello", None));
        assert!(!should_record("hello", Some("hello")));
        assert!(should_record("hello there", Some("hello")));
    }

    #[test]
    fn a_momentary_blank_clipboard_is_not_history() {
        // Applications clear the clipboard before writing to it. That empty instant must not land.
        assert!(!should_record("", Some("hello")));
        assert!(!should_record("   \n  ", None));
        // Nor should a whole file pasted in.
        assert!(!should_record(&"x".repeat(MAX_TEXT + 1), None));
    }

    #[test]
    fn popping_makes_the_next_value_the_one_that_pastes() {
        let h = vec!["third".to_string(), "second".to_string(), "first".to_string()];
        let (next, remove) = after_pop(&h);
        assert_eq!(next.as_deref(), Some("second"), "the one underneath is what pastes next");
        assert_eq!(remove, Some(0), "the consumed entry is the one removed");
    }

    #[test]
    fn popping_the_last_entry_does_not_blank_the_clipboard() {
        // Emptying the clipboard would lose something that was there a moment ago, and there is
        // nothing to paste next anyway.
        let (next, remove) = after_pop(&["only".to_string()]);
        assert_eq!(next.as_deref(), Some("only"));
        assert_eq!(remove, None, "nothing is consumed when there is nothing underneath");

        let (next, remove) = after_pop(&[]);
        assert_eq!(next, None);
        assert_eq!(remove, None);
    }

    #[test]
    fn the_vault_simulates_a_pop_instead_of_destroying_history() {
        // `after_pop` decides what the clipboard becomes; the vault path uses the text and ignores the
        // index, which is what makes it non-destructive. Asserted here because that asymmetry is the
        // entire design and is easy to "tidy up" into a bug.
        let h = vec!["newest".to_string(), "older".to_string()];
        let (text, index) = after_pop(&h);
        assert_eq!(text.as_deref(), Some("older"));
        assert!(index.is_some(), "the system clipboard would remove it");
        // The vault appends `text` and never acts on `index`, so its length only ever grows.
        assert_eq!(h.len(), 2, "the input history is untouched by the decision itself");
    }

    #[test]
    fn a_queue_advances_once_per_paste_and_then_reports_done() {
        assert_eq!(queue_advance(0, 3), (1, false));
        assert_eq!(queue_advance(1, 3), (2, false));
        // The last advance is done, and the cursor never runs past the end.
        assert_eq!(queue_advance(2, 3), (3, true));
        assert_eq!(queue_advance(3, 3), (3, true));
        assert_eq!(queue_advance(0, 1), (1, true));
    }
}
