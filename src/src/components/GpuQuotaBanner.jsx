import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { Cpu, X } from "lucide-react";

// One authenticated GET against Kaggle. Not free the way the AI ledger is (that reads a local
// file), so poll slowly — the number only moves while a session is actually burning GPU, and it
// moves at one minute per minute.
const POLL_MS = 300_000;

// Roughly what one video clip costs at the expensive end of the catalogue (Wan 2.2 5B, ~35 min),
// plus room to finish it. Below this, starting a render is a decision rather than a formality.
const LOW_WATER_MINUTES = 45;

/**
 * How much free Kaggle GPU is left this week, said while it can still change a decision.
 *
 * Kaggle gives roughly thirty hours a week, and the app spends it from several places: music
 * (heartmula, acestep), images (ComfyUI/FLUX) and video each start their own session. `idle_guard`
 * exists precisely because that budget is scarce and a forgotten session eats it. Until now the real
 * figure was only ever visible on the Video Gen page, via the advisor — so every other page could
 * spend the quota without showing it.
 *
 * Modelled on AiBudgetBanner, deliberately, including its two limits:
 *
 *   * Silent above the threshold. Thirty hours is not a fact anybody needs on Tuesday.
 *   * Silent when Kaggle is not the answer. No token, no connection, or an unexpected shape all mean
 *     "say nothing" — a user on their own GPU or a rented box has no weekly allowance to run out of.
 *
 * The unit is hours-and-minutes rather than raw minutes, because "3h 20m left" is a decision and
 * "200" is arithmetic homework.
 */
export default function GpuQuotaBanner() {
  const navigate = useNavigate();
  const [quota, setQuota] = useState(null);
  const [dismissed, setDismissed] = useState("");

  useEffect(() => {
    let alive = true;
    const check = async () => {
      try {
        const q = await api.kaggleQuota();
        if (alive) setQuota(q);
      } catch {
        // No token, no network, an older backend. Quota reporting must never itself be noise.
      }
    };
    check();
    const timer = setInterval(check, POLL_MS);
    return () => { alive = false; clearInterval(timer); };
  }, []);

  // `ok:false` is the honest "could not ask" — not zero. Saying nothing is right for both.
  if (!quota?.ok) return null;

  const left = Number(quota.left_minutes ?? 0);
  if (!Number.isFinite(left) || left > LOW_WATER_MINUTES) return null;

  const signature = `${Math.floor(left / 10)}`; // re-show every ten minutes burned, not every minute
  if (dismissed === signature) return null;

  const empty = left <= 0;
  const hours = Math.floor(Math.max(left, 0) / 60);
  const mins = Math.max(left, 0) % 60;
  const pretty = hours > 0 ? `${hours}h ${mins}m` : `${mins}m`;
  const resets = quota.resets_at
    ? new Date(quota.resets_at).toLocaleDateString(undefined, { weekday: "long" })
    : null;

  return (
    <div
      data-testid="gpu-quota-banner"
      className={`flex items-center gap-2 border-b px-4 py-2 text-xs ${
        empty ? "border-red-500/40 bg-red-500/10" : "border-amber-500/40 bg-amber-500/10"
      }`}
    >
      <Cpu className={`h-4 w-4 shrink-0 ${empty ? "text-red-400" : "text-amber-400"}`} />

      <div className="flex-1 min-w-0">
        {empty ? (
          <span>
            <span className="font-medium">This week's free Kaggle GPU is used up</span>
            <span className="text-muted-foreground">
              {resets ? ` — it resets ${resets}.` : "."} Another free account, or your own GPU, keeps
              you going now.
            </span>
          </span>
        ) : (
          <span>
            <span className="font-medium">{pretty} of free GPU left this week</span>
            <span className="text-muted-foreground">
              {" "}— one video clip can take 35 minutes of that, and a session left running spends it
              whether or not anything is rendering.
            </span>
          </span>
        )}
      </div>

      <button
        type="button"
        onClick={() => navigate("/settings")}
        className="shrink-0 rounded border border-border px-2 py-0.5 hover:bg-background/40"
      >
        Add an account
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
