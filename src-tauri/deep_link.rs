//! Coming back from a sign-in that happened outside the app.
//!
//! Google and Kaggle refuse to sign in inside an embedded webview, so the system browser is not a
//! preference — it is the only route (RFC 8252 says so too). That leaves the return journey, and
//! without it the whole flow half-works in the worst way: the browser finishes, shows a page nobody
//! can act on, and the app never learns that consent was granted.
//!
//! The Android manifest routes the callback to the running activity (`launchMode="singleTask"` plus a
//! VIEW intent filter). This module is what happens next: turn that URL into the two values the
//! existing `oauth_callback` command already knows how to use.
//!
//! # Why the parsing is its own tested function
//!
//! An OAuth callback arrives in more shapes than it should. The authorization-code flow puts
//! `code` and `state` in the **query**; the implicit flow puts them in the **fragment**; an error
//! comes back as `error` plus `error_description` and no code at all. A provider that changes which
//! one it uses breaks a sign-in silently, and "silently" is the whole problem here — so every shape
//! is parsed and every shape is a test.

use serde::Serialize;

/// What a returning URL turned out to be.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Callback {
    /// The authorization code, when the provider granted one.
    pub code: String,
    /// The `state` value the app sent out, echoed back. Not decoration: it is what ties the answer
    /// to the request that asked for it, and a callback whose state we never issued is discarded.
    pub state: String,
    /// The provider's own error, in its own words.
    pub error: String,
    /// Everything else, so a provider that adds a parameter does not need this file changed.
    pub extra: std::collections::BTreeMap<String, String>,
    /// The scheme it arrived on, for the log — which tells you *which* configured client answered.
    pub scheme: String,
}

impl Callback {
    /// Did this actually carry an authorization?
    pub fn is_grant(&self) -> bool {
        !self.code.is_empty() && self.error.is_empty()
    }

    /// A sentence for the interface. The provider's own words where there are any, because
    /// "access_denied" means something and a generic failure message would throw it away.
    pub fn message(&self) -> String {
        if self.is_grant() {
            return "Signed in — you can close the browser tab.".into();
        }
        match self.error.as_str() {
            "" => "That sign-in came back without an authorization code. Try again from the app."
                .into(),
            "access_denied" => "The sign-in was declined. Nothing was changed.".into(),
            other => {
                let detail = self.extra.get("error_description")
                    .map(|d| format!(" — {d}"))
                    .unwrap_or_default();
                format!("The provider refused the sign-in ({other}){detail}")
            }
        }
    }
}

/// Parse a returning URL.
///
/// `None` for anything that is not a callback at all, so an unrelated deep link (a share, a file
/// association) does not get mistaken for a half-finished sign-in.
pub fn parse(url: &str) -> Option<Callback> {
    let url = url.trim();
    if url.is_empty() { return None; }
    let scheme = url.split(':').next().unwrap_or("").to_string();
    if scheme.is_empty() || scheme.len() == url.len() { return None; }

    // Both halves: the code flow answers in the query, the implicit flow in the fragment, and a
    // provider that moves between them must not break a sign-in silently.
    let after_scheme = &url[scheme.len() + 1..];
    let query = after_scheme.split('?').nth(1).map(|q| q.split('#').next().unwrap_or(q)).unwrap_or("");
    let fragment = after_scheme.split('#').nth(1).unwrap_or("");

    let mut params: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for chunk in [query, fragment] {
        for pair in chunk.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let key = percent_decode(k);
            if key.is_empty() { continue; }
            // First one wins: a query value is the authoritative one when both are present.
            params.entry(key).or_insert_with(|| percent_decode(v));
        }
    }

    let take = |map: &mut std::collections::BTreeMap<String, String>, k: &str| {
        map.remove(k).unwrap_or_default()
    };
    let code = take(&mut params, "code");
    let state = take(&mut params, "state");
    let error = take(&mut params, "error");

    // Not a callback: no code, no error, nothing to act on. A share link lands here and is ignored.
    if code.is_empty() && error.is_empty() { return None; }

    Some(Callback { code, state, error, extra: params, scheme })
}

