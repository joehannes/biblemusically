//! The app's persistence layer: **plain JSON files, no database server.**
//!
//! This replaces the bundled `mongod` sidecar entirely. It exposes the same small slice of the
//! MongoDB API the codebase actually used (`find_one` / `find` / `insert_one` / `update_one` /
//! `delete_*` / `count_documents`, plus `.sort()`, `.limit()`, `.projection()`, `.upsert()`), so the
//! ~380 call sites across `commands/` kept their query shape — filters are still written with
//! `bson::doc! { … }`. Only the storage underneath changed. `bson` stays as a *pure* dependency
//! here: it is a serialization crate with no client, no sockets and no server, and it cross-compiles
//! to Android — which the `mongodb` driver plus a native `mongod` binary never could.
//!
//! # Where data lives
//!
//! ```text
//! GLOBAL      <config>/studio-lightkid/data/<collection>.json
//! PER PROJECT <project_folder>/data/<collection>.json     ← inside the project's own git repo
//! ```
//!
//! A project's *content* (`songs`, `sections`, `characters`, `uploads`) is stored in that project's
//! folder, which `commands/project_git.rs` has always kept as a git repository. So the project's
//! data is versioned, diffable and pushable alongside its media, and copying the folder copies the
//! project. Everything cross-project (`settings`, `projects`, `channels`, `oauth_clients`, presets,
//! `jobs`, …) stays in the global folder.
//!
//! Reads of a project-scoped collection transparently union the global shard with every known
//! project shard, so an unfiltered `find(doc! {})` still sees everything — callers can't tell the
//! data is sharded. Writes route to the shard that owns the document (see [`Inner::shard_for`]).
//!
//! # File format
//!
//! ```json
//! { "version": 1, "collection": "songs", "updated_at": "…", "documents": [ { … }, … ] }
//! ```
//!
//! Written with `serde_json`'s sorted-key maps and pretty-printing, then `rename`d into place — so
//! the files are hand-readable, produce minimal git diffs, and a crash can't leave a half-written
//! file behind.

use bson::{Bson, Document};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::future::IntoFuture;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// ────────────────────────────────────────────────────────────────────────────
// The sealed cache
// ────────────────────────────────────────────────────────────────────────────
//
// A project's shards can be encrypted with a key derived from the signed entitlement
// (`commands::subscription::cache_key_material`). This is the half of the licensing design that is
// actually load-bearing: the entitlement check itself runs on the user's own machine and a
// determined person will patch it out, but the *work* stays sealed even then, because the key was
// never in the binary — it comes from the account's server-issued salt.
//
// What it closes: making a fresh trial account and re-importing the old account's projects. A new
// account has a different salt, derives a different key, and finds noise.
//
// Deliberately narrow:
//
//   • **Only project shards.** The global folder — `settings`, `projects`, `channels` — stays plain
//     text, because the entitlement token itself lives in `settings`. Sealing that would mean the
//     app could not read the thing it needs in order to derive the key to read it.
//   • **Reading plain text always works.** An existing install is not rewritten on sight; a shard
//     turns sealed the next time it is written. So upgrading is uneventful and a half-converted
//     folder is a normal state rather than a broken one.
//   • **A sealed shard with no key is an error, not an empty list.** Returning "no songs" for
//     "cannot decrypt" would look exactly like data loss and would invite the user to regenerate
//     over the top of work that is still there.

/// The key project shards are sealed with, or `None` when nothing is unlocked.
static CACHE_KEY: once_cell::sync::Lazy<std::sync::RwLock<Option<[u8; 32]>>> =
    once_cell::sync::Lazy::new(|| std::sync::RwLock::new(None));

/// Whether new writes seal. Off until an entitlement supplies key material.
static SEALING_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Marks a file as this app's sealed cache rather than a corrupt JSON document.
const SEAL_MARKER: &str = "studio-lightkid-sealed-cache";

/// Derive the shard key from the entitlement's material (`"<account>:<salt>"`).
///
/// Argon2id over the whole string, salted with a hash of it, so the derivation is deterministic:
/// the same account on another machine derives the same key and can open a synced project, and a
/// different account cannot.
fn derive_cache_key(material: &str) -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(SEAL_MARKER.as_bytes());
    hasher.update(material.as_bytes());
    let salt = hasher.finalize();
    let mut key = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(material.as_bytes(), &salt[..16], &mut key)
        .ok()?;
    Some(key)
}

/// Hand the store the entitlement's key material, or `None` to lock it again.
///
/// Called at startup and whenever the entitlement changes. `enable` decides whether *new* writes
/// seal — a user who has turned sealing off can still read what was sealed earlier.
pub fn set_cache_key(material: Option<&str>, enable: bool) {
    let key = material.and_then(derive_cache_key);
    SEALING_ON.store(enable && key.is_some(), std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut guard) = CACHE_KEY.write() {
        *guard = key;
    }
}

/// Is a key loaded? (Not the same as "sealing is on" — an unlocked-but-not-sealing store reads
/// sealed shards and writes plain ones.)
pub fn cache_unlocked() -> bool {
    CACHE_KEY.read().map(|k| k.is_some()).unwrap_or(false)
}

pub fn cache_sealing() -> bool {
    SEALING_ON.load(std::sync::atomic::Ordering::Relaxed)
}

fn cache_key() -> Option<[u8; 32]> {
    CACHE_KEY.read().ok().and_then(|k| *k)
}

/// Is this JSON a sealed envelope rather than documents?
fn is_sealed(v: &Value) -> bool {
    v.get("sealed").and_then(|s| s.as_str()) == Some(SEAL_MARKER)
}

fn seal_documents(key: &[u8; 32], collection: &str, docs: &[Value]) -> Result<Value> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};
    use rand::RngCore;

    let plaintext = serde_json::to_vec(&Value::Array(docs.to_vec()))?;
    let mut nonce_bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let ciphertext = XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(&nonce_bytes), plaintext.as_ref())
        .map_err(|_| Error("could not seal this collection".into()))?;
    Ok(json!({
        "version": 1,
        "collection": collection,
        "sealed": SEAL_MARKER,
        "updated_at": now_iso(),
        // Said plainly in the file itself, because somebody will open it in a text editor and
        // deserve an explanation rather than a wall of base64.
        "note": "Encrypted with a key derived from the signed-in account. Sign in with the same \
                 account to open it; the app does this for you.",
        "nonce": B64.encode(nonce_bytes),
        "data": B64.encode(&ciphertext),
    }))
}

fn open_documents(envelope: &Value, path: &Path) -> Result<Vec<Value>> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};

    let key = cache_key().ok_or_else(|| {
        Error(format!(
            "{} is sealed to an account. Sign in with the account that made it to open this project.",
            path.display()
        ))
    })?;
    let nonce = B64
        .decode(envelope.get("nonce").and_then(|n| n.as_str()).unwrap_or(""))
        .map_err(|err| Error(format!("{} is damaged: {err}", path.display())))?;
    let data = B64
        .decode(envelope.get("data").and_then(|d| d.as_str()).unwrap_or(""))
        .map_err(|err| Error(format!("{} is damaged: {err}", path.display())))?;
    let plain = XChaCha20Poly1305::new((&key).into())
        .decrypt(XNonce::from_slice(&nonce), data.as_ref())
        .map_err(|_| {
            Error(format!(
                "{} belongs to a different account and cannot be opened with this one.",
                path.display()
            ))
        })?;
    let parsed: Value = serde_json::from_slice(&plain)?;
    Ok(match parsed {
        Value::Array(a) => a.into_iter().filter(|d| d.is_object()).collect(),
        _ => Vec::new(),
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Error
// ────────────────────────────────────────────────────────────────────────────

/// Store failure. Deliberately a thin string wrapper: every caller in the app funnels errors into
/// `Result<_, String>` for the frontend, so a rich error enum would only be flattened again.
#[derive(Debug, Clone)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self { Error(e.to_string()) }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self { Error(e.to_string()) }
}
impl From<String> for Error {
    fn from(e: String) -> Self { Error(e) }
}
impl From<&str> for Error {
    fn from(e: &str) -> Self { Error(e.to_string()) }
}

pub type Result<T> = std::result::Result<T, Error>;

type BoxFuture<T> = Pin<Box<dyn std::future::Future<Output = T> + Send>>;

// ────────────────────────────────────────────────────────────────────────────
// Collection routing
// ────────────────────────────────────────────────────────────────────────────

/// Collections that belong to a single project, and the field that identifies its owner.
///
/// `song_id` entries are resolved indirectly: the store looks the song up (across every shard) and
/// takes *its* `project_id`. Anything not listed here is global.
///
/// `jobs` is intentionally **not** here: a job is transient machine state (queue position, logs,
/// retry counters), not project content, and writing it into the project repo would churn git
/// history on every progress tick.
const PROJECT_SCOPED: &[(&str, &str)] = &[
    ("songs", "project_id"),
    ("sections", "song_id"),
    ("characters", "project_id"),
    ("uploads", "song_id"),
    // Where each generated media file ended up once it was offloaded to an asset host — the
    // manifest that lets a fresh clone of the repo find its audio/video/images again.
    ("assets", "project_id"),
    // Derived shorts/posts for other platforms, and their publish state per destination.
    ("derivatives", "project_id"),
];

fn owner_field(collection: &str) -> Option<&'static str> {
    PROJECT_SCOPED.iter().find(|(c, _)| *c == collection).map(|(_, f)| *f)
}

