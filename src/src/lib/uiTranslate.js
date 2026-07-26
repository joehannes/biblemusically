import { api } from "./api";
import INVENTORY from "../i18n/ui-strings.json";
import { mayTranslate as mayTranslateRule, baselineOriginal, languageSlug } from "./uiTranslateRules";

// ─────────────────────────────────────────────────────────────────────────────
// Runtime GUI translation.
//
// The interface has thousands of literal strings spread across ~25 pages, so a conventional i18n
// refactor (extract every string into catalogs, render through t()) would touch every component.
// Instead this translates the RENDERED interface: swap the visible text nodes and chrome attributes,
// and keep doing it for nodes React adds later (navigation, dialogs, tours, toasts).
//
// What may be translated is decided by ONE list: src/i18n/ui-strings.json, the inventory of literal
// strings extracted from the source by scripts/extract-ui-strings.mjs. That list is the difference
// between a translator and a money pit:
//
//   • every language in the picker ships as a full catalog (src/i18n/<code>.json) keyed by those
//     strings, so switching is instant, offline, and costs no AI call at all — including the panels
//     that only render on demand (tours, guides, dialogs), which used to stay English because nothing
//     had ever translated them before they appeared.
//   • a language the user types in themselves has no catalog, so its whole inventory is translated
//     ONCE in background batches and cached — later-appearing panels are already covered instead of
//     waiting ~20s on the AI mid-click.
//   • Runtime data is never sent: counts, timestamps, model ids, filenames, song text. Those strings
//     are unbounded — every new value would be a new AI request. This is what previously turned a
//     10-minute session into hundreds of translate calls and burned a day's free credits.
//
// Other safeguards, because translating the wrong thing is worse than not translating:
//   • user content is skipped — inputs/textareas, code/pre, anything marked [data-no-i18n], and
//     long strings (song lyrics, chapter text) which are the user's data, not interface chrome;
//   • each string is requested at most once per session, whatever the provider answers, and a
//     provider failure pauses translation instead of retrying on every render;
//   • the original text is retained per node and re-baselined when React writes something new into
//     it, so switching back to English is exact and a stale translation is never re-applied — that
//     re-apply was what made labels flicker between English and the target language.
// ─────────────────────────────────────────────────────────────────────────────

export const UI_LANGUAGES = [
  { code: "en", label: "English", native: "English" },
  { code: "de", label: "German", native: "Deutsch" },
  { code: "es", label: "Spanish", native: "Español" },
  { code: "pt", label: "Portuguese", native: "Português" },
  { code: "ru", label: "Russian", native: "Русский" },
  { code: "fr", label: "French", native: "Français" },
  { code: "it", label: "Italian", native: "Italiano" },
  { code: "nl", label: "Dutch", native: "Nederlands" },
  { code: "pl", label: "Polish", native: "Polski" },
  { code: "he", label: "Hebrew", native: "עברית" },
  { code: "ar", label: "Arabic", native: "العربية" },
  { code: "hi", label: "Hindi", native: "हिन्दी" },
  { code: "id", label: "Indonesian", native: "Bahasa Indonesia" },
  { code: "ja", label: "Japanese", native: "日本語" },
  { code: "ko", label: "Korean", native: "한국어" },
  { code: "zh", label: "Chinese", native: "中文" },
];

// Every language in the picker ships a committed catalog, so picking one costs no AI requests and
// works with no key and no connection. Loaded on demand — only the chosen one is ever fetched, which
// is why sixteen catalogs cost nothing at startup.
const BUNDLED = {
  de: () => import("../i18n/de.json"),
  es: () => import("../i18n/es.json"),
  pt: () => import("../i18n/pt.json"),
  ru: () => import("../i18n/ru.json"),
  fr: () => import("../i18n/fr.json"),
  it: () => import("../i18n/it.json"),
  nl: () => import("../i18n/nl.json"),
  pl: () => import("../i18n/pl.json"),
  he: () => import("../i18n/he.json"),
  ar: () => import("../i18n/ar.json"),
  hi: () => import("../i18n/hi.json"),
  id: () => import("../i18n/id.json"),
  ja: () => import("../i18n/ja.json"),
  ko: () => import("../i18n/ko.json"),
  zh: () => import("../i18n/zh.json"),
};
export const BUNDLED_LANGUAGES = Object.keys(BUNDLED);
export const isBundledLanguage = (code) => Object.prototype.hasOwnProperty.call(BUNDLED, code);

