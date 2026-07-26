// Lets a page publish its own controls into the shared top bar instead of burying them in the page
// body. Shell owns the state and renders what is published; this carries the registry down through
// context so any component nested under Shell can publish.
//
// Three ways to publish, because pages have three kinds of action:
//
//   • `usePageActions` — arbitrary JSX, for bespoke controls that are not a button. The original.
//   • `useBarAction` — the action belongs to the whole view (Save config, Run all, Refresh). It stays in
//     the bar as long as the view is mounted.
//   • `useSectionAction` — the action belongs to ONE section (the final Generate under a specific
//     control). It appears only while that section is actually in the viewport and disappears when you
//     scroll past it. A Generate button in the bar while the thing it generates is off screen is a
//     button whose effect you cannot see.
//
// Ordering is deterministic — by `priority`, then by id — never by the order things happened to
// register. Sorting by registration time means a button slides sideways when a neighbour scrolls into
// view, which makes the bar feel unstable and gets things mis-clicked. Beyond `MAX_VISIBLE` the rest
// collapse into an overflow menu rather than growing the bar without limit.
import { createContext, useContext, useEffect, useMemo, useRef, useState } from "react";
// The ordering rule lives in a plain .js file so it can be unit-tested without a JSX loader.
import { sortActions, splitActions, MAX_VISIBLE } from "./actionOrder";

export { sortActions, splitActions, MAX_VISIBLE };

export const PageActionsContext = createContext(null);
export const ActionRegistryContext = createContext(null);

/**
 * Publish arbitrary JSX into the top bar.
 *
 * Memoise `node` if it has real children, or this re-publishes on every render.
 */
export function usePageActions(node) {
  const setPageActions = useContext(PageActionsContext);
  useEffect(() => {
    if (!setPageActions) return;
    setPageActions(node ?? null);
    return () => setPageActions(null);
  }, [setPageActions, node]);
}

/**
 * Publish one view-wide action into the bar.
 *
 * `action`: `{ id, label, icon, onClick, variant, priority, disabled, title }`. `id` must be stable
 * across renders — it is the identity used for ordering and for replacing the entry when a callback
 * changes.
 */
export function useBarAction(action) {
  const registry = useContext(ActionRegistryContext);
  const id = action?.id;
  // The latest action lives in a ref so a changing onClick does not churn the registry, while the
  // button still calls the current version.
  const latest = useRef(action);
  latest.current = action;

  useEffect(() => {
    if (!registry || !id) return;
    registry.register({ ...latest.current, id, _live: latest });
    return () => registry.unregister(id);
    // Everything the button needs comes from `latest`, so only identity and appearance re-register.
  }, [registry, id, action?.disabled, action?.label]);
}

/**
 * Publish an action that only exists while its section is on screen.
 *
 * Returns a ref to put on the section element. Uses an IntersectionObserver rather than scroll maths, so
 * it costs nothing while nothing moves.
 *
 * `threshold` is how much of the section must be visible: the default 0.15 means a section merely
 * peeking in does not yet claim a slot in the bar.
 */
export function useSectionAction(action, threshold = 0.15) {
  const registry = useContext(ActionRegistryContext);
  const ref = useRef(null);
  const [visible, setVisible] = useState(false);
  const latest = useRef(action);
  latest.current = action;
  const id = action?.id;

  useEffect(() => {
    const el = ref.current;
    if (!el || typeof IntersectionObserver === "undefined") {
      // No observer, or no element yet: show the action rather than hiding it. A missing button is
      // worse than an early one.
      setVisible(true);
      return;
    }
    const obs = new IntersectionObserver(
      (entries) => setVisible(entries.some((en) => en.isIntersecting)),
      { threshold },
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, [threshold]);

  useEffect(() => {
    if (!registry || !id) return;
    if (!visible) { registry.unregister(id); return; }
    registry.register({ ...latest.current, id, _live: latest });
    return () => registry.unregister(id);
  }, [registry, id, visible, action?.disabled, action?.label]);

  return ref;
}

/** The registry Shell holds. Lives here so the ordering rule sits next to the hooks that feed it. */
export function useActionRegistry() {
  const [actions, setActions] = useState({});

  const registry = useMemo(() => ({
    register: (action) => setActions((prev) => {
      const existing = prev[action.id];
      // Re-registering the same shape must not cause a state update, or a component that re-renders on
      // every keystroke re-renders the whole Shell with it.
      if (existing && existing.label === action.label && existing.disabled === action.disabled
          && existing.priority === action.priority) {
        existing._live = action._live;
        return prev;
      }
      return { ...prev, [action.id]: action };
    }),
    unregister: (id) => setActions((prev) => {
      if (!(id in prev)) return prev;
      const next = { ...prev };
      delete next[id];
      return next;
    }),
  }), []);

  const ordered = useMemo(() => sortActions(Object.values(actions)), [actions]);
  return { registry, ordered };
}
