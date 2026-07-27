// Opening a link beside the instructions instead of instead of them.
//
// A guide that says "go to the Kaggle token page and copy the JSON" is useless if following it hides
// the guide. On a desktop the app's own browser tab solves this. On a phone there is no tab strip to
// come back to, so the page has to appear *beside* the steps: lower half in portrait, right half in
// landscape.
//
// # The limit, stated rather than hidden
//
// An iframe is the only way to put a page beside something in a webview, and the sites this app
// sends people to are exactly the ones that refuse it. Google, Kaggle, Meta and GitHub all send
// `X-Frame-Options: DENY` or a frame-ancestors CSP. A blank half-screen with no explanation is worse
// than a browser jump, so the refusal is detected and the fallback says what happened and why.
//
// Detection cannot be done by reading the response — the browser blocks the load before any script
// can see it, and a cross-origin frame's contents are unreadable by design. What *is* observable:
// a frame that refuses never fires `load`. So: a timer, and a known list of hosts that will certainly
// refuse, which saves the timer's wait for the common cases.

/** Hosts known to refuse framing. Checked first so the user never waits to be told the obvious. */
const REFUSES_FRAMING = [
  "google.com", "accounts.google.com", "aistudio.google.com", "console.cloud.google.com",
  "youtube.com", "kaggle.com", "github.com", "facebook.com", "instagram.com", "x.com",
  "twitter.com", "linkedin.com", "tiktok.com", "printify.com", "suno.com", "midjourney.com",
];

/** How long to wait for a frame to prove it loaded, in milliseconds. */
export const FRAME_TIMEOUT_MS = 4000;

/** Will this URL certainly refuse to be framed? */
export function knownToRefuse(url) {
  try {
    const host = new URL(url).hostname.replace(/^www\./, "");
    return REFUSES_FRAMING.some((h) => host === h || host.endsWith(`.${h}`));
  } catch {
    return false;
  }
}

/**
 * Which way to split, from the viewport's shape.
 *
 * Portrait puts the page underneath — a phone held upright has height to give and no width. Landscape
 * puts it to the right, where a wide screen has room and the eye already travels.
 */
export function splitFor({ width, height } = {}) {
  const w = width ?? (typeof window !== "undefined" ? window.innerWidth : 0);
  const h = height ?? (typeof window !== "undefined" ? window.innerHeight : 0);
  if (!w || !h) return "none";
  // Below this there is not enough of either dimension to split at all, and half of a small phone
  // screen is not a usable page.
  if (Math.min(w, h) < 380) return "none";
  return h >= w ? "vertical" : "horizontal";
}

/** The preference a user has set, or "auto". */
const PREF_KEY = "studio:link-target";
export const linkPreference = () => {
  try { return localStorage.getItem(PREF_KEY) || "auto"; } catch { return "auto"; }
};
export const setLinkPreference = (value) => {
  try {
    if (value && value !== "auto") localStorage.setItem(PREF_KEY, value);
    else localStorage.removeItem(PREF_KEY);
  } catch { /* a preference that cannot be saved is not worth an error */ }
};

/**
 * Decide, before anything is rendered, how a link should open.
 *
 * @returns {"split"|"browser"|"tab"} — `tab` is the desktop's in-app browser page.
 */
export function planFor(url, { mobile, preference = linkPreference(), viewport } = {}) {
  if (preference === "browser") return "browser";
  if (preference === "split") return knownToRefuse(url) ? "browser" : "split";
  if (!mobile) return "tab";                       // the desktop already has a browser tab
  if (knownToRefuse(url)) return "browser";        // do not open a blank half-screen to prove a point
  return splitFor(viewport) === "none" ? "browser" : "split";
}

/**
 * Watch a frame and report whether it ever loaded.
 *
 * The callback receives `true` when the page appeared and `false` when it did not — which is the
 * only signal available, since the browser blocks a refused frame before any script can observe it
 * and a cross-origin frame is unreadable by design.
 */
export function watchFrame(frame, onSettled, timeout = FRAME_TIMEOUT_MS) {
  if (!frame) return () => {};
  let settled = false;
  const done = (ok) => {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    frame.removeEventListener?.("load", loaded);
    onSettled(ok);
  };
  const loaded = () => done(true);
  const timer = setTimeout(() => done(false), timeout);
  frame.addEventListener?.("load", loaded);
  return () => { clearTimeout(timer); frame.removeEventListener?.("load", loaded); };
}

/** The sentence to show when a page refused to sit beside the steps. */
export function refusalMessage(url) {
  let host = url;
  try { host = new URL(url).hostname.replace(/^www\./, ""); } catch { /* keep the raw string */ }
  return `${host} does not allow itself to be shown inside another app, so it opened in your ` +
         `browser instead. Come back here when you are done — nothing is lost.`;
}
