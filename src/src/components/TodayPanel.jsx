import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Sunrise, Loader2, ArrowRight, RefreshCw } from "lucide-react";

// ─────────────────────────────────────────────────────────────────────────────
// What today looks like, for one project.
//
// The other half of the map problem. The sidebar folds to fifteen stops, but fifteen is still a
// question — and the answer is knowable: a project with four songs that have audio and no images has
// exactly one obvious next thing, and the app already knows it.
//
// So the steps come from what the project actually contains rather than from a script. `guide_today`
// counts the artefacts (not the status field, which is set by whichever step last finished and by
// nothing at all for a manual import), builds the plain correct plan, and lets the AI reword and
// reorder it without inventing a step. Each one names a route, so this is clickable rather than
// advisory.
//
// Failure is a quieter version of itself rather than an empty box: no key, no network, or a spent
// free tier gives the plain plan, which is always right about what exists.
// ─────────────────────────────────────────────────────────────────────────────

export default function TodayPanel({ projectId, onStartInterview }) {
  const nav = useNavigate();
  const [today, setToday] = useState(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    if (!projectId) return;
    setBusy(true);
    try { setToday(await api.guideToday(projectId)); }
    catch { setToday(null); }
    finally { setBusy(false); }
  }, [projectId]);

  useEffect(() => { load(); }, [load]);

  if (!projectId) return null;
  if (busy && !today) {
    return (
      <Card className="p-4 mb-5 flex items-center gap-2 text-[11px] text-muted-foreground">
        <Loader2 className="w-3.5 h-3.5 animate-spin" />Looking at where this project got to…
      </Card>
    );
  }
  if (!today?.steps?.length) return null;

  const s = today.shape || {};

  return (
    <Card className="p-4 mb-5 space-y-3" data-testid="today-panel">
      <div className="flex items-start gap-2">
        <div className="p-1.5 rounded-md bg-primary/10 shrink-0"><Sunrise className="w-4 h-4 text-primary" /></div>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-semibold leading-tight">Today</div>
          <div className="text-[11px] text-muted-foreground">
            {/* The greeting is generated text about this project, so it is never a catalogue
                string; the standing line beside it is, and has to be JSX text to be extracted. */}
            {today.greeting ? <span data-no-i18n>{today.greeting}</span>
                            : <span>What this project is waiting on.</span>}
          </div>
        </div>
        <button onClick={load} disabled={busy} title="Look again"
                className="p-1.5 rounded shrink-0 text-muted-foreground hover:bg-muted/40 disabled:opacity-50">
          {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <RefreshCw className="w-3.5 h-3.5" />}
        </button>
      </div>

      <div className="space-y-1.5">
        {today.steps.map((step, i) => (
          <button
            key={`${step.route}-${i}`}
            onClick={() => (step.route === "/" && onStartInterview ? onStartInterview() : nav(step.route))}
            data-testid={`today-step-${i}`}
            className="w-full text-left rounded-lg border border-border p-2.5 flex items-start gap-2.5
                       hover:border-primary/60 hover:bg-primary/5 transition-all group"
          >
            <span className="text-mono text-[10px] text-muted-foreground/70 mt-0.5 w-4 shrink-0">{i + 1}</span>
            <span className="min-w-0 flex-1">
              <span className="block text-sm font-medium">{step.label}</span>
              {step.why && <span className="block text-[11px] text-muted-foreground mt-0.5 leading-snug">{step.why}</span>}
            </span>
            <ArrowRight className="w-3.5 h-3.5 shrink-0 mt-1 text-muted-foreground/50 group-hover:text-primary" />
          </button>
        ))}
      </div>

      {/* The counts the steps were derived from, so a step can be disagreed with rather than
          just obeyed. */}
      {Number(s.songs) > 0 && (
        <div className="flex flex-wrap gap-x-3 gap-y-1 text-[10px] text-muted-foreground/80 pt-0.5">
          <span className="text-mono">{s.songs} songs</span>
          {Number(s.no_lyrics) > 0 && <span className="text-mono">{s.no_lyrics} without words</span>}
          {Number(s.no_audio) > 0 && <span className="text-mono">{s.no_audio} without audio</span>}
          {Number(s.no_images) > 0 && <span className="text-mono">{s.no_images} without pictures</span>}
          {Number(s.uploaded) > 0 && <span className="text-mono">{s.uploaded} published</span>}
        </div>
      )}

      {!today.has_brief && onStartInterview && (
        <Button size="sm" variant="secondary" onClick={onStartInterview} className="text-xs">
          Tell the guide what this project is
        </Button>
      )}
    </Card>
  );
}
