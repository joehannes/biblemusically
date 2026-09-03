//! The walk between pages.
//!
//! Fourteen guided flows cover fourteen pages, and every one of them begins and ends inside its own
//! page. So a person is guided *within* whichever of thirty-five doors they happen to open, and
//! guided nowhere at all about which door that should be. The audit's phrase for it: the guided
//! experience is excellent and the app is still overwhelming, because the overwhelming part is the
//! map, not the pages.
//!
//! A **journey** is the map. Nine stops, in the order a song passes through them, each naming a
//! route — so the thing that carries somebody between pages stops being a sidebar of thirty-five
//! entries and becomes "you are here, and the next thing is there".
//!
//! ## Why this is not a wizard
//!
//! A wizard walks a script and asks the same questions of everybody. This walks the *project*:
//!
//!   * **Every stop's doneness is computed from what the project contains**, not from a flag the
//!     app set when somebody visited a page. Visiting `/music` proves nothing; a song with audio
//!     does. So a journey resumed after a month is correct without having remembered anything, and a
//!     stop that was done and then undone (a song added, an image deleted) reopens by itself.
//!   * **The stop list is fixed and the doneness is not.** Nine stops every time, so "step 4 of 9"
//!     means the same thing in January and in June — a total that shrank as the project grew would
//!     make the progress indicator unreadable. What varies is which of them are already satisfied.
//!   * **It can be left at any point and it is never the only way in.** The sidebar still has all
//!     thirty-five entries. This is a suggestion of an order, and the app's promise is that nothing
//!     is withheld.
//!
//! ## Why the doneness rules live here
//!
//! `guide_today` already answers "what should I do now" from the same shape. The journey answers a
//! different question — "where am I in the whole thing" — and the two must not disagree, so both
//! read `shape_of`. The stop list and its rules are pure functions over that shape, which is what
//! makes them testable and what keeps "is this done" from being re-derived slightly differently in
//! the interface.

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

/// One stop on the journey: a page, and what being there is for.
pub struct Stop {
    pub id: &'static str,
    pub route: &'static str,
    /// What this stop is called, in the voice of somebody doing it rather than of a menu.
    pub label: &'static str,
    /// One sentence on why it comes here. Shown while you are on it.
    pub why: &'static str,
}

/// The nine stops, in the order a song passes through them.
///
/// Nine rather than the thirteen `pageSteps.js` numbers or the thirty-five in the sidebar: Jobs and
/// Settings are places you go when something is wrong or when you are configuring, not stages a song
/// passes through, and the studios that refine one stage (Sound, Style, Transitions, Overlays,
/// Characters) are deliberately not stops — a journey that made you visit them would be claiming
/// they are required, which is the opposite of what they are.
pub const STOPS: &[Stop] = &[
    Stop { id: "brief", route: "/",
        label: "Say what this project is",
        why: "Everything downstream reads the brief — the words, the pictures, the way each channel \
              is spoken to. It is the one thing that is cheaper to do first." },
    Stop { id: "channels", route: "/channels",
        label: "Say where it goes",
        why: "A channel decides the language, the style and the audience each song is written for. \
              Without one the studio is guessing at all three." },
    Stop { id: "source", route: "/bible",
        label: "Pick the text",
        why: "The passage the song is a setting of. Skip it and write from your own idea instead — \
              either way this stop is behind you once there is a song." },
    Stop { id: "words", route: "/composer",
        label: "Write the words",
        why: "The lyric, and the decisions that shape it — how close to the source to stay, who is \
              speaking, what shape the song is." },
    Stop { id: "music", route: "/music",
        label: "Render the music",
        why: "The engine sings the lyric exactly as written. This is where the words become audio." },
    Stop { id: "sections", route: "/analysis",
        label: "Cut it into sections",
        why: "Sections are what the pictures hang on: one image per stretch of the song, timed to it." },
    Stop { id: "images", route: "/images",
        label: "Make the pictures",
        why: "One image per section, in whatever style pack this project uses." },
    Stop { id: "video", route: "/video",
        label: "Build the video",
        why: "The audio and the images become one file, with the transitions and overlays you chose." },
    Stop { id: "publish", route: "/upload",
        label: "Publish it",
        why: "Out into the world — and back, as the view counts the studio learns from." },
];

pub fn stop(id: &str) -> Option<&'static Stop> {
    STOPS.iter().find(|s| s.id == id)
}

