// AI macro recorder: storage, step tidying, and playback for interaction macros recorded
// inside the embedded browser webview.
//
// Recording happens in Rust-injected page scripts (see src-tauri/commands/webview.rs): every
// click / context-menu / paste / field edit / Enter is POSTed to the local server as one step,
// carrying a *positional* description of its target — a stable attribute selector when one
// exists, plus "item N of list L" context when the element sits in a repeated-sibling group.
// Playback (here) re-resolves each target by position at run time, so a macro recorded on
// "the first item in the downloads list" replays against whatever is first *now* (i.e. the
// latest asset), not the specific old element. Paste/fill steps are parameterizable: pass
// different clipboard text per run without re-recording.

import {
  sunoSubmitScript, sunoEngineSelectScript, sunoScrapeResultScript, midjourneyScrapeJobsScript,
  youtubeListChannelsScript, engineHealthScript,
} from "./browserSites";

const MACROS_KEY = "studio:webview-macros";
const CLIPBOARD_KEY = "studio:webview-clipboard";
const CLIPBOARD_QUEUE_KEY = "studio:webview-clipboard-queue";
const FIELD_BUCKET_KEY = "studio:webview-field-bucket";

// ── Virtual (pseudo) clipboard ────────────────────────────────────────────────
// Our own in-app clipboard bucket for the embedded webview. The record-time
// "Paste clipboard" button and the `clipboard` connector both read from here, so a macro
// can be replayed with whatever content the app (or the user) put in the bucket — no
// dependence on the OS clipboard, which native webviews can't reliably script.

export function getVirtualClipboard() {
  try { return localStorage.getItem(CLIPBOARD_KEY) || ""; } catch { return ""; }
}

export function setVirtualClipboard(text) {
  try { localStorage.setItem(CLIPBOARD_KEY, String(text ?? "")); } catch { /* ignore */ }
}

// ── Multi-entry clipboard bucket (queue) ──────────────────────────────────────
// A separate FIFO queue of entries, distinct from the single "current" clipboard value above.
// Prefill it with several values (one per macro run, e.g. a batch of titles or tags), then
// bind a step to the `clipboard-queue` connector: every playback pops the next entry off the
// front, so one macro replayed N times (or a loop that plays it N times) inserts a different
// value each run instead of repeating the same paste.

export function getClipboardQueue() {
  try {
    const raw = localStorage.getItem(CLIPBOARD_QUEUE_KEY);
    const list = raw ? JSON.parse(raw) : [];
    return Array.isArray(list) ? list : [];
  } catch { return []; }
}

function saveClipboardQueue(list) {
  try { localStorage.setItem(CLIPBOARD_QUEUE_KEY, JSON.stringify(list)); } catch { /* ignore */ }
  return list;
}

// Prefill: append entries to the back of the queue (accepts one string or an array).
export function pushClipboardEntries(entries) {
  const incoming = (Array.isArray(entries) ? entries : [entries]).map(String).filter((s) => s.length);
  const list = [...getClipboardQueue(), ...incoming];
  return saveClipboardQueue(list);
}

// Pop: remove and return the front entry (null when the bucket is empty, so a run can still
// proceed and fall back to inserting nothing rather than erroring out).
export function popClipboardEntry() {
  const list = getClipboardQueue();
  if (!list.length) return null;
  const [next, ...rest] = list;
  saveClipboardQueue(rest);
  return next;
}

export function clearClipboardQueue() {
  return saveClipboardQueue([]);
}

// ── Named field bucket ─────────────────────────────────────────────────────────
// A small keyed bucket — {songId, title, styles, lyrics} — distinct from the single-slot and
// queue buckets above: those hold one anonymous string each, this holds several *named* fields
// that all belong to the same song at once, so a macro can paste "the title" into one field and
// "the lyrics" into another within the same run. This is what any generation pipeline (see
// genPipeline.js) stages a song into before driving a site's create page, and what any *new*
// engine site — one with no bespoke script yet — can be automated against just by recording a
// macro that pastes from the `song-title` / `song-lyrics` / `song-style` connectors below,
// no code required.

export function setSongFieldBucket({ songId, title, styles, lyrics } = {}) {
  const bucket = { songId: songId || "", title: title || "", styles: styles || "", lyrics: lyrics || "" };
  try { localStorage.setItem(FIELD_BUCKET_KEY, JSON.stringify(bucket)); } catch { /* ignore */ }
  return bucket;
}

export function getSongFieldBucket() {
  try {
    const raw = localStorage.getItem(FIELD_BUCKET_KEY);
    const b = raw ? JSON.parse(raw) : null;
    return b && typeof b === "object" ? b : { songId: "", title: "", styles: "", lyrics: "" };
  } catch { return { songId: "", title: "", styles: "", lyrics: "" }; }
}

export function clearSongFieldBucket() {
  try { localStorage.removeItem(FIELD_BUCKET_KEY); } catch { /* ignore */ }
}