fn is_project_scoped(collection: &str) -> bool { owner_field(collection).is_some() }

// ────────────────────────────────────────────────────────────────────────────
// Options (mirrors the `mongodb::options` builders the call sites used)
// ────────────────────────────────────────────────────────────────────────────

pub mod options {
    use bson::Document;

    /// Mirror of `crate::store::UpdateOptions` — only `upsert` was ever set.
    #[derive(Debug, Clone, Default)]
    pub struct UpdateOptions {
        pub upsert: Option<bool>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct UpdateOptionsBuilder {
        upsert: Option<bool>,
    }

    impl UpdateOptions {
        pub fn builder() -> UpdateOptionsBuilder { UpdateOptionsBuilder::default() }
    }

    impl UpdateOptionsBuilder {
        pub fn upsert(mut self, v: impl Into<Option<bool>>) -> Self {
            self.upsert = v.into();
            self
        }
        pub fn build(self) -> UpdateOptions { UpdateOptions { upsert: self.upsert } }
    }

    /// Mirror of `crate::store::FindOneOptions` — only `projection` was ever set.
    #[derive(Debug, Clone, Default)]
    pub struct FindOneOptions {
        pub projection: Option<Document>,
        pub sort: Option<Document>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct FindOneOptionsBuilder {
        projection: Option<Document>,
        sort: Option<Document>,
    }

    impl FindOneOptions {
        pub fn builder() -> FindOneOptionsBuilder { FindOneOptionsBuilder::default() }
    }

    impl FindOneOptionsBuilder {
        pub fn projection(mut self, v: impl Into<Option<Document>>) -> Self {
            self.projection = v.into();
            self
        }
        pub fn sort(mut self, v: impl Into<Option<Document>>) -> Self {
            self.sort = v.into();
            self
        }
        pub fn build(self) -> FindOneOptions {
            FindOneOptions { projection: self.projection, sort: self.sort }
        }
    }
}

pub use options::{FindOneOptions, UpdateOptions};

// ────────────────────────────────────────────────────────────────────────────
// Results (field-compatible with the driver's, for the two call sites that read them)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct UpdateResult {
    pub matched_count: u64,
    pub modified_count: u64,
    pub upserted_id: Option<Bson>,
}

#[derive(Debug, Clone, Default)]
pub struct DeleteResult {
    pub deleted_count: u64,
}

#[derive(Debug, Clone, Default)]
pub struct InsertOneResult {
    pub inserted_id: Bson,
}

#[derive(Debug, Clone, Default)]
pub struct InsertManyResult {
    pub inserted_ids: Vec<Bson>,
}

// ────────────────────────────────────────────────────────────────────────────
// bson ⇄ json conversion
// ────────────────────────────────────────────────────────────────────────────

/// Convert BSON to plain, hand-readable JSON — *not* MongoDB Extended JSON. Exotic BSON types
/// (ObjectId, DateTime, Binary, …) collapse to strings/numbers rather than `{"$oid": …}` wrappers,
/// because the stored files are meant to be read and edited by a human (and diffed by git).
pub fn bson_to_json(b: &Bson) -> Value {
    match b {
        Bson::Double(v) => json!(v),
        Bson::String(v) => json!(v),
        Bson::Array(v) => Value::Array(v.iter().map(bson_to_json).collect()),
        Bson::Document(d) => {
            let mut m = Map::new();
            for (k, v) in d {
                m.insert(k.clone(), bson_to_json(v));
            }
            Value::Object(m)
        }
        Bson::Boolean(v) => json!(v),
        Bson::Null | Bson::Undefined => Value::Null,
        Bson::RegularExpression(r) => json!(r.pattern),
        Bson::JavaScriptCode(c) => json!(c),
        Bson::JavaScriptCodeWithScope(c) => json!(c.code),
        Bson::Int32(v) => json!(v),
        Bson::Int64(v) => json!(v),
        Bson::Timestamp(t) => json!(t.time),
        Bson::Binary(bin) => json!(bin.bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()),
        Bson::ObjectId(id) => json!(id.to_hex()),
        Bson::DateTime(dt) => json!(dt.try_to_rfc3339_string().unwrap_or_else(|_| dt.to_string())),
        Bson::Symbol(s) => json!(s),
        Bson::Decimal128(d) => json!(d.to_string()),
        Bson::MaxKey | Bson::MinKey | Bson::DbPointer(_) => Value::Null,
    }
}

/// Convert a BSON document to a JSON object.
pub fn doc_to_json(d: &Document) -> Value { bson_to_json(&Bson::Document(d.clone())) }

/// Convert plain JSON back to BSON. Integral floats become `Int64` so round-tripping a value that
/// entered as `doc! { "index": 1 }` doesn't turn it into `1.0`.
pub fn json_to_bson(v: &Value) -> Bson {
    match v {
        Value::Null => Bson::Null,
        Value::Bool(b) => Bson::Boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Bson::Int64(i)
            } else {
                Bson::Double(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => Bson::String(s.clone()),
        Value::Array(a) => Bson::Array(a.iter().map(json_to_bson).collect()),
        Value::Object(o) => {
            let mut d = Document::new();
            for (k, val) in o {
                d.insert(k.clone(), json_to_bson(val));
            }
            Bson::Document(d)
        }
    }
}

/// Collapse any MongoDB Extended JSON wrappers a `Serialize` impl may have produced (bson's own
/// `Serialize` emits `{"$oid": …}` / `{"$date": …}` for human-readable serializers) into plain
/// scalars, so stored files never contain driver-specific envelopes.
fn sanitize(v: &mut Value) {
    if let Value::Object(map) = v {
        // A one-key object whose key starts with '$' is an ext-JSON envelope.
        if map.len() == 1 {
            let key = map.keys().next().cloned().unwrap_or_default();
            match key.as_str() {
                "$oid" | "$symbol" | "$code" => {
                    if let Some(s) = map.get(&key).and_then(|x| x.as_str()) {
                        *v = json!(s);
                        return;
                    }
                }
                "$numberLong" | "$numberInt" | "$numberDouble" | "$numberDecimal" => {
                    if let Some(s) = map.get(&key).and_then(|x| x.as_str()) {
                        if let Ok(i) = s.parse::<i64>() {
                            *v = json!(i);
                            return;
                        }
                        if let Ok(f) = s.parse::<f64>() {
                            *v = json!(f);
                            return;
                        }
                    }
                    if let Some(n) = map.get(&key).and_then(|x| x.as_f64()) {
                        *v = json!(n);
                        return;
                    }
                }
                "$date" => {
                    let inner = map.get("$date").cloned().unwrap_or(Value::Null);
                    let as_str = match &inner {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => n.as_i64().and_then(|ms| {
                            chrono::DateTime::from_timestamp_millis(ms).map(|d| d.to_rfc3339())
                        }),
                        Value::Object(o) => o
                            .get("$numberLong")
                            .and_then(|x| x.as_str())
                            .and_then(|s| s.parse::<i64>().ok())
                            .and_then(|ms| {
                                chrono::DateTime::from_timestamp_millis(ms).map(|d| d.to_rfc3339())
                            }),
                        _ => None,
                    };
                    if let Some(s) = as_str {
                        *v = json!(s);
                        return;
                    }
                }
                _ => {}
            }
        }
        for (_, val) in map.iter_mut() {
            sanitize(val);
        }
    } else if let Value::Array(arr) = v {
        for val in arr.iter_mut() {
            sanitize(val);
        }
    }
}

fn to_json_object<T: Serialize>(value: &T) -> Result<Value> {
    let mut v = serde_json::to_value(value)?;
    sanitize(&mut v);
    if !v.is_object() {
        return Err(Error("only object-shaped documents can be stored".into()));
    }
    Ok(v)
}

fn from_json<T: DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v).map_err(Error::from)
}