// ── Languages the user adds ──────────────────────────────────────────────────
// Sixteen languages is not "every language", and the ones missing are exactly the ones least likely
// to be shipped by anybody: a regional variety, a minority language, the language someone actually
// prays in. So the picker takes a name typed by hand and the AI translates into it on demand. Codes
// are BCP-47 private-use (`x-…`) so they can never collide with a real code we later bundle.
const CUSTOM_KEY = "studio:ui-languages-custom";

export function customLanguages() {
  try {
    const list = JSON.parse(localStorage.getItem(CUSTOM_KEY) || "[]");
    return Array.isArray(list) ? list.filter((l) => l?.code && l?.label) : [];
  } catch { return []; }
}

/** All languages the picker offers: the bundled ones, then anything the user added. */
export const allLanguages = () => [...UI_LANGUAGES, ...customLanguages()];
export const isCustomLanguage = (code) => typeof code === "string" && code.startsWith("x-");

/**
 * Remember a language the user typed. The name is passed to the AI verbatim, so "Swiss German",
 * "Tagalog", "Brazilian Portuguese, informal" and "Koine Greek" all work — the model is asked for a
 * language by name, not by code.
 */
export function addCustomLanguage(name) {
  const label = String(name || "").trim().slice(0, 60);
  if (label.length < 2) return { ok: false, error: "Type the name of the language." };
  const s = languageSlug(label);
  if (!s) return { ok: false, error: "That name has no letters in it." };
  const code = `x-${s}`;
  if (UI_LANGUAGES.some((l) => l.label.toLowerCase() === label.toLowerCase() || l.native.toLowerCase() === label.toLowerCase())) {
    const built = UI_LANGUAGES.find((l) => l.label.toLowerCase() === label.toLowerCase() || l.native.toLowerCase() === label.toLowerCase());
    return { ok: true, code: built.code, existing: "bundled" };   // already shipped — use the real one
  }
  const list = customLanguages();
  if (!list.some((l) => l.code === code)) {
    list.push({ code, label, native: label, custom: true });
    try { localStorage.setItem(CUSTOM_KEY, JSON.stringify(list)); } catch { /* quota — fine */ }
  }
  return { ok: true, code };
}

export function removeCustomLanguage(code) {
  const list = customLanguages().filter((l) => l.code !== code);
  try { localStorage.setItem(CUSTOM_KEY, JSON.stringify(list)); } catch { /* ignore */ }
  try { localStorage.removeItem(cacheKey(code)); } catch { /* ignore */ }
  langState.delete(code);
  return list;
}

const LANG_KEY = "studio:ui-language";
const cacheKey = (code) => `studio:ui-i18n:${code}`;
const MAX_LEN = 140;        // longer than this is user content, not a UI label
// Free tiers count REQUESTS, not tokens: OpenRouter's free models allow 50 per day for the whole
// account. So ask for a lot of strings at once, cap what one session may spend, and cap the day
// across sessions — GUI translation must never be the reason a lyrics generation has no quota left.
const BATCH = 120;          // strings per AI request (~14 requests covers the entire inventory)
const AI_BATCH_BUDGET = 16; // requests per language per session
const DAILY_CAP = 24;       // requests per day, all languages, persisted across restarts
const UNKNOWN_BUDGET = 300; // strings outside the inventory we're willing to pay to translate
const FAILURE_PAUSE_MS = 5 * 60 * 1000; // after a provider error, stop asking for a while
const DEBOUNCE_MS = 400;    // coalesce a render burst into one AI pass
const ATTRS = ["placeholder", "title", "aria-label"];
const SKIP_TAGS = new Set(["SCRIPT", "STYLE", "CODE", "PRE", "TEXTAREA", "INPUT", "SELECT", "OPTION", "KBD", "NOSCRIPT"]);

