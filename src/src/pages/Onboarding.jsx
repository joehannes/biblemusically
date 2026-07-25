import { useState, useEffect } from "react";
import { api } from "../lib/api";
import { GUIDE_STEPS } from "../lib/guideSteps";
import { Button } from "../components/ui/button";
import {
  Sparkles, CheckCircle2, ArrowRight, ArrowLeft, Rocket, Loader2,
} from "lucide-react";

// Guided setup. The individual steps live in lib/guideSteps.jsx so the SAME panels can be re-opened
// later on demand (see components/GuideStepDialog.jsx). This file sequences them.
//
// Two modes:
//   • first run  (resume=false): a welcome + every step + a done screen.
//   • resume     (resume=true) : shown on later launches with ONLY the steps still outstanding
//     (the caller passes the filtered list). It skips the welcome and offers a clear dismiss, so a
//     fully-configured app never nags — once every step's isDone() is true there's nothing to show.

export default function Onboarding({ steps: requested = GUIDE_STEPS, resume = false, onDone, onSkip }) {
  const [idx, setIdx] = useState(0);
  const [saving, setSaving] = useState(false);
  const [settings, setSettings] = useState(null);
  const [doneIds, setDoneIds] = useState(() => new Set());
  // "Don't auto-show on startup" — persists immediately on toggle so it sticks even if the user
  // just dismisses. The setup is still reachable any time from the Tours button in the menubar.
  const [noAutopop, setNoAutopop] = useState(false);
  const toggleAutopop = async (v) => {
    setNoAutopop(v);
    try { await api.saveSettings({ onboarding_autopop_disabled: v }); } catch { /* ignore */ }
  };

  // The "what are you here to do?" step writes a plan into settings.setup_plan: which of the remaining
  // steps this goal actually needs. Honour it — a user who said "just trying it out" should not be
  // walked through publishing credentials, and one aiming at fifty channels should be.
  const plan = settings?.setup_plan;
  const steps = (() => {
    if (!Array.isArray(plan) || !plan.length) return requested;
    const need = Object.fromEntries(plan.map((p) => [p.id, p.need]));
    const keep = requested.filter((s) => need[s.id] !== "skip");
    // Required first, then optional — without reordering within each group, so the sequence still
    // reads like the app's own order.
    return [
      ...keep.filter((s) => need[s.id] !== "optional"),
      ...keep.filter((s) => need[s.id] === "optional"),
    ];
  })();
  const optionalIds = new Set((Array.isArray(plan) ? plan : []).filter((p) => p.need === "optional").map((p) => p.id));
  const planReason = (id) => (Array.isArray(plan) ? plan.find((p) => p.id === id)?.why : "") || "";

  // screens: [welcome?] + steps + [done]
  const screens = [
    ...(resume ? [] : [{ kind: "welcome" }]),
    ...steps.map((s) => ({ kind: "step", step: s })),
    { kind: "done" },
  ];
  const total = screens.length;
  const cur = screens[Math.min(idx, total - 1)];

  useEffect(() => {
    (async () => {
      try {
        const s = (await api.getSettings()) || {};
        setSettings(s);
        setNoAutopop(s.onboarding_autopop_disabled === true);
      } catch { setSettings({}); }
    })();
  }, []);

  const next = () => setIdx((i) => Math.min(i + 1, total - 1));
  const back = () => setIdx((i) => Math.max(i - 1, 0));

  const finishAll = async () => {
    setSaving(true);
    try { await api.saveSettings({ onboarded: true }); } catch { /* ignore */ }
    setSaving(false);
    onDone?.();
  };

  return (
    <div className="min-h-screen w-full bg-background text-foreground flex flex-col items-center overflow-y-auto">
      <div className="w-full max-w-2xl px-6 py-10">
        <div className="flex items-center gap-1.5 mb-8">
          {screens.map((_, i) => (
            <div key={i} className={`h-1.5 rounded-full flex-1 transition-colors ${i <= idx ? "bg-primary" : "bg-muted"}`} />
          ))}
        </div>
        <div className="flex items-center justify-between mb-2">
          <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground">
            {resume ? "Finish setup" : "Setup"} · step {idx + 1}/{total}
          </div>
          {resume && onSkip && (
            <button onClick={onSkip} className="text-xs text-muted-foreground hover:text-foreground">Not now</button>
          )}
        </div>

        {/* Opt out of the auto-popup — the whole guide stays available from the Tours button. */}
        <label className="flex items-center gap-2 mb-6 text-xs text-muted-foreground cursor-pointer select-none">
          <input type="checkbox" className="accent-primary" checked={noAutopop}
            data-testid="onboarding-disable-autopop"
            onChange={(e) => toggleAutopop(e.target.checked)} />
          Don't show this automatically on startup (I'll open it from the Tours button when I need it)
        </label>

        {cur.kind === "welcome" && (
          <div className="fade-in space-y-5">
            <div className="flex items-center gap-3">
              <Sparkles className="w-8 h-8 text-primary" />
              <h1 className="text-4xl font-bold">Welcome to AI Music Video Studio</h1>
            </div>
            <p className="text-muted-foreground text-lg">
              This app turns scripture into finished, uploadable music videos — lyrics, music, imagery,
              and video — driven by AI, most of it running on <span className="text-foreground font-medium">free</span> GPU servers.
            </p>
            <div className="rounded-lg border border-border bg-muted/30 p-4 space-y-2 text-sm">
              <div className="font-medium">In the next minute we'll set up:</div>
              <ul className="space-y-1.5 text-muted-foreground">
                {steps.map((s) => {
                  const Icon = s.icon;
                  return (
                    <li key={s.id} className="flex gap-2">
                      <Icon className="w-4 h-4 text-primary shrink-0 mt-0.5" />
                      <span><b className="text-foreground">{s.title}</b> — {s.blurb}</span>
                    </li>
                  );
                })}
              </ul>
            </div>
            <p className="text-xs text-muted-foreground">
              Every step can be skipped and re-run later — the app re-prompts you with the same panel
              exactly when it needs that piece. Nothing leaves your machine except calls you trigger.
            </p>
            <div className="flex justify-end">
              <Button onClick={next}>Let's go <ArrowRight className="w-4 h-4 ml-2" /></Button>
            </div>
          </div>
        )}

        {cur.kind === "step" && (
          <div className="fade-in space-y-5">
            <div className="flex items-center gap-3">
              <cur.step.icon className="w-7 h-7 text-primary" />
              <h1 className="text-3xl font-bold">{cur.step.title}</h1>
              {doneIds.has(cur.step.id) && <CheckCircle2 className="w-5 h-5 text-emerald-500" />}
            </div>
            {resume && <p className="text-sm text-muted-foreground -mt-2">This still needs setting up.</p>}
            {planReason(cur.step.id) && (
              <p className="text-sm text-primary/80 -mt-2">
                {optionalIds.has(cur.step.id) ? "Optional for your goal — " : ""}{planReason(cur.step.id)}
              </p>
            )}
            {settings === null ? (
              <div className="text-sm text-muted-foreground py-8">Loading…</div>
            ) : (
              <cur.step.Body
                settings={settings}
                onDone={async () => {
                  setDoneIds((d) => new Set(d).add(cur.step.id));
                  // The goal step's plan only exists in settings, so pick it up before moving on.
                  try { setSettings((await api.getSettings()) || {}); } catch { /* keep what we have */ }
                  next();
                }}
              />
            )}
            <div className="flex justify-between pt-2 border-t border-border/50">
              <Button variant="ghost" onClick={back} disabled={idx === 0}><ArrowLeft className="w-4 h-4 mr-2" />Back</Button>
              <Button variant="ghost" onClick={next}>Skip for now</Button>
            </div>
          </div>
        )}

        {cur.kind === "done" && (
          <div className="fade-in space-y-5 text-center">
            <Rocket className="w-12 h-12 text-primary mx-auto" />
            <h1 className="text-4xl font-bold">{resume ? "Good to go" : "You're all set"}</h1>
            <p className="text-muted-foreground max-w-md mx-auto">
              Anything still unset will be offered again the moment it's actually needed — no hunting
              through Settings.
            </p>
            {!resume && (
              <div className="rounded-lg border border-border bg-muted/30 p-4 text-sm text-left max-w-md mx-auto">
                <div className="font-medium mb-1">Next: your first project</div>
                <div className="text-muted-foreground">On the Dashboard, create a project, pick a book/chapter, and run the workflow — lyrics → music → images → video.</div>
              </div>
            )}
            <Button size="lg" onClick={finishAll} disabled={saving}>
              {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <>Go to the Dashboard<ArrowRight className="w-4 h-4 ml-2" /></>}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