/// Read an integer field without caring which BSON width it came back as.
///
/// JSON has one number type; BSON has three. A value written as `doc! { "index": 1 }` (Int32) and
/// read back after a JSON round-trip can legitimately arrive as `Int32`, `Int64` or `Double`, so
/// `get_i32()`/`get_i64()`/`as_i64()` are all individually wrong. Callers that just want the number
/// use this instead.
pub fn get_num(doc: &Document, key: &str) -> Option<i64> {
    match doc.get(key)? {
        Bson::Int32(v) => Some(*v as i64),
        Bson::Int64(v) => Some(*v),
        Bson::Double(v) => Some(*v as i64),
        Bson::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Same, for fractional values (durations, timings).
pub fn get_float(doc: &Document, key: &str) -> Option<f64> {
    match doc.get(key)? {
        Bson::Double(v) => Some(*v),
        Bson::Int32(v) => Some(*v as f64),
        Bson::Int64(v) => Some(*v as f64),
        Bson::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn now_iso() -> String { chrono::Utc::now().to_rfc3339() }

// ────────────────────────────────────────────────────────────────────────────
// Query matching
// ────────────────────────────────────────────────────────────────────────────

/// Read a possibly dotted path (`"a.b.c"`) out of a document.
fn get_path<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = doc;
    for part in path.split('.') {
        match cur {
            Value::Object(m) => cur = m.get(part)?,
            Value::Array(a) => {
                let idx: usize = part.parse().ok()?;
                cur = a.get(idx)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

fn set_path(doc: &mut Value, path: &str, val: Value) {
    let parts: Vec<&str> = path.split('.').collect();
    let mut cur = doc;
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        if !cur.is_object() {
            *cur = Value::Object(Map::new());
        }
        let map = cur.as_object_mut().unwrap();
        if last {
            map.insert((*part).to_string(), val);
            return;
        }
        cur = map.entry((*part).to_string()).or_insert_with(|| Value::Object(Map::new()));
    }
}

fn unset_path(doc: &mut Value, path: &str) {
    let parts: Vec<&str> = path.split('.').collect();
    let mut cur = doc;
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        let Some(map) = cur.as_object_mut() else { return };
        if last {
            map.remove(*part);
            return;
        }
        match map.get_mut(*part) {
            Some(next) => cur = next,
            None => return,
        }
    }
}

/// Numeric-aware, type-tolerant equality. Numbers compare by value across int/float, everything
/// else structurally — so `doc! { "index": 1 }` matches a stored `1.0`, as MongoDB would.
fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(xf), Some(yf)) => xf == yf,
            _ => x == y,
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(i, j)| values_eq(i, j))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| y.get(k).map(|w| values_eq(v, w)).unwrap_or(false))
        }
        _ => a == b,
    }
}

/// Total ordering used by `$gt`/`$lt` and by `sort`. Mirrors MongoDB's cross-type ordering closely
/// enough for the app's data (null < number < string < bool < array < object).
fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    fn rank(v: &Value) -> u8 {
        match v {
            Value::Null => 0,
            Value::Number(_) => 1,
            Value::String(_) => 2,
            Value::Bool(_) => 3,
            Value::Array(_) => 4,
            Value::Object(_) => 5,
        }
    }
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x
            .as_f64()
            .partial_cmp(&y.as_f64())
            .unwrap_or(Ordering::Equal),
        (Value::String(x), Value::String(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Null, Value::Null) => Ordering::Equal,
        _ => rank(a).cmp(&rank(b)),
    }
}