/** The literal strings the interface is built from — the only text allowed near the AI. */
const KNOWN = new Set(INVENTORY.strings || []);

let current = "en";
let observer = null;
let switching = false;
const listeners = new Set();

const emit = () => listeners.forEach((fn) => { try { fn(current); } catch { /* ignore */ } });
export const subscribeLanguage = (fn) => { listeners.add(fn); fn(current); return () => listeners.delete(fn); };
export const getUiLanguage = () => current;

// ── Per-language state ───────────────────────────────────────────────────────
// `requested` is the spend guard: a string goes in when it is sent, not when it comes back, so a
// provider that omits or mangles an entry still can't make the app ask for it again.
const langState = new Map();
function stateFor(code) {
  let st = langState.get(code);
  if (!st) {
    st = { catalog: loadCache(code), bundled: null, requested: new Set(), batches: 0, unknownSpent: 0, pausedUntil: 0, prefetched: false };
    langState.set(code, st);
  }
  return st;
}

function loadCache(code) {
  try { return JSON.parse(localStorage.getItem(cacheKey(code)) || "{}"); } catch { return {}; }
}

// ── Daily request ledger ─────────────────────────────────────────────────────
// Session budgets alone can't protect a per-day quota: closing and reopening the app resets them.
// This ledger survives restarts, so the interface can never spend more than `DAILY_CAP` requests on
// translation in one day no matter how often the language is switched.
const LEDGER_KEY = "studio:ui-i18n-requests";
const today = () => new Date().toISOString().slice(0, 10);
function requestsToday() {
  try {
    const l = JSON.parse(localStorage.getItem(LEDGER_KEY) || "{}");
    return l.date === today() ? (l.count || 0) : 0;
  } catch { return 0; }
}
function countRequest() {
  try { localStorage.setItem(LEDGER_KEY, JSON.stringify({ date: today(), count: requestsToday() + 1 })); }
  catch { /* ignore */ }
}
/** Persist only what the AI produced; the bundled catalog is already on disk. */
function saveCache(code, learned) {
  try { localStorage.setItem(cacheKey(code), JSON.stringify(learned)); } catch { /* quota — fine */ }
}

async function ensureBundled(code, st) {
  if (st.bundled || !isBundledLanguage(code)) return;
  try {
    const mod = await BUNDLED[code]();
    const strings = (mod?.default || mod)?.strings || {};
    st.bundled = strings;
    // The committed catalog is reviewed text; it outranks anything the AI cached earlier.
    st.catalog = { ...st.catalog, ...strings };
  } catch (err) {
    console.warn(`[i18n] bundled catalog for ${code} failed to load:`, err);
    st.bundled = {};
  }
}

// ── Eligibility ──────────────────────────────────────────────────────────────

/** Should this text node be touched at all? */
function eligible(node) {
  const text = node.nodeValue;
  if (!text) return false;
  const trimmed = text.trim();
  if (trimmed.length < 2 || trimmed.length > MAX_LEN) return false;
  if (!/[a-zA-ZЀ-ӿ]/.test(trimmed)) return false;   // numbers/symbols only
  const el = node.parentElement;
  if (!el) return false;
  if (SKIP_TAGS.has(el.tagName)) return false;
  if (el.closest("[data-no-i18n]")) return false;        // explicit opt-out (user content)
  if (el.isContentEditable) return false;
  return true;
}

/** May this string be SENT to the AI? See `uiTranslateRules.js` for the reasoning and its tests. */
const mayTranslate = (s, st) => mayTranslateRule(s, KNOWN, st.unknownSpent, UNKNOWN_BUDGET);

