import { api } from "./api";

// Collecting what a page offers, so the AI can write a macro against it.
//
// The AI never sees raw HTML: one modern page is hundreds of kilobytes of framework noise, which
// buries the handful of elements that matter and blows the context window on a single scan. Instead
// this walks the DOM in the page itself and returns a digest — the interactive elements, each with
// the most stable selector available, its visible label and what it accepts.
//
// Selector preference matches what the macro player resolves best: an id, then a test attribute,
// then name/aria, and only then a structural path. A macro is exactly as durable as its selectors.

const DIGEST_JS = `(() => {
  const MAX = 120;
  const vis = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width < 4 || r.height < 4) return false;
    const s = getComputedStyle(el);
    return s.visibility !== 'hidden' && s.display !== 'none' && Number(s.opacity) > 0.05;
  };
  const esc = (v) => (window.CSS && CSS.escape ? CSS.escape(v) : String(v).replace(/[^a-zA-Z0-9_-]/g, '\\\\$&'));
  const selectorFor = (el) => {
    if (el.id) return '#' + esc(el.id);
    for (const attr of ['data-testid', 'data-test', 'data-qa', 'name', 'aria-label']) {
      const v = el.getAttribute(attr);
      if (v) return el.tagName.toLowerCase() + '[' + attr + '=' + JSON.stringify(v) + ']';
    }
    // Structural fallback: tag + nth-of-type chain, capped so it stays readable.
    const parts = [];
    let node = el;
    while (node && node.nodeType === 1 && parts.length < 4 && node !== document.body) {
      const tag = node.tagName.toLowerCase();
      const siblings = node.parentElement ? [...node.parentElement.children].filter((c) => c.tagName === node.tagName) : [];
      parts.unshift(siblings.length > 1 ? tag + ':nth-of-type(' + (siblings.indexOf(node) + 1) + ')' : tag);
      node = node.parentElement;
    }
    return parts.join(' > ');
  };
  const label = (el) => (
    el.getAttribute('aria-label') ||
    el.getAttribute('placeholder') ||
    (el.labels && el.labels[0] && el.labels[0].innerText) ||
    (el.innerText || '').trim().split('\\n')[0] ||
    el.getAttribute('title') || ''
  ).trim().slice(0, 80);

  const nodes = [...document.querySelectorAll(
    'a[href], button, input, textarea, select, [role=button], [role=textbox], [contenteditable=true]'
  )].filter(vis).slice(0, MAX);

  return {
    url: location.href,
    title: document.title,
    elements: nodes.map((el) => ({
      selector: selectorFor(el),
      label: label(el),
      role: el.getAttribute('role') || el.tagName.toLowerCase(),
      type: el.getAttribute('type') || undefined,
      accepts_text: ['input', 'textarea'].includes(el.tagName.toLowerCase()) || el.isContentEditable || undefined,
      href: el.tagName.toLowerCase() === 'a' ? (el.getAttribute('href') || '').slice(0, 120) : undefined,
    })),
  };
})()`;

/**
 * Scan the page currently open in the app's browser and hand the digest to the macro author.
 * `pageId` targets a specific browser tab; omitted means the active one.
 */
export async function scanPageForMacro(pageId) {
  const key = `macro-digest:${Date.now().toString(36)}`;
  // The webview bridge posts results back by key rather than returning them — see the CSP note in
  // the embedded-browser code; a plain fetch() would be blocked on strict-CSP sites.
  await api.webviewEval(
    `(() => { const d = ${DIGEST_JS}; window.bmChannel && window.bmChannel.postMessage(JSON.stringify({ key: ${JSON.stringify(key)}, data: d })); })()`,
    pageId,
  );
  // Give the bridge a moment, then collect.
  for (let i = 0; i < 20; i++) {
    // eslint-disable-next-line no-await-in-loop
    await new Promise((r) => setTimeout(r, 150));
    // eslint-disable-next-line no-await-in-loop
    const scraped = await api.webviewTakeScrape(key).catch(() => null);
    if (scraped) return typeof scraped === "string" ? JSON.parse(scraped) : scraped;
  }
  return null;
}

/** Scan the open page, then have the AI write a macro for `goal` against it. */
export async function authorMacroForOpenPage({ goal, pageId, name }) {
  const page = await scanPageForMacro(pageId);
  if (!page?.elements?.length) {
    throw new Error("Could not read the page — open it in the app's browser and let it finish loading first.");
  }
  return api.authorMacro({ goal, url: page.url, page, name: name || null });
}
