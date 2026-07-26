// The ordering rule for the top bar's actions. Pure, and in its own file so it can be tested without a
// JSX loader — it decides where buttons sit, which is worth testing.
//
// Order is by `priority`, then by id. Never by the order things registered: a section-scoped action
// appears when its section scrolls into view, and if that reshuffled the bar, the button someone was
// about to click would move out from under the pointer.

/** How many actions the bar shows before the rest go into the overflow menu. */
export const MAX_VISIBLE = 4;

/** Lower priority numbers sit further left, which is where a primary action belongs. */
export function sortActions(list) {
  return [...list].sort((a, b) => {
    const pa = Number.isFinite(a.priority) ? a.priority : 50;
    const pb = Number.isFinite(b.priority) ? b.priority : 50;
    if (pa !== pb) return pa - pb;
    return String(a.id).localeCompare(String(b.id));
  });
}

/** Split into what the bar shows and what goes into the overflow menu. Nothing is ever dropped. */
export function splitActions(list, max = MAX_VISIBLE) {
  const sorted = sortActions(list);
  return { visible: sorted.slice(0, max), overflow: sorted.slice(max) };
}
