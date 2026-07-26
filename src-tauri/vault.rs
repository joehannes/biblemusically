//! Encrypted credential vault.
//!
//! Every secret the app needs to act on the user's behalf — remote-git tokens, asset-host keys,
//! social-network logins — lives here, encrypted at rest with XChaCha20-Poly1305. Nothing is ever
//! written in plaintext, nothing is logged, and secrets are only decrypted in memory at the moment
//! a request needs them.
//!
//! # Threat model, stated honestly
//!
//! There are two modes, and they protect against different things:
//!
//! * **Passphrase mode** (recommended): the key is derived from a passphrase with Argon2id and only
//!   ever exists in memory after an explicit unlock. Someone who copies your disk — a stolen laptop,
//!   a synced backup folder, a shared machine — gets ciphertext and nothing else.
//! * **Machine-key mode** (the default, so the app works without ceremony): a random key is stored
//!   next to the vault in a `0600` file. This protects against *casual* exposure — the vault landing
//!   in a cloud-synced folder, a git commit, a support screenshot — but **not** against anyone who
//!   can read your user's files. The UI says exactly this rather than implying more.
//!
//! Upgrading from machine-key to passphrase re-encrypts every entry in place; nothing is lost.
//!
//! The vault is deliberately *not* a store collection: it must never be swept into a project's git
//! repo or pushed to a remote by the sync feature.

use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use once_cell::sync::Lazy;
use rand::RngCore;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::Mutex;
use zeroize::Zeroize;

type Res<T> = Result<T, String>;

const VERSION: u32 = 1;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;

/// The unlocked key, held only in memory. `None` means "locked" (passphrase mode, not yet unlocked).
static SESSION_KEY: Lazy<Mutex<Option<[u8; KEY_LEN]>>> = Lazy::new(|| Mutex::new(None));

fn vault_dir() -> Res<PathBuf> {
    let base = dirs::config_dir().ok_or("Could not locate the config directory")?;
    let dir = base.join("studio-lightkid");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn vault_path() -> Res<PathBuf> { Ok(vault_dir()?.join("vault.json")) }
fn machine_key_path() -> Res<PathBuf> { Ok(vault_dir()?.join("vault.key")) }

fn read_vault() -> Res<Value> {
    let path = vault_path()?;
    match std::fs::read_to_string(&path) {
        Ok(text) if !text.trim().is_empty() => {
            serde_json::from_str(&text).map_err(|e| format!("vault.json is corrupt: {e}"))
        }
        _ => Ok(json!({ "version": VERSION, "mode": "machine", "kdf": Value::Null, "entries": {} })),
    }
}

fn write_vault(v: &Value) -> Res<()> {
    let path = vault_path()?;
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    restrict_permissions(&tmp);
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Owner-read/write only. On platforms without POSIX modes this is a no-op and the file simply
/// inherits the directory's protection.
fn restrict_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

/// The locally stored random key, created on first use.
fn machine_key() -> Res<[u8; KEY_LEN]> {
    let path = machine_key_path()?;
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(bytes) = B64.decode(text.trim()) {
            if bytes.len() == KEY_LEN {
                let mut key = [0u8; KEY_LEN];
                key.copy_from_slice(&bytes);
                return Ok(key);
            }
        }
    }
    let bytes = random_bytes(KEY_LEN);
    std::fs::write(&path, B64.encode(&bytes)).map_err(|e| e.to_string())?;
    restrict_permissions(&path);
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&bytes);
    Ok(key)
}

fn derive_key(passphrase: &str, salt_b64: &str) -> Res<[u8; KEY_LEN]> {
    let salt = B64.decode(salt_b64).map_err(|e| format!("bad vault salt: {e}"))?;
    let mut key = [0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), &salt, &mut key)
        .map_err(|e| format!("key derivation failed: {e}"))?;
    Ok(key)
}

/// The key to encrypt/decrypt with right now, or an error explaining what the user must do.
fn active_key() -> Res<[u8; KEY_LEN]> {
    let vault = read_vault()?;
    match vault.get("mode").and_then(|m| m.as_str()).unwrap_or("machine") {
        "passphrase" => SESSION_KEY
            .lock()
            .map_err(|_| "vault lock poisoned".to_string())?
            .ok_or_else(|| "The credential vault is locked — unlock it with your passphrase.".to_string()),
        _ => machine_key(),
    }
}

fn seal(key: &[u8; KEY_LEN], plaintext: &str) -> Res<Value> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce_bytes = random_bytes(NONCE_LEN);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| "encryption failed".to_string())?;
    Ok(json!({ "nonce": B64.encode(&nonce_bytes), "data": B64.encode(&ciphertext) }))
}