// ── App-wide connector actions ────────────────────────────────────────────────
// Named sources the app plugs into recorded `connector` steps. A connector step remembers
// *where* it was recorded (the focused element, positionally); at playback the bound
// connector resolves *what* to insert there. Bindings are edited in the Macro Manager.

export const CONNECTORS = [
  {
    id: "clipboard",
    label: "App clipboard (pseudo-clipboard bucket)",
    description: "Inserts whatever the in-app clipboard bucket holds at playback time.",
    resolve: () => getVirtualClipboard(),
  },
  {
    id: "clipboard-queue",
    label: "Clipboard bucket — pop next (multi-paste)",
    description: "Pops the next prefilled entry off the clipboard bucket queue each time it plays — a different value per run instead of the same one every time.",
    resolve: () => popClipboardEntry() ?? "",
  },
  {
    id: "datetime",
    label: "Current date & time",
    description: "Inserts the local date/time string at the moment of playback.",
    resolve: () => new Date().toLocaleString(),
  },
  {
    id: "custom-text",
    label: "Fixed text (per step)",
    description: "Inserts a fixed text configured on the step in the Macro Manager.",
    resolve: (binding) => binding?.text || "",
  },
  {
    id: "song-title",
    label: "Song field bucket — title",
    description: "Inserts the title of whichever song a generation pipeline (or you) last staged into the field bucket.",
    resolve: () => getSongFieldBucket().title,
  },
  {
    id: "song-styles",
    label: "Song field bucket — musical styles",
    description: "Inserts the styles/genre string of the currently staged song.",
    resolve: () => getSongFieldBucket().styles,
  },
  {
    id: "song-lyrics",
    label: "Song field bucket — lyrics",
    description: "Inserts the full lyrics of the currently staged song.",
    resolve: () => getSongFieldBucket().lyrics,
  },
];

export async function resolveConnector(connectorId, binding) {
  const c = CONNECTORS.find((x) => x.id === connectorId) || CONNECTORS[0];
  return String((await c.resolve(binding)) ?? "");
}

// ── Macro "action" steps ───────────────────────────────────────────────────────
// A step type distinct from recorded clicks: instead of replaying a positional DOM
// interaction, it runs one of the app's existing, hand-tuned automation scripts (the same
// ones behind the Browser tab's toolbar buttons) and reports through the normal scrape
// pipeline. This is what the default per-engine macro presets below are built from — a
// *recorded* macro is a snapshot of one specific page's DOM at record time and only replays
// reliably on the same still-live-and-logged-in page, so shipping fabricated recorded clicks
// as "defaults" for sites we've never actually recorded against would silently fail. A script
// action has no such dependency: it's the same selector logic already exercised by the
// toolbar, just invoked from inside a macro.
export const MACRO_ACTIONS = [
  {
    id: "youtube-list-channels",
    label: "YouTube: list channels (channel switcher)",
    script: () => youtubeListChannelsScript(),
    scrapeKey: "youtube:channels",
  },
  {
    id: "suno-submit",
    label: "Suno: click Create/Compose",
    script: () => sunoSubmitScript(),
    scrapeKey: "suno:submit",
  },
  {
    id: "suno-engine-select",
    label: "Suno: pick newest engine version (paid plans only)",
    description: "Opens Suno's model-version picker and selects the newest one. Not run automatically by the generation pipeline — on a free plan the newest version is locked, so this opens a picker with nothing selectable and gets stuck. Only add this to a macro if your Suno plan actually has the latest engine unlocked.",
    script: () => sunoEngineSelectScript(),
    scrapeKey: "suno:engine",
  },
  {
    id: "suno-capture-result",
    label: "Suno: wait for and capture the finished clip",
    description: "Scans the page for a finished clip's audio URL, polling for up to ~3.5 minutes — the same script the Browser tab's own \"Capture result\" button and the AI generation pipeline use. Put this right after \"Suno: click Create/Compose\" in a macro to wait for the render instead of moving on immediately.",
    script: () => sunoScrapeResultScript([]),
    scrapeKey: "suno:result",
    timeoutMs: 220000, // outlives the script's own ~210s internal poll budget
  },
  {
    id: "midjourney-scrape-jobs",
    label: "Midjourney: scrape generated image URLs",
    script: () => midjourneyScrapeJobsScript(),
    scrapeKey: "midjourney:jobs",
  },
  {
    id: "acestep-health",
    label: "ACE-Step: health check",
    script: () => engineHealthScript("acestep"),
    scrapeKey: "acestep:health",
  },
  {
    id: "heartmula-health",
    label: "HeartMuLa: health check",
    script: () => engineHealthScript("heartmula"),
    scrapeKey: "heartmula:health",
  },
  {
    id: "comfyui-health",
    label: "ComfyUI: health check",
    script: () => engineHealthScript("comfyui"),
    scrapeKey: "comfyui:health",
  },
];