fn bson_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() { "long" } else { "double" }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Does `cond` (a filter value, possibly an operator object) accept `actual` (which may be absent)?
fn match_condition(actual: Option<&Value>, cond: &Value) -> bool {
    let is_operator_obj = cond
        .as_object()
        .map(|m| m.keys().any(|k| k.starts_with('$')))
        .unwrap_or(false);

    if !is_operator_obj {
        return match actual {
            Some(a) => {
                if values_eq(a, cond) {
                    true
                } else if let (Value::Array(items), false) = (a, cond.is_array()) {
                    // MongoDB matches a scalar filter against any element of a stored array.
                    items.iter().any(|i| values_eq(i, cond))
                } else {
                    false
                }
            }
            // A filter of `null` also matches a missing field, as MongoDB does.
            None => cond.is_null(),
        };
    }

    let ops = cond.as_object().unwrap();
    for (op, want) in ops {
        let ok = match op.as_str() {
            "$eq" => match_condition(actual, want),
            "$ne" => !match_condition(actual, want),
            "$in" => want
                .as_array()
                .map(|list| {
                    list.iter().any(|w| match actual {
                        Some(a) => {
                            values_eq(a, w)
                                || a.as_array().map(|items| items.iter().any(|i| values_eq(i, w))).unwrap_or(false)
                        }
                        None => w.is_null(),
                    })
                })
                .unwrap_or(false),
            "$nin" => !want
                .as_array()
                .map(|list| {
                    list.iter().any(|w| match actual {
                        Some(a) => values_eq(a, w),
                        None => w.is_null(),
                    })
                })
                .unwrap_or(false),
            "$exists" => {
                let should_exist = want.as_bool().unwrap_or(true);
                actual.is_some() == should_exist
            }
            "$gt" => actual.map(|a| compare_values(a, want).is_gt()).unwrap_or(false),
            "$gte" => actual.map(|a| compare_values(a, want).is_ge()).unwrap_or(false),
            "$lt" => actual.map(|a| compare_values(a, want).is_lt()).unwrap_or(false),
            "$lte" => actual.map(|a| compare_values(a, want).is_le()).unwrap_or(false),
            "$regex" => {
                let pattern = want.as_str().unwrap_or("");
                let case_insensitive = ops
                    .get("$options")
                    .and_then(|o| o.as_str())
                    .map(|o| o.contains('i'))
                    .unwrap_or(false);
                let built = if case_insensitive {
                    regex::RegexBuilder::new(pattern).case_insensitive(true).build()
                } else {
                    regex::Regex::new(pattern)
                };
                match (built, actual.and_then(|a| a.as_str())) {
                    (Ok(re), Some(s)) => re.is_match(s),
                    _ => false,
                }
            }
            // Consumed together with $regex above.
            "$options" => true,
            "$type" => {
                let want_name = match want {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => String::new(),
                };
                actual
                    .map(|a| {
                        let name = bson_type_name(a);
                        name == want_name
                            || (want_name == "number" && matches!(a, Value::Number(_)))
                            || (want_name == "int" && matches!(a, Value::Number(_)))
                            || (want_name == "bool" && matches!(a, Value::Bool(_)))
                    })
                    .unwrap_or(false)
            }
            "$size" => actual
                .and_then(|a| a.as_array())
                .map(|arr| want.as_u64().map(|n| arr.len() as u64 == n).unwrap_or(false))
                .unwrap_or(false),
            "$all" => match (actual.and_then(|a| a.as_array()), want.as_array()) {
                (Some(have), Some(need)) => need.iter().all(|n| have.iter().any(|h| values_eq(h, n))),
                _ => false,
            },
            "$elemMatch" => actual
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().any(|item| matches_filter(item, want)))
                .unwrap_or(false),
            "$not" => !match_condition(actual, want),
            // Unknown operator: refuse to match rather than silently matching everything.
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    true
}

/// Does a stored document satisfy a filter?
pub fn matches_filter(doc: &Value, filter: &Value) -> bool {
    let Some(clauses) = filter.as_object() else { return true };
    for (key, cond) in clauses {
        let ok = match key.as_str() {
            "$or" => cond
                .as_array()
                .map(|list| list.iter().any(|f| matches_filter(doc, f)))
                .unwrap_or(false),
            "$and" => cond
                .as_array()
                .map(|list| list.iter().all(|f| matches_filter(doc, f)))
                .unwrap_or(true),
            "$nor" => cond
                .as_array()
                .map(|list| !list.iter().any(|f| matches_filter(doc, f)))
                .unwrap_or(true),
            "$expr" | "$comment" => true,
            _ => match_condition(get_path(doc, key), cond),
        };
        if !ok {
            return false;
        }
    }
    true
}

// ────────────────────────────────────────────────────────────────────────────
// Update application
// ────────────────────────────────────────────────────────────────────────────

/// Apply an update document in place. Returns whether anything actually changed (so
/// `modified_count` can be honest, which `commands/songs.rs` and `commands/sections.rs` report).
fn apply_update(doc: &mut Value, update: &Value, is_insert: bool) -> bool {
    let before = doc.clone();
    let Some(ops) = update.as_object() else { return false };
    let has_operators = ops.keys().any(|k| k.starts_with('$'));

    if !has_operators {
        // Whole-document replacement, preserving identity fields.
        let mut next = update.clone();
        for key in ["_id", "id"] {
            if let (Some(old), None) = (before.get(key), next.get(key)) {
                set_path(&mut next, key, old.clone());
            }
        }
        *doc = next;
        return *doc != before;
    }

    for (op, arg) in ops {
        let Some(fields) = arg.as_object() else { continue };
        match op.as_str() {
            "$set" => {
                for (path, val) in fields {
                    set_path(doc, path, val.clone());
                }
            }
            "$setOnInsert" => {
                if is_insert {
                    for (path, val) in fields {
                        set_path(doc, path, val.clone());
                    }
                }
            }
            "$unset" => {
                for (path, _) in fields {
                    unset_path(doc, path);
                }
            }
            "$inc" => {
                for (path, val) in fields {
                    let delta = val.as_f64().unwrap_or(0.0);
                    let current = get_path(doc, path).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let sum = current + delta;
                    let is_int = val.as_i64().is_some()
                        && get_path(doc, path).map(|v| v.as_i64().is_some() || v.is_null()).unwrap_or(true);
                    set_path(doc, path, if is_int { json!(sum as i64) } else { json!(sum) });
                }
            }
            "$push" => {
                for (path, val) in fields {
                    let mut arr = get_path(doc, path)
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default();
                    // `{ $each: [...] }` pushes many; a bare value pushes one.
                    match val.as_object().and_then(|o| o.get("$each")).and_then(|e| e.as_array()) {
                        Some(each) => arr.extend(each.iter().cloned()),
                        None => arr.push(val.clone()),
                    }
                    if let Some(slice) = val
                        .as_object()
                        .and_then(|o| o.get("$slice"))
                        .and_then(|s| s.as_i64())
                    {
                        // Negative $slice keeps the last N (the only form MongoDB allows with $push).
                        let keep = slice.unsigned_abs() as usize;
                        if arr.len() > keep {
                            let start = arr.len() - keep;
                            arr = arr[start..].to_vec();
                        }
                    }
                    set_path(doc, path, Value::Array(arr));
                }
            }
            "$addToSet" => {
                for (path, val) in fields {
                    let mut arr = get_path(doc, path)
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default();
                    let candidates: Vec<Value> = match val
                        .as_object()
                        .and_then(|o| o.get("$each"))
                        .and_then(|e| e.as_array())
                    {
                        Some(each) => each.clone(),
                        None => vec![val.clone()],
                    };
                    for c in candidates {
                        if !arr.iter().any(|x| values_eq(x, &c)) {
                            arr.push(c);
                        }
                    }
                    set_path(doc, path, Value::Array(arr));
                }
            }
            "$pull" => {
                for (path, val) in fields {
                    if let Some(arr) = get_path(doc, path).and_then(|v| v.as_array().cloned()) {
                        let kept: Vec<Value> = arr
                            .into_iter()
                            .filter(|item| !(values_eq(item, val) || matches_filter(item, val)))
                            .collect();
                        set_path(doc, path, Value::Array(kept));
                    }
                }
            }
            "$pop" => {
                for (path, val) in fields {
                    if let Some(mut arr) = get_path(doc, path).and_then(|v| v.as_array().cloned()) {
                        if !arr.is_empty() {
                            if val.as_i64().unwrap_or(1) < 0 { arr.remove(0); } else { arr.pop(); }
                        }
                        set_path(doc, path, Value::Array(arr));
                    }
                }
            }
            "$rename" => {
                for (path, val) in fields {
                    if let (Some(to), Some(existing)) = (val.as_str(), get_path(doc, path).cloned()) {
                        unset_path(doc, path);
                        set_path(doc, to, existing);
                    }
                }
            }
            _ => {}
        }
    }
    *doc != before
}

/// Build the document an upsert should insert: the filter's plain equality fields, then the update.
fn upsert_seed(filter: &Value, update: &Value) -> Value {
    let mut seed = Value::Object(Map::new());
    if let Some(clauses) = filter.as_object() {
        for (key, cond) in clauses {
            if key.starts_with('$') {
                continue;
            }
            let plain = !cond
                .as_object()
                .map(|m| m.keys().any(|k| k.starts_with('$')))
                .unwrap_or(false);
            if plain {
                set_path(&mut seed, key, cond.clone());
            } else if let Some(eq) = cond.as_object().and_then(|m| m.get("$eq")) {
                set_path(&mut seed, key, eq.clone());
            }
        }
    }
    apply_update(&mut seed, update, true);
    seed
}

fn sort_documents(docs: &mut [Value], sort: &Value) {
    let Some(keys) = sort.as_object() else { return };
    let spec: Vec<(String, i64)> = keys
        .iter()
        .map(|(k, v)| (k.clone(), v.as_i64().unwrap_or_else(|| if v.as_f64().unwrap_or(1.0) < 0.0 { -1 } else { 1 })))
        .collect();
    docs.sort_by(|a, b| {
        for (key, dir) in &spec {
            let av = get_path(a, key).unwrap_or(&Value::Null);
            let bv = get_path(b, key).unwrap_or(&Value::Null);
            let ord = compare_values(av, bv);
            if ord != std::cmp::Ordering::Equal {
                return if *dir < 0 { ord.reverse() } else { ord };
            }
        }
        std::cmp::Ordering::Equal
    });
}

fn apply_projection(doc: &mut Value, projection: &Value) {
    let Some(spec) = projection.as_object() else { return };
    let excluding = spec.values().any(|v| v.as_i64() == Some(0) || v.as_bool() == Some(false));
    if excluding {
        for (key, want) in spec {
            if want.as_i64() == Some(0) || want.as_bool() == Some(false) {
                unset_path(doc, key);
            }
        }
    } else {
        let keep: HashSet<&str> = spec.keys().map(|k| k.as_str()).collect();
        if let Some(map) = doc.as_object_mut() {
            map.retain(|k, _| keep.contains(k.as_str()) || k == "_id");
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Shards
// ────────────────────────────────────────────────────────────────────────────

/// One collection file, cached in memory. Reloaded when the file changes underneath us (a user
/// hand-editing a project's JSON, or a `git checkout` of an older project version — both of which
/// the project-git feature makes ordinary).
struct Shard {
    path: PathBuf,
    docs: Vec<Value>,
    loaded_mtime: Option<std::time::SystemTime>,
    loaded: bool,
    /// Project shards may be sealed; the global folder never is (see the sealed-cache section).
    sealable: bool,
}

impl Shard {
    fn new(path: PathBuf, sealable: bool) -> Self {
        Shard { path, docs: Vec::new(), loaded_mtime: None, loaded: false, sealable }
    }

    fn file_mtime(&self) -> Option<std::time::SystemTime> {
        std::fs::metadata(&self.path).ok().and_then(|m| m.modified().ok())
    }

    fn ensure_loaded(&mut self) -> Result<()> {
        let mtime = self.file_mtime();
        if self.loaded && mtime == self.loaded_mtime {
            return Ok(());
        }
        self.docs = match std::fs::read_to_string(&self.path) {
            Ok(text) if !text.trim().is_empty() => {
                let parsed: Value = serde_json::from_str(&text).map_err(|err| {
                    Error(format!("{} is not valid JSON: {err}", self.path.display()))
                })?;
                // A sealed shard needs the account key; plain text is read as it always was, so a
                // folder that is half converted still opens.
                if is_sealed(&parsed) {
                    open_documents(&parsed, &self.path)?
                } else {
                    // Accept both the envelope and a bare array, so a hand-written file works too.
                    let list = match parsed {
                        Value::Array(a) => a,
                        Value::Object(mut o) => match o.remove("documents") {
                            Some(Value::Array(a)) => a,
                            _ => Vec::new(),
                        },
                        _ => Vec::new(),
                    };
                    list.into_iter().filter(|d| d.is_object()).collect()
                }
            }
            _ => Vec::new(),
        };
        self.loaded = true;
        self.loaded_mtime = mtime;
        Ok(())
    }

    fn flush(&mut self, collection: &str) -> Result<()> {
        self.write_as(collection, self.sealable && cache_sealing())
    }

    /// Write the shard, sealed or plain. Split out from `flush` so the "seal my projects" and
    /// "unseal them again" conversions are the same code path as an ordinary save.
    fn write_as(&mut self, collection: &str, sealed: bool) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let envelope = match (sealed, cache_key()) {
            (true, Some(key)) => seal_documents(&key, collection, &self.docs)?,
            _ => json!({
                "version": 1,
                "collection": collection,
                "updated_at": now_iso(),
                "documents": self.docs,
            }),
        };
        let text = serde_json::to_string_pretty(&envelope)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text.as_bytes())?;
        std::fs::rename(&tmp, &self.path)?;
        self.loaded_mtime = self.file_mtime();
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Db
// ────────────────────────────────────────────────────────────────────────────

struct Inner {
    global_root: PathBuf,
    shards: Mutex<HashMap<PathBuf, Arc<Mutex<Shard>>>>,
    /// project id → that project's `data/` directory (inside its git repo).
    project_roots: Mutex<HashMap<String, PathBuf>>,
    roots_refreshed: Mutex<Option<Instant>>,
    /// Projects whose files changed since the last git auto-commit sweep (see `project_sync.rs`).
    dirty_projects: Mutex<HashSet<String>>,
}

/// A handle to the JSON store. Cheap to clone (the app clones it into every background task).
#[derive(Clone)]
pub struct Db {
    inner: Arc<Inner>,
}

impl Db {
    /// Open (creating if needed) the store rooted at `global_root`.
    pub fn open(global_root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&global_root)?;
        Ok(Db {
            inner: Arc::new(Inner {
                global_root,
                shards: Mutex::new(HashMap::new()),
                project_roots: Mutex::new(HashMap::new()),
                roots_refreshed: Mutex::new(None),
                dirty_projects: Mutex::new(HashSet::new()),
            }),
        })
    }

    /// The conventional location: `<config>/studio-lightkid/data`, matching where the JSON
    /// learnings store already writes.
    pub fn default_root() -> Result<PathBuf> {
        let base = crate::paths::config_dir().ok_or_else(|| Error("no config directory".into()))?;
        Ok(base.join("studio-lightkid").join("data"))
    }

    pub fn open_default() -> Result<Self> { Self::open(Self::default_root()?) }

    pub fn global_root(&self) -> &Path { &self.inner.global_root }

    pub fn collection<T>(&self, name: &str) -> Collection<T> {
        Collection { inner: self.inner.clone(), name: name.to_string(), _marker: PhantomData }
    }

    /// Tell the store where a project keeps its data (called when a project's folder is created or
    /// changes). Idempotent.
    pub async fn register_project(&self, project_id: &str, folder: &str) {
        if project_id.is_empty() || folder.trim().is_empty() {
            return;
        }
        let root = PathBuf::from(folder).join("data");
        self.inner.project_roots.lock().await.insert(project_id.to_string(), root);
    }

    /// Drain the set of projects whose JSON changed — the git auto-commit sweep consumes this.
    pub async fn take_dirty_projects(&self) -> Vec<String> {
        let mut dirty = self.inner.dirty_projects.lock().await;
        dirty.drain().collect()
    }

    /// Mark a project dirty by hand (used after bulk imports).
    pub async fn mark_project_dirty(&self, project_id: &str) {
        self.inner.dirty_projects.lock().await.insert(project_id.to_string());
    }

    /// Force-drop cached shards, so the next read re-parses from disk. Used after a git checkout
    /// swaps a project's files underneath the app.
    pub async fn invalidate_cache(&self) {
        self.inner.shards.lock().await.clear();
        *self.inner.roots_refreshed.lock().await = None;
    }

    /// Where everything lives, for the UI's "your data is here" panel.
    pub async fn locations(&self) -> Value {
        self.inner.refresh_project_roots().await;
        let roots = self.inner.project_roots.lock().await;
        let projects: Vec<Value> = roots
            .iter()
            .map(|(id, path)| json!({ "project_id": id, "path": path.to_string_lossy() }))
            .collect();
        json!({
            "global": self.inner.global_root.to_string_lossy(),
            "projects": projects,
            "project_scoped_collections": PROJECT_SCOPED.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        })
    }

    /// Per-collection document counts — used by the migration verifier and the data panel.
    pub async fn counts(&self) -> Result<Value> {
        let mut out = Map::new();
        let names = self.collection_names().await?;
        for name in names {
            let n = self.collection::<Document>(&name).count_documents(Document::new()).await?;
            out.insert(name, json!(n));
        }
        Ok(Value::Object(out))
    }

    /// Seal (or unseal) every project shard on disk, in place.
    ///
    /// Turning sealing on only affects files as they are next written, which would leave a project
    /// nobody has touched lying in plain text indefinitely. This walks them all so "my projects are
    /// sealed" is true when the setting says it is — and unseals them again on the way back, so the
    /// choice is reversible rather than a one-way door.
    pub async fn reseal_projects(&self, sealed: bool) -> Result<Value> {
        if sealed && !cache_unlocked() {
            return Err(Error("Sign in first — the key comes from your account.".into()));
        }
        self.inner.refresh_project_roots().await;
        let roots: Vec<PathBuf> = self.inner.project_roots.lock().await.values().cloned().collect();
        let mut converted = 0usize;
        let mut failed: Vec<String> = Vec::new();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let Some(collection) = path.file_stem().and_then(|s| s.to_str()).map(String::from)
                else {
                    continue;
                };
                let shard = self.inner.shard(path.clone()).await;
                let mut guard = shard.lock().await;
                // Load then rewrite: a shard already in the wanted form is rewritten identically,
                // which costs one file write and keeps the code a single path.
                match guard.ensure_loaded().and_then(|_| guard.write_as(&collection, sealed)) {
                    Ok(()) => converted += 1,
                    Err(err) => failed.push(format!("{}: {err}", path.display())),
                }
            }
        }
        Ok(json!({ "ok": failed.is_empty(), "sealed": sealed, "files": converted, "failed": failed }))
    }

    /// Every collection with at least one shard file on disk.
    pub async fn collection_names(&self) -> Result<Vec<String>> {
        self.inner.refresh_project_roots().await;
        let mut names: HashSet<String> = HashSet::new();
        let mut dirs = vec![self.inner.global_root.clone()];
        dirs.extend(self.inner.project_roots.lock().await.values().cloned());
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        names.insert(stem.to_string());
                    }
                }
            }
        }
        let mut sorted: Vec<String> = names.into_iter().collect();
        sorted.sort();
        Ok(sorted)
    }
}