/// `%20` and `+` back to what they were.
fn percent_decode(raw: &str) -> String {
    let bytes = raw.replace('+', " ");
    let bytes = bytes.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&String::from_utf8_lossy(&bytes[i + 1..i + 3]), 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// The most recent callback, held until something collects it.
///
/// A held value rather than only an event, because the two can race: on Android the intent can
/// arrive before the webview has finished loading and attached its listener, and an event fired at
/// nobody is a sign-in that silently did nothing. The event is the fast path; this is the one that
/// cannot be missed.
static PENDING: once_cell::sync::Lazy<std::sync::Mutex<Option<Callback>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

pub fn remember(cb: Callback) {
    if let Ok(mut slot) = PENDING.lock() {
        *slot = Some(cb);
    }
}

/// Take the pending callback, if there is one. Taking clears it: a sign-in is consumed once.
pub fn take() -> Option<Callback> {
    PENDING.lock().ok().and_then(|mut slot| slot.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_code_flow_answers_in_the_query() {
        let cb = parse("com.googleusercontent.apps.123-abc:/oauth2redirect?code=4/0AX&state=chan-7")
            .expect("a callback");
        assert_eq!(cb.code, "4/0AX");
        assert_eq!(cb.state, "chan-7");
        assert!(cb.is_grant());
        assert!(cb.message().contains("Signed in"));
        assert_eq!(cb.scheme, "com.googleusercontent.apps.123-abc");
    }

    #[test]
    fn the_implicit_flow_answers_in_the_fragment() {
        // A provider that moves between the two must not break a sign-in silently, which is exactly
        // what parsing only the query would do.
        let cb = parse("studio.lightkid.app://cb#code=abc123&state=chan-2").expect("a callback");
        assert_eq!(cb.code, "abc123");
        assert_eq!(cb.state, "chan-2");
        assert!(cb.is_grant());
    }

    #[test]
    fn a_refusal_keeps_the_providers_own_words() {
        // "access_denied" means the user said no; a generic failure message would throw that away
        // and send somebody to check their network.
        let denied = parse("studio.lightkid.app://cb?error=access_denied&state=x").unwrap();
        assert!(!denied.is_grant());
        assert!(denied.message().contains("declined"), "{}", denied.message());

        let other = parse(
            "studio.lightkid.app://cb?error=invalid_client&error_description=Unauthorized%20client")
            .unwrap();
        assert!(other.message().contains("invalid_client"), "{}", other.message());
        assert!(other.message().contains("Unauthorized client"), "{}", other.message());
    }

    #[test]
    fn percent_and_plus_encoding_survive() {
        // Authorization codes contain slashes; descriptions contain spaces. Both arrive encoded.
        let cb = parse("x://cb?code=4%2F0AX%2Bnope&state=a+b&error_description=one+two").unwrap();
        assert_eq!(cb.code, "4/0AX+nope");
        assert_eq!(cb.state, "a b");
        assert_eq!(cb.extra.get("error_description").unwrap(), "one two");
    }

    #[test]
    fn a_link_that_is_not_a_sign_in_is_left_alone() {
        // A share link, a file association, an empty intent. Treating any of these as a
        // half-finished sign-in would be worse than ignoring them.
        assert!(parse("studio.lightkid.app://open?project=abc").is_none());
        assert!(parse("https://example.com/").is_none());
        assert!(parse("").is_none());
        assert!(parse("not-a-url").is_none());
        assert!(parse("   ").is_none());
    }

    #[test]
    fn unknown_parameters_are_kept_rather_than_dropped() {
        // A provider adding a parameter should not need this file changed, and losing it silently is
        // how a future flow breaks with no clue why.
        let cb = parse("x://cb?code=a&state=b&scope=email%20profile&authuser=0").unwrap();
        assert_eq!(cb.extra.get("scope").unwrap(), "email profile");
        assert_eq!(cb.extra.get("authuser").unwrap(), "0");
    }

    #[test]
    fn a_callback_waits_to_be_collected_and_is_only_collected_once() {
        // The intent can land before the webview has attached its listener, so an event alone would
        // lose the sign-in. Taking it clears it: a code is consumed exactly once.
        let _ = take();
        assert!(take().is_none());
        remember(parse("x://cb?code=abc&state=s").unwrap());
        assert_eq!(take().unwrap().code, "abc");
        assert!(take().is_none(), "a second collector must not get the same code");
    }

    #[test]
    fn a_query_value_wins_over_a_fragment_of_the_same_name() {
        let cb = parse("x://cb?code=from-query#code=from-fragment&state=s").unwrap();
        assert_eq!(cb.code, "from-query");
        assert_eq!(cb.state, "s", "the fragment still contributes what the query lacks");
    }
}
