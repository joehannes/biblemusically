import { useEffect, useState } from "react";
import { Activity, AlertTriangle, Loader2 } from "lucide-react";
import { Progress } from "./ui/progress";

// Stage vocabulary published by the Rust job runner (jobs.rs `set_stage`), shared by the music and
// image job paths. Order matters — the stepper uses the index to decide which stages are behind us.
export const GEN_STAGES = [
  { key: "preparing", label: "Prepare" },
  { key: "submitting", label: "Send" },
  { key: "generating", label: "Render" },
  { key: "fetching", label: "Fetch" },
  { key: "done", label: "Ready" },
];

const fmtElapsed = (s) =>
  s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;

/**
 * Staged progress panel for any running generation job (music, image, character).
 *
 * Reads the `stage` / `stage_message` / `stage_started_at` fields the job runner publishes, so it
 * reports what is actually happening ("Rendering audio on the GPU…") instead of echoing the last
 * raw log line — which, because `set_progress` logged every tick, was usually just "progress 42%".
 *
 * `label` is the heading (e.g. "HeartMuLa generation", "ComfyUI image"). `job` is a raw job row
 * from list_jobs.
 */
export default function GenerationProgress({ job, label = "Generation" }) {
  // Local 1s ticker: the job list only refreshes every ~2s, but a clock that jumps in 2s steps
  // looks stalled. This keeps the elapsed readout moving smoothly between polls.
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNowMs(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);

  if (!job) return null;

  const failed = job.status === "failed" || job.stage === "failed";
  const stage = job.stage || "preparing";
  const foundIdx = GEN_STAGES.findIndex((s) => s.key === stage);
  const activeIdx = foundIdx < 0 ? 0 : foundIdx;
  const pct = Math.max(0, Math.min(100, job.progress ?? 0));
  const elapsed = job.stage_started_at
    ? Math.max(0, Math.floor(nowMs / 1000 - job.stage_started_at))
    : null;
  const message =
    job.stage_message ||
    (job.logs?.length ? job.logs[job.logs.length - 1].replace(/^\[.*?\]\s*/, "") : "Queueing job…");

  return (
    <div className="space-y-2.5">
      <div className="flex items-center justify-between text-xs font-semibold text-mono">
        <span className={`flex items-center gap-1.5 ${failed ? "text-destructive" : "text-primary"}`}>
          {failed ? <AlertTriangle className="w-3.5 h-3.5" /> : <Activity className="w-3.5 h-3.5 animate-pulse" />}
          {label}
        </span>
        <span className="flex items-center gap-2 tabular-nums">
          {elapsed !== null && <span className="text-muted-foreground font-normal">{fmtElapsed(elapsed)}</span>}
          <span>{pct}%</span>
        </span>
      </div>

      <Progress value={pct} className={`h-1.5 ${failed ? "[&>*]:bg-destructive" : ""}`} />

      <div className="flex items-center gap-1 flex-wrap">
        {GEN_STAGES.map((s, i) => {
          const isDone = stage === "done" || i < activeIdx;
          const isActive = i === activeIdx && !failed && stage !== "done";
          return (
            <div key={s.key} className="flex items-center gap-1">
              <span
                className={
                  "px-1.5 py-0.5 rounded text-[9px] uppercase tracking-wide transition-colors " +
                  (failed && i === activeIdx
                    ? "bg-destructive/20 text-destructive"
                    : isDone
                    ? "bg-primary/20 text-primary"
                    : isActive
                    ? "bg-primary text-primary-foreground animate-pulse"
                    : "bg-muted text-muted-foreground")
                }
              >
                {s.label}
              </span>
              {i < GEN_STAGES.length - 1 && <span className="text-muted-foreground text-[9px]">›</span>}
            </div>
          );
        })}
      </div>

      <div
        className={`text-[10px] text-mono truncate flex items-center gap-1.5 ${
          failed ? "text-destructive" : "text-muted-foreground"
        }`}
      >
        {!failed && stage !== "done" && <Loader2 className="w-3 h-3 animate-spin shrink-0" />}
        {failed ? job.error || message : message}
      </div>
    </div>
  );
}
