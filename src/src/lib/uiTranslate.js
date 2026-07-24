import { api } from "./api";

// ─────────────────────────────────────────────────────────────────────────────
// Runtime GUI translation.
//
// The interface has thousands of literal strings spread across ~25 pages, so a conventional i18n
// refactor (extract every string into catalogs, render through t()) would touch every component.
// Instead this translates the RENDERED interface: collect the visible text nodes, translate the
// distinct strings with the configured free AI provider, and swap them in place — then keep doing
// it for nodes React adds later (navigation, dialogs, toasts) via a MutationObserver.
//
// Deliberate safeguards, because translating the wrong thing is worse than not translating:
//   • user content is skipped — inputs/textareas, code/pre, anything marked [data-no-i18n], and
//     long strings (song lyrics, chapter text) which are the user's data, not interface chrome;
//   • translations are cached per language in localStorage, so switching back is instant and the
//     AI is called once per unseen string rather than on every render;
//   • the original text is retained on each node, so switching back to English is exact and
//     re-translating never compounds on an earlier translation.
// ─────────────────────────────────────────────────────────────────────────────

export const UI_LANGUAGES = [
  { code: "en", label: "English", native: "English" },
  { code: "de", label: "German", native: "Deutsch" },
  { code: "es", label: "Spanish", native: "Español" },
  { code: "fr", label: "French", native: "Français" },
  { code: "pt", label: "Portuguese", native: "Português" },
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

const LANG_KEY = "studio:ui-language";
const cacheKey = (code) => `studio:ui-i18n:${code}`;
const MAX_LEN = 140;        // longer than this is user content, not a UI label
const BATCH = 60;
const SKIP_TAGS = new Set(["SCRIPT", "STYLE", "CODE", "PRE", "TEXTAREA", "INPUT", "SELECT", "OPTION", "KBD", "NOSCRIPT"]);

let current = "en";
let observer = null;
let translating = false;
const listeners = new Set();

const emit = () => listeners.forEach((fn) => { try { fn(current); } catch { /* ignore */ } });
export const subscribeLanguage = (fn) => { listeners.add(fn); fn(current); return () => listeners.delete(fn); };
export const getUiLanguage = () => current;

const loadCache = (code) => {
  try { return JSON.parse(localStorage.getItem(cacheKey(code)) || "{}"); } catch { return {}; }
};
const saveCache = (code, map) => {
  try { localStorage.setItem(cacheKey(code), JSON.stringify(map)); } catch { /* quota — fine */ }
};

/** Should this text node be translated? */
function eligible(node) {
  const text = node.nodeValue;
  if (!text) return false;
  const trimmed = text.trim();
  if (trimmed.length < 2 || trimmed.length > MAX_LEN) return false;
  if (!/[a-zA-Z]/.test(trimmed)) return false;           // numbers/symbols only
  const el = node.parentElement;
  if (!el) return false;
  if (SKIP_TAGS.has(el.tagName)) return false;
  if (el.closest("[data-no-i18n]")) return false;        // explicit opt-out (user content)
  if (el.isContentEditable) return false;
  return true;
}

function collectNodes(root) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => (eligible(n) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT),
  });
  const nodes = [];
  let n;
  while ((n = walker.nextNode())) nodes.push(n);
  return nodes;
}

/** Remember the untranslated text once, so English restore is exact and re-translation never stacks. */
const originalOf = (node) => {
  if (node.__i18nOriginal === undefined) node.__i18nOriginal = node.nodeValue;
  return node.__i18nOriginal;
};

function applyMap(nodes, map) {
  let n = 0;
  for (const node of nodes) {
    const orig = originalOf(node);
    const key = orig.trim();
    const translated = map[key];
    if (!translated || translated === key) continue;
    // Preserve the original leading/trailing whitespace around the label.
    const lead = orig.match(/^\s*/)[0];
    const tail = orig.match(/\s*$/)[0];
    const next = `${lead}${translated}${tail}`;
    if (node.nodeValue !== next) { node.nodeValue = next; n++; }
  }
  return n;
}

