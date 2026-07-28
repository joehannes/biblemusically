import { useState, useEffect, useCallback } from "react";
import { api } from "../lib/api";
import { getGuideStep } from "../lib/guideSteps";
import { Button } from "./ui/button";
import { X } from "lucide-react";

// Renders ONE guided-setup step in a modal, so a prerequisite can be satisfied in place instead of
// sending the user off to Settings and losing their flow. Pair with `useRequireGuideStep` below:
//
//   const { ensure, dialog } = useRequireGuideStep("youtube");
//   ...  if (!(await ensure())) return;   // opens the step, resolves false if unmet
//   ...  {dialog}
export default function GuideStepDialog({ stepId, open, onClose, onDone }) {
  const step = getGuideStep(stepId);
  const [settings, setSettings] = useState(null);

  useEffect(() => {
    if (!open) return;
    (async () => {
      try { setSettings((await api.getSettings()) || {}); } catch { setSettings({}); }
    })();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e) => { if (e.key === "Escape") onClose?.(); };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open || !step || !step.Body) return null;
  const Icon = step.icon;
  const Body = step.Body;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center p-4">
      <button className="absolute inset-0 bg-background/80 backdrop-blur-sm" onClick={onClose} aria-label="Close" />
      <div className="relative z-10 w-full max-w-2xl max-h-[85vh] overflow-y-auto scroll-thin rounded-xl border border-border bg-card shadow-2xl p-6">
        <div className="flex items-start justify-between gap-3 mb-4">
          <div className="flex items-center gap-2.5">
            {Icon && <Icon className="w-6 h-6 text-primary" />}
            <div>
              <h2 className="text-xl font-bold">{step.title}</h2>
              <p className="text-xs text-muted-foreground">{step.blurb}</p>
            </div>
          </div>
          <button onClick={onClose} className="p-1 rounded hover:bg-muted/50 text-muted-foreground shrink-0" aria-label="Close">
            <X className="w-4 h-4" />
          </button>
        </div>
        {settings === null ? (
          <div className="text-sm text-muted-foreground py-6 text-center">Loading…</div>
        ) : (
          // Same `save` the wizard passes — the terms step is reachable from here too, and was
          // equally unusable without it.
          <Body
            settings={settings}
            save={async (patch) => {
              try {
                await api.saveSettings(patch);
                setSettings((await api.getSettings()) || {});
              } catch (err) { console.warn("guide step save failed", err); }
            }}
            onDone={() => { onDone?.(); onClose?.(); }}
          />
        )}
      </div>
    </div>
  );
}

/**
 * Gate an action behind a guided step. `ensure()` resolves true when the step's prerequisite is
 * already satisfied; otherwise it opens the step's dialog and resolves false so the caller can
 * abort cleanly. Render `dialog` somewhere in the component.
 */
export function useRequireGuideStep(stepId) {
  const [open, setOpen] = useState(false);
  const step = getGuideStep(stepId);

  const ensure = useCallback(async () => {
    let settings = {};
    try { settings = (await api.getSettings()) || {}; } catch { return true; } // fail open
    let done = false;
    try { done = !!step?.isDone(settings); } catch { done = true; }
    if (!done) setOpen(true);
    return done;
  }, [step]);

  const dialog = <GuideStepDialog stepId={stepId} open={open} onClose={() => setOpen(false)} />;
  return { ensure, dialog, openStep: () => setOpen(true) };
}
