// The two decisions that keep runtime GUI translation from leaking requests, kept free of any DOM or
// Tauri import so they can be tested directly (see tests/ui-translate-rules.test.mjs).
//
// Both exist because of a real incident: ten minutes of clicking around in German exhausted
// OpenRouter's 50-requests-per-day free allowance, and labels flickered between German and English
// the whole time. The cause was one rule missing on each side — which strings may be sent, and what
// counts as a node's untranslated text.

/**
 * May this string be sent to the translation API?
 *
 * `inventory` is the set of literal strings extracted from the source. Being in it is the ordinary
 * answer. Everything else is treated as runtime data — a count, a timestamp, a model id, a channel
 * name, an API error — because those never repeat identically, so each new value would cost another
 * request. A conservative allowance is made for label-like strings the extractor missed, bounded by
 * `spent < budget` so even that cannot run away.
 */
export function mayTranslate(s, inventory, spent = 0, budget = 0) {
  if (typeof s !== "string" || s.length < 2) return false;
  if (inventory.has(s)) return true;
  if (/\d/.test(s)) return false;                        // counts, times, versions, ids
  if (s.length > 80) return false;
  if (/[<>{}|\\]|https?:|@|\/\//.test(s)) return false;   // markup, urls, emails
  if (s.split(/\s+/).length > 12) return false;
  return spent < budget;
}

/**
 * The untranslated text of a node, re-baselined when something other than us wrote to it.
 *
 * React reuses text nodes: the node that held "Save" can be handed "Saved" on the next render.
 * Treating the first value seen as the original forever meant re-applying the translation of text
 * that was no longer there — React wrote English, we wrote back a stale translation, React wrote
 * English again. That ping-pong was the visible flicker. Mutates and returns the baseline.
 *
 * `node` only needs `nodeValue`, `__i18nOriginal` and `__i18nApplied`, so a plain object works.
 */
export function baselineOriginal(node) {
  const v = node.nodeValue;
  if (node.__i18nApplied !== undefined && v === node.__i18nApplied) return node.__i18nOriginal;
  if (node.__i18nOriginal === undefined || v !== node.__i18nOriginal) {
    node.__i18nOriginal = v;
    node.__i18nApplied = undefined;
  }
  return node.__i18nOriginal;
}
