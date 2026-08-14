// Finding, revealing and measuring the thing a guide is talking about.
//
// The welcome tour could already spotlight, but only ever a sidebar link: `locate` looked for
// `[data-tour-nav="<route>"]` and nothing else. So a guide could say "open Settings" and point at
// the word Settings, and then had no way at all to point at the field it actually wanted — which is
// the sentence that needed the help. Everything past the first click was prose.
//
// Nothing new has to be annotated to fix that. The app already carries 198 distinct `data-testid`
// attributes on its real controls, put there for the tests, and they are exactly the stable handles
// a guide needs. So a target may be given as a testid, a route, or a raw selector, and the same
// spotlight machinery works for all three.
//
// The other half is scrolling. A nav link is always on screen; a settings field can be three
// thousand pixels down a page that has just been navigated to, and spotlighting an element nobody
// can see is worse than not spotlighting at all — the veil dims the window and highlights nothing.

/** Milliseconds to let a freshly navigated route render before measuring anything. */
export const SETTLE_MS = 280;
/** Breathing room drawn around a spotlit element. */
export const SPOTLIGHT_PAD = 8;

/**
 * Resolve a guide target to a visible element.
 *
 * `target` may be:
 *   - `{ testid: "settings-heartmula-url" }` — the usual case, a real control
 *   - `{ route: "/settings" }` — a nav item, what the welcome tour uses
 *   - `{ selector: "#some-node" }` — an escape hatch
 *   - a bare string, tried as a testid, then a route, then a selector
 */
export function locateTarget(target) {
  if (typeof document === "undefined" || !target) return null;

  const selectors = [];
  if (typeof target === "string") {
    selectors.push(`[data-testid="${CSS.escape(target)}"]`,
                   `[data-tour-nav="${CSS.escape(target)}"]`,
                   target);
  } else {
    if (target.testid) selectors.push(`[data-testid="${CSS.escape(target.testid)}"]`);
    if (target.route) selectors.push(`[data-tour-nav="${CSS.escape(target.route)}"]`);
    if (target.selector) selectors.push(target.selector);
  }

  for (const sel of selectors) {
    let nodes = [];
    try {
      nodes = Array.from(document.querySelectorAll(sel));
    } catch {
      continue; // a hand-written selector is allowed to be wrong without taking the guide down
    }
    for (const el of nodes) {
      const r = el.getBoundingClientRect();
      // The desktop sidebar and the mobile bar both exist in the DOM; only one has a size. The same
      // is true of any panel behind a closed accordion, which must not be spotlit either.
      if (r.width > 0 && r.height > 0) return el;
    }
  }
  return null;
}

/** Measure an element in viewport coordinates, which is what a `position: fixed` veil needs. */
export function rectOf(el) {
  if (!el) return null;
  const r = el.getBoundingClientRect();
  return { top: r.top, left: r.left, width: r.width, height: r.height };
}

/**
 * Bring a target into view, and resolve once it has stopped moving.
 *
 * Resolving on `scrollend` where it exists, and on a short settle everywhere else: measuring during
 * a smooth scroll produces a rectangle that is already stale by the time it is painted, which reads
 * as the spotlight lagging behind the page.
 */
export function revealTarget(el, { behavior = "smooth" } = {}) {
  return new Promise((resolve) => {
    if (!el) return resolve(null);
    const r = el.getBoundingClientRect();
    const fullyVisible = r.top >= 0 && r.bottom <= window.innerHeight;
    if (fullyVisible) return resolve(el);

    let done = false;
    const finish = () => { if (!done) { done = true; resolve(el); } };
    try {
      el.scrollIntoView({ behavior, block: "center", inline: "nearest" });
    } catch {
      el.scrollIntoView();
    }
    if ("onscrollend" in window) {
      window.addEventListener("scrollend", finish, { once: true });
      setTimeout(finish, 900); // scrollend never fires if nothing actually scrolled
    } else {
      setTimeout(finish, 420);
    }
  });
}

/**
 * The four veil panels that surround a spotlight.
 *
 * Four panels rather than one with a hole punched in it, because `backdrop-filter` cannot be
 * masked: a hole cut with a border-radius still blurs whatever is behind it, so the "revealed"
 * element comes out blurred — the one thing the spotlight exists to keep sharp.
 */
export function veilPanels(rect, pad = SPOTLIGHT_PAD) {
  if (!rect) return [{ key: "all", style: { inset: 0 } }];
  return [
    { key: "t", style: { top: 0, left: 0, right: 0, height: Math.max(0, rect.top - pad) } },
    { key: "b", style: { top: rect.top + rect.height + pad, left: 0, right: 0, bottom: 0 } },
    { key: "l", style: { top: Math.max(0, rect.top - pad), left: 0, width: Math.max(0, rect.left - pad), height: rect.height + pad * 2 } },
    { key: "r", style: { top: Math.max(0, rect.top - pad), left: rect.left + rect.width + pad, right: 0, height: rect.height + pad * 2 } },
  ];
}