export function findMacroAction(actionId) {
  return MACRO_ACTIONS.find((a) => a.id === actionId) || null;
}

// ── Browser "wait" conditions ───────────────────────────────────────────────────
// Lets a macro pause for something to actually happen in the page — a spinner going away, a new
// item landing in a list, an AJAX call settling — instead of a fixed sleep that's either too
// short (races the page) or too long (bulk runs waiting out a worst case every single time).
// Each condition is checked by `waitConditionScript`, a single-shot, no-internal-loop script
// (same reasoning as sunoScrapeOnceScript in browserSites.js: a script's own setTimeout loop
// dies silently if the page it's running in navigates mid-wait, which is exactly the kind of
// thing some of these conditions are waiting *for*) — playMacro re-injects it on every poll
// tick and drives the timeout/retry loop itself.
export const WAIT_CONDITIONS = [
  {
    id: "appear",
    label: "Element appears",
    needsTarget: true,
    hint: "Waits until something matching the target becomes visible — a result card, a success banner, a newly-enabled button.",
  },
  {
    id: "disappear",
    label: "Element disappears (e.g. a loading spinner)",
    needsTarget: true,
    hint: "Waits until nothing matching the target is visible anymore — point it at a spinner, skeleton loader, or \"Generating…\" badge.",
  },
  {
    id: "list-grow",
    label: "New item appears in a list/container",
    needsContainer: true,
    hint: "Waits until the container has more direct children than it did when the wait started — a history list, a results grid, a download queue.",
  },
  {
    id: "text-appears",
    label: "Text appears on the page",
    needsText: true,
    needsContainerOptional: true,
    hint: "Waits until the given text shows up anywhere in the page (or inside a container, if set).",
  },
  {
    id: "text-disappears",
    label: "Text disappears from the page",
    needsText: true,
    needsContainerOptional: true,
    hint: "Waits until the given text is no longer present — e.g. \"Generating…\" or \"Please wait\".",
  },
  {
    id: "ajax-idle",
    label: "Network goes quiet (AJAX finished)",
    needsQuiet: true,
    hint: "Waits until fetch()/XHR activity on the page has been quiet for a bit — a generic stand-in for \"whatever background request that click kicked off is done\".",
  },
];

export function findWaitCondition(id) {
  return WAIT_CONDITIONS.find((c) => c.id === id) || WAIT_CONDITIONS[0];
}

// Single-shot condition check, evaluated fresh on every poll tick from playMacro (see below).
// Reports `{ met, count?, inFlight?, quietFor? }` through window.__bmScrape(key, …); never loops
// or times out internally — the caller owns pacing and the deadline.
function waitConditionScript(step, key) {
  const j = JSON.stringify;
  return `(() => {
    const KEY = ${j(key)};
    const T = ${j(step.target || {})};
    const CONTAINER = ${j(step.containerSelector || "")};
    const TEXT = ${j(step.text || "")};
    const QUIET_MS = ${j(step.quietMs || 1200)};
    const CONDITION = ${j(step.condition || "appear")};

    const visible = (el) => {
      if (!el) return false;
      const r = el.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) return false;
      const st = getComputedStyle(el);
      return st.visibility !== 'hidden' && st.display !== 'none' && st.opacity !== '0';
    };
    const bySelector = (sel) => { try { return sel ? [...document.querySelectorAll(sel)] : []; } catch (e) { return []; } };
    const byText = (tag, text) => [...document.querySelectorAll(tag || '*')].filter((el) => (el.innerText || el.textContent || '').includes(text));
    const targetEls = () => (T.selector ? bySelector(T.selector) : T.byText ? byText(T.byText.tag, T.byText.text) : []);
    const scopeEl = () => (CONTAINER ? document.querySelector(CONTAINER) : document.body);

    let result = { met: false };
    if (CONDITION === 'appear') {
      result = { met: targetEls().some(visible) };
    } else if (CONDITION === 'disappear') {
      result = { met: !targetEls().some(visible) };
    } else if (CONDITION === 'list-grow') {
      const c = scopeEl();
      result = { met: false, count: c ? c.children.length : 0, found: !!c };
    } else if (CONDITION === 'text-appears') {
      const s = scopeEl();
      result = { met: !!s && (s.innerText || s.textContent || '').includes(TEXT) };
    } else if (CONDITION === 'text-disappears') {
      const s = scopeEl();
      result = { met: !s || !(s.innerText || s.textContent || '').includes(TEXT) };
    } else if (CONDITION === 'ajax-idle') {
      const net = window.__bmNet || { inFlight: 0, lastActivity: 0 };
      result = { met: net.inFlight === 0 && (Date.now() - net.lastActivity) >= QUIET_MS, inFlight: net.inFlight, quietFor: Date.now() - net.lastActivity };
    }
    window.__bmScrape(KEY, result);
    return true;
  })();`;
}

