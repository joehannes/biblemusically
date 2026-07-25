import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import { Button } from "./ui/button";
import { Card } from "./ui/card";
import GuidedFlow from "./GuidedFlow";
import { catalogFor, describeControls, applyPatch } from "../lib/controlCatalogs";
import { visibility, mergeStatus, remainingSteps, resumeIndex, progressSummary, STATUS_META } from "../lib/guidedView";
import { engineContext } from "../lib/engineCapabilities";
import {
  Wand2, SlidersHorizontal, Play, Loader2, Sparkles, RotateCcw, ChevronRight, Check,
} from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// A guided view: template first, then only the questions that are still open, with the page's
// sections manifesting as they get settled.
//
// The shape of a session:
//   1. The studio reads what it knows (brief, channel styles, goal, today's topic, social signal,
//      past behaviour) and offers 3–4 templates — one pick that fills most of the page.
//   2. Only the questions that template could not reasonably answer are asked.
//   3. Each settled section appears; the two most recent stay expanded, older ones collapse to a
//      status chip and can be reopened at any time.
//   4. Leaving is normal. Everything is persisted per project and view, so coming back shows the
//      state it was left in, colour-coded, with a Resume button at the top and the bottom.
//
// The page keeps rendering its own controls. It asks this hook where they belong.
// ─────────────────────────────────────────────────────────────────────────────

