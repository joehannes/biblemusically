import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { Button } from "./ui/button";
import { toast } from "sonner";
import { Download, ArrowUpCircle, Loader2, X } from "lucide-react";

/**
 * A release landed — say so wherever the user happens to be.
 *
 * The whole update chain already existed before this component: the backend compares versions and
 * decides update-vs-upgrade (commands/updater.rs), and the Worker reads the GitHub releases page and
 * ignores drafts. What was missing was anywhere to *see* it. The only caller was the Account page,
 * checked once when that page mounted — so an update announced itself exclusively to people who had
 * already gone looking for it, which is the set of people who least needed telling.
 *
 * Three things are deliberate:
 *
 *   * It follows the backend's own cadence rather than inventing one. `check_update` returns
 *     `interval_minutes` (15), and the answer is cheap — the Worker caches GitHub's reply for ten
 *     minutes, so a hundred installs are one API call, not a hundred.
 *   * `kind: "blocked"` renders nothing. That is the trial case, and a banner reading "updates need a
 *     subscription" on every screen is nagging, not news. The Account page says it once, in the place
 *     where somebody can act on it.
 *   * Dismissing an *update* hides that version until the next one (or a restart); dismissing an
 *     *upgrade* is permanent and goes through `dismiss_upgrade`, because a major version is a separate
 *     purchase and somebody who said no has said no.
 */
export default function UpdateBanner() {
  const navigate = useNavigate();
  const [update, setUpdate] = useState(null);
  const [hidden, setHidden] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    let timer = null;
    const check = async () => {
      try {
        const u = await api.checkUpdate();
        if (!alive) return;
        setUpdate(u);
        // Follow the backend's stated interval rather than hardcoding one here, so changing the
        // policy is a one-line change in updater.rs rather than a hunt through the frontend.
        const mins = Number(u?.interval_minutes) || 15;
        timer = setTimeout(check, mins * 60_000);
      } catch {
        // Offline, or not signed in. An update check must never be a source of noise — try later.
        if (alive) timer = setTimeout(check, 15 * 60_000);
      }
    };
    check();
    return () => { alive = false; if (timer) clearTimeout(timer); };
  }, []);

  const kind = update?.kind;
  if (!update || kind === "none" || kind === "blocked") return null;
  if (hidden === update.latest) return null;

  const isUpgrade = kind === "upgrade";

  const download = async () => {
    setBusy(true);
    try {
      const r = await api.downloadUpdate();
      toast.success(`Downloaded to ${r.path}`, { duration: 12000 });
      toast.message(r.next, { duration: 14000 });
    } catch (err) {
      toast.error(`${err}`, { duration: 10000 });
    } finally {
      setBusy(false);
    }
  };

  const dismiss = async () => {
    if (!isUpgrade) { setHidden(update.latest); return; }
    try {
      await api.dismissUpgrade(Number(String(update.latest).split(".")[0]) || 1);
      setHidden(update.latest);
      toast.success("Won't mention it again.");
    } catch {
      setHidden(update.latest);
    }
  };

  return (
    <div
      data-testid="update-banner"
      className="flex items-center gap-2 border-b border-primary/40 bg-primary/10 px-4 py-2 text-xs"
    >
      {isUpgrade
        ? <ArrowUpCircle className="h-4 w-4 shrink-0 text-primary" />
        : <Download className="h-4 w-4 shrink-0 text-primary" />}

      <div className="flex-1 min-w-0">
        <span className="font-medium">
          {isUpgrade ? `Version ${update.latest} is out` : `Version ${update.latest} is ready`}
        </span>
        <span className="text-muted-foreground"> — {update.message}</span>
      </div>

      {!isUpgrade && (
        <Button size="sm" className="h-7 shrink-0" disabled={busy} onClick={download}>
          {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : null}
          Update
        </Button>
      )}
      {isUpgrade && (
        <Button size="sm" className="h-7 shrink-0" onClick={() => navigate("/account")}>
          See what is new
        </Button>
      )}

      <button
        type="button"
        onClick={dismiss}
        title={isUpgrade ? "Don't show again" : "Not now"}
        className="shrink-0 rounded p-1 text-muted-foreground hover:text-foreground"
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
