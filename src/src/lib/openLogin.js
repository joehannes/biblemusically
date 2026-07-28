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
export function openLoginUrl(url, { navigate, label, recommended, force } = {}) {
  const pref = force || loginPreference();
  const mustLeave = needsExternalBrowser(url);
  const target = pref || (mustLeave ? "external" : recommended || "internal");

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