export function useGuidedView({ view, projectId, flow, sectionOrder, ctx }) {
  const [state, setState] = useState(null);        // persisted: template, answers, sections, cursor
  const [templates, setTemplates] = useState([]);
  const [phase, setPhase] = useState("loading");   // loading | template | flow | done | off
  const [busy, setBusy] = useState(false);
  const [pinned, setPinned] = useState({});
  const [showAll, setShowAll] = useState(false);
  const askedTemplates = useRef(false);

  const controls = useMemo(() => catalogFor(view, ctx), [view, ctx]);

  // ── load persisted progress ───────────────────────────────────────────────
  useEffect(() => {
    if (!projectId) { setPhase("template"); return; }
    let alive = true;
    (async () => {
      try {
        const s = await api.getWorkflowState(projectId, view);
        if (!alive) return;
        setState(s);
        // A session that was left mid-way resumes as a flow; a fresh one starts with templates.
        setPhase(s?.completed ? "done" : s?.template ? "flow" : "template");
      } catch {
        if (alive) setPhase("template");
      }
    })();
    return () => { alive = false; };
  }, [projectId, view]);

  const persist = useCallback(async (patch) => {
    setState((prev) => ({ ...(prev || {}), ...patch }));
    if (!projectId) return;
    try { await api.saveWorkflowState(projectId, view, patch); } catch { /* local state still holds */ }
  }, [projectId, view]);

  // ── templates ─────────────────────────────────────────────────────────────
  const loadTemplates = useCallback(async () => {
    if (askedTemplates.current) return;
    askedTemplates.current = true;
    setBusy(true);
    try {
      const r = await api.guideTemplates({
        view,
        goal: ctx?.project?.goal || ctx?.project?.topic || "",
        controls: describeControls(controls),
        steps: (flow.steps || []).map((s) => ({
          id: s.id,
          question: typeof s.question === "function" ? s.question(ctx) : s.question,
          options: [],
        })),
        context: {
          project_id: projectId || "",
          project_name: ctx?.project?.name || "",
          daily_topic: ctx?.project?.daily_topic || "",
          brief: ctx?.project?.brief || {},
          channels: (ctx?.channels || []).map((c) => ({ name: c.name, language: c.language, region: c.region, styles: c.styles })),
          engines: engineContext(ctx?.settings),
        },
      });
      setTemplates(r?.templates || []);
    } catch (err) {
      // No templates is a survivable state: the flow still asks everything itself.
      console.warn("[guide] templates unavailable:", err);
      setTemplates([]);
    } finally { setBusy(false); }
  }, [view, controls, flow, ctx, projectId]);

  useEffect(() => { if (phase === "template") loadTemplates(); }, [phase, loadTemplates]);

  /** Take a template: apply its patch, mark what it covered, and keep only the open questions. */
  const chooseTemplate = useCallback(async (tpl) => {
    const applied = tpl.patch ? applyPatch(controls, tpl.patch, ctx) : [];
    // Sections a template settled are "sensible", not "chosen" — the user has not seen them yet.
    const sections = { ...(state?.sections || {}) };
    const covered = new Set(tpl.covers || []);
    for (const id of applied) {
      const section = controls.find((c) => c.id === id)?.section;
      if (section) covered.add(section);
    }
    for (const id of covered) {
      sections[id] = { status: mergeStatus(sections[id]?.status, "common"), at: new Date().toISOString() };
    }
    await persist({
      template: { id: tpl.id, label: tpl.label, why: tpl.why || "", open_questions: tpl.open_questions || [], covers: [...covered] },
      sections,
      cursor: 0,
      completed: false,
    });
    api.recordLearningSignal("project", projectId || null, "guided_template", `${view}`, tpl.label).catch(() => {});
    setPhase("flow");
    toast.success(`"${tpl.label}" applied`, {
      description: applied.length ? `${applied.length} settings filled in — you'll be asked about the rest.` : undefined,
    });
  }, [controls, ctx, state, persist, projectId, view]);

  const skipTemplates = useCallback(async () => {
    await persist({ template: { id: "none", label: "Start from the page defaults", covers: [] }, cursor: 0 });
    setPhase("flow");
  }, [persist]);

  // ── the remaining questions ───────────────────────────────────────────────
  const steps = useMemo(() => {
    const all = (flow.steps || []).filter((s) => (typeof s.when === "function" ? s.when(ctx) : true));
    if (!state?.template) return all;
    return remainingSteps({
      steps: all,
      answers: state.answers || {},
      coveredSections: state.template.covers || [],
      openQuestions: state.template.open_questions || [],
    });
  }, [flow, ctx, state]);

  /** Record an answer, manifest that step's section as "chosen", and move on. */
  const recordAnswer = useCallback(async (stepId, optionId, sectionId) => {
    const answers = { ...(state?.answers || {}), [stepId]: optionId };
    const sections = { ...(state?.sections || {}) };
    if (sectionId) {
      sections[sectionId] = { status: mergeStatus(sections[sectionId]?.status, "tuned"), at: new Date().toISOString() };
    }
    await persist({ answers, sections, cursor: resumeIndex(steps, answers) });
  }, [state, persist, steps]);

  /** The page calls this when the user edits a section's controls by hand. */
  const markFineTuned = useCallback((sectionId) => {
    const sections = { ...(state?.sections || {}) };
    const next = mergeStatus(sections[sectionId]?.status, "fine");
    if (sections[sectionId]?.status === next) return;
    sections[sectionId] = { status: next, at: new Date().toISOString() };
    persist({ sections });
  }, [state, persist]);

  // Manifestation order: sections in the order they were settled, which is what the collapse rule
  // reads. Rebuilt from the persisted timestamps so a resumed session looks the way it was left.
  const manifested = useMemo(() => {
    const entries = Object.entries(state?.sections || {})
      .filter(([, v]) => v?.status && v.status !== "untouched")
      .sort((a, b) => String(a[1].at || "").localeCompare(String(b[1].at || "")));
    return entries.map(([id]) => id);
  }, [state]);

  const vis = useMemo(
    () => visibility({ order: sectionOrder || [], manifested, pinned, showAll: showAll || phase === "off" }),
    [sectionOrder, manifested, pinned, showAll, phase],
  );

  return {
    phase, setPhase, busy, templates, state, steps, controls,
    chooseTemplate, skipTemplates, recordAnswer, markFineTuned,
    // What the page asks about each of its sections.
    sectionVisible: (id) => vis[id]?.visible ?? false,
    sectionOpen: (id) => (showAll || phase === "off" ? undefined : vis[id]?.open ?? false),
    sectionStatus: (id) => state?.sections?.[id]?.status || "untouched",
    pinSection: (id) => (open) => setPinned((p) => ({ ...p, [id]: open })),
    showAll: showAll || phase === "off",
    setShowAll,
    resume: () => setPhase(state?.template ? "flow" : "template"),
    finish: () => { persist({ completed: true, cursor: steps.length }); setPhase("done"); },
    reset: async () => {
      if (projectId) { try { await api.resetWorkflowState(projectId, view); } catch { /* ignore */ } }
      askedTemplates.current = false;
      setState(null); setTemplates([]); setPinned({}); setPhase("template");
    },
    summary: progressSummary({ steps: flow.steps || [], answers: state?.answers || {}, sections: state?.sections || {} }),
  };
}

// ── UI ───────────────────────────────────────────────────────────────────────

const STATUS_CLASS = {
  muted: "border-border text-muted-foreground",
  red: "border-red-500/40 text-red-400 bg-red-500/5",
  amber: "border-amber-500/40 text-amber-400 bg-amber-500/5",
  green: "border-emerald-500/40 text-emerald-400 bg-emerald-500/5",
  blue: "border-sky-500/40 text-sky-400 bg-sky-500/5",
};

/** The traffic light on a section header. Collapsed sections are read at a glance by this alone. */
export function StatusChip({ status }) {
  const meta = STATUS_META[status] || STATUS_META.untouched;
  return (
    <span title={meta.hint}
      className={`text-[9px] uppercase tracking-wide px-1.5 py-0.5 rounded border ${STATUS_CLASS[meta.color]}`}>
      {meta.label}
    </span>
  );
}

