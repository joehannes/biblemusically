import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { Zap, X } from "lucide-react";

// Cheap to ask — it reads a small local file, not a provider — but there is no point asking often
// either, since the number only moves when the app itself spends a request.
const POLL_MS = 120_000;

/**
 * How much free AI is left today, said while it can still change a decision.
 *
 * OpenRouter's free tier is fifty requests a day for the whole account. Running out is not a soft
 * degradation: every remaining call fails, so a workflow started with four requests left dies
 * half-finished, and the half that finished is the cheap half. The ledger in ai_budget.rs has known
 * the real number for a while; nothing showed it.
 *
 * Two deliberate limits, both about not becoming wallpaper:
 *
 *   * Silent above the threshold. A counter that is always on screen is a counter nobody reads, and
 *     "38 of 50 remaining" is not a fact anybody needs at request twelve.
 *   * Silent when nothing is rationed. A user on billed keys has no allowance to run out of, only a
 *     bill, and that is their business rather than a banner's.
 *
 * The unit is songs, not requests, because "two songs left today" is a decision and "9 requests
 * left" is arithmetic homework.
 */
export default function AiBudgetBanner() {
  const navigate = useNavigate();
  const [budget, setBudget] = useState(null);
  const [dismissed, setDismissed] = useState("");

  useEffect(() => {
    let alive = true;
    const check = async () => {
      try {
        const b = await api.aiBudgetReport();
        if (alive) setBudget(b);
      } catch {
        // An older backend, or no settings yet. Budget reporting must never itself be noise.
      }
    };
    check();
    const timer = setInterval(check, POLL_MS);
    return () => { alive = false; clearInterval(timer); };
  }, []);

  if (!budget) return null;

  const rationed = (budget.providers || []).filter((p) => p.configured && p.daily_cap != null);
  if (rationed.length === 0) return null;      // nothing on a free tier — nothing to warn about

  const left = Number(budget.free_remaining ?? 0);
  const songs = Number(budget.songs_left_today ?? 0);
  const perSong = Number(budget?.cost?.per_song_core ?? 4);

  // Quiet until it matters: roughly two songs' worth. Far enough ahead to finish what is running and
  // decide about the next one.
  if (left > perSong * 2) return null;

  // A new threshold re-shows a dismissed banner; dismissing the same one twice is not needed.
  const signature = `${left}`;
  if (dismissed === signature) return null;

  const empty = left <= 0;
  const spent = rationed.filter((p) => p.remaining === 0).map((p) => p.id);

  return (
    <div
      data-testid="ai-budget-banner"
      className={`flex items-center gap-2 border-b px-4 py-2 text-xs ${
        empty ? "border-red-500/40 bg-red-500/10" : "border-amber-500/40 bg-amber-500/10"
      }`}
    >
      <Zap className={`h-4 w-4 shrink-0 ${empty ? "text-red-400" : "text-amber-400"}`} />

      <div className="flex-1 min-w-0">
        {empty ? (
          <span>
            <span className="font-medium">Today's free AI allowance is used up</span>
            {spent.length > 0 && <span className="text-muted-foreground"> on {spent.join(", ")}</span>}
            <span className="text-muted-foreground">
              {" "}— it resets at midnight UTC. Another provider's key keeps you going now.
            </span>
          </span>
        ) : (
          <span>
            <span className="font-medium">
              {left} free AI {left === 1 ? "request" : "requests"} left today
            </span>
            <span className="text-muted-foreground">
              {" "}— about {songs === 0 ? "less than one song" : songs === 1 ? "one more song" : `${songs} more songs`},
              at roughly {perSong} a song. Starting a run you cannot finish wastes the requests it does spend.
            </span>
          </span>
        )}
      </div>

      <button
        type="button"
        onClick={() => navigate("/settings")}
        className="shrink-0 rounded border border-border px-2 py-0.5 hover:bg-background/40"
      >
        Add a key
      </button>
      <button
        type="button"
        onClick={() => setDismissed(signature)}
        title="Dismiss until it changes"
        className="shrink-0 rounded p-1 text-muted-foreground hover:text-foreground"
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
