// ─────────────────────────────────────────────────────────────────────────────
// The rules behind a guided view, as pure functions.
//
// A guided page is not a form and not a wizard: sections *manifest* as the guide settles them, and
// only the newest two stay expanded so the page grows without ever becoming the wall of controls it
// was. Each manifested section carries how well it is settled — a traffic light, from "a template
// guessed this" to "you tuned it by hand" — so a user returning tomorrow can see at a glance what
// still deserves attention.
//
// Kept free of React and of any page so the rules can be tested directly
// (see tests/guided-view.test.mjs).
// ─────────────────────────────────────────────────────────────────────────────

/** How settled a section is. Ordered: later beats earlier when two sources disagree. */
export const STATUS_ORDER = ["untouched", "crude", "common", "tuned", "fine"];

export const STATUS_META = {
  untouched: { label: "not set", color: "muted", hint: "The page's own defaults — nothing has decided this yet." },
  crude: { label: "rough", color: "red", hint: "Filled bluntly so the run can proceed. Worth a look." },
  common: { label: "sensible", color: "amber", hint: "A template set this from your brief and channels." },
  tuned: { label: "chosen", color: "green", hint: "You answered this in the guide." },
  fine: { label: "fine-tuned", color: "blue", hint: "You edited these controls by hand." },
};

/** Never downgrade a section: a hand edit outranks a template that later re-runs. */
export function mergeStatus(current, next) {
  const a = STATUS_ORDER.indexOf(current || "untouched");
  const b = STATUS_ORDER.indexOf(next || "untouched");
  return STATUS_ORDER[Math.max(a === -1 ? 0 : a, b === -1 ? 0 : b)];
}

/**
 * Which sections are visible, and which of those are expanded.
 *
 * `order` is the page's own section order; `manifested` the ids the guide has settled so far, in the
 * order they were settled. The rule the user asked for: as soon as a second-next section appears,
 * everything but the previous one collapses — so at most the two most recent stay open, plus whatever
 * the user pinned open by hand.
 */
export function visibility({ order = [], manifested = [], pinned = {}, showAll = false }) {
  if (showAll) {
    return Object.fromEntries(order.map((id) => [id, { visible: true, open: pinned[id] !== false }]));
  }
  const shown = order.filter((id) => manifested.includes(id));
  // Expanded set: the last two in *manifestation* order, which is what the user watched happen.
  const recent = manifested.filter((id) => order.includes(id)).slice(-2);
  return Object.fromEntries(order.map((id) => {
    const visible = shown.includes(id);
    const open = visible && (pinned[id] === true || (pinned[id] !== false && recent.includes(id)));
    return [id, { visible, open }];
  }));
}

/**
 * The next step to ask, given what a template already settled.
 *
 * A template's whole purpose is to remove questions, so steps it covers are skipped unless the user
 * reopens them. `openQuestions` is the template's own ranking of what still matters; anything not
 * ranked keeps the flow's original order behind it.
 */
export function remainingSteps({ steps = [], answers = {}, coveredSections = [], openQuestions = [] }) {
  const ranked = openQuestions.filter((id) => steps.some((s) => s.id === id));
  const isCovered = (s) => s.reveals && coveredSections.includes(s.reveals);
  const rest = steps
    .filter((s) => !ranked.includes(s.id))
    .filter((s) => !isCovered(s) || answers[s.id] !== undefined)
    .map((s) => s.id);
  const ordered = [...ranked, ...rest];
  return ordered
    .map((id) => steps.find((s) => s.id === id))
    .filter(Boolean);
}

/** Where to resume: the first unanswered step, or the end. */
export function resumeIndex(steps = [], answers = {}) {
  const i = steps.findIndex((s) => answers[s.id] === undefined);
  return i === -1 ? steps.length : i;
}

/** One-line summary for the resume bar. */
export function progressSummary({ steps = [], answers = {}, sections = {} }) {
  const answered = steps.filter((s) => answers[s.id] !== undefined).length;
  const statuses = Object.values(sections).map((s) => s?.status).filter(Boolean);
  const rough = statuses.filter((s) => s === "crude").length;
  const parts = [`${answered}/${steps.length} answered`];
  if (rough) parts.push(`${rough} rough`);
  return parts.join(" · ");
}
