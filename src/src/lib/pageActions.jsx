// Lets a page publish its own "page-wide" controls (a final Generate/Render/Publish button,
// a Save-config button, etc.) into the shared top bar instead of burying them in the page body.
// Shell owns the actual state + renders whatever's published; this just carries the setter down
// through context so any page component nested under Shell can call usePageActions(...).
import { createContext, useContext, useEffect } from "react";

export const PageActionsContext = createContext(null);

// Call once per page with the JSX for that page's top-bar controls (or null/undefined for
// pages with nothing to publish). Clears itself on unmount so a stale page's buttons don't
// linger after navigating away. Memoize `node` (useMemo) if it has real children so this
// doesn't re-publish on every render — harmless either way, just wasted work.
export function usePageActions(node) {
  const setPageActions = useContext(PageActionsContext);
  useEffect(() => {
    if (!setPageActions) return;
    setPageActions(node ?? null);
    return () => setPageActions(null);
  }, [setPageActions, node]);
}