function restoreOriginals(root) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
  let n;
  while ((n = walker.nextNode())) {
    if (n.__i18nOriginal !== undefined) n.nodeValue = n.__i18nOriginal;
  }
}

// Returns { applied, error } — how many nodes actually got non-trivial translated text, and the
// first provider error if the AI call failed (so the caller can report the truth, not a fake success).
async function translateNodes(nodes, code, langName) {
  if (!nodes.length) return { applied: 0, error: null };
  const cache = loadCache(code);
  const wanted = [...new Set(nodes.map((n) => originalOf(n).trim()))];
  const missing = wanted.filter((s) => cache[s] === undefined);

  // Paint what we already know immediately, so cached UI switches feel instant.
  let applied = applyMap(nodes, cache);
  let error = null;

  for (let i = 0; i < missing.length; i += BATCH) {
    const batch = missing.slice(i, i + BATCH);
    try {
      const r = await api.aiTranslateUi(batch, langName);
      const t = r?.translations || {};
      Object.assign(cache, t);
      saveCache(code, cache);
      applied += applyMap(nodes, cache);
    } catch (err) {
      error = String(err?.message || err);
      console.warn("[i18n] batch failed:", error);
      break; // don't hammer a failing provider
    }
  }
  return { applied, error };
}

async function translateWhole(code, langName) {
  const root = document.getElementById("root") || document.body;
  return translateNodes(collectNodes(root), code, langName);
}

function startObserver(code, langName) {
  stopObserver();
  const root = document.getElementById("root") || document.body;
  let pending = [];
  let timer = null;
  observer = new MutationObserver((records) => {
    for (const rec of records) {
      for (const added of rec.addedNodes) {
        if (added.nodeType === Node.TEXT_NODE && eligible(added)) pending.push(added);
        else if (added.nodeType === Node.ELEMENT_NODE) pending.push(...collectNodes(added));
      }
      if (rec.type === "characterData" && eligible(rec.target)) pending.push(rec.target);
    }
    if (!pending.length) return;
    clearTimeout(timer);
    timer = setTimeout(() => {
      const batch = pending;
      pending = [];
      translateNodes(batch, code, langName).catch(() => {});
    }, 120); // coalesce a render burst into one pass
  });
  observer.observe(root, { childList: true, subtree: true, characterData: true });
}

function stopObserver() {
  if (observer) { observer.disconnect(); observer = null; }
}

/** Switch the interface language. `en` restores the original text and stops translating. */
export async function setUiLanguage(code) {
  const lang = UI_LANGUAGES.find((l) => l.code === code) || UI_LANGUAGES[0];
  current = lang.code;
  try { localStorage.setItem(LANG_KEY, lang.code); } catch { /* ignore */ }
  emit();

  if (lang.code === "en") {
    stopObserver();
    restoreOriginals(document.getElementById("root") || document.body);
    return { ok: true, restored: true };
  }
  if (translating) return { ok: false, busy: true };
  translating = true;
  try {
    const { applied, error } = await translateWhole(lang.code, lang.label);
    // Only keep observing (and count it a success) if the provider actually produced translations;
    // otherwise restore English so the UI isn't left in a half-state, and report the real error.
    if (error && applied === 0) {
      restoreOriginals(document.getElementById("root") || document.body);
      current = "en";
      try { localStorage.setItem(LANG_KEY, "en"); } catch { /* ignore */ }
      emit();
      return { ok: false, error };
    }
    startObserver(lang.code, lang.label);
    return { ok: true, applied, error };
  } finally {
    translating = false;
  }
}

/** Re-apply the saved language on startup (cached strings paint with no AI call). */
export function initUiLanguage() {
  let saved = "en";
  try { saved = localStorage.getItem(LANG_KEY) || "en"; } catch { /* ignore */ }
  if (saved && saved !== "en") setUiLanguage(saved).catch(() => {});
  else current = "en";
}
