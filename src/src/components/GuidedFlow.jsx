import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import { Button } from "./ui/button";
import Spotlight from "./Spotlight";
import { Card } from "./ui/card";
import {
  Sparkles, ArrowRight, ArrowLeft, Check, Wand2, Loader2, SlidersHorizontal,
  ChevronRight, RotateCcw, Lightbulb, Mic, MicOff, Volume2, VolumeX, Radio,
} from "lucide-react";
import { toast } from "sonner";
import { speak, stopSpeaking, listen, interpretAnswer, voicePrefs, setVoicePrefs, voiceInputAvailable } from "../lib/voice";
import { useConversation } from "../lib/useConversation";

// ─────────────────────────────────────────────────────────────────────────────
// The guided layer.
//
// Pages like the AI Composer show every control at once, which is complete but unreadable: the user
// has to know which of forty fields matter today. This renders the same page as a conversation —
// one question at a time, two to four concrete choices, a recommended one already marked — and
// reveals the underlying section only when its question is live.
//
// Three things make it a guide rather than a wizard:
//   • It proposes, it does not decide. Every step can be skipped, revisited from the rail, or
//     abandoned entirely with "All controls", which drops the user into the full page unchanged.
//   • The recommendation is personal. `api.guideProposal` sends the project brief, the day's topic,
//     the learnings store and the selected engines' capabilities, and gets back a pick per step with
//     a one-line reason. When the AI is unavailable it falls back to what this user picked last time,
//     then to the step's own default — so it always has an answer.
//   • It learns. Each choice is recorded as a learning signal, which is what the next proposal reads.
//
// A flow is data (see lib/guidedFlows.js): steps with a question, options, and an `apply` that writes
// into the page's own state. No page logic lives here.
// ─────────────────────────────────────────────────────────────────────────────

const CHOICE_KEY = (flowId) => `studio:guided:${flowId}`;

/** Remember choices per flow so a returning user sees their own habits as defaults. */
function loadLocalChoices(flowId) {
  try { return JSON.parse(localStorage.getItem(CHOICE_KEY(flowId)) || "{}"); } catch { return {}; }
}
function saveLocalChoice(flowId, stepId, optionId) {
  try {
    const all = loadLocalChoices(flowId);
    all[stepId] = { option: optionId, at: new Date().toISOString() };
    localStorage.setItem(CHOICE_KEY(flowId), JSON.stringify(all));
  } catch { /* ignore */ }
}

/**
 * Step text may be a string or a function of the context — a hint like "HeartMuLa sings anything that
 * is not a section header" only makes sense once the engine is known. Rendering a function as a React
 * child would throw, so every text field goes through here.
 */
const resolve = (value, ctx) => (typeof value === "function" ? value(ctx) : value);

/**
 * Resolve the steps that apply right now. `when(ctx)` drops steps that make no sense for the current
 * project or engine (e.g. Midjourney flag controls when FLUX is selected), so the flow length adapts
 * instead of showing dead ends.
 */
function visibleSteps(flow, ctx) {
  return (flow.steps || []).filter((s) => (typeof s.when === "function" ? s.when(ctx) : true));
}

