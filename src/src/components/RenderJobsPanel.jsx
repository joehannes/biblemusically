import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { RefreshCw, Loader2, ExternalLink } from "lucide-react";
import { toast } from "sonner";
import { openUrl } from "@tauri-apps/plugin-opener";

// What the remote renders actually did.
//
// Submitting has worked for a while — pick a provider here, hand a song off from the Composer — and
// then nothing ever asked for the answer, so a job read "running" forever however it had really
// ended. The worker was holding up its end the whole time: `render_worker.py` prints one BM_RESULT
// line. A Kaggle notebook simply has no route back to this machine, so that line sits in the run's
// log until something goes and looks. `reconcile_render_jobs` is that something.
//
// Placed under the provider picker rather than on its own page: the question "did it work?" is asked
// in the same breath as "where does it run?", and a third place to check is a place to forget.

const STATUS_STYLE = {
  uploaded: "text-emerald-500 border-emerald-500/40",
  rendered: "text-emerald-500 border-emerald-500/40",
  running: "text-amber-400 border-amber-500/40",
  failed: "text-red-400 border-red-500/40",
};

const when = (iso) => {
  if (!iso) return "";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "" : d.toLocaleString();
};

export default function RenderJobsPanel() {
  const [jobs, setJobs] = useState(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try { setJobs(await api.listRenderJobs(15)); } catch { setJobs([]); }
  }, []);
  useEffect(() => { load(); }, [load]);

  const refresh = async () => {
    setBusy(true);
    try {
      const r = await api.reconcileRenderJobs();
      await load();
      if (r?.updated) toast.success(`${r.updated} job(s) finished since you last looked.`);
      else if (r?.still_running) toast.message(`${r.still_running} still working.`);
      else if (r?.unsupported) toast.message(
        `${r.unsupported} job(s) can't be checked from here — GitHub Actions leaves its result in an artifact, and a self-hosted worker posts back on its own.`,
        { duration: 10000 });
      else toast.message("Nothing outstanding.");
    } catch (e) {
      toast.error(`Could not check: ${e?.message || e}`);
    } finally { setBusy(false); }
  };

  if (!jobs?.length) return null; // nothing submitted yet — no empty shelf

  return (
    <div className="mt-4 pt-4 border-t border-border/60 space-y-2">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <h3 className="text-sm font-medium">Recent remote renders</h3>
        <Button size="sm" variant="outline" className="h-7 text-[11px]" disabled={busy} onClick={refresh}>
          {busy ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" /> : <RefreshCw className="w-3.5 h-3.5 mr-1.5" />}
          Check for results
        </Button>
      </div>
      <div className="space-y-1">
        {jobs.map((j) => (
          <div key={j.id} className="flex items-center gap-2 text-xs rounded border border-border/50 px-2 py-1.5">
            <Badge variant="outline" className={`text-[10px] shrink-0 ${STATUS_STYLE[j.status] || ""}`}>
              {j.status || "unknown"}
            </Badge>
            <span className="text-[10px] uppercase tracking-wide text-muted-foreground shrink-0">{j.provider}</span>
            <span className="flex-1 min-w-0 truncate text-muted-foreground">
              {j.result?.error || j.launch?.error || j.song_id || j.id}
            </span>
            <span className="text-[11px] text-muted-foreground shrink-0 hidden sm:inline">
              {when(j.finished_at || j.created_at)}
            </span>
            {/* The notebook page is where a Kaggle run explains itself; a finished upload has a video. */}
            {(j.launch?.monitor || j.result?.video_id) && (
              <button
                title={j.result?.video_id ? "Open the published video" : "Open the run on Kaggle"}
                onClick={() => {
                  const url = j.result?.video_id
                    ? `https://www.youtube.com/watch?v=${j.result.video_id}`
                    : j.launch.monitor;
                  openUrl(url).catch(() => { try { window.open(url, "_blank", "noopener"); } catch { /* ignore */ } });
                }}
                className="shrink-0 text-muted-foreground hover:text-foreground"
              >
                <ExternalLink className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