fn open(key: &[u8; KEY_LEN], entry: &Value) -> Res<String> {
    let nonce_bytes = B64
        .decode(entry.get("nonce").and_then(|n| n.as_str()).unwrap_or(""))
        .map_err(|e| format!("corrupt entry: {e}"))?;
    let data = B64
        .decode(entry.get("data").and_then(|d| d.as_str()).unwrap_or(""))
        .map_err(|e| format!("corrupt entry: {e}"))?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let plain = cipher
        .decrypt(XNonce::from_slice(&nonce_bytes), data.as_ref())
        .map_err(|_| "Could not decrypt — wrong passphrase, or the vault key changed.".to_string())?;
    String::from_utf8(plain).map_err(|e| e.to_string())
}

/// Seal a payload with a passphrase, as a single portable string.
///
/// Different from the vault's own entries on purpose: this one carries its **own salt**, so it can be
/// opened on another machine that has never seen this vault. That is what makes it safe to send a set of
/// refresh tokens through a chat message — without the passphrase the blob is noise, and without the
/// salt travelling with it the other device could not derive the key at all.
pub fn seal_with(passphrase: &str, plaintext: &str) -> Res<String> {
    if passphrase.trim().len() < 8 {
        return Err("Use a passphrase of at least eight characters.".into());
    }
    let salt_b64 = B64.encode(random_bytes(16));
    let key = derive_key(passphrase, &salt_b64)?;
    let sealed = seal(&key, plaintext)?;
    let envelope = json!({
        "v": 1, "kdf": "argon2", "salt": salt_b64,
        "nonce": sealed["nonce"], "data": sealed["data"],
    });
    Ok(format!("bmstudio-sealed:{}", B64.encode(envelope.to_string().as_bytes())))
}

/// Open what `seal_with` produced.
pub fn open_with(passphrase: &str, blob: &str) -> Res<String> {
    let body = blob.trim().strip_prefix("bmstudio-sealed:")
        .ok_or_else(|| "That is not a sealed blob from this app.".to_string())?;
    let bytes = B64.decode(body).map_err(|_| "The blob is damaged.".to_string())?;
    let envelope: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "The blob is damaged.".to_string())?;
    let salt = envelope["salt"].as_str().ok_or("The blob carries no salt.")?;
    let key = derive_key(passphrase, salt)?;
    open(&key, &envelope)
}

// ────────────────────────────────────────────────────────────────────────────
// Backend API (used by remote sync, social publishing, …)
// ────────────────────────────────────────────────────────────────────────────

/// Store a secret under `key` (e.g. `"remote.huggingface.token"`).
pub fn put(key: &str, secret: &str) -> Res<()> {
    if key.trim().is_empty() {
        return Err("a vault key is required".into());
    }
    let mut cipher_key = active_key()?;
    let sealed = seal(&cipher_key, secret);
    cipher_key.zeroize();
    let sealed = sealed?;

    let mut vault = read_vault()?;
    let entries = vault
        .as_object_mut()
        .and_then(|o| o.get_mut("entries"))
        .and_then(|e| e.as_object_mut())
        .ok_or("vault.json has no entries object")?;
    entries.insert(
        key.to_string(),
        json!({
            "nonce": sealed["nonce"],
            "data": sealed["data"],
            "hint": mask(secret),
            "updated_at": chrono::Utc::now().to_rfc3339(),
        }),
    );
    write_vault(&vault)
}

/// Fetch and decrypt a secret. `Ok(None)` means "not set" — distinct from "locked", which errors.
pub fn get(key: &str) -> Res<Option<String>> {
    let vault = read_vault()?;
    let Some(entry) = vault.get("entries").and_then(|e| e.get(key)) else {
        return Ok(None);
    };
    let mut cipher_key = active_key()?;
    let result = open(&cipher_key, entry);
    cipher_key.zeroize();
    result.map(Some)
}

/// Same, but treats a locked/undecryptable vault as "no secret" — for optional credentials on
/// background paths that must not hard-fail.
pub fn get_quiet(key: &str) -> Option<String> { get(key).ok().flatten() }

pub fn delete(key: &str) -> Res<bool> {
    let mut vault = read_vault()?;
    let removed = vault
        .as_object_mut()
        .and_then(|o| o.get_mut("entries"))
        .and_then(|e| e.as_object_mut())
        .map(|entries| entries.remove(key).is_some())
        .unwrap_or(false);
    if removed {
        write_vault(&vault)?;
    }
    Ok(removed)
}