// ── Default macro presets ──────────────────────────────────────────────────────
// Built-in starter macros, one per engine, shown in the Macro Manager and added to your own
// saved macros on request (never auto-injected — you choose when your macro list gets one).
// Each is just a nav step + one action step, so it needs nothing recorded against a live page.
export const DEFAULT_MACRO_PRESETS = [
  {
    id: "preset-youtube-list-channels",
    name: "YouTube: List all channels → scrape handles",
    startUrl: "https://www.youtube.com/channel_switcher",
    steps: [
      { type: "nav", href: "https://www.youtube.com/channel_switcher" },
      { type: "action", action: "youtube-list-channels" },
    ],
  },
  {
    id: "preset-suno-submit",
    name: "Suno: Open Create page → click Compose",
    startUrl: "https://suno.com/create",
    steps: [
      { type: "nav", href: "https://suno.com/create" },
      { type: "action", action: "suno-submit" },
    ],
  },
  {
    id: "preset-suno-full-run",
    name: "Suno: Create → submit → wait for clip",
    startUrl: "https://suno.com/create",
    steps: [
      { type: "nav", href: "https://suno.com/create" },
      { type: "action", action: "suno-submit" },
      { type: "action", action: "suno-capture-result" },
    ],
  },
  {
    id: "preset-midjourney-scrape",
    name: "Midjourney: Open /imagine → scrape generated images",
    startUrl: "https://www.midjourney.com/imagine",
    steps: [
      { type: "nav", href: "https://www.midjourney.com/imagine" },
      { type: "action", action: "midjourney-scrape-jobs" },
    ],
  },
  {
    id: "preset-acestep-health",
    name: "ACE-Step: Health check",
    startUrl: "",
    steps: [{ type: "action", action: "acestep-health" }],
  },
  {
    id: "preset-heartmula-health",
    name: "HeartMuLa: Health check",
    startUrl: "",
    steps: [{ type: "action", action: "heartmula-health" }],
  },
  {
    id: "preset-comfyui-health",
    name: "ComfyUI: Health check",
    startUrl: "",
    steps: [{ type: "action", action: "comfyui-health" }],
  },
];

// Clone a built-in preset into your own saved macros (so it can be edited/deleted like any
// other macro from here on).
export function addPresetMacro(presetId) {
  const preset = DEFAULT_MACRO_PRESETS.find((p) => p.id === presetId);
  if (!preset) throw new Error("Unknown preset: " + presetId);
  return saveMacro({ name: preset.name, steps: preset.steps, startUrl: preset.startUrl });
}

// ── Macro packages ──────────────────────────────────────────────────────────────
// A package is just a named group of the presets above — "set up everything for this site/
// workflow in one click" instead of adding presets one at a time. Grouped by what you'd
// actually reach for together (every macro in a package still lands in your saved list as its
// own independent, editable macro — a package is only a one-shot install action, not a
// standing relationship between the macros).
export const MACRO_PACKAGES = [
  {
    id: "package-suno-essentials",
    label: "Suno essentials",
    description: "The two Suno macros: a bare submit, and a full create→submit→wait-for-clip run you can drop into a bulk loop.",
    presetIds: ["preset-suno-submit", "preset-suno-full-run"],
  },
  {
    id: "package-youtube-ops",
    label: "YouTube ops",
    description: "Channel-switcher scraping, for feeding discovered channels into the Channel Manager.",
    presetIds: ["preset-youtube-list-channels"],
  },
  {
    id: "package-midjourney-ops",
    label: "Midjourney ops",
    description: "Scrape generated /imagine results.",
    presetIds: ["preset-midjourney-scrape"],
  },
  {
    id: "package-engine-health",
    label: "Engine health checks",
    description: "One health-check macro per free self-hosted engine (ACE-Step, HeartMuLa, ComfyUI) — handy as a quick \"are my Kaggle servers up\" panel.",
    presetIds: ["preset-acestep-health", "preset-heartmula-health", "preset-comfyui-health"],
  },
];

export function findMacroPackage(packageId) {
  return MACRO_PACKAGES.find((p) => p.id === packageId) || null;
}

// Installs every preset in a package as its own saved macro in one go. Returns the newly
// created macros (not the whole list) so a caller can e.g. select the last one.
export function addPresetPackage(packageId) {
  const pkg = findMacroPackage(packageId);
  if (!pkg) throw new Error("Unknown package: " + packageId);
  return pkg.presetIds.map((id) => addPresetMacro(id));
}

// ── Storage ───────────────────────────────────────────────────────────────────

export function loadMacros() {
  try {
    const raw = localStorage.getItem(MACROS_KEY);
    const list = raw ? JSON.parse(raw) : [];
    return Array.isArray(list) ? list : [];
  } catch {
    return [];
  }
}