/// Whether a stop is finished, from what the project contains rather than from having been visited.
///
/// Visiting `/music` proves nothing and a song with audio proves everything, which is why nothing
/// here reads a "seen" flag. The consequence worth knowing: a stop can go back to unfinished on its
/// own when a new song is added, and that is correct — the journey is a description of the project,
/// not a record of where somebody has clicked.
pub fn is_done(id: &str, shape: &Value, has_brief: bool, channels: u64, sources: u64) -> bool {
    let n = |k: &str| shape[k].as_u64().unwrap_or(0);
    match id {
        "brief" => has_brief,
        "channels" => channels > 0,
        // A stored chapter, or any song at all: a song exists because its text came from somewhere,
        // and a project writing from its own ideas rather than from a passage has answered this
        // question just as completely as one that loaded a chapter. Without the second half, a
        // freeform project would be held at a page it has no reason to open.
        "source" => sources > 0 || n("songs") > 0,
        // Songs existing is not enough: a song with no lyrics is the composer's unfinished business.
        "words" => n("songs") > 0 && n("no_lyrics") == 0,
        "music" => n("songs") > 0 && n("no_audio") == 0,
        "sections" => n("songs") > 0 && n("no_sections") == 0,
        "images" => n("songs") > 0 && n("no_images") == 0,
        "video" => n("songs") > 0 && n("no_video") == 0,
        "publish" => n("uploaded") > 0,
        _ => false,
    }
}

/// How many things are outstanding at this stop, when that is a number worth showing.
fn outstanding(id: &str, shape: &Value) -> Option<u64> {
    let n = |k: &str| shape[k].as_u64().unwrap_or(0);
    let count = match id {
        "words" => n("no_lyrics"),
        "music" => n("no_audio"),
        "sections" => n("no_sections"),
        "images" => n("no_images"),
        "video" => n("no_video"),
        _ => return None,
    };
    (count > 0).then_some(count)
}

/// The whole journey for one project: which stops apply, which are done, and where you are.
///
/// Pure over the project's shape, so the interface and any future caller cannot disagree about what
/// "step 4 of 9" means.
///
/// "Where you are" is the first stop that is not done — not the furthest one reached. A person who
/// jumps ahead to the images and then adds a song has unfinished words again, and a journey that
/// kept pointing at the images because that is where they got to would be describing their history
/// instead of their project.
pub fn journey(shape: &Value, has_brief: bool, channels: u64, sources: u64) -> Value {
    let stops: Vec<Value> = STOPS.iter()
        .map(|s| {
            let done = is_done(s.id, shape, has_brief, channels, sources);
            json!({
                "id": s.id, "route": s.route, "label": s.label, "why": s.why,
                "done": done,
                "outstanding": outstanding(s.id, shape),
            })
        })
        .collect();

    let current = stops.iter().position(|s| s["done"] != json!(true));
    let done_count = stops.iter().filter(|s| s["done"] == json!(true)).count();

    json!({
        "stops": stops,
        "total": stops.len(),
        // Null when everything is done, which is a real state and not step ten of nine.
        "current": current,
        "current_id": current.and_then(|i| stops.get(i)).map(|s| s["id"].clone()).unwrap_or(Value::Null),
        "done": done_count,
        "finished": current.is_none(),
    })
}