impl Inner {
    /// Re-read project folders from the (global) `projects` collection, at most every few seconds.
    async fn refresh_project_roots(&self) {
        {
            let last = self.roots_refreshed.lock().await;
            if let Some(t) = *last {
                if t.elapsed() < Duration::from_secs(3) {
                    return;
                }
            }
        }
        let path = self.global_root.join("projects.json");
        let shard = self.shard(path).await;
        let docs = {
            let mut guard = shard.lock().await;
            if guard.ensure_loaded().is_err() {
                return;
            }
            guard.docs.clone()
        };
        let mut roots = self.project_roots.lock().await;
        for doc in docs {
            let id = doc.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let folder = doc.get("project_folder").and_then(|v| v.as_str()).unwrap_or("");
            if !id.is_empty() && !folder.trim().is_empty() {
                roots.insert(id.to_string(), PathBuf::from(folder).join("data"));
            }
        }
        *self.roots_refreshed.lock().await = Some(Instant::now());
    }

    async fn shard(&self, path: PathBuf) -> Arc<Mutex<Shard>> {
        // Anything outside the global folder belongs to a project, and only those may be sealed —
        // `settings` holds the entitlement the key is derived from, so sealing it would lock the
        // app out of its own bootstrap.
        let sealable = path.parent() != Some(self.global_root.as_path());
        let mut map = self.shards.lock().await;
        map.entry(path.clone())
            .or_insert_with(|| Arc::new(Mutex::new(Shard::new(path, sealable))))
            .clone()
    }

    /// Every shard that could hold a document of `collection`, global first.
    async fn shards_for(&self, collection: &str) -> Vec<(Option<String>, Arc<Mutex<Shard>>)> {
        let mut out = vec![(
            None,
            self.shard(self.global_root.join(format!("{collection}.json"))).await,
        )];
        if is_project_scoped(collection) {
            self.refresh_project_roots().await;
            let roots: Vec<(String, PathBuf)> = {
                let guard = self.project_roots.lock().await;
                guard.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            };
            for (pid, root) in roots {
                out.push((Some(pid), self.shard(root.join(format!("{collection}.json"))).await));
            }
        }
        out
    }