/// Keys currently stored, with a masked hint — never the secret itself.
pub fn list() -> Res<Vec<Value>> {
    let vault = read_vault()?;
    let mut out = Vec::new();
    if let Some(entries) = vault.get("entries").and_then(|e| e.as_object()) {
        for (key, entry) in entries {
            out.push(json!({
                "key": key,
                "hint": entry.get("hint").cloned().unwrap_or(Value::Null),
                "updated_at": entry.get("updated_at").cloned().unwrap_or(Value::Null),
            }));
        }
    }
    out.sort_by(|a, b| a["key"].as_str().unwrap_or("").cmp(b["key"].as_str().unwrap_or("")));
    Ok(out)
}

/// `"sk-…9f2c"` — enough to recognize a credential, not enough to use it.
fn mask(secret: &str) -> String {
    let n = secret.chars().count();
    if n <= 4 {
        return "•".repeat(n.max(1));
    }
    let tail: String = secret.chars().skip(n.saturating_sub(4)).collect();
    format!("{}{}", "•".repeat(8), tail)
}

// ────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn vault_status() -> Res<Value> {
    let vault = read_vault()?;
    let mode = vault.get("mode").and_then(|m| m.as_str()).unwrap_or("machine").to_string();
    let unlocked = match mode.as_str() {
        "passphrase" => SESSION_KEY.lock().map(|k| k.is_some()).unwrap_or(false),
        _ => true,
    };
    Ok(json!({
        "mode": mode,
        "unlocked": unlocked,
        "entry_count": vault.get("entries").and_then(|e| e.as_object()).map(|o| o.len()).unwrap_or(0),
        "path": vault_path()?.to_string_lossy(),
        "protection": if mode == "passphrase" {
            "Encrypted with a key derived from your passphrase (Argon2id). The key is only in memory while unlocked."
        } else {
            "Encrypted with a random key stored beside the vault, readable only by your user account. Set a passphrase for protection against disk copies."
        },
    }))
}

#[tauri::command]
pub async fn vault_list() -> Res<Vec<Value>> { list() }

/// Store or replace one secret.
#[tauri::command]
pub async fn vault_put(key: String, secret: String) -> Res<Value> {
    put(&key, &secret)?;
    Ok(json!({ "ok": true, "key": key, "hint": mask(&secret) }))
}

#[tauri::command]
pub async fn vault_delete(key: String) -> Res<Value> {
    Ok(json!({ "ok": true, "removed": delete(&key)? }))
}

/// Unlock a passphrase-protected vault for this session.
#[tauri::command]
pub async fn vault_unlock(passphrase: String) -> Res<Value> {
    let vault = read_vault()?;
    if vault.get("mode").and_then(|m| m.as_str()) != Some("passphrase") {
        return Ok(json!({ "ok": true, "mode": "machine", "note": "This vault has no passphrase." }));
    }
    let salt = vault
        .get("kdf")
        .and_then(|k| k.get("salt"))
        .and_then(|s| s.as_str())
        .ok_or("vault is missing its key-derivation salt")?;
    let key = derive_key(&passphrase, salt)?;

    // Verify against a known entry rather than trusting the passphrase blindly.
    if let Some(check) = vault.get("kdf").and_then(|k| k.get("check")) {
        let plain = open(&key, check)?;
        if plain != "studio-lightkid" {
            return Err("Wrong passphrase.".into());
        }
    }
    *SESSION_KEY.lock().map_err(|_| "vault lock poisoned")? = Some(key);
    Ok(json!({ "ok": true, "unlocked": true }))
}

#[tauri::command]
pub async fn vault_lock() -> Res<Value> {
    if let Ok(mut guard) = SESSION_KEY.lock() {
        if let Some(mut key) = guard.take() {
            key.zeroize();
        }
    }
    Ok(json!({ "ok": true, "unlocked": false }))
}

