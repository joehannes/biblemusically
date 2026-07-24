import { useState, useEffect } from "react";
import { api } from "../lib/api";
import { GUIDE_STEPS } from "../lib/guideSteps";
import { Button } from "../components/ui/button";
import {
  Sparkles, Bot, Cpu, FolderOpen, CheckCircle2,
  ArrowRight, ArrowLeft, Rocket, Loader2,
} from "lucide-react";

// First-run guided setup. The individual steps live in lib/guideSteps.jsx so the SAME panels can be
// re-opened later on demand (see components/GuideStepDialog.jsx) — e.g. the publish flow re-runs
// just the YouTube step if it was skipped here. This file only sequences them.

export default function Onboarding({ onDone }) {
  // index 0 = welcome, 1..N = GUIDE_STEPS, N+1 = done
  const [step, setStep] = useState(0);
  const [saving, setSaving] = useState(false);
  const [settings, setSettings] = useState(null);
  const [doneIds, setDoneIds] = useState(() => new Set());

  const total = GUIDE_STEPS.length + 2;

  useEffect(() => {
    (async () => {
      try { setSettings((await api.getSettings()) || {}); } catch { setSettings({}); }
    })();
  }, []);

  const next = () => setStep((s) => Math.min(s + 1, total - 1));
  const back = () => setStep((s) => Math.max(s - 1, 0));

  const finishAll = async () => {
    setSaving(true);
    try { await api.saveSettings({ onboarded: true }); } catch { /* ignore */ }
    setSaving(false);
    onDone?.();
  };

  const guideIdx = step - 1;
  const current = guideIdx >= 0 && guideIdx < GUIDE_STEPS.length ? GUIDE_STEPS[guideIdx] : null;

  return (
    <div className="min-h-screen w-full bg-background text-foreground flex flex-col items-center overflow-y-auto">
      <div className="w-full max-w-2xl px-6 py-10">
        {/* progress rail */}
        <div className="flex items-center gap-1.5 mb-8">
          {Array.from({ length: total }).map((_, i) => (
            <div key={i} className={`h-1.5 rounded-full flex-1 transition-colors ${i <= step ? "bg-primary" : "bg-muted"}`} />
          ))}
        </div>
        <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">
          Setup · step {step + 1}/{total}
        </div>

        {/* ── Welcome ─────────────────────────────────────────── */}
        {step === 0 && (
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
                {GUIDE_STEPS.map((s) => {
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
              Every step can be skipped and re-run later — the app will re-prompt you with the same
              panel exactly when it needs that piece. Nothing leaves your machine except calls you trigger.
            </p>
            <div className="flex justify-end">
              <Button onClick={next}>Let's go <ArrowRight className="w-4 h-4 ml-2" /></Button>
            </div>
          </div>
        )}

        {/* ── One guided step ─────────────────────────────────── */}
        {current && (
          <div className="fade-in space-y-5">
            <div className="flex items-center gap-3">
              <current.icon className="w-7 h-7 text-primary" />
              <h1 className="text-3xl font-bold">{current.title}</h1>
              {doneIds.has(current.id) && <CheckCircle2 className="w-5 h-5 text-emerald-500" />}
            </div>
            {settings === null ? (
              <div className="text-sm text-muted-foreground py-8">Loading…</div>
            ) : (
              <current.Body
                settings={settings}
                onDone={() => { setDoneIds((d) => new Set(d).add(current.id)); next(); }}
              />
            )}
            <div className="flex justify-between pt-2 border-t border-border/50">
              <Button variant="ghost" onClick={back}><ArrowLeft className="w-4 h-4 mr-2" />Back</Button>
              <Button variant="ghost" onClick={next}>Skip for now</Button>
            </div>
          </div>
        )}

        {/* ── Done ────────────────────────────────────────────── */}
        {step === total - 1 && (
          <div className="fade-in space-y-5 text-center">
            <Rocket className="w-12 h-12 text-primary mx-auto" />
            <h1 className="text-4xl font-bold">You're all set</h1>
            <p className="text-muted-foreground max-w-md mx-auto">
              Anything you skipped will be offered again the moment it's actually needed — no hunting
              through Settings.
            </p>
            <div className="rounded-lg border border-border bg-muted/30 p-4 text-sm text-left max-w-md mx-auto">
              <div className="font-medium mb-1">Next: your first project</div>
              <div className="text-muted-foreground">On the Dashboard, create a project, pick a book/chapter, and run the workflow — lyrics → music → images → video.</div>
            </div>
            <Button size="lg" onClick={finishAll} disabled={saving}>
              {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <>Go to the Dashboard<ArrowRight className="w-4 h-4 ml-2" /></>}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