    /// Which project owns this document? `None` ⇒ it goes to the global shard.
    async fn owner_project(&self, collection: &str, doc: &Value) -> Option<String> {
        let field = owner_field(collection)?;
        // Direct owner reference.
        if let Some(pid) = doc.get(field).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            if field == "project_id" {
                return Some(pid.to_string());
            }
            // Indirect: `song_id` → that song's project.
            if field == "song_id" {
                return self.project_of_song(pid).await;
            }
        }
        // `characters` may be attached to a song instead of a project.
        if collection == "characters" {
            if let Some(sid) = doc.get("song_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                return self.project_of_song(sid).await;
            }
        }
        None
    }

    async fn project_of_song(&self, song_id: &str) -> Option<String> {
        let shards = self.shards_for("songs").await;
        for (_, shard) in shards {
            let mut guard = shard.lock().await;
            if guard.ensure_loaded().is_err() {
                continue;
            }
            for doc in &guard.docs {
                if doc.get("id").and_then(|v| v.as_str()) == Some(song_id) {
                    return doc
                        .get("project_id")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string());
                }
            }
        }
        None
    }

    /// The shard a new/updated document should be written to.
    async fn shard_for(
        &self,
        collection: &str,
        doc: &Value,
    ) -> (Option<String>, Arc<Mutex<Shard>>) {
        if let Some(pid) = self.owner_project(collection, doc).await {
            self.refresh_project_roots().await;
            let root = self.project_roots.lock().await.get(&pid).cloned();
            if let Some(root) = root {
                return (Some(pid), self.shard(root.join(format!("{collection}.json"))).await);
            }
        }
        (None, self.shard(self.global_root.join(format!("{collection}.json"))).await)
    }

    async fn mark_dirty(&self, project: &Option<String>) {
        if let Some(pid) = project {
            self.dirty_projects.lock().await.insert(pid.clone());
        }
    }

    /// Collect documents matching `filter` from every shard.
    async fn query(&self, collection: &str, filter: &Value) -> Result<Vec<Value>> {
        let shards = self.shards_for(collection).await;
        let mut out = Vec::new();
        for (_, shard) in shards {
            let mut guard = shard.lock().await;
            guard.ensure_loaded()?;
            for doc in &guard.docs {
                if matches_filter(doc, filter) {
                    out.push(doc.clone());
                }
            }
        }
        Ok(out)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Collection + actions
// ────────────────────────────────────────────────────────────────────────────

/// A named collection. `T` is the shape returned by reads (the app always uses `bson::Document`,
/// but any `Deserialize` type works).
pub struct Collection<T> {
    inner: Arc<Inner>,
    name: String,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Collection<T> {
    fn clone(&self) -> Self {
        Collection { inner: self.inner.clone(), name: self.name.clone(), _marker: PhantomData }
    }
}

impl<T> Collection<T> {
    pub fn name(&self) -> &str { &self.name }

    pub fn find_one(&self, filter: Document) -> FindOne<T> {
        FindOne {
            inner: self.inner.clone(),
            name: self.name.clone(),
            filter,
            sort: None,
            projection: None,
            _marker: PhantomData,
        }
    }

    pub fn find(&self, filter: Document) -> Find<T> {
        Find {
            inner: self.inner.clone(),
            name: self.name.clone(),
            filter,
            sort: None,
            projection: None,
            limit: None,
            skip: None,
            _marker: PhantomData,
        }
    }

    pub fn insert_one(&self, doc: impl Serialize) -> InsertOne {
        InsertOne {
            inner: self.inner.clone(),
            name: self.name.clone(),
            doc: to_json_object(&doc),
        }
    }

    pub fn insert_many(&self, docs: impl IntoIterator<Item = impl Serialize>) -> InsertMany {
        InsertMany {
            inner: self.inner.clone(),
            name: self.name.clone(),
            docs: docs.into_iter().map(|d| to_json_object(&d)).collect(),
        }
    }

    pub fn update_one(&self, filter: Document, update: Document) -> Update {
        Update {
            inner: self.inner.clone(),
            name: self.name.clone(),
            filter,
            update,
            upsert: false,
            many: false,
        }
    }

    pub fn update_many(&self, filter: Document, update: Document) -> Update {
        Update {
            inner: self.inner.clone(),
            name: self.name.clone(),
            filter,
            update,
            upsert: false,
            many: true,
        }
    }

    pub fn delete_one(&self, filter: Document) -> Delete {
        Delete { inner: self.inner.clone(), name: self.name.clone(), filter, many: false }
    }

    pub fn delete_many(&self, filter: Document) -> Delete {
        Delete { inner: self.inner.clone(), name: self.name.clone(), filter, many: true }
    }

    pub fn count_documents(&self, filter: Document) -> Count {
        Count { inner: self.inner.clone(), name: self.name.clone(), filter }
    }
}

// ── find_one ────────────────────────────────────────────────────────────────

pub struct FindOne<T> {
    inner: Arc<Inner>,
    name: String,
    filter: Document,
    sort: Option<Document>,
    projection: Option<Document>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> FindOne<T> {
    pub fn sort(mut self, sort: Document) -> Self {
        self.sort = Some(sort);
        self
    }
    pub fn projection(mut self, projection: Document) -> Self {
        self.projection = Some(projection);
        self
    }
    pub fn with_options(mut self, opts: impl Into<Option<FindOneOptions>>) -> Self {
        if let Some(o) = opts.into() {
            if o.projection.is_some() {
                self.projection = o.projection;
            }
            if o.sort.is_some() {
                self.sort = o.sort;
            }
        }
        self
    }
}

impl<T: DeserializeOwned + Send + 'static> IntoFuture for FindOne<T> {
    type Output = Result<Option<T>>;
    type IntoFuture = BoxFuture<Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let filter = doc_to_json(&self.filter);
            let mut docs = self.inner.query(&self.name, &filter).await?;
            if let Some(sort) = &self.sort {
                sort_documents(&mut docs, &doc_to_json(sort));
            }
            let Some(mut first) = docs.into_iter().next() else { return Ok(None) };
            if let Some(projection) = &self.projection {
                apply_projection(&mut first, &doc_to_json(projection));
            }
            Ok(Some(from_json(first)?))
        })
    }
}

// ── find ────────────────────────────────────────────────────────────────────

pub struct Find<T> {
    inner: Arc<Inner>,
    name: String,
    filter: Document,
    sort: Option<Document>,
    projection: Option<Document>,
    limit: Option<i64>,
    skip: Option<u64>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Find<T> {
    pub fn sort(mut self, sort: Document) -> Self {
        self.sort = Some(sort);
        self
    }
    pub fn projection(mut self, projection: Document) -> Self {
        self.projection = Some(projection);
        self
    }
    pub fn limit(mut self, limit: impl Into<i64>) -> Self {
        self.limit = Some(limit.into());
        self
    }
    pub fn skip(mut self, skip: impl Into<u64>) -> Self {
        self.skip = Some(skip.into());
        self
    }
}

impl<T: DeserializeOwned + Send + 'static> IntoFuture for Find<T> {
    type Output = Result<Cursor<T>>;
    type IntoFuture = BoxFuture<Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let filter = doc_to_json(&self.filter);
            let mut docs = self.inner.query(&self.name, &filter).await?;
            if let Some(sort) = &self.sort {
                sort_documents(&mut docs, &doc_to_json(sort));
            }
            if let Some(skip) = self.skip {
                let skip = skip as usize;
                docs = if skip >= docs.len() { Vec::new() } else { docs.split_off(skip) };
            }
            if let Some(limit) = self.limit {
                if limit > 0 && docs.len() > limit as usize {
                    docs.truncate(limit as usize);
                }
            }
            if let Some(projection) = &self.projection {
                let p = doc_to_json(projection);
                for doc in docs.iter_mut() {
                    apply_projection(doc, &p);
                }
            }
            Ok(Cursor { items: docs.into_iter(), _marker: PhantomData })
        })
    }
}

/// A materialized result set. Implements `Stream`, so the existing
/// `while let Some(Ok(doc)) = cursor.next().await` / `cursor.try_next().await?` loops work
/// unchanged via `futures_util`.
pub struct Cursor<T> {
    items: std::vec::IntoIter<Value>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Cursor<T> {
    pub fn is_empty(&self) -> bool { self.items.len() == 0 }
    pub fn len(&self) -> usize { self.items.len() }
}

impl<T: DeserializeOwned> futures_util::Stream for Cursor<T> {
    type Item = Result<T>;

    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.items.next() {
            Some(v) => std::task::Poll::Ready(Some(from_json(v))),
            None => std::task::Poll::Ready(None),
        }
    }
}

// ── insert ──────────────────────────────────────────────────────────────────

pub struct InsertOne {
    inner: Arc<Inner>,
    name: String,
    doc: Result<Value>,
}

impl IntoFuture for InsertOne {
    type Output = Result<InsertOneResult>;
    type IntoFuture = BoxFuture<Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let doc = self.doc?;
            let (project, shard) = self.inner.shard_for(&self.name, &doc).await;
            let id = doc
                .get("_id")
                .or_else(|| doc.get("id"))
                .map(json_to_bson)
                .unwrap_or(Bson::Null);
            {
                let mut guard = shard.lock().await;
                guard.ensure_loaded()?;
                guard.docs.push(doc);
                guard.flush(&self.name)?;
            }
            self.inner.mark_dirty(&project).await;
            Ok(InsertOneResult { inserted_id: id })
        })
    }
}

pub struct InsertMany {
    inner: Arc<Inner>,
    name: String,
    docs: Vec<Result<Value>>,
}