/** Resume/expand bar. Rendered at the top and the bottom of a guided page. */
export function GuidedBar({ guide, position = "top" }) {
  const { phase, summary, showAll, setShowAll, resume, reset } = guide;
  const midSession = phase === "flow" || phase === "done";
  return (
    <div className={`flex items-center gap-2 flex-wrap text-[11px] text-muted-foreground ${position === "top" ? "mb-3" : "mt-6"}`}>
      {midSession && phase !== "flow" && (
        <Button size="sm" variant="secondary" onClick={resume}>
          <Play className="w-3 h-3 mr-1.5" />Resume the guide
        </Button>
      )}
      <Button size="sm" variant="ghost" onClick={() => setShowAll(!showAll)}>
        <SlidersHorizontal className="w-3.5 h-3.5 mr-1.5" />{showAll ? "Show only what the guide prepared" : "Show all controls"}
      </Button>
      {midSession && <span>{summary}</span>}
      {midSession && (
        <button onClick={reset} className="inline-flex items-center gap-1 hover:text-foreground">
          <RotateCcw className="w-3 h-3" />start over
        </button>
      )}
    </div>
  );
}

/** The template chooser: live-aggregated preset packs, each a direction rather than a settings dump. */
export function TemplatePicker({ guide, ctx }) {
  const { templates, busy, chooseTemplate, skipTemplates, controls } = guide;
  return (
    <Card className="p-4 mb-4 border-primary/30 space-y-3">
      <div className="flex items-start gap-2">
        <div className="p-1.5 rounded-md bg-primary/10 shrink-0"><Wand2 className="w-4 h-4 text-primary" /></div>
        <div className="min-w-0">
          <div className="text-sm font-semibold leading-tight">Where should today start?</div>
          <div className="text-[11px] text-muted-foreground">
            Read from your brief, your channels and today's topic. Picking one fills most of this page —
            you'll only be asked about what's genuinely still open.
          </div>
        </div>
      </div>

      {busy && (
        <div className="flex items-center gap-2 text-[11px] text-muted-foreground py-4">
          <Loader2 className="w-3.5 h-3.5 animate-spin" />Reading your project…
        </div>
      )}

      {!busy && (
        <div className="grid gap-2 sm:grid-cols-2">
          {templates.map((t) => (
            <button key={t.id} onClick={() => chooseTemplate(t)}
              className="text-left rounded-lg border border-border p-3 hover:border-primary/60 hover:bg-primary/5 transition-all">
              <div className="flex items-center gap-1.5">
                <span className="text-sm font-medium">{t.label}</span>
                {t.confidence >= 0.75 && <span className="text-[9px] uppercase tracking-wide text-primary">strong fit</span>}
              </div>
              {t.pitch && <div className="text-[11px] text-muted-foreground mt-0.5 leading-snug">{t.pitch}</div>}
              {t.why && (
                <div className="flex items-start gap-1 mt-1.5 text-[10px] text-primary/80">
                  <Sparkles className="w-3 h-3 shrink-0 mt-0.5" /><span>{t.why}</span>
                </div>
              )}
              <div className="text-[10px] text-muted-foreground/70 mt-1.5">
                sets {Object.keys(t.patch || {}).length} settings
                {t.open_questions?.length ? ` · ${t.open_questions.length} question${t.open_questions.length === 1 ? "" : "s"} left` : " · nothing left to ask"}
              </div>
            </button>
          ))}
          {!templates.length && (
            <div className="text-[11px] text-muted-foreground">
              No templates right now — the guide will ask the questions itself.
            </div>
          )}
        </div>
      )}

      <button onClick={skipTemplates} className="text-[11px] text-muted-foreground hover:text-foreground inline-flex items-center gap-1">
        Start from the page defaults instead<ChevronRight className="w-3 h-3" />
      </button>
      {!busy && templates.length > 0 && (
        <div className="text-[10px] text-muted-foreground/70">
          {controls.length} controls on this page are available to a template.
        </div>
      )}
    </Card>
  );
}

/**
 * Everything above a guided page's sections: the bar, the template picker, and the question flow.
 * A page renders `<GuidedHeader guide={guide} flow={flow} ctx={ctx} />` and then its own sections.
 */
export function GuidedHeader({ guide, flow, ctx }) {
  const { phase, steps, state, recordAnswer, finish } = guide;

  if (phase === "loading") return null;
  if (phase === "off") return <GuidedBar guide={guide} />;

  return (
    <>
      <GuidedBar guide={guide} />
      {phase === "template" && <TemplatePicker guide={guide} ctx={ctx} />}
      {phase === "flow" && (
        <div className="mb-4">
          <GuidedFlow
            flow={{ ...flow, steps }}
            ctx={ctx}
            startAt={resumeIndex(steps, state?.answers || {})}
            answered={state?.answers || {}}
            onAnswer={(stepId, optionId, sectionId) => recordAnswer(stepId, optionId, sectionId)}
            onRevealSection={() => {}}
            onShowAll={() => guide.setShowAll(true)}
            onFinish={finish}
          />
        </div>
      )}
      {phase === "done" && state?.template && (
        <Card className="p-3 mb-4 flex items-center gap-2 text-[11px] text-muted-foreground">
          <Check className="w-3.5 h-3.5 text-emerald-500 shrink-0" />
          <span><b className="text-foreground">{state.template.label}</b> — {guide.summary}. Everything below stays editable.</span>
        </Card>
      )}
    </>
  );
}
