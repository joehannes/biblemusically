import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { useStudio } from "../lib/store";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Brain, Trash2, FolderOpen, Loader2, Info } from "lucide-react";
import { toast } from "sonner";

// What the app has worked out about your taste, and how to take it back.
//
// The store behind this (commands/learnings.rs) is not passive: `learnings_prompt_block` folds the
// strongest tallies into the system prompt of every generation as "bias choices toward these". So it
// shapes output, silently, on evidence the user never agreed to and until now could not see. A
// personalisation store you cannot read or empty is one you cannot refuse, which is the whole reason
// this panel exists rather than a settings toggle.
//
// Two scopes, because the files are two: one global across projects, one inside each project's own
// folder (and therefore inside its git repo).

const SCOPES = [
  { id: "user", label: "Across all projects" },
  { id: "project", label: "This project only" },
];

// The tally keys are machine-ish (`kept_image`, `guided_choice`). Say them in words where we know
// them, and fall back to a de-slugged version rather than inventing a label for something new.
const KIND_LABEL = {
  kept_image: "Images you kept",
  discarded_image: "Images you discarded",
  guided_choice: "Choices in the guided flow",
  style_pick: "Styles you picked",
  genre_pick: "Genres you picked",
};
const kindLabel = (k) => KIND_LABEL[k] || k.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());

export default function LearningsPanel() {
  const { activeProjectId } = useStudio();
  const [scope, setScope] = useState("user");
  const [data, setData] = useState(null);
  const [locations, setLocations] = useState(null);
  const [busy, setBusy] = useState("");

  const load = useCallback(async () => {
    try {
      const d = scope === "project"
        ? (activeProjectId ? await api.getProjectLearnings(activeProjectId) : {})
        : await api.getUserLearnings();
      setData(d || {});
    } catch { setData({}); }
    try { setLocations(await api.learningsLocations(activeProjectId || null)); } catch { setLocations(null); }
  }, [scope, activeProjectId]);
  useEffect(() => { load(); }, [load]);

  const forget = async (kind, key) => {
    const what = key ? `“${key}”` : kind ? `everything under “${kindLabel(kind)}”` : "everything the app has learned";
    if (!window.confirm(`Forget ${what}?\n\nThis rewrites the learnings file on disk. Generations after this will no longer be biased by it.`)) return;
    setBusy(key || kind || "all");
    try {
      await api.forgetLearnings(scope, scope === "project" ? activeProjectId : null, kind, key);
      await load();
      toast.success(key || kind ? "Forgotten." : "The store is empty again.");
    } catch (e) {
      toast.error(`Could not forget that: ${e?.message || e}`);
    } finally { setBusy(""); }
  };

  const tally = data?.tally || {};
  const kinds = Object.keys(tally).filter((k) => Object.keys(tally[k] || {}).length > 0);
  const stated = typeof data?.preferences === "string" ? data.preferences.trim() : "";
  const path = scope === "project" ? locations?.project : locations?.global;
  const projectMissing = scope === "project" && !activeProjectId;

  return (
    <Card className="p-4 space-y-3">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <h2 className="font-medium flex items-center gap-2">
          <Brain className="w-4 h-4 text-primary" /> What the app has learned about you
        </h2>
        <div className="flex gap-1">
          {SCOPES.map((s) => (
            <Button key={s.id} size="sm" variant={scope === s.id ? "default" : "outline"}
                    className="h-7 text-[11px]" onClick={() => setScope(s.id)}>
              {s.label}
            </Button>
          ))}
        </div>
      </div>

      <p className="text-xs text-muted-foreground flex items-start gap-1.5">
        <Info className="w-3.5 h-3.5 shrink-0 mt-px" />
        These are counted from what you keep, discard and pick. The strongest of them are added to
        the instructions behind every generation, so anything here is quietly shaping what you get.
      </p>

      {projectMissing ? (
        <p className="text-xs text-muted-foreground">Open a project to see what it has learned.</p>
      ) : !data ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="w-3.5 h-3.5 animate-spin" />Reading…
        </div>
      ) : kinds.length === 0 && !stated ? (
        <p className="text-xs text-muted-foreground">
          Nothing learned yet. This fills in as you keep and discard things.
        </p>
      ) : (
        <div className="space-y-3">
          {stated && (
            <div className="rounded-md border border-border/60 bg-muted/20 p-2.5 space-y-1">
              <div className="text-[11px] uppercase tracking-widest text-muted-foreground">
                What you told it directly
              </div>
              <div className="text-sm">{stated}</div>
            </div>
          )}
          {kinds.map((kind) => {
            const entries = Object.entries(tally[kind] || {})
              .sort((a, b) => b[1] - a[1]);
            return (
              <div key={kind} className="space-y-1.5">
                <div className="flex items-center justify-between gap-2">
                  <div className="text-[11px] uppercase tracking-widest text-muted-foreground">
                    {kindLabel(kind)}
                  </div>
                  <Button size="sm" variant="ghost" className="h-6 text-[11px] text-muted-foreground hover:text-red-400"
                          disabled={busy === kind} onClick={() => forget(kind, null)}>
                    {busy === kind ? <Loader2 className="w-3 h-3 animate-spin" /> : "Forget all"}
                  </Button>
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {entries.map(([key, count]) => (
                    <Badge key={key} variant="secondary" className="text-[11px] gap-1 pr-1">
                      {key} <span className="text-muted-foreground">×{count}</span>
                      <button onClick={() => forget(kind, key)} disabled={busy === key}
                              title={`Forget "${key}"`}
                              className="ml-0.5 rounded hover:text-red-400 disabled:opacity-50">
                        {busy === key ? <Loader2 className="w-2.5 h-2.5 animate-spin" /> : "×"}
                      </button>
                    </Badge>
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      )}

      <div className="flex items-center justify-between gap-2 flex-wrap pt-2 border-t border-border/50">
        {/* The file is plain JSON and hand-readable by design (see the module header), so the most
            useful thing to offer is its path — copied, since there is no reliable cross-platform
            "reveal in file manager" here and a button that silently does nothing is worse. */}
        {path ? (
          <button
            onClick={() => {
              navigator.clipboard?.writeText(path)
                .then(() => toast.success("Path copied."))
                .catch(() => toast.message(path));
            }}
            title={`${path} — click to copy`}
            className="text-[11px] text-muted-foreground hover:text-foreground flex items-center gap-1 min-w-0"
          >
            <FolderOpen className="w-3 h-3 shrink-0" />
            <span className="font-mono truncate max-w-[28rem]">{path}</span>
          </button>
        ) : <span />}
        <Button size="sm" variant="destructive" className="h-7 text-[11px]"
                disabled={busy === "all" || projectMissing || (kinds.length === 0 && !stated)}
                onClick={() => forget(null, null)}>
          {busy === "all" ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                          : <Trash2 className="w-3.5 h-3.5 mr-1.5" />}
          Forget everything
        </Button>
      </div>
    </Card>
  );
}