impl IntoFuture for InsertMany {
    type Output = Result<InsertManyResult>;
    type IntoFuture = BoxFuture<Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut ids = Vec::new();
            for doc in self.docs {
                let doc = doc?;
                let (project, shard) = self.inner.shard_for(&self.name, &doc).await;
                ids.push(
                    doc.get("_id").or_else(|| doc.get("id")).map(json_to_bson).unwrap_or(Bson::Null),
                );
                {
                    let mut guard = shard.lock().await;
                    guard.ensure_loaded()?;
                    guard.docs.push(doc);
                    guard.flush(&self.name)?;
                }
                self.inner.mark_dirty(&project).await;
            }
            Ok(InsertManyResult { inserted_ids: ids })
        })
    }
}

// ── update ──────────────────────────────────────────────────────────────────

pub struct Update {
    inner: Arc<Inner>,
    name: String,
    filter: Document,
    update: Document,
    upsert: bool,
    many: bool,
}

impl Update {
    pub fn upsert(mut self, on: impl Into<Option<bool>>) -> Self {
        self.upsert = on.into().unwrap_or(false);
        self
    }
    pub fn with_options(mut self, opts: impl Into<Option<UpdateOptions>>) -> Self {
        if let Some(o) = opts.into() {
            self.upsert = o.upsert.unwrap_or(self.upsert);
        }
        self
    }
}

impl IntoFuture for Update {
    type Output = Result<UpdateResult>;
    type IntoFuture = BoxFuture<Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let filter = doc_to_json(&self.filter);
            let update = doc_to_json(&self.update);
            let mut result = UpdateResult::default();

            let shards = self.inner.shards_for(&self.name).await;
            for (project, shard) in &shards {
                let mut changed_any = false;
                {
                    let mut guard = shard.lock().await;
                    guard.ensure_loaded()?;
                    for doc in guard.docs.iter_mut() {
                        if !matches_filter(doc, &filter) {
                            continue;
                        }
                        result.matched_count += 1;
                        if apply_update(doc, &update, false) {
                            result.modified_count += 1;
                            changed_any = true;
                        }
                        if !self.many {
                            break;
                        }
                    }
                    if changed_any {
                        guard.flush(&self.name)?;
                    }
                }
                if changed_any {
                    self.inner.mark_dirty(project).await;
                }
                if result.matched_count > 0 && !self.many {
                    break;
                }
            }

            if result.matched_count == 0 && self.upsert {
                let seed = upsert_seed(&filter, &update);
                let id = seed
                    .get("_id")
                    .or_else(|| seed.get("id"))
                    .map(json_to_bson)
                    .unwrap_or(Bson::Null);
                let (project, shard) = self.inner.shard_for(&self.name, &seed).await;
                {
                    let mut guard = shard.lock().await;
                    guard.ensure_loaded()?;
                    guard.docs.push(seed);
                    guard.flush(&self.name)?;
                }
                self.inner.mark_dirty(&project).await;
                result.upserted_id = Some(id);
            }

            Ok(result)
        })
    }
}

// ── delete ──────────────────────────────────────────────────────────────────

pub struct Delete {
    inner: Arc<Inner>,
    name: String,
    filter: Document,
    many: bool,
}

impl IntoFuture for Delete {
    type Output = Result<DeleteResult>;
    type IntoFuture = BoxFuture<Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let filter = doc_to_json(&self.filter);
            let mut deleted = 0u64;
            let shards = self.inner.shards_for(&self.name).await;
            for (project, shard) in &shards {
                let mut changed = false;
                {
                    let mut guard = shard.lock().await;
                    guard.ensure_loaded()?;
                    if self.many {
                        let before = guard.docs.len();
                        guard.docs.retain(|d| !matches_filter(d, &filter));
                        let removed = (before - guard.docs.len()) as u64;
                        if removed > 0 {
                            deleted += removed;
                            changed = true;
                        }
                    } else if let Some(idx) = guard.docs.iter().position(|d| matches_filter(d, &filter)) {
                        guard.docs.remove(idx);
                        deleted += 1;
                        changed = true;
                    }
                    if changed {
                        guard.flush(&self.name)?;
                    }
                }
                if changed {
                    self.inner.mark_dirty(project).await;
                    if !self.many {
                        break;
                    }
                }
            }
            Ok(DeleteResult { deleted_count: deleted })
        })
    }
}

// ── count ───────────────────────────────────────────────────────────────────

pub struct Count {
    inner: Arc<Inner>,
    name: String,
    filter: Document,
}

