// Errors that reach the server instead of dying in a console nobody opens.
//
// A crash nobody reports is a crash nobody fixes, and asking somebody who has just lost their work
// to please fill in a form is asking at the worst possible moment. So an *error* is sent on its own;
// anything with the person's own words in it is only ever sent because they pressed send. The terms
// say exactly this rather than burying it, and `track_events` in commands/subscription.rs applies
// the matching rule to analytics.
//
// Four things this has to get right, and each of them is a way to make the feature worse than
// nothing:
//
//   1. **Never ship a secret.** Error text is full of URLs and headers, and this app holds API keys
//      for a dozen providers. `redact()` runs on everything before it leaves.
//   2. **Never storm.** A render loop throwing every frame would send thousands of reports and
//      bury the one that matters. Identical errors are counted locally and sent once a session.
//   3. **Never interrupt twice for the same thing.** The notification appears once per fingerprint.
//   4. **Never block the app.** Every path here swallows its own failures: a reporting system that
//      throws is a second bug on top of the first.

const seen = new Map();          // fingerprint → { count, notified, sentAt }
const MAX_PER_SESSION = 25;      // a whole session's budget, not per error
let sentThisSession = 0;
let notifier = null;             // set by installErrorReporting
let sender = null;

/// Patterns that must never leave the machine. Deliberately broad: a false positive costs a few
/// characters of context, a false negative costs somebody their API key.
const SECRETS = [
  /\b(sk|pk|rk)-[A-Za-z0-9_-]{8,}/g,             // OpenAI / OpenRouter / Recraft style
  /\bBearer\s+[A-Za-z0-9._~+/-]{10,}=*/gi,
  /\bxi-api-key["':\s]+[A-Za-z0-9_-]{8,}/gi,
  /\bAIza[0-9A-Za-z_-]{10,}/g,                   // Google
  /\bghp_[A-Za-z0-9]{10,}/g,                     // GitHub
  /\bhf_[A-Za-z0-9]{10,}/g,                      // Hugging Face
  /\b[A-Za-z0-9_-]{12,}\.[A-Za-z0-9_-]{12,}\.[A-Za-z0-9_-]{12,}\b/g, // JWT-ish
  /([?&](?:key|token|api_key|access_token|password|secret)=)[^&\s"']+/gi,
  /("(?:[a-z_]*(?:key|token|secret|password|cookie))"\s*:\s*")[^"]+/gi,
];

/** Strip anything that looks like a credential, and shorten a home directory to `~`. */
export function redact(text) {
  if (!text) return "";
  let out = String(text);
  for (const pattern of SECRETS) {
    out = out.replace(pattern, (match, prefix) => (prefix ? `${prefix}<redacted>` : "<redacted>"));
  }
  // A path carries the user's real name often enough to be worth removing on its own. Stopping at
  // the next slash rather than at whitespace matters: "/Users/Anna Smith/…" is an ordinary macOS
  // home directory, and a pattern that stops at the space leaves the surname behind.
  out = out.replace(/\/home\/[^/\n"']+/g, "~").replace(/\/Users\/[^/\n"']+/g, "~");
  return out.slice(0, 6000);
}

/**
 * A stable identity for "the same error again".
 *
 * Line numbers and object ids change between builds and between runs, so they are stripped: the
 * point is that twenty copies of one crash are counted once rather than filed twenty times, and the
 * server dedupes on this same string.
 */
export function fingerprint(message, stack = "") {
  return redact(`${message} @ ${topFrame(stack)}`)
    .replace(/\d+/g, "#")                 // line numbers, ports, ids
    .replace(/0x[0-9a-f]+/gi, "#")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 200);
}

/**
 * The first stack frame that belongs to this app.
 *
 * Only one frame, and only its file rather than its position. The same error thrown from a
 * synchronous path and from an awaited one carries *different* framework frames underneath it —
 * which is enough to make a loop look like a hundred distinct bugs if the whole stack is hashed.
 */
function topFrame(stack) {
  const lines = String(stack || "").split("\n");
  const own = lines.find((l) =>
    l.includes("at ") && !l.includes("node:internal") && !l.includes("node_modules"));
  if (!own) return "";
  // Keep the file name; drop the line, the column and the wrapping punctuation.
  const file = own.match(/([\w.-]+\.(?:js|jsx|mjs|ts|tsx|rs)):\d+/);
  return file ? file[1] : own.replace(/[:(]\d+.*$/, "").trim();
}

/** Should this error be sent, or has it already been said? */
export function shouldSend(fp, now = Date.now()) {
  if (sentThisSession >= MAX_PER_SESSION) return false;
  const record = seen.get(fp);
  if (!record) return true;
  // The same error an hour later is worth knowing about; the same error in the next frame is not.
  return now - record.sentAt > 3600_000;
}

/** Has the user already been interrupted about this one? */
export function shouldNotify(fp) {
  return !seen.get(fp)?.notified;
}

function remember(fp, { sent, notified }, now = Date.now()) {
  const record = seen.get(fp) || { count: 0, notified: false, sentAt: 0 };
  record.count += 1;
  if (sent) { record.sentAt = now; sentThisSession += 1; }
  if (notified) record.notified = true;
  seen.set(fp, record);
  return record;
}

/**
 * Report one error. Safe to call from anywhere, including from inside another error handler.
 *
 * @param {Error|string} error
 * @param {object} opts `{ where, silent }` — `silent` skips the notification but still sends.
 */
export async function reportError(error, opts = {}) {
  try {
    const message = redact(error?.message || String(error || "Unknown error"));
    const stack = redact(error?.stack || "");
    const fp = fingerprint(message, stack);
    const first = !seen.has(fp);
    const send = shouldSend(fp);
    const notify = !opts.silent && shouldNotify(fp);
    const record = remember(fp, { sent: send, notified: notify });

    if (send && sender) {
      await sender({
        kind: "error",
        title: opts.where ? `${opts.where}: ${message.slice(0, 120)}` : message.slice(0, 160),
        message,
        stack,
        fingerprint: fp,
      }).catch(() => {});
    }
    if (notify && notifier) notifier({ message, fingerprint: fp, count: record.count, first });
    return fp;
  } catch {
    // A reporting system that throws is a second bug on top of the first.
    return "";
  }
}

/**
 * Install the global handlers.
 *
 * @param {object} deps `{ send, notify }` — injected so this module can be tested without Tauri and
 *   without a toast library.
 */
export function installErrorReporting({ send, notify } = {}) {
  sender = send || null;
  notifier = notify || null;
  if (typeof window === "undefined") return () => {};

  const onError = (event) => {
    reportError(event?.error || event?.message || "Unknown error", { where: "window" });
  };
  const onRejection = (event) => {
    // An unhandled promise rejection is the shape most of this app's failures actually take: an
    // await on a Tauri command with no catch around it.
    reportError(event?.reason || "Unhandled promise rejection", { where: "promise" });
  };
  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onRejection);
  return () => {
    window.removeEventListener("error", onError);
    window.removeEventListener("unhandledrejection", onRejection);
  };
}

/** What has gone wrong this session, for the feedback view. */
export function sessionErrors() {
  return [...seen.entries()].map(([fingerprint, r]) => ({ fingerprint, ...r }));
}

/** Test seam: forget everything. */
export function resetErrorReporting() {
  seen.clear();
  sentThisSession = 0;
}
