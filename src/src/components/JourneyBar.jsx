import { useCallback, useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { Button } from "./ui/button";
import { Compass, ArrowRight, Check, X, Loader2 } from "lucide-react";

// ─────────────────────────────────────────────────────────────────────────────
// Where you are in the whole thing.
//
// Fourteen guided flows cover fourteen pages, and each begins and ends inside its own page — so a
// person is guided *within* whichever of thirty-five doors they open, and guided nowhere about which
// door that should be. This is the thin strip that carries them between: nine stops, which one is
// current, why it comes here, and one button to the next.
//
// It sits in the Shell above the page rather than inside any page, because a journey that unmounted
// on navigation would be a journey that could not survive being followed.
//
// Three rules keep it from being a wizard:
//   * Doneness is computed on the backend from what the project contains, so this never has to
//     remember anything and is correct after a month away.
//   * It is off by default and closes for good on one click. The sidebar still has everything.
//   * On the page for the current stop it says what this stop is for; anywhere else it says where
//     you are and offers the way back — never a block, never a redirect.
// ─────────────────────────────────────────────────────────────────────────────

const HIDDEN_KEY = "bm.journey.off";

export default function JourneyBar({ projectId }) {
  const [journey, setJourney] = useState(null);
  const [dismissed, setDismissed] = useState(() => {
    try { return localStorage.getItem(HIDDEN_KEY) === "1"; } catch { return false; }
  });
  const [busy, setBusy] = useState(false);
  const loc = useLocation();
  const nav = useNavigate();

  const load = useCallback(async () => {
    if (!projectId || dismissed) return;
    setBusy(true);
    try { setJourney(await api.projectJourney(projectId)); }
    catch { setJourney(null); }
    finally { setBusy(false); }
  }, [projectId, dismissed]);

  // Reloaded on every navigation: the stops are computed from the project's contents, so arriving
  // back from a page where something was made is exactly when the answer changes.
  useEffect(() => { load(); }, [load, loc.pathname]);

  const close = () => {
    setDismissed(true);
    try { localStorage.setItem(HIDDEN_KEY, "1"); } catch { /* a private window still closes it */ }
  };

  if (dismissed || !journey || journey.finished) return null;
  const stops = journey.stops || [];
  const current = stops[journey.current];
  if (!current) return null;

  const here = loc.pathname === current.route;
  const step = (journey.current ?? 0) + 1;

  return (
    <div data-testid="journey-bar"
         className="shrink-0 border-b border-border/60 bg-primary/[0.04] px-3 sm:px-4 py-1.5
                    flex items-center gap-2 sm:gap-3 text-[11px]">
      <Compass className="w-3.5 h-3.5 text-primary shrink-0" />

      {/* The dots are the map: nine of them, filled for done, ringed for where you are. Small
          enough to ignore and enough to answer "how much of this is there". */}
      <div className="hidden sm:flex items-center gap-1 shrink-0">
        {stops.map((s, i) => (
          <button key={s.id} onClick={() => nav(s.route)} title={s.label}
                  className={`w-1.5 h-1.5 rounded-full transition-all
                    ${s.done ? "bg-primary/70"
                      : i === journey.current ? "bg-primary ring-2 ring-primary/30 scale-125"
                      : "bg-muted-foreground/30"}`} />
        ))}
      </div>

      <div className="min-w-0 flex-1 flex items-baseline gap-1.5 flex-wrap">
        <span className="text-mono text-muted-foreground shrink-0">
          {step}/{journey.total}
        </span>
        <span className="font-medium">{current.label}</span>
        {current.outstanding > 0 && (
          <span className="text-muted-foreground">
            <span className="text-mono">{current.outstanding}</span>
            <span> left</span>
          </span>
        )}
        {/* The reason, but only where it is the thing you are looking at. Elsewhere it would be a
            paragraph about a page you are not on. */}
        {here && <span className="text-muted-foreground truncate hidden md:inline">{current.why}</span>}
      </div>

      {busy && <Loader2 className="w-3 h-3 animate-spin text-muted-foreground shrink-0" />}

      {!here && (
        <Button size="sm" variant="ghost" className="h-6 text-[11px] px-2 shrink-0"
                onClick={() => nav(current.route)}>
          Take me there<ArrowRight className="w-3 h-3 ml-1" />
        </Button>
      )}
      {here && journey.done > 0 && (
        <span className="text-muted-foreground shrink-0 hidden sm:flex items-center gap-1">
          <Check className="w-3 h-3 text-primary" />
          <span className="text-mono">{journey.done}</span>
          <span>done</span>
        </span>
      )}

      <button onClick={close} title="Hide the journey — the sidebar still has everything"
              className="p-1 rounded hover:bg-muted/50 text-muted-foreground shrink-0">
        <X className="w-3 h-3" />
      </button>
    </div>
  );
}
