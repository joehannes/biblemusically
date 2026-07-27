// Counting which screens people reach, and where they stop.
//
// This is a one-person project, so the difference between "step four loses everybody" and "nobody
// mentioned it" is whether anything is counted at all. What it records is deliberately thin: the
// name of a view and the name of a step. Never lyrics, never a song, never a file path, never a key.
//
// Two rules, both enforced in `track_events` on the backend rather than here, because a rule the
// interface owns is a rule the interface can be talked out of:
//
//   * During the trial, counting is part of the deal and the terms say so.
//   * Once somebody is paying, they are only counted if they said yes. Being studied is a favour,
//     not something to extract from a customer.
//
// Batched because the free Cloudflare tier allows a thousand KV writes a day, and one write per
// click would spend that on a handful of users.

const queue = [];
let timer = null;
let sender = null;
let session = "";

const FLUSH_AFTER_MS = 20_000;
const FLUSH_AT = 20;
/** A whole session's ceiling. A loop that fires an event per frame must not spend the day's writes. */
const MAX_PER_SESSION = 400;
let countedThisSession = 0;

/** A random id for this run, so a funnel can be followed without identifying anybody. */
function newSession() {
  try {
    return crypto.randomUUID();
  } catch {
    return `s-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
  }
}

/**
 * Record one event.
 *
 * `name` should be a stable, lowercase, dotted string — `images.generate`, `onboarding.step.4` —
 * because these become counter keys on the server and a name that varies per user is a name that
 * counts nothing.
 */
export function track(name, n = 1) {
  if (!name || countedThisSession >= MAX_PER_SESSION) return;
  countedThisSession += 1;
  const existing = queue.find((e) => e.name === name);
  if (existing) existing.n += n;
  else queue.push({ name: String(name).slice(0, 80), n });

  if (queue.length >= FLUSH_AT) { flush(); return; }
  if (!timer && typeof setTimeout === "function") {
    timer = setTimeout(flush, FLUSH_AFTER_MS);
  }
}

/** Send whatever is queued. Safe to call at any time, including when nothing is queued. */
export async function flush() {
  if (timer) { clearTimeout(timer); timer = null; }
  if (!queue.length || !sender) return;
  const events = queue.splice(0, queue.length);
  try {
    await sender({ events, session });
  } catch {
    // Analytics must never surface as an error. If the send fails the counts are simply lost —
    // they are not worth a retry that could turn one outage into a queue that grows all day.
  }
}

/**
 * Start counting.
 *
 * @param {function} send the backend call, injected so this is testable without Tauri.
 */
export function installAnalytics(send) {
  sender = send || null;
  session = newSession();
  if (typeof window !== "undefined") {
    // A session that ends without flushing is a session that told us nothing, and closing the app is
    // exactly when the last (most interesting) step happens.
    window.addEventListener("beforeunload", () => { flush(); });
    document?.addEventListener?.("visibilitychange", () => {
      if (document.visibilityState === "hidden") flush();
    });
  }
  return () => { sender = null; };
}

/** Test seam. */
export function resetAnalytics() {
  queue.length = 0;
  countedThisSession = 0;
  if (timer) { clearTimeout(timer); timer = null; }
}

/** What is waiting to be sent — for tests and for the "what is collected" panel. */
export function pending() {
  return queue.map((e) => ({ ...e }));
}

/**
 * Load Hotjar — only when the user has said yes, and only if a site id is configured.
 *
 * Kept apart from everything above because it is a different kind of thing: the counters are
 * first-party and go to this app's own server, while this hands a third party a live view of the
 * interface. That is why it is a question in the welcome guide rather than a default, why it is off
 * forever until switched on, and why it can be switched off again — `remove()` drops the script and
 * the cookies it set.
 */
export function installHotjar({ optedIn, siteId }) {
  if (typeof document === "undefined") return () => {};
  const existing = document.getElementById("hotjar-script");
  if (!optedIn || !siteId) {
    if (existing) forgetHotjar(existing);
    return () => {};
  }
  if (existing) return () => forgetHotjar(existing);

  const script = document.createElement("script");
  script.id = "hotjar-script";
  script.async = true;
  script.src = `https://static.hotjar.com/c/hotjar-${siteId}.js?sv=6`;
  window._hjSettings = { hjid: Number(siteId), hjsv: 6 };
  window.hj = window.hj || function () { (window.hj.q = window.hj.q || []).push(arguments); };
  document.head.appendChild(script);
  return () => forgetHotjar(script);
}

function forgetHotjar(script) {
  script.remove();
  delete window.hj;
  delete window._hjSettings;
  // Switching it off has to actually switch it off, including what it left behind.
  for (const cookie of String(document.cookie || "").split(";")) {
    const name = cookie.split("=")[0]?.trim();
    if (name?.startsWith("_hj")) {
      document.cookie = `${name}=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/`;
    }
  }
}