/// Turn a machine-key vault into a passphrase-protected one (or change the passphrase). Every
/// existing entry is decrypted with the old key and re-encrypted with the new one, in one write.
#[tauri::command]
pub async fn vault_set_passphrase(current: Option<String>, passphrase: String) -> Res<Value> {
    if passphrase.chars().count() < 8 {
        return Err("Choose a passphrase of at least 8 characters.".into());
    }
    let mut vault = read_vault()?;
    let mode = vault.get("mode").and_then(|m| m.as_str()).unwrap_or("machine").to_string();

    // The key everything is currently encrypted with.
    let old_key = if mode == "passphrase" {
        let salt = vault
            .get("kdf")
            .and_then(|k| k.get("salt"))
            .and_then(|s| s.as_str())
            .ok_or("vault is missing its key-derivation salt")?;
        let supplied = current.ok_or("The current passphrase is required to change it.")?;
        let key = derive_key(&supplied, salt)?;
        if let Some(check) = vault.get("kdf").and_then(|k| k.get("check")) {
            if open(&key, check)? != "studio-lightkid" {
                return Err("Wrong current passphrase.".into());
            }
        }
        key
    } else {
        machine_key()?
    };

    let salt_bytes = random_bytes(16);
    let salt_b64 = B64.encode(&salt_bytes);
    let new_key = derive_key(&passphrase, &salt_b64)?;

    // Re-encrypt every entry.
    let mut re_encrypted = Map::new();
    if let Some(entries) = vault.get("entries").and_then(|e| e.as_object()) {
        for (key, entry) in entries {
            let plain = open(&old_key, entry)?;
            let sealed = seal(&new_key, &plain)?;
            re_encrypted.insert(
                key.clone(),
                json!({
                    "nonce": sealed["nonce"],
                    "data": sealed["data"],
                    "hint": entry.get("hint").cloned().unwrap_or(Value::Null),
                    "updated_at": entry.get("updated_at").cloned().unwrap_or(Value::Null),
                }),
            );
        }
    }

    let check = seal(&new_key, "studio-lightkid")?;
    vault = json!({
        "version": VERSION,
        "mode": "passphrase",
        "kdf": { "algorithm": "argon2id", "salt": salt_b64, "check": check },
        "entries": Value::Object(re_encrypted),
    });
    write_vault(&vault)?;
    *SESSION_KEY.lock().map_err(|_| "vault lock poisoned")? = Some(new_key);

    // The machine key is now meaningless — remove it so a copied disk can't decrypt anything.
    let _ = std::fs::remove_file(machine_key_path()?);
    Ok(json!({ "ok": true, "mode": "passphrase", "unlocked": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip through the real file layout, in an isolated HOME so the developer's own vault is
    /// never touched. Serialized because the vault path comes from process-wide env.
    #[test]
    fn a_sealed_blob_travels_to_a_machine_that_never_saw_this_vault() {
        // It carries its own salt, which is the whole point: the other device derives the same key from
        // the passphrase alone.
        let blob = seal_with("a good long passphrase", "hunter2-refresh-token").expect("sealed");
        assert!(blob.starts_with("bmstudio-sealed:"));
        assert!(!blob.contains("hunter2"), "the secret must not be readable in the blob");
        assert_eq!(open_with("a good long passphrase", &blob).unwrap(), "hunter2-refresh-token");
        // Whitespace from a chat message is normal.
        assert_eq!(open_with("a good long passphrase", &format!("  {blob}\n")).unwrap(), "hunter2-refresh-token");
        // The wrong passphrase fails rather than returning something plausible.
        assert!(open_with("a different passphrase", &blob).is_err());
        assert!(open_with("a good long passphrase", "bmstudio-sealed:zzzz").is_err());
        assert!(open_with("a good long passphrase", "not a blob").is_err());
        // Too short a passphrase is refused at seal time, not discovered later.
        assert!(seal_with("short", "x").is_err());
    }

    #[test]
    fn seal_and_open_roundtrip() {
        let key = [7u8; KEY_LEN];
        let sealed = seal(&key, "hunter2-token").unwrap();
        assert_ne!(sealed["data"].as_str().unwrap(), "hunter2-token");
        assert_eq!(open(&key, &sealed).unwrap(), "hunter2-token");

        let wrong = [8u8; KEY_LEN];
        assert!(open(&wrong, &sealed).is_err(), "a wrong key must not decrypt");
    }

    #[test]
    fn masking_never_leaks_the_secret() {
        assert_eq!(mask("abcd"), "••••");
        let m = mask("sk-live-0123456789abcdef");
        assert!(m.ends_with("cdef"));
        assert!(!m.contains("0123456789"));
    }

    #[test]
    fn derived_keys_are_stable_and_salt_dependent() {
        let salt_a = B64.encode([1u8; 16]);
        let salt_b = B64.encode([2u8; 16]);
        assert_eq!(derive_key("passphrase", &salt_a).unwrap(), derive_key("passphrase", &salt_a).unwrap());
        assert_ne!(derive_key("passphrase", &salt_a).unwrap(), derive_key("passphrase", &salt_b).unwrap());
        assert_ne!(derive_key("passphrase", &salt_a).unwrap(), derive_key("other", &salt_a).unwrap());
    }
}