// ── Collecting targets ───────────────────────────────────────────────────────

function collectTextNodes(root, out) {
  if (root.nodeType === Node.TEXT_NODE) { if (eligible(root)) out.push(root); return; }
  if (root.nodeType !== Node.ELEMENT_NODE && root.nodeType !== Node.DOCUMENT_NODE && root.nodeType !== Node.DOCUMENT_FRAGMENT_NODE) return;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => (eligible(n) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT),
  });
  let n;
  while ((n = walker.nextNode())) out.push(n);
}

function collectAttrTargets(root, out) {
  if (root.nodeType !== Node.ELEMENT_NODE && root.nodeType !== Node.DOCUMENT_NODE) return;
  const push = (el) => {
    if (el.closest?.("[data-no-i18n]")) return;
    for (const attr of ATTRS) {
      if (!el.hasAttribute?.(attr)) continue;
      const v = el.getAttribute(attr);
      if (!v || v.trim().length < 2 || v.length > MAX_LEN) continue;
      if (!/[a-zA-ZЀ-ӿ]/.test(v)) continue;
      out.push({ el, attr });
    }
  };
  if (root.nodeType === Node.ELEMENT_NODE) push(root);
  root.querySelectorAll?.(ATTRS.map((a) => `[${a}]`).join(",")).forEach(push);
}

// ── Applying ────────────────────────────────────────────────────────────────

/** The untranslated text for a node — see `uiTranslateRules.js` for why it re-baselines. */
const originalOf = (node) => baselineOriginal(node);

function attrOriginalOf(el, attr) {
  const store = (el.__i18nAttrs ||= {});
  const v = el.getAttribute(attr);
  const rec = store[attr];
  if (rec && v === rec.applied) return rec.original;
  store[attr] = { original: v, applied: undefined };
  return v;
}

/**
 * Paint everything already known, synchronously, and report what is missing.
 * Synchronous matters: a debounced apply leaves a visible English frame on every re-render.
 */
function applyKnown(textNodes, attrTargets, catalog, missing) {
  let applied = 0;
  for (const node of textNodes) {
    const orig = originalOf(node);
    if (!orig) continue;
    const key = orig.trim();
    const hit = catalog[key];
    if (hit === undefined) { missing?.add(key); continue; }
    if (!hit || hit === key) continue;
    const next = `${orig.match(/^\s*/)[0]}${hit}${orig.match(/\s*$/)[0]}`;
    if (node.nodeValue !== next) {
      node.nodeValue = next;
      node.__i18nApplied = next;
      applied++;
    }
  }
  for (const { el, attr } of attrTargets) {
    const orig = attrOriginalOf(el, attr);
    if (!orig) continue;
    const key = orig.trim();
    const hit = catalog[key];
    if (hit === undefined) { missing?.add(key); continue; }
    if (!hit || hit === key) continue;
    if (el.getAttribute(attr) !== hit) {
      el.setAttribute(attr, hit);
      el.__i18nAttrs[attr].applied = hit;
      applied++;
    }
  }
  return applied;
}

/** Repaint the whole document from the catalog (used after new translations arrive). */
function repaintDocument(code) {
  const st = stateFor(code);
  const textNodes = [];
  const attrTargets = [];
  collectTextNodes(document.body, textNodes);
  collectAttrTargets(document.body, attrTargets);
  return applyKnown(textNodes, attrTargets, st.catalog, null);
}

function restoreOriginals() {
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null);
  let n;
  while ((n = walker.nextNode())) {
    if (n.__i18nOriginal !== undefined && n.nodeValue === n.__i18nApplied) {
      n.nodeValue = n.__i18nOriginal;
      n.__i18nApplied = undefined;
    }
  }
  document.body.querySelectorAll(ATTRS.map((a) => `[${a}]`).join(",")).forEach((el) => {
    const store = el.__i18nAttrs;
    if (!store) return;
    for (const attr of ATTRS) {
      const rec = store[attr];
      if (rec && rec.original != null && el.getAttribute(attr) === rec.applied) {
        el.setAttribute(attr, rec.original);
        rec.applied = undefined;
      }
    }
  });
}

