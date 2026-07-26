import { api } from "./api";

// ─────────────────────────────────────────────────────────────────────────────
// Autosave: stage on focus shift, commit on view change.
//
// Two halves, because they answer different questions:
//
//   • Leaving a field STAGES it — the value is written into that feature's JSON file inside the
//     project's git repo and `git add`ed. Cheap, reversible, and it means the value is on disk before
//     anything else can go wrong.
//   • Leaving a VIEW commits what is staged, with a message naming the view and the fields. That is the
//     moment a person believes their work is safe, so it is the moment history gets a real entry.
//
// Fields opt in with `data-autosave="feature:field"`. That is deliberate rather than automatic: not
// every input on a page is a persisted project value — some are filters, some are search boxes, some
// are one-shot prompts — and staging those would fill the history with noise and, worse, write things
// into the project that were never meant to be part of it. The attribute is the declaration that a
// value belongs to the project.
//
// A commit with nothing staged is a no-op on the backend, so navigating without editing leaves the log
// untouched.
// ─────────────────────────────────────────────────────────────────────────────

const DEBOUNCE_MS = 400;

let projectId = null;
let currentView = "app";
/** Fields staged since the last commit, in order — they become the commit message. */
const touched = new Set();
const timers = new Map();
let installed = false;
let committing = null;

export function setAutosaveProject(id) {
  projectId = id || null;
}

export function setAutosaveView(view) {
  currentView = view || "app";
}

/** What the field's element is currently worth, as JSON. */
function valueOf(el) {
  if (!el) return null;
  if (el.type === "checkbox") return !!el.checked;
  if (el.type === "radio") return el.checked ? el.value : undefined;
  if (el.type === "number") {
    const n = Number(el.value);
    return Number.isFinite(n) ? n : el.value;
  }
  if (el.isContentEditable) return el.textContent ?? "";
  return el.value ?? "";
}

/**
 * Write one value into the project's JSON and stage it.
 *
 * Debounced per field: a slider or a text input fires constantly, and one `git add` per keystroke would
 * spend the whole time in git rather than in the app.
 */
export function stageField(feature, field, value) {
  if (!projectId || !feature || !field) return;
  const key = `${feature}:${field}`;
  clearTimeout(timers.get(key));
  timers.set(key, setTimeout(async () => {
    timers.delete(key);
    try {
      await api.stageField({ project_id: projectId, feature, field, value });
      touched.add(field);
    } catch (err) {
      // Staging failing is not worth interrupting the user for — the value is still in the app, and the
      // next save reports the real problem. But it must be visible somewhere.
      console.warn(`[autosave] could not stage ${key}:`, err);
    }
  }, DEBOUNCE_MS));
}

/** Flush any debounced stages so a commit cannot race ahead of the last edit. */
async function flush() {
  const pending = [...timers.keys()];
  if (!pending.length) return;
  // Fire each pending timer now rather than waiting out its debounce.
  for (const key of pending) {
    clearTimeout(timers.get(key));
    timers.delete(key);
  }
  // The values are already in the DOM; re-read them from the elements that declared them.
  const els = document.querySelectorAll("[data-autosave]");
  const jobs = [];
  for (const el of els) {
    const [feature, field] = String(el.dataset.autosave || "").split(":");
    if (!feature || !field || !pending.includes(`${feature}:${field}`)) continue;
    const value = valueOf(el);
    if (value === undefined) continue;
    jobs.push(api.stageField({ project_id: projectId, feature, field, value })
      .then(() => touched.add(field)).catch(() => {}));
  }
  await Promise.all(jobs);
}

/**
 * Commit whatever is staged.
 *
 * `reason` is "autosave" (view change), "manual" (save button) or "switch" (changing project) — it only
 * changes the message, but the message is the whole value of doing this per view rather than on a timer.
 */
export async function commitPending(reason = "autosave", view = currentView) {
  if (!projectId) return { committed: false };
  if (committing) return committing;            // one commit at a time; git is not reentrant
  const fields = [...touched];
  committing = (async () => {
    try {
      await flush();
      const res = await api.autosaveCommit({
        project_id: projectId,
        view,
        fields: [...new Set([...fields, ...touched])],
        reason,
      });
      if (res?.committed) touched.clear();
      return res;
    } catch (err) {
      console.warn("[autosave] commit failed:", err);
      return { committed: false, error: String(err) };
    } finally {
      committing = null;
    }
  })();
  return committing;
}

export function pendingFields() {
  return [...touched];
}

/**
 * Listen for values leaving their fields.
 *
 * `focusout` rather than `blur` because blur does not bubble, and capture phase so a component that
 * stops propagation on its own handlers cannot silently disable saving. `change` covers checkboxes,
 * radios and selects, which are committed by clicking rather than by leaving.
 */
export function installAutosave() {
  if (installed) return;
  installed = true;

  const handle = (event) => {
    const el = event.target?.closest?.("[data-autosave]") || event.target;
    const decl = el?.dataset?.autosave;
    if (!decl) return;
    const [feature, field] = String(decl).split(":");
    if (!feature || !field) return;
    const value = valueOf(el);
    if (value === undefined) return;
    stageField(feature, field, value);
  };

  document.addEventListener("focusout", handle, true);
  document.addEventListener("change", handle, true);

  // Closing the window is also a focus shift, and the last edit before a quit is the one people are
  // most annoyed to lose. `visibilitychange` fires where `beforeunload` is unreliable.
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") commitPending("autosave");
  });
}