export function saveMacro({ name, steps, startUrl }) {
  const macro = {
    id: `macro-${Date.now().toString(36)}`,
    name: name || `Macro ${new Date().toLocaleString()}`,
    createdAt: new Date().toISOString(),
    startUrl: startUrl || steps.find((s) => s.type === "nav")?.href || "",
    steps,
  };
  const list = loadMacros();
  list.push(macro);
  localStorage.setItem(MACROS_KEY, JSON.stringify(list));
  return macro;
}

export function deleteMacro(id) {
  const list = loadMacros().filter((m) => m.id !== id);
  localStorage.setItem(MACROS_KEY, JSON.stringify(list));
  return list;
}

export function updateMacro(id, patch) {
  const list = loadMacros().map((m) => (m.id === id ? { ...m, ...patch } : m));
  localStorage.setItem(MACROS_KEY, JSON.stringify(list));
  return list;
}

// ── Import / export ─────────────────────────────────────────────────────────────
// Macros only live in this browser's localStorage, so moving one to another machine (or just
// backing it up) needs an explicit round trip through a file. Export bundles the chosen macros
// (or all of them) into one JSON document; import merges them into the local set, resolving any
// id collision instead of overwriting an existing macro that happens to share one.

const MACRO_EXPORT_KIND = "biblemusically-macros";

export function exportMacros(ids) {
  const all = loadMacros();
  const chosen = ids?.length ? all.filter((m) => ids.includes(m.id)) : all;
  return { kind: MACRO_EXPORT_KIND, version: 1, exportedAt: new Date().toISOString(), macros: chosen };
}

