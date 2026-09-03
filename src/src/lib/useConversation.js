import { useCallback, useEffect, useRef, useState } from "react";
import { speak, speakInterruptible, stopSpeaking, listen, interpretAnswer, voiceInputAvailable }
  from "./voice";
import { decide, speakableStep, confirmationOf } from "./conversation";

// The hands-free loop, as a hook.
//
// speak → listen → interpret → decide → apply → speak the next. The decisions are pure and live in
// conversation.js; everything here is the plumbing that is hard to test and easy to get wrong:
// cancelling cleanly when the step changes, never running two loops at once, and putting the
// microphone down the moment the mode is switched off.
//
// It is opt-in and one click ends it, because an assistant that keeps talking is worse than one that
// never starts — and it is exactly when you want to stop it that being unable to is intolerable.

export function useConversation({ step, options, recommendedId, onChoose, onSkip, enabled }) {
  const [phase, setPhase] = useState("idle");   // idle | speaking | listening | thinking
  const run = useRef(0);                        // bumped to cancel whatever is in flight
  const misses = useRef(0);
  const doneFor = useRef(null);

  const stop = useCallback(() => {
    run.current += 1;
    stopSpeaking();
    setPhase("idle");
  }, []);

  // Leaving the mode, or the component, must put the microphone down — not on the next tick.
  useEffect(() => stop, [stop]);
  useEffect(() => { if (!enabled) stop(); }, [enabled, stop]);

  const stepKey = step ? `${step.id}` : null;
  useEffect(() => { misses.current = 0; }, [stepKey]);

  useEffect(() => {
    if (!enabled || !step || !voiceInputAvailable()) return;
    // One pass per step. Without this, every re-render (and answering itself changes state) would
    // start a second loop talking over the first.
    if (doneFor.current === stepKey) return;
    doneFor.current = stepKey;

    const ticket = ++run.current;
    const live = () => ticket === run.current;

    (async () => {
      // Up to MAX_MISSES attempts, then the question goes back to the person. The bound is in
      // `decide`; this loop just honours what it returns.
      for (;;) {
        if (!live()) return;

        setPhase("speaking");
        const line = speakableStep({ question: step.question, options }, { max: 4 });
        const suggestion = options.find((o) => o.id === recommendedId);
        await speakInterruptible([line, suggestion ? `I'd suggest ${suggestion.label}.` : ""]
          .filter(Boolean).join(" ")).catch(() => {});
        if (!live()) return;

        setPhase("listening");
        const heard = await listen({ maxMs: 12000 }).catch(() => null);
        if (!live()) return;

        setPhase("thinking");
        const match = heard
          ? await interpretAnswer(heard, options, { recommended: recommendedId, question: step.question })
              .catch(() => null)
          : null;
        if (!live()) return;

        const d = decide({ heard, match, misses: misses.current });
        misses.current = d.misses;

        if (d.action === "apply") {
          const chosen = options.find((o) => o.id === d.option);
          setPhase("idle");
          if (chosen) onChoose?.(chosen, { spoken: heard });
          return;
        }
        if (d.action === "skip") {
          setPhase("idle");
          await speak(d.say).catch(() => {});
          onSkip?.();
          return;
        }
        if (d.action === "confirm") {
          // Asked in two utterances so both halves are catalogue lookups: the phrase ships
          // translated, and the option's label is already in the inventory from the page it is on.
          const { ask, label } = confirmationOf({ options }, d.option);
          setPhase("speaking");
          await speakInterruptible(ask).catch(() => {});
          if (!live()) return;
          if (label) await speakInterruptible(label).catch(() => {});
          if (!live()) return;
          setPhase("listening");
          const yes = await listen({ maxMs: 6000 }).catch(() => null);
          if (!live()) return;
          const agreed = await interpretAnswer(yes || "", options, {
            recommended: d.option, question: `${ask} ${label}`,
          }).catch(() => null);
          if (!live()) return;
          if (agreed?.option === d.option) {
            const chosen = options.find((o) => o.id === d.option);
            setPhase("idle");
            if (chosen) onChoose?.(chosen, { spoken: yes || heard });
            return;
          }
          // Not confirmed: that counts as one miss and the question comes round again.
          misses.current += 1;
          if (misses.current < 2) continue;
          setPhase("idle");
          return;
        }
        if (d.action === "hand_back") {
          setPhase("idle");
          await speak(d.say).catch(() => {});
          return;   // the buttons are still there; the loop simply stops pushing
        }
        // reask
        await speak(d.say).catch(() => {});
        if (!live()) return;
      }
    })();
  }, [enabled, step, stepKey, options, recommendedId, onChoose, onSkip]);

  // A new step is a new pass. Cleared here rather than in the effect above so that re-running the
  // same step (after a hand-back, say) needs a deliberate restart rather than happening by itself.
  useEffect(() => { if (stepKey && doneFor.current !== stepKey) setPhase("idle"); }, [stepKey]);

  const restart = useCallback(() => {
    doneFor.current = null;
    misses.current = 0;
    run.current += 1;
    setPhase("idle");
  }, []);

  return { phase, stop, restart, active: phase !== "idle" };
}
