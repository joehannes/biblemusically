import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { AlertTriangle, X, Clock } from "lucide-react";

// How often to re-ask the backend. The backend's own validator runs every 15 minutes and writes
// its verdict to settings; this only reads that verdict, so polling is cheap.
const POLL_MS = 60_000;

/**
 * A persistent warning when the engines this project actually uses can't run.
 *
 * The backend has always tracked cookie/profile/endpoint health, but it was only visible if you
 * opened Settings and pressed "Test" — so an overnight cookie expiry took a whole batch of queued
 * jobs down silently (TODOS.md #16). Two deliberate limits keep this from becoming wallpaper:
 * it only reports the engines currently selected (a stale Midjourney profile is irrelevant when
 * you generate with ComfyUI), and dismissing it hides that exact set of problems until they
 * change or the app restarts.
 */
export default function HealthBanner() {
  const navigate = useNavigate();
  const [health, setHealth] = useState(null);
  const [dismissed, setDismissed] = useState("");

  useEffect(() => {
    let alive = true;
    const check = async () => {
      try {
        const h = await api.serviceHealth();
        if (alive) setHealth(h);
      } catch {
        // Health reporting must never itself become a source of noise.
      }
    };
    check();
    const timer = setInterval(check, POLL_MS);
    return () => { alive = false; clearInterval(timer); };
  }, []);

  const blocking = health?.blocking || [];
  const aiMissing = health?.ai && !health.ai.configured;
  const problems = [...blocking, ...(aiMissing ? [health.ai] : [])];
  if (problems.length === 0) return null;

  // Identity of the current problem set — a new problem re-shows a dismissed banner.
  const signature = problems.map((p) => `${p.id}:${p.status || "unset"}`).join("|");
  if (dismissed === signature) return null;

  const stale = problems.some((p) => p.stale && p.checked_minutes_ago != null);

  return (
    <div
      data-testid="health-banner"
      className="flex items-start gap-2 border-b border-amber-500/40 bg-amber-500/10 px-4 py-2 text-xs text-amber-200"
    >
      <AlertTriangle className="h-4 w-4 mt-0.5 shrink-0 text-amber-400" />
      <div className="flex-1 min-w-0 space-y-0.5">
        {problems.map((p) => (
          <div key={p.id} className="truncate">
            <span className="font-medium">{p.label}:</span> {p.explanation}
            {p.checked_minutes_ago != null && (
              <span className="ml-1 opacity-70">
                (checked {p.checked_minutes_ago < 1 ? "just now" : `${p.checked_minutes_ago} min ago`})
              </span>
            )}
          </div>
        ))}
        {stale && (
          <div className="flex items-center gap-1 opacity-70">
            <Clock className="h-3 w-3" /> This status may be out of date — open Settings to re-test.
          </div>
        )}
        {health?.fallback_engine && health.fallback_engine !== "none" && (
          <div className="opacity-70">Music will fall back to {health.fallback_engine}.</div>
        )}
      </div>
      <button
        onClick={() => navigate("/settings")}
        className="shrink-0 rounded bg-amber-500/20 px-2 py-1 hover:bg-amber-500/30 transition-colors"
      >
        Fix in Settings
      </button>
      <button
        onClick={() => setDismissed(signature)}
        className="shrink-0 rounded p-1 hover:bg-amber-500/20 transition-colors"
        aria-label="Dismiss"
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
