import { useCallback, useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { toast } from "sonner";
import {
  BarChart3, RefreshCw, Loader2, TrendingUp, Gauge, AlertTriangle, LayoutGrid, FlaskConical,
} from "lucide-react";

const humanSize = (n) => {
  const bytes = Number(n) || 0;
  if (!bytes) return "—";
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) { value /= 1024; i += 1; }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[i]}`;
};

function Section({ icon: Icon, title, subtitle, children, action }) {
  return (
    <section className="rounded-lg border border-border bg-card p-4 space-y-3">
      <header className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-3">
          <Icon className="h-5 w-5 mt-0.5 text-primary shrink-0" />
          <div>
            <h2 className="font-semibold leading-tight">{title}</h2>
            {subtitle && <p className="text-xs text-muted-foreground mt-0.5 max-w-2xl">{subtitle}</p>}
          </div>
        </div>
        {action}
      </header>
      {children}
    </section>
  );
}

// One row of the pipeline board: a proportional bar per stage, so a project's shape is readable
// at a glance instead of needing the numbers to be compared mentally.
function StageBar({ label, counts, stages, total }) {
  const palette = [
    "bg-muted-foreground/40", "bg-sky-500/70", "bg-indigo-500/70",
    "bg-violet-500/70", "bg-fuchsia-500/70", "bg-emerald-500/80",
  ];
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-xs">
        <span className="truncate font-medium">{label}</span>
        <span className="text-muted-foreground shrink-0 ml-2">{total}</span>
      </div>
      <div className="flex h-2.5 w-full overflow-hidden rounded bg-muted">
        {counts.map((n, i) =>
          n > 0 ? (
            <div
              key={i}
              className={palette[i] || "bg-primary"}
              style={{ width: `${(n / Math.max(total, 1)) * 100}%` }}
              title={`${stages[i]?.label}: ${n}`}
            />
          ) : null
        )}
      </div>
    </div>
  );
}

// Readable names for the report's dimensions. The keys are the report's own, and a dimension
// without an entry falls back to its key rather than disappearing — a new axis added on the Rust
// side should show up here as itself, not vanish.
const DIM_LABEL = {
  combo: "channel · language · style",
  channel: "channel",
  language: "language",
  style: "musical style",
  image_style: "image style pack",
  length: "song length",
  hour: "hour published (channel's own time)",
  weekday: "day published",
  title_form: "title shape",
  title_length: "title length",
};

export default function Insights() {
  const { activeProjectId, projects } = useStudio();
  const [scope, setScope] = useState("project");
  const [overview, setOverview] = useState(null);
  const [performance, setPerformance] = useState(null);
  const [quota, setQuota] = useState(null);
  const [busy, setBusy] = useState("");

  const projectId = scope === "project" ? activeProjectId : null;

  const refresh = useCallback(async () => {
    try { setOverview(await api.pipelineOverview(projectId)); } catch (e) { toast.error(String(e)); }
    try { setPerformance(await api.performanceReport(projectId)); } catch { setPerformance(null); }
    try { setQuota(await api.quotaReport()); } catch { setQuota(null); }
  }, [projectId]);

  useEffect(() => { refresh(); }, [refresh]);

  const stages = overview?.stages || [];
  const projectName = (id) => projects?.find((p) => p.id === id)?.name || id;

  return (
    <div className="p-4 sm:p-6 space-y-4 max-w-5xl">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h1 className="text-4xl sm:text-5xl font-bold flex items-center gap-3">
            <BarChart3 className="h-6 w-6 text-primary" /> Insights
          </h1>
          <p className="text-sm text-muted-foreground mt-1">
            Where every song is in the pipeline, which combinations actually get watched, and what
            you have left of each free quota.
          </p>
        </div>
        <div className="flex gap-2 shrink-0">
          <div className="flex rounded border border-border overflow-hidden text-xs">
            {[["project", "This project"], ["all", "All projects"]].map(([id, label]) => (
              <button
                key={id}
                onClick={() => setScope(id)}
                className={`px-2 py-1 ${scope === id ? "bg-primary text-primary-foreground" : "hover:bg-muted"}`}
              >
                {label}
              </button>
            ))}
          </div>
          <Button size="sm" variant="outline" onClick={refresh}>
            <RefreshCw className="h-3.5 w-3.5 mr-1" /> Refresh
          </Button>
        </div>
      </div>

      {/* Pipeline board */}
      <Section
        icon={LayoutGrid}
        title="Pipeline"
        subtitle="Every song by stage. The bar is proportional, so a project that stalls at one stage is visible without reading numbers."
      >
        {overview && (
          <>
            <div className="flex flex-wrap gap-1.5">
              {stages.map((s, i) => (
                <Badge key={s.id} variant="secondary" className="text-[11px]">
                  {s.label} <span className="ml-1 opacity-70">{overview.totals?.[i] ?? 0}</span>
                </Badge>
              ))}
            </div>

            {[["by_project", "By project"], ["by_language", "By language"], ["by_style", "By style"]].map(([key, label]) => (
              (overview[key] || []).length > 0 && (
                <div key={key} className="space-y-1.5 pt-1">
                  <div className="text-[10px] uppercase tracking-wider text-muted-foreground">{label}</div>
                  {overview[key].slice(0, 12).map((row) => (
                    <StageBar
                      key={row.key}
                      label={key === "by_project" ? projectName(row.key) : row.key}
                      counts={row.counts}
                      stages={stages}
                      total={row.total}
                    />
                  ))}
                </div>
              )
            ))}

            {(overview.stuck || []).length > 0 && (
              <div className="pt-1">
                <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1 flex items-center gap-1">
                  <AlertTriangle className="h-3 w-3 text-amber-500" /> Unfinished for a week or more
                </div>
                <div className="max-h-40 overflow-auto space-y-0.5">
                  {overview.stuck.map((s) => (
                    <div key={s.id} className="flex items-center justify-between gap-2 rounded bg-muted/40 px-2 py-1 text-[11px]">
                      <span className="truncate">{s.title}</span>
                      <span className="shrink-0 text-muted-foreground">{s.status} · {s.days}d</span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
        )}
      </Section>

      {/* Performance */}
      <Section
        icon={TrendingUp}
        title="What performs"
        subtitle={performance?.note}
        action={
          <Button size="sm" variant="outline" disabled={busy === "stats"} onClick={() => {
            setBusy("stats");
            api.refreshUploadAnalytics()
              .then((r) => { toast.success(`Updated ${r?.updated ?? 0} video(s)`); return refresh(); })
              .catch((err) => toast.error(String(err)))
              .finally(() => setBusy(""));
          }}>
            {busy === "stats" ? <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5 mr-1" />}
            Fetch YouTube stats
          </Button>
        }
      >
        {/* What has never been tested, which no ranking can ever tell you: an axis set the same way
            every time has no data behind it and never will, however much is published. */}
        {performance?.experiment && (
          <div className="mb-3 rounded-lg border border-primary/40 bg-primary/5 p-2.5 space-y-1">
            <div className="text-[11px] font-medium flex items-center gap-1.5">
              <FlaskConical className="h-3.5 w-3.5 text-primary" />
              <span>Never varied:</span>
              <span>{DIM_LABEL[performance.experiment.axis] || performance.experiment.axis}</span>
            </div>
            <div className="text-[11px] text-muted-foreground">
              <span className="text-mono">{performance.experiment.videos}</span>
              <span> of </span>
              <span className="text-mono">{performance.experiment.of}</span>
              <span> measured videos are </span>
              <span className="text-foreground">{performance.experiment.current}</span>
              <span>, so the ranking has nothing to compare it against — however many more you publish.</span>
            </div>
            {performance.experiment.try && (
              <div className="text-[11px]">
                <span className="text-muted-foreground">Try </span>
                <span className="text-foreground">{performance.experiment.try}</span>
                <span className="text-muted-foreground"> next, and keep the rest of the run the same.</span>
              </div>
            )}
          </div>
        )}

        {performance?.measured_videos ? (
          <div className="grid gap-3 sm:grid-cols-2">
            {/* The creative axes first, then packaging and timing — the app varies all of these
                and until now crossed only the first four with what actually got watched. */}
            {["combo", "channel", "language", "style", "image_style", "length",
              "hour", "weekday", "title_form", "title_length"].map((dim) => (
              (performance.by?.[dim] || []).length > 0 && (
                <div key={dim}>
                  <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1">
                    {DIM_LABEL[dim] || dim}
                  </div>
                  <div className="space-y-0.5">
                    {performance.by[dim].slice(0, 6).map((row) => (
                      <div key={row.key} className="flex items-center justify-between gap-2 rounded bg-muted/40 px-2 py-1 text-[11px]">
                        <span className="truncate">{row.key}</span>
                        <span className="shrink-0 text-muted-foreground">
                          {row.median_views.toLocaleString()} med · {row.videos} vid
                          {row.videos < 5 && <span className="ml-1 opacity-60">(thin)</span>}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )
            ))}
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">
            No view counts yet. Publish something, then press “Fetch YouTube stats” — it reads the
            counts back through the OAuth connection the Channel Manager already has.
          </p>
        )}
      </Section>

      {/* Quotas */}
      <Section icon={Gauge} title="Quotas" subtitle="The limits you'd otherwise discover by hitting them.">
        {quota && (
          <div className="grid gap-2 sm:grid-cols-2">
            {["youtube", "kaggle", "ai", "disk"].map((key) => {
              const q = quota[key];
              if (!q) return null;
              return (
                <div key={key} className="rounded bg-muted/40 p-2 space-y-1">
                  <div className="flex items-center justify-between">
                    <span className="text-sm font-medium">{q.label}</span>
                    <span className="text-xs text-muted-foreground">
                      {key === "youtube" && `${q.used_today}/${q.limit} today`}
                      {key === "kaggle" && `${q.accounts} account(s)`}
                      {key === "ai" && `${q.jobs_today} job(s) today`}
                      {key === "disk" && humanSize(q.free_bytes) + " free"}
                    </span>
                  </div>
                  {key === "youtube" && (
                    <div className="h-1.5 w-full rounded bg-muted overflow-hidden">
                      <div
                        className={`h-full ${q.used_today >= q.limit ? "bg-destructive" : "bg-primary"}`}
                        style={{ width: `${Math.min(100, (q.used_today / Math.max(q.limit, 1)) * 100)}%` }}
                      />
                    </div>
                  )}
                  {key === "ai" && q.failed_today > 0 && (
                    <p className="text-[11px] text-amber-500">{q.failed_today} failed today</p>
                  )}
                  <p className="text-[11px] text-muted-foreground">{q.limit_note}</p>
                </div>
              );
            })}
          </div>
        )}
      </Section>
    </div>
  );
}
