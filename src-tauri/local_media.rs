//! Somewhere to put bytes that arrived as bytes.
//!
//! Every generation engine so far hands back a *URL* — a Kaggle tunnel, a provider's CDN — and the
//! rest of the pipeline (analysis, overlay, video, upload) simply fetches it. ElevenLabs Music does
//! not: it answers the generation request with the MP3 itself. So those bytes need a URL before the
//! six stages downstream can treat that track like any other.
//!
//! Hence a file on disk plus one route on the local server the app already runs. `file://` was the
//! obvious alternative and does not work: every downstream stage fetches with `reqwest`, which has
//! no file scheme, so the track would generate perfectly and then fail at analysis.
//!
//! # The path parameter is the dangerous part, so it is the tested part
//!
//! A localhost server that serves `?path=` is a file-read primitive for anything that can reach it.
//! [`resolve`] is the whole defence and it is deliberately boring: canonicalise, require the result
//! to sit inside the media root, require a known audio extension. `../` cannot escape because the
//! check happens *after* canonicalisation, and a symlink pointing outside cannot escape for the same
//! reason.

use std::path::{Path, PathBuf};

/// Where generated media lives. Under the cache directory rather than the data directory: these are
/// reproducible artefacts, and a user who clears their cache should lose nothing they cannot make
/// again.
pub fn media_root() -> PathBuf {
    crate::paths::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("studio-lightkid")
        .join("media")
}

/// The extensions this route will serve. An allowlist rather than a blocklist, because the failure
/// of a blocklist is silent and this one would be a file-disclosure bug.
// `webm` matters more than it looks: SaveWEBM is what the ComfyUI video graphs write, so without it
// a re-homed hero clip would be stored and then refused by the very route that serves it.
const SERVABLE: &[&str] = &["mp3", "wav", "ogg", "flac", "m4a", "opus", "png", "jpg", "jpeg", "webp", "svg", "mp4", "webm", "gif"];

/// Write bytes into the media root and return where they landed.
pub fn store(kind: &str, ext: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let dir = media_root().join(safe_segment(kind));
    std::fs::create_dir_all(&dir)?;
    let name = format!("{}.{}", uuid::Uuid::new_v4(), safe_segment(ext));
    let path = dir.join(name);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// A URL the rest of the pipeline can fetch, for a file inside the media root.
pub fn url_for(path: &Path) -> String {
    format!("http://127.0.0.1:3337/media?path={}", urlencoding::encode(&path.to_string_lossy()))
}

/// Keep a caller-supplied word from becoming a path.
fn safe_segment(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() { "misc".into() } else { trimmed }
}

/// Turn a `?path=` parameter into a file this server is willing to read — or `None`.
///
/// Split out from the route so the rule can be tested without a running server, because the rule is
/// the entire security boundary.
pub fn resolve(param: &str, root: &Path) -> Option<PathBuf> {
    let candidate = PathBuf::from(param);
    if !candidate.is_absolute() { return None; }
    // Canonicalise *both* sides before comparing: without it, `/…/media/../../etc/passwd` compares
    // as a string that starts with the root and passes.
    let real = std::fs::canonicalize(&candidate).ok()?;
    let real_root = std::fs::canonicalize(root).ok()?;
    if !real.starts_with(&real_root) { return None; }
    if !real.is_file() { return None; }
    let ext = real.extension()?.to_str()?.to_ascii_lowercase();
    if !SERVABLE.contains(&ext.as_str()) { return None; }
    Some(real)
}

/// The MIME type for a file this route will serve.
pub fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lightkid-media-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_files_inside_the_media_root_are_served() {
        let root = sandbox("root");
        let inside = root.join("track.mp3");
        std::fs::write(&inside, b"id3").unwrap();
        assert_eq!(resolve(inside.to_str().unwrap(), &root).unwrap(), std::fs::canonicalize(&inside).unwrap());

        // The attack this exists to stop: a localhost route with a path parameter is a file-read
        // primitive for anything that can reach it.
        let outside = sandbox("outside").join("secrets.mp3");
        std::fs::write(&outside, b"nope").unwrap();
        assert!(resolve(outside.to_str().unwrap(), &root).is_none());

        // Traversal, spelled the way it is actually attempted. The check is after canonicalisation,
        // so a string that merely *starts* with the root does not pass.
        let traversal = format!("{}/../{}", root.display(), outside.file_name().unwrap().to_str().unwrap());
        assert!(resolve(&traversal, &root).is_none());

        // A relative path is refused outright rather than resolved against whatever the process's
        // working directory happens to be.
        assert!(resolve("track.mp3", &root).is_none());
        assert!(resolve("", &root).is_none());
    }

    #[test]
    fn a_symlink_out_of_the_root_does_not_escape_it() {
        #[cfg(unix)]
        {
            let root = sandbox("symlink-root");
            let elsewhere = sandbox("symlink-target").join("private.mp3");
            std::fs::write(&elsewhere, b"private").unwrap();
            let link = root.join("looks-innocent.mp3");
            std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
            // Canonicalising follows the link, so the containment check sees where it really goes.
            assert!(resolve(link.to_str().unwrap(), &root).is_none());
        }
    }

    #[test]
    fn only_media_extensions_are_served() {
        let root = sandbox("ext");
        for (name, servable) in [("a.mp3", true), ("b.png", true), ("c.MP3", true),
                                 ("d.json", false), ("e.rs", false), ("f", false)] {
            let path = root.join(name);
            std::fs::write(&path, b"x").unwrap();
            assert_eq!(resolve(path.to_str().unwrap(), &root).is_some(), servable, "{name}");
        }
        // A directory inside the root is not a file, whatever it is called.
        let dir = root.join("folder.mp3");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(resolve(dir.to_str().unwrap(), &root).is_none());
    }

    #[test]
    fn a_kind_from_a_caller_cannot_become_a_path() {
        assert_eq!(safe_segment("../../etc"), "etc");
        assert_eq!(safe_segment("elevenlabs"), "elevenlabs");
        assert_eq!(safe_segment(""), "misc");
        assert_eq!(safe_segment("///"), "misc");
        assert!(!safe_segment("a/b").contains('/'));
    }

    #[test]
    fn a_stored_file_is_reachable_through_the_url_it_reports() {
        let path = store("test-engine", "mp3", b"audio").expect("stored");
        assert!(path.starts_with(media_root()));
        let url = url_for(&path);
        assert!(url.starts_with("http://127.0.0.1:3337/media?path="));
        // Percent-encoded, so a space or a plus in the path cannot truncate the parameter.
        assert!(!url.contains(' '));
        assert_eq!(resolve(path.to_str().unwrap(), &media_root()).unwrap(),
                   std::fs::canonicalize(&path).unwrap());
        assert_eq!(content_type(&path), "audio/mpeg");
        let _ = std::fs::remove_file(path);
    }
}