// Accepts either our own export shape ({ kind, macros: [...] }) or a bare array of macros, so a
// hand-edited or hand-written JSON file works too. Returns { macros, imported, skipped }.
export function importMacros(json, { replaceAll = false } = {}) {
  let payload;
  try {
    payload = typeof json === "string" ? JSON.parse(json) : json;
  } catch {
    throw new Error("That file isn't valid JSON.");
  }
  const incoming = Array.isArray(payload) ? payload : Array.isArray(payload?.macros) ? payload.macros : null;
  if (!incoming) throw new Error("Doesn't look like a macro export — expected an array of macros, or { macros: [...] }.");

  const existing = replaceAll ? [] : loadMacros();
  const usedIds = new Set(existing.map((m) => m.id));
  let skipped = 0;
  const imported = incoming
    .filter((m) => m && Array.isArray(m.steps))
    .map((m) => {
      let id = m.id && !usedIds.has(m.id) ? m.id : `macro-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
      usedIds.add(id);
      return {
        id,
        name: m.name || `Imported macro ${new Date().toLocaleString()}`,
        createdAt: m.createdAt || new Date().toISOString(),
        startUrl: m.startUrl || "",
        steps: m.steps,
      };
    });
  skipped = incoming.length - imported.length;

  const merged = [...existing, ...imported];
  localStorage.setItem(MACROS_KEY, JSON.stringify(merged));
  return { macros: merged, imported: imported.length, skipped };
}

// ── Step tidying ──────────────────────────────────────────────────────────────
// The raw recorder is deliberately chatty (it re-arms on every page load and records
// low-level events); clean the stream into a replayable sequence.

export function tidySteps(raw) {
  const steps = [];
  for (const s of raw || []) {
    if (!s || !s.type) continue;
    const prev = steps[steps.length - 1];
    // Collapse repeated navs to the same URL (re-arm after SPA route changes etc.).
    if (s.type === "nav" && prev?.type === "nav" && prev.href === s.href) continue;
    // Clicks/Enter on the page body carry no replayable intent.
    if ((s.type === "click" || s.type === "enter") && ["body", "html", ""].includes(s.target?.tag)) continue;
    // Successive edits of the same field: keep only the final value.
    if (
      s.type === "fill" && prev?.type === "fill" &&
      prev.target?.fallback === s.target?.fallback
    ) {
      steps[steps.length - 1] = s;
      continue;
    }
    steps.push(s);
  }
  return steps;
}

export function describeStep(s) {
  const label = s.target?.text || s.target?.selector || s.target?.tag || "";
  const list = s.target?.list ? ` [item ${s.target.list.index + 1}/${s.target.list.count}]` : "";
  switch (s.type) {
    case "nav": return `go to ${s.href}`;
    case "click": return `click ${label}${list}`;
    case "contextmenu": return `right-click ${label}${list}`;
    case "paste": return `paste into ${label}`;
    case "paste-clipboard": return `paste app clipboard into ${label || "focused field"}`;
    case "connector": {
      const c = CONNECTORS.find((x) => x.id === s.connector);
      return `connector → ${c ? c.label : s.connector || "unbound"} into ${label || "focused field"}`;
    }
    case "fill": return `type "${(s.value || "").slice(0, 30)}" into ${label}`;
    case "enter": return `press Enter in ${label}`;
    case "action": {
      const a = findMacroAction(s.action);
      return `run action → ${a ? a.label : s.action || "unknown"}`;
    }
    case "wait": {
      const c = findWaitCondition(s.condition);
      const extra = c.id === "text-appears" || c.id === "text-disappears" ? ` "${(s.text || "").slice(0, 30)}"`
        : c.id === "list-grow" ? ` in ${s.containerSelector || "(no container set)"}`
        : "";
      return `wait for → ${c.label}${extra}`;
    }
    case "download": return `click ${label}${list} → save download to "${s.folderName || "macro-download"}"`;
    default: return s.type;
  }
}

// Prefixed onto the description of a step marked `disabled` — kept out of describeStep itself
// so callers that already show a separate disabled indicator (a checkbox, strikethrough) aren't
// forced into this exact wording.
export function describeStepWithState(s) {
  const base = describeStep(s);
  return s.disabled ? `(skipped) ${base}` : base;
}

// ── Record-time special steps ─────────────────────────────────────────────────
// JS evaluated in the page when a record-only toolbar button is pressed. It describes the
// currently focused element (through the recorder's own __bmDescribe), appends the step via
// __bmRecordStep, optionally inserts text right away (so recording stays WYSIWYG), and
// reports back on the `macro:special` scrape key.

export function recordSpecialStepScript(step, insertText) {
  const j = JSON.stringify;
  return `(() => {
    const report = (r) => window.__bmScrape('macro:special', r);
    if (!window.__bmDescribe || !window.__bmRecordStep) { report({ ok: false, reason: 'recorder not injected' }); return false; }
    const el = (document.activeElement && document.activeElement !== document.body) ? document.activeElement : null;
    if (!el) { report({ ok: false, reason: 'no focused element — click into a field in the page first' }); return false; }
    const rec = Object.assign({}, ${j(step)}, { target: window.__bmDescribe(el) });
    window.__bmRecordStep(rec);
    const text = ${j(insertText ?? null)};
    window.__bmSuppressRecord = true; // don't double-record the insertion below as a fill step
    try {
    if (text != null && text !== '') {
      if (el.isContentEditable) {
        el.focus();
        try { document.execCommand('insertText', false, text); } catch (e) { el.textContent = text; }
        el.dispatchEvent(new InputEvent('input', { bubbles: true }));
      } else if (/^(input|textarea)$/i.test(el.tagName)) {
        const proto = el.tagName === 'TEXTAREA' ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype;
        const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set;
        if (setter) setter.call(el, text); else el.value = text;
        el.dispatchEvent(new Event('input', { bubbles: true }));
        el.dispatchEvent(new Event('change', { bubbles: true }));
      }
    }
    } finally { window.__bmSuppressRecord = false; }
    report({ ok: true, inserted: text != null && text !== '' });
    return true;
  })();`;
}

// ── Playback ──────────────────────────────────────────────────────────────────

// JS evaluated in the page to resolve one recorded step's target and perform its action.
// Reports { ok, how, reason } back through window.__bmScrape(key, …).
function stepScript(step, key, opts = {}) {
  const j = JSON.stringify;
  const listMode = opts.listMode || "recorded"; // recorded | fromEnd | first | last
  const pasteText = step.type === "paste" && opts.pasteText != null ? opts.pasteText : null;
  const fillValue = step.type === "fill" ? (opts.pasteText != null && step.param === "text" && opts.overrideFills ? opts.pasteText : step.value ?? "") : null;
  return `(() => {
    const step = ${j(step)};
    const LIST_MODE = ${j(listMode)};
    const PASTE_TEXT = ${j(pasteText)};
    const FILL_VALUE = ${j(fillValue)};
    const report = (r) => window.__bmScrape(${j(key)}, r);
    const q = (root, sel) => { try { return sel ? root.querySelector(sel) : null; } catch (e) { return null; } };

    function resolveTarget(t) {
      if (!t) return { el: document.body, how: 'body' };
      // 1. Positional list context: pick the item by position in the *current* list.
      if (t.list) {
        const listEl = q(document, t.list.listSelector);
        if (listEl) {
          const items = [...listEl.children].filter(c => c.tagName.toLowerCase() === t.list.itemTag);
          if (items.length) {
            let idx = t.list.index;
            if (LIST_MODE === 'fromEnd') idx = items.length - 1 - t.list.indexFromEnd;
            else if (LIST_MODE === 'first') idx = 0;
            else if (LIST_MODE === 'last') idx = items.length - 1;
            idx = Math.max(0, Math.min(items.length - 1, idx));
            const item = items[idx];
            let el = item;
            if (t.list.within) {
              el = q(item, ':scope > ' + t.list.within) || q(item, t.list.within) || item;
            }
            if (el) return { el, how: 'list[' + idx + '/' + items.length + ']' };
          }
        }
      }
      // 2. Stable attribute selector.
      if (t.selector) { const el = q(document, t.selector); if (el) return { el, how: 'selector' }; }
      // 3. Button/link text.
      if (t.byText) {
        const el = [...document.querySelectorAll(t.byText.tag)]
          .find(n => (n.innerText || '').trim().startsWith(t.byText.text));
        if (el) return { el, how: 'text' };
      }
      // 4. Structural nth-of-type path (most brittle, last resort).
      if (t.fallback) { const el = q(document, t.fallback); if (el) return { el, how: 'fallback' }; }
      return { el: null, how: null };
    }

    const setVal = (el, val) => {
      if (el.isContentEditable) {
        el.focus();
        try { document.execCommand('selectAll', false, null); document.execCommand('insertText', false, val); return true; } catch (e) {}
        el.textContent = val;
        el.dispatchEvent(new InputEvent('input', { bubbles: true }));
        return true;
      }
      const proto = el.tagName === 'TEXTAREA' ? window.HTMLTextAreaElement.prototype
        : el.tagName === 'SELECT' ? window.HTMLSelectElement.prototype
        : window.HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set;
      if (setter) setter.call(el, val); else el.value = val;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    };

    const { el, how } = resolveTarget(step.target);
    if (!el) { report({ ok: false, reason: 'target not found', step: step.type }); return false; }
    try { el.scrollIntoView({ block: 'center', inline: 'center' }); } catch (e) {}

    const mouse = (type, extra) => el.dispatchEvent(new MouseEvent(type, Object.assign({
      bubbles: true, cancelable: true, view: window,
    }, extra || {})));

    switch (step.type) {
      case 'click': {
        mouse('pointerdown'); mouse('mousedown'); mouse('pointerup'); mouse('mouseup');
        if (typeof el.click === 'function') el.click(); else mouse('click');
        break;
      }
      case 'contextmenu': {
        // Replays in-page custom context menus (native OS menus can't be scripted).
        mouse('pointerdown', { button: 2 }); mouse('mousedown', { button: 2 });
        mouse('contextmenu', { button: 2 });
        mouse('pointerup', { button: 2 }); mouse('mouseup', { button: 2 });
        break;
      }
      case 'paste': {
        const text = PASTE_TEXT != null ? PASTE_TEXT : (step.value || '');
        el.focus();
        try {
          const dt = new DataTransfer();
          dt.setData('text/plain', text);
          el.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
        } catch (e) {}
        setVal(el, text);
        break;
      }
      case 'fill': {
        setVal(el, FILL_VALUE != null ? FILL_VALUE : (step.value || ''));
        break;
      }
      case 'enter': {
        el.focus();
        const k = { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true };
        el.dispatchEvent(new KeyboardEvent('keydown', k));
        el.dispatchEvent(new KeyboardEvent('keypress', k));
        el.dispatchEvent(new KeyboardEvent('keyup', k));
        const form = el.form || el.closest && el.closest('form');
        if (form && typeof form.requestSubmit === 'function') { try { form.requestSubmit(); } catch (e) {} }
        break;
      }
      default:
        report({ ok: false, reason: 'unsupported step type ' + step.type });
        return false;
    }
    report({ ok: true, how, step: step.type });
    return true;
  })();`;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Replay a macro inside the embedded webview.
 *
 * @param steps  tidied steps
 * @param opts   { pasteText, listMode: 'recorded'|'fromEnd'|'first'|'last', overrideFills, stopOnFailure }
 * @param io     { evalJs(js), navigate(url), currentUrl(), takeScrape(key), armDownload(folder, filename, key),
 *                 onProgress(i, total, note) } — armDownload only needs to be provided if the macro has a
 *                 "download" step.
 * @returns      { ok, results: [{i, type, ok, how?, reason?, skipped?}] }
 */
export async function playMacro(steps, opts, io) {
  const results = [];
  const total = steps.length;
  let prevT = null;

  for (let i = 0; i < total; i++) {
    const step = steps[i];

    if (step.disabled) {
      io.onProgress?.(i, total, `(skipped) ${describeStep(step)}`);
      results.push({ i, type: step.type, ok: true, skipped: true });
      continue;
    }
    io.onProgress?.(i, total, describeStep(step));

    // Pace playback roughly like the recording (clamped so runs stay brisk but SPA-safe).
    const delta = prevT != null && step.t ? step.t - prevT : 800;
    prevT = step.t ?? prevT;
    await sleep(Math.min(3000, Math.max(400, delta)));

    if (step.type === "wait") {
      const timeoutMs = step.timeoutMs || 20000;
      const pollMs = step.pollMs || 700;
      const started = Date.now();
      let baseline = typeof step.baselineCount === "number" ? step.baselineCount : null;
      let met = false;
      let detail = null;
      while (Date.now() - started < timeoutMs) {
        const key = `macro:wait:${i}:${Date.now().toString(36)}`;
        let res = null;
        try {
          await io.evalJs(waitConditionScript(step, key));
          for (let tries = 0; tries < 6 && res == null; tries++) { await sleep(200); res = await io.takeScrape(key); }
        } catch { /* treat as not-yet-met and try again next tick */ }
        if (res) {
          detail = res;
          if (step.condition === "list-grow") {
            if (baseline == null) baseline = res.count;
            else if (res.count > baseline) { met = true; break; }
          } else if (res.met) { met = true; break; }
        }
        if (Date.now() - started < timeoutMs) await sleep(pollMs);
      }
      results.push({ i, type: "wait", ok: met, reason: met ? undefined : "condition wasn't met before the timeout", detail });
      if (!met && opts?.stopOnFailure) break;
      continue;
    }

    if (step.type === "download") {
      const folderName = (step.folderName || "macro-download").trim() || "macro-download";
      const resultKey = `macro:download:${i}:${Date.now().toString(36)}`;
      let ok = false, reason = null, path = null;
      try {
        if (!io.armDownload) throw new Error("this app build can't arm native downloads");
        await io.armDownload(folderName, step.filename || null, resultKey);
        await io.evalJs(stepScript({ ...step, type: "click" }, `macro:downloadclick:${i}:${Date.now().toString(36)}`, opts));
        const timeoutMs = step.timeoutMs || 30000;
        const started = Date.now();
        while (Date.now() - started < timeoutMs && !ok && reason == null) {
          await sleep(500);
          const res = await io.takeScrape(resultKey);
          if (res) { ok = !!res.ok; reason = res.error || null; path = res.path || null; break; }
        }
        if (!ok && reason == null) reason = "no download started — the click may not have triggered one";
      } catch (e) {
        reason = String(e);
      }
      results.push({ i, type: "download", ok, reason, path });
      if (!ok && opts?.stopOnFailure) break;
      continue;
    }

    if (step.type === "nav") {
      // Clicks that caused this navigation already replayed; only navigate if we're elsewhere.
      let here = "";
      try { here = await io.currentUrl(); } catch { /* treat as elsewhere */ }
      const norm = (u) => String(u || "").replace(/[#?].*$/, "").replace(/\/$/, "");
      if (norm(here) !== norm(step.href)) {
        await io.navigate(step.href);
        await sleep(2500); // page load settle
      }
      results.push({ i, type: "nav", ok: true });
      continue;
    }

    if (step.type === "action") {
      // Runs one of the app's existing automation scripts (see MACRO_ACTIONS) instead of
      // resolving a recorded DOM target — no positional element to find, so this bypasses
      // stepScript entirely.
      const action = findMacroAction(step.action);
      if (!action) {
        results.push({ i, type: "action", ok: false, reason: "unknown action: " + step.action });
        if (opts?.stopOnFailure) break;
        continue;
      }
      const key = `macro:action:${i}:${Date.now().toString(36)}`;
      let res = null;
      try {
        await io.evalJs(action.script());
        // Most actions answer almost immediately (a click, a quick scrape); a few — like
        // waiting out a Suno render — run their own multi-minute internal poll and only report
        // once genuinely done, so the wait budget here has to be per-action, not one size fits all.
        const pollMs = 400;
        const deadline = Date.now() + (action.timeoutMs || 4800);
        while (res == null && Date.now() < deadline) {
          await sleep(pollMs);
          res = await io.takeScrape(action.scrapeKey);
        }
      } catch (e) {
        res = { ok: false, reason: String(e) };
      }
      const actionOk = res != null && res.ok !== false;
      results.push({
        i, type: "action", ok: actionOk,
        reason: res == null ? "no response from page" : res.reason,
        data: res,
      });
      if (!actionOk && opts?.stopOnFailure) break;
      continue;
    }

    // Special steps insert app-provided text at the recorded (positional) target:
    // paste-clipboard reads the virtual clipboard bucket, connector steps resolve their
    // bound connector action. Both replay through the regular paste mechanics.
    let execStep = step;
    let execOpts = opts;
    if (step.type === "paste-clipboard" || step.type === "connector") {
      let text = "";
      try {
        text = step.type === "paste-clipboard"
          ? (opts?.pasteText != null ? opts.pasteText : getVirtualClipboard())
          : await resolveConnector(step.connector, step.binding);
      } catch (e) {
        results.push({ i, type: step.type, ok: false, reason: "connector failed: " + e });
        if (opts?.stopOnFailure) break;
        continue;
      }
      execStep = { ...step, type: "paste", value: text };
      execOpts = { ...opts, pasteText: text };
    }

    const key = `macro:play:${i}:${Date.now().toString(36)}`;
    let res = null;
    try {
      await io.evalJs(stepScript(execStep, key, execOpts));
      for (let tries = 0; tries < 8 && res == null; tries++) {
        await sleep(350);
        res = await io.takeScrape(key);
      }
    } catch (e) {
      res = { ok: false, reason: String(e) };
    }
    const stepOk = !!res?.ok;
    results.push({ i, type: step.type, ok: stepOk, how: res?.how, reason: res?.reason || (res == null ? "no response from page" : undefined) });
    if (!stepOk && opts?.stopOnFailure) break;
  }

  return { ok: results.every((r) => r.ok), results };
}