// ── Asking the AI ───────────────────────────────────────────────────────────

/**
 * Translate strings the catalog doesn't cover. Returns the first provider error, if any.
 * Every guard lives here: the pause after a failure, the per-session request budget, and the
 * "asked once" set that makes a repeat impossible even when the answer was useless.
 */
async function requestMissing(code, langName, wanted) {
  const st = stateFor(code);
  if (Date.now() < st.pausedUntil) return null;

  const todo = [];
  for (const s of wanted) {
    if (st.requested.has(s) || st.catalog[s] !== undefined) continue;
    if (!mayTranslate(s, st)) { st.requested.add(s); continue; }  // remember the refusal too
    if (!KNOWN.has(s)) st.unknownSpent++;
    todo.push(s);
  }
  if (!todo.length) return null;

  let error = null;
  const learned = loadCache(code);
  for (let i = 0; i < todo.length; i += BATCH) {
    if (current !== code) break;                       // user switched away — stop spending
    if (st.batches >= AI_BATCH_BUDGET) {
      console.warn(`[i18n] request budget reached for ${code}; remaining strings stay English this session`);
      break;
    }
    if (requestsToday() >= DAILY_CAP) {
      console.warn(`[i18n] daily translation request cap (${DAILY_CAP}) reached; leaving the rest of the free quota for generation`);
      break;
    }
    const batch = todo.slice(i, i + BATCH);
    batch.forEach((s) => st.requested.add(s));
    st.batches++;
    countRequest();
    try {
      const r = await api.aiTranslateUi(batch, langName);
      const t = r?.translations || {};
      Object.assign(st.catalog, t);
      Object.assign(learned, t);
      saveCache(code, learned);
    } catch (err) {
      error = String(err?.message || err);
      // A failing provider fails for every batch. Back off instead of turning each render into
      // another doomed request.
      st.pausedUntil = Date.now() + FAILURE_PAUSE_MS;
      console.warn("[i18n] batch failed, pausing translation:", error);
      break;
    }
  }
  if (current === code) repaintDocument(code);
  return error;
}

/**
 * Translate the rest of the inventory in the background, so a panel that renders for the first time
 * ten minutes from now (a tour, a dialog, a wizard step) is already translated. Bundled languages
 * skip this entirely — their catalog already covers the inventory.
 */
async function prefetchInventory(code, langName) {
  const st = stateFor(code);
  if (st.prefetched) return;
  st.prefetched = true;
  const pending = INVENTORY.strings.filter((s) => st.catalog[s] === undefined && !st.requested.has(s));
  if (!pending.length) return;
  for (let i = 0; i < pending.length; i += BATCH) {
    if (current !== code || Date.now() < st.pausedUntil) break;
    if (st.batches >= AI_BATCH_BUDGET || requestsToday() >= DAILY_CAP) break;
    await requestMissing(code, langName, pending.slice(i, i + BATCH));
    await new Promise((r) => setTimeout(r, 1500));      // stay friendly to free-tier rate limits
  }
}

// ── Observing ───────────────────────────────────────────────────────────────