impl IntoFuture for Count {
    type Output = Result<u64>;
    type IntoFuture = BoxFuture<Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let filter = doc_to_json(&self.filter);
            let docs = self.inner.query(&self.name, &filter).await?;
            Ok(docs.len() as u64)
        })
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use futures_util::StreamExt;

    fn temp_db(tag: &str) -> Db {
        let dir = std::env::temp_dir().join(format!("lightkid-store-test-{tag}-{}", uuid::Uuid::new_v4()));
        Db::open(dir).unwrap()
    }

    #[tokio::test]
    async fn insert_find_update_delete_roundtrip() {
        let db = temp_db("crud");
        let coll = db.collection::<Document>("channels");

        coll.insert_one(doc! { "id": "c1", "name": "Alpha", "language": "English" }).await.unwrap();
        coll.insert_one(doc! { "id": "c2", "name": "Beta", "language": "German" }).await.unwrap();

        let found = coll.find_one(doc! { "id": "c1" }).await.unwrap().unwrap();
        assert_eq!(found.get_str("name").unwrap(), "Alpha");

        let res = coll.update_one(doc! { "id": "c1" }, doc! { "$set": { "name": "Alpha 2" } }).await.unwrap();
        assert_eq!(res.matched_count, 1);
        assert_eq!(res.modified_count, 1);
        let found = coll.find_one(doc! { "id": "c1" }).await.unwrap().unwrap();
        assert_eq!(found.get_str("name").unwrap(), "Alpha 2");

        assert_eq!(coll.count_documents(doc! {}).await.unwrap(), 2);
        let gone = coll.delete_one(doc! { "id": "c2" }).await.unwrap();
        assert_eq!(gone.deleted_count, 1);
        assert_eq!(coll.count_documents(doc! {}).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn survives_a_reopen() {
        let dir = std::env::temp_dir().join(format!("lightkid-store-persist-{}", uuid::Uuid::new_v4()));
        {
            let db = Db::open(dir.clone()).unwrap();
            db.collection::<Document>("settings")
                .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "theme": "obsidian" } })
                .upsert(true)
                .await
                .unwrap();
        }
        let db = Db::open(dir).unwrap();
        let s = db.collection::<Document>("settings").find_one(doc! { "_id": "singleton" }).await.unwrap();
        assert_eq!(s.unwrap().get_str("theme").unwrap(), "obsidian");
    }

    #[tokio::test]
    async fn upsert_seeds_from_filter() {
        let db = temp_db("upsert");
        let coll = db.collection::<Document>("settings");
        let res = coll
            .update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie": "abc" } })
            .upsert(true)
            .await
            .unwrap();
        assert!(res.upserted_id.is_some());
        let got = coll.find_one(doc! { "_id": "singleton" }).await.unwrap().unwrap();
        assert_eq!(got.get_str("_id").unwrap(), "singleton");
        assert_eq!(got.get_str("suno_cookie").unwrap(), "abc");

        // A second upsert must update, not duplicate.
        coll.update_one(doc! { "_id": "singleton" }, doc! { "$set": { "suno_cookie": "def" } })
            .upsert(true)
            .await
            .unwrap();
        assert_eq!(coll.count_documents(doc! {}).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn operators_in_filters() {
        let db = temp_db("ops");
        let coll = db.collection::<Document>("jobs");
        coll.insert_one(doc! { "id": "j1", "kind": "music", "status": "queued", "attempts": 0 }).await.unwrap();
        coll.insert_one(doc! { "id": "j2", "kind": "image", "status": "done", "attempts": 3 }).await.unwrap();
        coll.insert_one(doc! { "id": "j3", "kind": "video", "status": "failed" }).await.unwrap();

        let n = coll.count_documents(doc! { "status": { "$in": ["queued", "failed"] } }).await.unwrap();
        assert_eq!(n, 2);
        let n = coll.count_documents(doc! { "status": { "$ne": "done" } }).await.unwrap();
        assert_eq!(n, 2);
        let n = coll.count_documents(doc! { "attempts": { "$exists": true } }).await.unwrap();
        assert_eq!(n, 2);
        let n = coll.count_documents(doc! { "attempts": { "$gt": 1 } }).await.unwrap();
        assert_eq!(n, 1);
        let n = coll
            .count_documents(doc! { "$or": [ { "kind": "music" }, { "kind": "video" } ] })
            .await
            .unwrap();
        assert_eq!(n, 2);
        let n = coll.count_documents(doc! { "kind": { "$regex": "^IMA", "$options": "i" } }).await.unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn push_and_inc_operators() {
        let db = temp_db("push");
        let coll = db.collection::<Document>("jobs");
        coll.insert_one(doc! { "id": "j1", "logs": [], "attempts": 0 }).await.unwrap();
        coll.update_one(doc! { "id": "j1" }, doc! { "$push": { "logs": "started" } }).await.unwrap();
        coll.update_one(doc! { "id": "j1" }, doc! { "$push": { "logs": "finished" } }).await.unwrap();
        coll.update_one(doc! { "id": "j1" }, doc! { "$inc": { "attempts": 1 } }).await.unwrap();
        let job = coll.find_one(doc! { "id": "j1" }).await.unwrap().unwrap();
        let logs = job.get_array("logs").unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(get_num(&job, "attempts").unwrap(), 1);

        coll.update_one(doc! { "id": "j1" }, doc! { "$unset": { "attempts": "" } }).await.unwrap();
        let job = coll.find_one(doc! { "id": "j1" }).await.unwrap().unwrap();
        assert!(job.get("attempts").is_none());
    }

    #[tokio::test]
    async fn sort_limit_and_cursor_stream() {
        let db = temp_db("sort");
        let coll = db.collection::<Document>("sections");
        for i in [3, 1, 2] {
            coll.insert_one(doc! { "id": format!("s{i}"), "song_id": "song", "index": i }).await.unwrap();
        }
        let mut cursor = coll.find(doc! { "song_id": "song" }).sort(doc! { "index": 1 }).await.unwrap();
        let mut order = Vec::new();
        while let Some(Ok(doc)) = cursor.next().await {
            order.push(get_num(&doc, "index").unwrap());
        }
        assert_eq!(order, vec![1, 2, 3]);

        let mut cursor = coll.find(doc! {}).sort(doc! { "index": -1 }).limit(2i64).await.unwrap();
        let mut desc = Vec::new();
        while let Some(Ok(doc)) = cursor.next().await {
            desc.push(get_num(&doc, "index").unwrap());
        }
        assert_eq!(desc, vec![3, 2]);
    }

    #[tokio::test]
    async fn projection_strips_id() {
        let db = temp_db("proj");
        let coll = db.collection::<Document>("settings");
        coll.insert_one(doc! { "_id": "singleton", "theme": "aurora" }).await.unwrap();
        let got = coll
            .find_one(doc! { "_id": "singleton" })
            .with_options(FindOneOptions::builder().projection(doc! { "_id": 0 }).build())
            .await
            .unwrap()
            .unwrap();
        assert!(got.get("_id").is_none());
        assert_eq!(got.get_str("theme").unwrap(), "aurora");
    }

    /// Project content must land in the project's own folder (its git repo), while global
    /// collections stay in the app folder — and reads must not be able to tell the difference.
    #[tokio::test]
    async fn project_scoped_documents_live_in_the_project_folder() {
        let base = std::env::temp_dir().join(format!("lightkid-store-shard-{}", uuid::Uuid::new_v4()));
        let project_folder = base.join("MyProject");
        std::fs::create_dir_all(&project_folder).unwrap();
        let db = Db::open(base.join("global")).unwrap();

        db.collection::<Document>("projects")
            .insert_one(doc! {
                "id": "p1", "name": "MyProject",
                "project_folder": project_folder.to_string_lossy().to_string(),
            })
            .await
            .unwrap();

        db.collection::<Document>("songs")
            .insert_one(doc! { "id": "song1", "project_id": "p1", "title": "Psalm 23" })
            .await
            .unwrap();
        // A section only knows its song — the store resolves the owning project through it.
        db.collection::<Document>("sections")
            .insert_one(doc! { "id": "sec1", "song_id": "song1", "index": 0, "line": "The Lord" })
            .await
            .unwrap();

        assert!(project_folder.join("data").join("songs.json").exists());
        assert!(project_folder.join("data").join("sections.json").exists());
        assert!(!db.global_root().join("songs.json").exists());

        // Reads are transparent across shards.
        let song = db.collection::<Document>("songs").find_one(doc! { "id": "song1" }).await.unwrap();
        assert_eq!(song.unwrap().get_str("title").unwrap(), "Psalm 23");
        let sec = db.collection::<Document>("sections").find_one(doc! { "song_id": "song1" }).await.unwrap();
        assert!(sec.is_some());

        // …and so are writes and deletes.
        db.collection::<Document>("songs")
            .update_one(doc! { "id": "song1" }, doc! { "$set": { "status": "music_ready" } })
            .await
            .unwrap();
        let song = db.collection::<Document>("songs").find_one(doc! { "id": "song1" }).await.unwrap().unwrap();
        assert_eq!(song.get_str("status").unwrap(), "music_ready");

        let dirty = db.take_dirty_projects().await;
        assert!(dirty.contains(&"p1".to_string()), "the project should be flagged for git commit");

        db.collection::<Document>("sections").delete_many(doc! { "song_id": "song1" }).await.unwrap();
        assert_eq!(db.collection::<Document>("sections").count_documents(doc! {}).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn hand_written_bare_array_file_is_readable() {
        let dir = std::env::temp_dir().join(format!("lightkid-store-bare-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("style_presets.json"),
            r#"[{ "id": "photoreal", "name": "Photoreal" }]"#,
        )
        .unwrap();
        let db = Db::open(dir).unwrap();
        let got = db
            .collection::<Document>("style_presets")
            .find_one(doc! { "id": "photoreal" })
            .await
            .unwrap();
        assert_eq!(got.unwrap().get_str("name").unwrap(), "Photoreal");
    }

    #[test]
    fn ext_json_envelopes_are_flattened() {
        let mut v = json!({ "_id": { "$oid": "64b7f9e2c1a2b3d4e5f60718" }, "n": { "$numberLong": "42" } });
        sanitize(&mut v);
        assert_eq!(v["_id"], json!("64b7f9e2c1a2b3d4e5f60718"));
        assert_eq!(v["n"], json!(42));
    }

    // ── The sealed cache ────────────────────────────────────────────────────
    //
    // These share one test because the key is process-global: running them separately would let
    // them race each other over it. Nothing else in this file seals, because every other test
    // writes to a global root and only project shards are sealable.

    #[test]
    fn a_project_shard_seals_to_one_account_and_opens_for_nobody_else() {
        let mine = derive_cache_key("user-1:salt-aaa").expect("derives");
        let theirs = derive_cache_key("user-2:salt-bbb").expect("derives");
        assert_ne!(mine, theirs, "two accounts must not share a key");
        assert_eq!(
            mine,
            derive_cache_key("user-1:salt-aaa").unwrap(),
            "the same account on another machine must derive the same key, or a synced project \
             would be unopenable there"
        );

        let docs = vec![json!({ "id": "s1", "lyrics": "the hidden manna" })];
        let envelope = seal_documents(&mine, "songs", &docs).expect("seals");
        assert!(is_sealed(&envelope));
        let on_disk = serde_json::to_string(&envelope).unwrap();
        assert!(!on_disk.contains("hidden manna"), "the work must not be readable in the file");
        assert!(on_disk.contains("Sign in"), "a person opening the file deserves an explanation");

        let path = Path::new("/tmp/songs.json");
        // Locked: an error, not an empty list. "No songs" would look like data loss and invite the
        // user to regenerate over work that is still there.
        set_cache_key(None, false);
        let err = open_documents(&envelope, path).unwrap_err().0;
        assert!(err.contains("Sign in"), "{err}");

        set_cache_key(Some("user-1:salt-aaa"), true);
        let opened = open_documents(&envelope, path).expect("opens for its own account");
        assert_eq!(opened, docs);
        assert!(cache_unlocked() && cache_sealing());

        // The whole point: a fresh trial account re-importing the old account's projects.
        set_cache_key(Some("user-2:salt-bbb"), true);
        let err = open_documents(&envelope, path).unwrap_err().0;
        assert!(err.contains("different account"), "{err}");

        // Unlocked but not sealing: still reads what was sealed earlier.
        set_cache_key(Some("user-1:salt-aaa"), false);
        assert!(cache_unlocked() && !cache_sealing());
        assert_eq!(open_documents(&envelope, path).unwrap(), docs);

        set_cache_key(None, false);
    }

    #[tokio::test]
    async fn the_global_folder_is_never_sealed() {
        // `settings` holds the entitlement the key is derived from. Sealing it would mean the app
        // could not read the thing it needs in order to read anything.
        let db = temp_db("global-plain");
        let path = db.global_root().join("settings.json");
        let shard = db.inner.shard(path).await;
        assert!(!shard.lock().await.sealable);

        let project_shard = db.inner.shard(std::env::temp_dir().join("proj").join("data").join("songs.json")).await;
        assert!(project_shard.lock().await.sealable);
    }
}