export default function GuidedFlow({
  flow,                 // { id, title, intro, steps: [...] }
  ctx,                  // whatever the page's options/apply need (cfg, settings, channels, …)
  onRevealSection,      // (sectionId | null) => void — page opens that section as the step goes live
  onFinish,             // () => void — last step's primary action
  showAllLabel = "All controls",
  onShowAll,            // () => void — escape hatch into the untouched full page
  compact = false,
  startAt = 0,          // resume position, for a session picked up later
  answered,             // stepId -> optionId already recorded (persisted sessions)
  onAnswer,             // (stepId, optionId, sectionId) => void — for callers that persist progress
}) {
  const steps = useMemo(() => visibleSteps(flow, ctx), [flow, ctx]);
  const [i, setI] = useState(() => Math.min(Math.max(0, startAt), Math.max(0, steps.length - 1)));
  const [answers, setAnswers] = useState(() => answered || {});   // stepId -> optionId
  const [proposal, setProposal] = useState(null); // { picks: {stepId: {option, why}}, greeting }
  const [proposing, setProposing] = useState(false);
  const localChoices = useMemo(() => loadLocalChoices(flow.id), [flow.id]);
  const asked = useRef(false);

  const step = steps[i] || null;
  const options = useMemo(() => {
    if (!step) return [];
    const raw = typeof step.options === "function" ? step.options(ctx) : step.options || [];
    return raw.filter((o) => (typeof o.when === "function" ? o.when(ctx) : true));
  }, [step, ctx]);

  // Reveal the section this step is about, and clear it when the flow ends.
  useEffect(() => { onRevealSection?.(step?.reveals ?? null); }, [step, onRevealSection]);

  // Ask the AI once per mount for a personalised pick per step. Failure is silent by design — the
  // flow still works, it just recommends from history instead.
  useEffect(() => {
    if (asked.current || !steps.length) return;
    asked.current = true;
    (async () => {
      setProposing(true);
      try {
        const r = await api.guideProposal({
          flow: flow.id,
          title: flow.title,
          steps: steps.map((s) => ({
            id: s.id,
            question: s.question,
            options: (typeof s.options === "function" ? s.options(ctx) : s.options || [])
              .map((o) => ({ id: o.id, label: o.label, hint: o.hint || "" })),
          })),
          context: typeof flow.aiContext === "function" ? flow.aiContext(ctx) : {},
        });
        if (r?.picks) setProposal(r);
      } catch (err) {
        console.warn("[guide] proposal unavailable:", err);
      } finally {
        setProposing(false);
      }
    })();
  }, [steps, flow, ctx]);

  /** What to recommend for a step: the AI's pick, else this user's last choice, else the step default. */
  const recommendedFor = useCallback((s, opts) => {
    const aiPick = proposal?.picks?.[s.id]?.option;
    if (aiPick && opts.some((o) => o.id === aiPick)) return aiPick;
    const mine = localChoices[s.id]?.option;
    if (mine && opts.some((o) => o.id === mine)) return mine;
    const flagged = opts.find((o) => o.recommended);
    return flagged?.id || opts[0]?.id;
  }, [proposal, localChoices]);

  const reasonFor = useCallback((s, optionId) => {
    const pick = proposal?.picks?.[s.id];
    if (pick?.option === optionId && pick.why) return pick.why;
    if (localChoices[s.id]?.option === optionId) return "What you chose here last time.";
    return null;
  }, [proposal, localChoices]);

  const choose = async (option) => {
    if (!step) return;
    try {
      option.apply?.(ctx);
    } catch (err) {
      toast.error(`Couldn't apply that: ${err}`);
      return;
    }
    setAnswers((a) => ({ ...a, [step.id]: option.id }));
    saveLocalChoice(flow.id, step.id, option.id);
    // Feed the learnings store, which is what the next proposal reads. Best-effort: a guide that
    // fails to record a preference is still a working guide.
    api.recordLearningSignal("project", ctx?.projectId || null, "guided_choice",
      `${flow.id}:${step.id}`, option.id).catch(() => {});
    // Callers that own a persisted session (useGuidedView) record it there too, along with which
    // section this answer settled — that is what makes the section manifest.
    onAnswer?.(step.id, option.id, step.reveals || null);

    if (i + 1 < steps.length) setI(i + 1);
    else onFinish?.();
  };

  const skip = () => (i + 1 < steps.length ? setI(i + 1) : onFinish?.());

  // ── voice ─────────────────────────────────────────────────────────────────
  // The assistant reads each question and its choices, then waits. Speaking is opt-in and remembered;
  // everything here fails quietly, so a missing microphone or key never blocks clicking.
  const [prefs, setPrefs] = useState(() => voicePrefs());
  const [listening, setListening] = useState(false);
  const spokenFor = useRef(null);

  const recommendedId = step ? recommendedFor(step, options) : null;

  useEffect(() => {
    // In hands-free mode the loop does the speaking, and both doing it would talk over each other.
    if (!step || !prefs.speak || (prefs.handsFree && voiceInputAvailable())) return;
    // One reading per step, even as re-renders come and go.
    const key = `${flow.id}:${step.id}:${i}`;
    if (spokenFor.current === key) return;
    spokenFor.current = key;
    const question = resolve(step.question, ctx);
    const spokenOptions = options.slice(0, 4).map((o, n) => `${n + 1}. ${o.label}`).join(". ");
    const suggestion = options.find((o) => o.id === recommendedId);
    const line = [question, spokenOptions, suggestion ? `I'd suggest ${suggestion.label}.` : ""]
      .filter(Boolean).join(" ");
    speak(line).catch(() => {});
    return () => stopSpeaking();
  }, [step, i, options, prefs.speak, prefs.handsFree, flow.id, ctx, recommendedId]);

  /** Take a spoken answer: transcribe, map it to a choice, and act on it. */
  const answerByVoice = async () => {
    if (listening) return;
    stopSpeaking();
    setListening(true);
    try {
      const heard = await listen({ language: prefs.language });
      if (!heard) {
        toast.message("I didn't catch that — try again, or just click.");
        return;
      }
      const { option, confidence, reason } = await interpretAnswer(heard, options, {
        recommended: recommendedId,
        question: resolve(step?.question, ctx),
      });
      if (!option) {
        if (reason === "declined") { skip(); return; }
        toast.message(`Heard "${heard}" — but I'm not sure which option that is.`);
        return;
      }
      const chosen = options.find((o) => o.id === option);
      if (!chosen) return;
      toast.success(`"${heard}" → ${chosen.label}`, {
        description: confidence < 0.7 ? "Not certain — change it from the rail if that's wrong." : undefined,
      });
      choose(chosen);
    } finally { setListening(false); }
  };

  // ── hands-free ────────────────────────────────────────────────────────────
  // The same three pieces, chained: it asks, listens, interprets and moves on, and stops the moment
  // somebody talks over it. Opt-in, off by default, and one click ends it — this holds the microphone
  // open while it speaks, which is not something to do to a person who only wanted the questions
  // read aloud.
  const handsFree = prefs.handsFree && prefs.speak;
  const conversation = useConversation({
    step,
    options,
    recommendedId,
    enabled: handsFree,
    onChoose: (option, { spoken } = {}) => {
      if (spoken) toast.success(`"${spoken}" → ${option.label}`);
      choose(option);
    },
    onSkip: skip,
  });

  const toggleHandsFree = () => {
    const next = setVoicePrefs({ handsFree: !prefs.handsFree, speak: !prefs.handsFree || prefs.speak });
    setPrefs(next);
    if (!next.handsFree) conversation.stop();
    else conversation.restart();
  };

  const toggleSpeech = () => {
    const next = setVoicePrefs({ speak: !prefs.speak });
    setPrefs(next);
    if (!next.speak) stopSpeaking();
    else if (step) {
      spokenFor.current = null;   // let the effect read the current question straight away
      speak(resolve(step.question, ctx), { force: true }).catch(() => {});
    }
  };

  if (!steps.length) return null;
  const recommended = recommendedId;

  return (
    <>
      {/* Point at the control this step is about, when the step names one. Optional by design:
          a step that has nothing to point at simply does not, and the page stays undimmed. */}
      <Spotlight target={step?.spotlight} active={!!step?.spotlight} />
    <Card className={`overflow-hidden border-primary/30 ${compact ? "p-3" : "p-4"} space-y-3`}>
      {/* ── header + progress rail (every visited step is clickable: cherry-picking) ── */}
      <div className="flex items-start gap-2">
        <div className="p-1.5 rounded-md bg-primary/10 shrink-0">
          <Wand2 className="w-4 h-4 text-primary" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-semibold leading-tight">{flow.title}</div>
          <div className="text-[11px] text-muted-foreground">
            Step {i + 1} of {steps.length}
            {/* What the loop is doing right now. Without it, "listening" and "thinking" look
                identical from outside — a silent pause — and people talk over the wrong one. */}
            {handsFree && conversation.phase === "speaking" &&
              <span className="ml-2 text-primary">asking — talk over me any time</span>}
            {handsFree && conversation.phase === "listening" &&
              <span className="ml-2 text-primary">listening…</span>}
            {handsFree && conversation.phase === "thinking" &&
              <span className="ml-2 text-primary">working out what you meant…</span>}
            {proposing && <span className="inline-flex items-center gap-1 ml-2"><Loader2 className="w-3 h-3 animate-spin" />reading your project…</span>}
            {!proposing && proposal?.model && <span className="ml-2 opacity-70">suggestions by {proposal.model}</span>}
          </div>
        </div>
        {/* Hands-free: it asks, listens and moves on by itself. Only offered where there is a way
            to listen at all, and one click ends it — being unable to stop an assistant is exactly
            the thing that makes people never turn one on again. */}
        {voiceInputAvailable() && (
          <button
            onClick={toggleHandsFree}
            data-testid="guided-hands-free"
            title={handsFree ? "Stop the conversation" : "Hands-free: it asks, you answer out loud"}
            className={`p-1.5 rounded shrink-0 hover:bg-muted/40 ${handsFree ? "text-primary" : "text-muted-foreground"}`}
          >
            <Radio className={`w-4 h-4 ${handsFree ? "animate-pulse" : ""}`} />
          </button>
        )}
        <button
          onClick={toggleSpeech}
          title={prefs.speak ? "Stop reading the questions aloud" : "Read the questions aloud"}
          className={`p-1.5 rounded shrink-0 hover:bg-muted/40 ${prefs.speak ? "text-primary" : "text-muted-foreground"}`}
        >
          {prefs.speak ? <Volume2 className="w-4 h-4" /> : <VolumeX className="w-4 h-4" />}
        </button>
        {onShowAll && (
          <Button size="sm" variant="ghost" onClick={onShowAll} className="shrink-0 text-xs">
            <SlidersHorizontal className="w-3.5 h-3.5 mr-1.5" />{showAllLabel}
          </Button>
        )}
      </div>

      <div className="flex items-center gap-1 flex-wrap">
        {steps.map((s, k) => {
          const done = answers[s.id] !== undefined;
          const active = k === i;
          return (
            <button
              key={s.id}
              onClick={() => setI(k)}
              disabled={k > i && !done}
              title={s.title || s.question}
              className={`px-2 py-0.5 rounded text-[10px] border transition-colors ${
                active ? "border-primary text-primary bg-primary/10 font-medium"
                : done ? "border-emerald-500/40 text-emerald-500/90 hover:bg-emerald-500/10"
                : "border-border text-muted-foreground/60"
              }`}
            >
              {done && <Check className="w-2.5 h-2.5 inline mr-1" />}
              {s.title || `Step ${k + 1}`}
            </button>
          );
        })}
      </div>

      {proposal?.greeting && i === 0 && (
        <div className="flex items-start gap-2 text-[11px] text-foreground/80 bg-primary/5 border border-primary/15 rounded-lg px-2.5 py-1.5">
          <Sparkles className="w-3.5 h-3.5 text-primary shrink-0 mt-0.5" />
          <span>{proposal.greeting}</span>
        </div>
      )}

      {/* ── the question ── */}
      {step && (
        <div className="space-y-2.5">
          <div>
            <div className="text-sm font-medium">{resolve(step.question, ctx)}</div>
            {step.help && (
              <div className="text-[11px] text-muted-foreground mt-0.5 leading-relaxed">{resolve(step.help, ctx)}</div>
            )}
          </div>

          <div className={`grid gap-2 ${options.length > 2 ? "sm:grid-cols-2" : ""}`}>
            {options.map((o) => {
              const isRec = o.id === recommended;
              const why = reasonFor(step, o.id);
              return (
                <button
                  key={o.id}
                  onClick={() => choose(o)}
                  className={`text-left rounded-lg border p-2.5 transition-all hover:border-primary/60 hover:bg-primary/5 ${
                    isRec ? "border-primary/50 bg-primary/5" : "border-border"
                  }`}
                >
                  <div className="flex items-center gap-1.5">
                    <span className="text-sm font-medium">{o.label}</span>
                    {isRec && <span className="text-[9px] uppercase tracking-wide text-primary">suggested</span>}
                  </div>
                  {o.hint && <div className="text-[11px] text-muted-foreground mt-0.5 leading-snug">{o.hint}</div>}
                  {why && (
                    <div className="flex items-start gap-1 mt-1.5 text-[10px] text-primary/80">
                      <Lightbulb className="w-3 h-3 shrink-0 mt-0.5" /><span>{why}</span>
                    </div>
                  )}
                </button>
              );
            })}
          </div>

          {/* The step's own controls, when a choice alone isn't enough (free text, sliders, lists). */}
          {typeof step.render === "function" && (
            <div className="rounded-lg border border-border p-2.5">{step.render(ctx)}</div>
          )}

          <div className="flex items-center justify-between pt-0.5">
            <div className="flex items-center gap-1">
              <Button size="sm" variant="ghost" onClick={() => setI((x) => Math.max(0, x - 1))} disabled={i === 0}>
                <ArrowLeft className="w-3.5 h-3.5 mr-1.5" />Back
              </Button>
              {/* Answer by speaking. "Yes" takes the suggestion, "the second one" works, so does
                  naming the option — and anything ambiguous is checked with the AI before acting. */}
              {voiceInputAvailable() && (
                <Button size="sm" variant={listening ? "default" : "ghost"} onClick={answerByVoice}
                  title="Answer out loud">
                  {listening
                    ? <><Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />Listening…</>
                    : <><Mic className="w-3.5 h-3.5 mr-1.5" />Say it</>}
                </Button>
              )}
            </div>
            <div className="flex items-center gap-1.5">
              <Button size="sm" variant="ghost" onClick={skip} className="text-xs text-muted-foreground">
                Keep as is<ChevronRight className="w-3.5 h-3.5 ml-1" />
              </Button>
              {step.primary && (
                <Button size="sm" onClick={() => { step.primary.run?.(ctx); onFinish?.(); }}>
                  {step.primary.label}<ArrowRight className="w-3.5 h-3.5 ml-1.5" />
                </Button>
              )}
            </div>
          </div>
        </div>
      )}

      {Object.keys(answers).length > 0 && (
        <button
          onClick={() => { setAnswers({}); setI(0); }}
          className="text-[10px] text-muted-foreground hover:text-foreground inline-flex items-center gap-1"
        >
          <RotateCcw className="w-3 h-3" />Start the guide over
        </button>
      )}
    </Card>
    </>
  );
}