#[tauri::command]
pub async fn project_journey(state: State<'_, AppState>, project_id: String) -> Res<Value> {
    use futures_util::StreamExt;

    let project = state.db.collection::<Document>("projects")
        .find_one(doc! { "id": &project_id }).await.map_err(e)?
        .map(bson_to_value).unwrap_or_default();
    let has_brief = project["brief"].as_object()
        .map(|b| b.values().any(|v| v.as_str().is_some_and(|s| !s.trim().is_empty())))
        .unwrap_or(false);

    let shape = super::interview::shape_of(state.inner(), &project_id).await?;

    let mut channels = 0u64;
    let mut ch = state.db.collection::<Document>("channels").find(doc! {}).await.map_err(e)?;
    while let Some(Ok(_)) = ch.next().await { channels += 1; }

    // Stored source text. `pasted_chapters` carries no project id, so this is a signal about the
    // app rather than about this project — which is why it is only ever half of the `source` rule:
    // the other half is whether this project has songs, and that half *is* project-scoped.
    let mut sources = 0u64;
    if let Ok(mut cur) = state.db.collection::<Document>("pasted_chapters").find(doc! {}).await {
        while let Some(Ok(_)) = cur.next().await { sources += 1; }
    }

    let mut out = journey(&shape, has_brief, channels, sources);
    out["project_id"] = json!(project_id);
    out["shape"] = shape;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(songs: u64, no_lyrics: u64, no_audio: u64, no_sections: u64,
             no_images: u64, no_video: u64, uploaded: u64) -> Value {
        json!({ "songs": songs, "no_lyrics": no_lyrics, "no_audio": no_audio,
                "no_sections": no_sections, "no_images": no_images, "no_video": no_video,
                "uploaded": uploaded })
    }
    /// A project with nothing in it at all.
    fn empty() -> Value { shape(0, 0, 0, 0, 0, 0, 0) }
    /// One song, finished all the way through and published.
    fn finished() -> Value { shape(1, 0, 0, 0, 0, 0, 1) }

    #[test]
    fn a_brand_new_project_starts_at_the_brief() {
        let j = journey(&empty(), false, 0, 0);
        assert_eq!(j["current"], 0);
        assert_eq!(j["current_id"], "brief");
        assert_eq!(j["done"], 0);
    }

    #[test]
    fn doneness_comes_from_what_is_there_and_never_from_having_visited() {
        // No flag is ever set by opening a page: the only evidence a stop accepts is an artefact.
        let mid = shape(3, 0, 3, 3, 3, 3, 0);
        let j = journey(&mid, true, 2, 1);
        let by_id = |id: &str| j["stops"].as_array().unwrap().iter()
            .find(|s| s["id"] == id).unwrap().clone();
        assert_eq!(by_id("words")["done"], true, "three songs, all with lyrics");
        assert_eq!(by_id("music")["done"], false, "three have no audio");
        assert_eq!(j["current_id"], "music");
    }

    #[test]
    fn a_stop_can_reopen_when_the_project_changes_under_it() {
        // Everything done, then a new song appears with no words. The journey goes back, because it
        // describes the project rather than where somebody has clicked.
        let done = journey(&finished(), true, 1, 1);
        assert_eq!(done["finished"], true);
        assert_eq!(done["current"], Value::Null, "not step ten of nine");

        let plus_one = shape(2, 1, 1, 1, 1, 1, 1);
        let j = journey(&plus_one, true, 1, 1);
        assert_eq!(j["finished"], false);
        assert_eq!(j["current_id"], "words");
    }

    #[test]
    fn where_you_are_is_the_first_thing_unfinished_not_the_furthest_reached() {
        // Somebody who jumped ahead and made images, then added a song with no lyrics, is at the
        // words — pointing at the images would describe their history.
        let j = journey(&shape(2, 1, 1, 0, 0, 1, 0), true, 1, 1);
        assert_eq!(j["current_id"], "words");
    }

    #[test]
    fn a_project_writing_from_its_own_ideas_is_not_held_at_the_page_for_picking_a_text() {
        // No chapter was ever loaded, and four songs exist. Their text came from somewhere, so the
        // question this stop asks has been answered — by writing, rather than by loading.
        let freeform = journey(&shape(4, 0, 0, 0, 0, 0, 0), true, 1, 0);
        let src = |j: &Value| j["stops"].as_array().unwrap().iter()
            .find(|s| s["id"] == "source").expect("the stop is always listed").clone();
        assert_eq!(src(&freeform)["done"], true);
        // A project with a chapter loaded and nothing written is done here too.
        assert_eq!(src(&journey(&empty(), false, 0, 2))["done"], true);
        // And a project with neither is not.
        assert_eq!(src(&journey(&empty(), false, 0, 0))["done"], false);
    }

    #[test]
    fn the_total_is_the_same_number_in_january_and_in_june() {
        // A total that shrank as the project grew would make "step 4 of 9" unreadable, so what
        // varies between these is which stops are satisfied, never how many there are.
        for j in [journey(&empty(), false, 0, 0),
                  journey(&shape(4, 0, 0, 0, 0, 0, 0), true, 2, 0),
                  journey(&finished(), true, 3, 5)] {
            assert_eq!(j["total"], STOPS.len());
            assert_eq!(j["stops"].as_array().unwrap().len(), STOPS.len());
        }
    }

    #[test]
    fn a_stop_says_how_much_is_outstanding_where_that_is_a_number_worth_showing() {
        let j = journey(&shape(7, 0, 4, 7, 7, 7, 0), true, 1, 1);
        let music = j["stops"].as_array().unwrap().iter().find(|s| s["id"] == "music").unwrap().clone();
        assert_eq!(music["outstanding"], 4);
        // The brief is not a count of anything, and saying "0 outstanding" would be noise.
        let brief = j["stops"].as_array().unwrap().iter().find(|s| s["id"] == "brief").unwrap().clone();
        assert_eq!(brief["outstanding"], Value::Null);
    }

    #[test]
    fn an_empty_project_never_reports_a_later_stage_as_finished() {
        // Zero songs means zero songs missing audio, and a naive rule would call the music done.
        let j = journey(&empty(), true, 1, 0);
        for id in ["words", "music", "sections", "images", "video", "publish"] {
            let s = j["stops"].as_array().unwrap().iter().find(|s| s["id"] == id).unwrap().clone();
            assert_eq!(s["done"], false, "{id} cannot be done in an empty project");
        }
    }

    #[test]
    fn every_stop_names_a_route_and_says_what_it_is_for() {
        for s in STOPS {
            assert!(s.route.starts_with('/'), "{} has no route", s.id);
            assert!(s.why.len() > 60, "{} does not say why it comes here", s.id);
            assert!(!s.label.is_empty());
        }
        // The routes are distinct: two stops on one page would make "you are here" ambiguous.
        let mut routes: Vec<&str> = STOPS.iter().map(|s| s.route).collect();
        routes.sort_unstable();
        routes.dedup();
        assert_eq!(routes.len(), STOPS.len());
    }
}
