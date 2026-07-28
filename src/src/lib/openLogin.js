// Opening a provider's sign-in or API-key page.
//
// Default: the app's own browser tab. Staying inside means the page is right next to the field the
// key gets pasted into, and any session it establishes is the one the app's automation can reuse.
//
// Exception: Google refuses to sign in inside an embedded webview ("this browser or app may not be
// secure"), so anything behind a Google account has to leave the app. That is a property of the
// site, not a preference — hence `login: "external"` on those providers, with the reason shown.
//
// The user can always override per click; the choice is remembered.

import { openUrl } from "@tauri-apps/plugin-opener";

const PREF_KEY = "studio:login-target";

/** "internal" | "external" | "" (follow the provider's own recommendation). */
export const loginPreference = () => {
  try { return localStorage.getItem(PREF_KEY) || ""; } catch { return ""; }
};
export const setLoginPreference = (value) => {
  try {
    if (value) localStorage.setItem(PREF_KEY, value);
    else localStorage.removeItem(PREF_KEY);
  } catch { /* ignore */ }
};

/** Sites known to reject embedded webviews — always advised out to the system browser. */
const EXTERNAL_HOSTS = ["accounts.google.com", "aistudio.google.com", "console.cloud.google.com", "kaggle.com"];

export function needsExternalBrowser(url) {
  try { return EXTERNAL_HOSTS.some((h) => new URL(url).hostname.endsWith(h)); }
  catch { return false; }
}

/**
 * Open a login or key page.
 *
 * `navigate` is react-router's navigate — passing it keeps this file free of router imports and lets
 * a caller outside the router (a dialog, a toast action) fall back to the system browser.
 * Returns "internal" | "external" so the caller can tell the user where it went.
 */
/**
 * A phone has one webview and the app is already using it.
 *
 * The in-app browser is a second webview positioned over the window — machinery that exists only on
 * desktop (see `install_layout`, a no-op everywhere else). On Android, routing a sign-in "internally"
 * therefore navigates *the app's own window* to that page, or does nothing at all. Reported as both:
 * the OpenRouter key page opening inside the app instead of a browser, and the Kaggle and Google
 * links raising an OS error and opening nothing.
 *
 * So mobile always leaves. This is not a preference to be overridden — a remembered "internal" from
 * a desktop session must not follow the user onto a phone and break every sign-in there.
 */
const isMobile = () => {
  try {
    return /Android|iPhone|iPad|iPod/i.test(navigator.userAgent || "");
  } catch { return false; }
};

export function openLoginUrl(url, { navigate, label, recommended, force } = {}) {
  const pref = force || loginPreference();
  const mustLeave = needsExternalBrowser(url) || isMobile();
  const target = pref && !isMobile() ? pref : (mustLeave ? "external" : recommended || "internal");

  if (target === "internal" && !mustLeave && navigate) {
    navigate(`/browser?url=${encodeURIComponent(url)}&label=${encodeURIComponent(label || "Sign in")}`);
    return "internal";
  }
  // The opener plugin, not window.open.
  //
  // `window.open` does nothing at all in an Android WebView — no new window, no error, no rejected
  // promise. Every "create an account" and "get an API key" button on a phone was therefore a button
  // that did nothing, silently, which is the failure mode hardest to report and easiest to blame on
  // yourself. The plugin fires a real ACTION_VIEW intent and reaches the user's actual browser.
  //
  // window.open stays as the fallback for a plain browser build, where it is the correct call and
  // the plugin is not available.
  openUrl(url).catch(() => {
    try { window.open(url, "_blank", "noopener"); } catch { /* nothing left to try */ }
  });
  return "external";
}

/**
 * Open a plain link out to the system browser.
 *
 * For `<a target="_blank">` anchors, which are a silent no-op inside an Android WebView exactly as
 * `window.open` is — the Google Cloud credentials link was reported as "does nothing" for this
 * reason. The anchor stays in the markup so it is still a real link to a screen reader and still
 * middle-clickable on desktop; this just intercepts the ordinary click.
 */
export function openExternalUrl(url) {
  openUrl(url).catch(() => {
    try { window.open(url, "_blank", "noopener"); } catch { /* nothing left to try */ }
  });
}