function startObserver(code, langName) {
  stopObserver();
  let pending = new Set();
  let timer = null;
  observer = new MutationObserver((records) => {
    const st = stateFor(code);
    const textNodes = [];
    const attrTargets = [];
    for (const rec of records) {
      for (const added of rec.addedNodes) {
        collectTextNodes(added, textNodes);
        collectAttrTargets(added, attrTargets);
      }
      if (rec.type === "characterData") {
        // Ignore our own writes; anything else is React handing this node new text.
        if (rec.target.nodeValue !== rec.target.__i18nApplied && eligible(rec.target)) textNodes.push(rec.target);
      }
      if (rec.type === "attributes" && rec.target?.nodeType === Node.ELEMENT_NODE) {
        collectAttrTargets(rec.target, attrTargets);
      }
    }
    if (!textNodes.length && !attrTargets.length) return;
    // Paint the known text in this same task so nothing is ever briefly visible in English.
    applyKnown(textNodes, attrTargets, st.catalog, pending);
    if (!pending.size) return;
    clearTimeout(timer);
    timer = setTimeout(() => {
      const batch = [...pending];
      pending = new Set();
      requestMissing(code, langName, batch).catch(() => {});
    }, DEBOUNCE_MS);
  });
  // document.body, not #root: dialogs, toasts, tooltips and the tour veil are portalled to the body,
  // and observing only #root is why those never got translated.
  observer.observe(document.body, {
    childList: true, subtree: true, characterData: true,
    attributes: true, attributeFilter: ATTRS,
  });
}

function stopObserver() {
  if (observer) { observer.disconnect(); observer = null; }
}

/** Switch the interface language. `en` restores the original text and stops translating. */
export async function setUiLanguage(code) {
  const lang = allLanguages().find((l) => l.code === code) || UI_LANGUAGES[0];
  current = lang.code;
  try { localStorage.setItem(LANG_KEY, lang.code); } catch { /* ignore */ }
  emit();

  if (lang.code === "en") {
    stopObserver();
    restoreOriginals();
    return { ok: true, restored: true };
  }
  if (switching) return { ok: false, busy: true };
  switching = true;
  try {
    const st = stateFor(lang.code);
    await ensureBundled(lang.code, st);

    // 1. Everything already known — instant, no AI.
    const textNodes = [];
    const attrTargets = [];
    collectTextNodes(document.body, textNodes);
    collectAttrTargets(document.body, attrTargets);
    const missing = new Set();
    let applied = applyKnown(textNodes, attrTargets, st.catalog, missing);

    // Keep observing from here on: whatever renders next gets painted from the catalog.
    startObserver(lang.code, lang.label);

    // 2. Whatever is on screen and still missing (a bundled catalog leaves almost nothing).
    // `repaintDocument` only counts nodes it *changed*, so its result is added to what step 1
    // already painted — otherwise a fully-cached switch would report "0 labels" and look like a
    // failure.
    const error = await requestMissing(lang.code, lang.label, missing);
    applied += repaintDocument(lang.code);

    if (error && applied === 0) {
      // Nothing landed — don't leave the interface in a half-translated state, and report the truth.
      stopObserver();
      restoreOriginals();
      current = "en";
      try { localStorage.setItem(LANG_KEY, "en"); } catch { /* ignore */ }
      emit();
      return { ok: false, error };
    }

    // 3. The rest of the inventory, in the background, for panels not rendered yet.
    if (!isBundledLanguage(lang.code)) {
      prefetchInventory(lang.code, lang.label).catch(() => {});
    }
    return { ok: true, applied, error, bundled: isBundledLanguage(lang.code) };
  } finally {
    switching = false;
  }
}

/** Re-apply the saved language on startup (bundled/cached strings paint with no AI call). */
export function initUiLanguage() {
  let saved = "en";
  try { saved = localStorage.getItem(LANG_KEY) || "en"; } catch { /* ignore */ }
  if (saved && saved !== "en") setUiLanguage(saved).catch(() => {});
  else current = "en";
}

/** What the language picker shows about coverage, and what the i18n panel reports. */
export function uiTranslationStatus(code = current) {
  const st = langState.get(code);
  return {
    code,
    bundled: isBundledLanguage(code),
    custom: isCustomLanguage(code),
    inventory: KNOWN.size,
    known: st ? Object.keys(st.catalog).length : 0,
    aiRequests: st?.batches || 0,
    requestsToday: requestsToday(),
    dailyCap: DAILY_CAP,
    paused: st ? Date.now() < st.pausedUntil : false,
  };
}
