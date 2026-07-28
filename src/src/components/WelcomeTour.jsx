import { useState, useEffect, useRef, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { WELCOME_STOPS, storyFor, dwellMs, DEFAULT_LEVEL } from "../lib/welcomeStory";
import { speak, stopSpeaking, voicePrefs, setVoicePrefs } from "../lib/voice";
import { Button } from "./ui/button";
import { X, ArrowRight, ArrowLeft, Volume2, VolumeX, Sparkles, Compass } from "lucide-react";

// ─────────────────────────────────────────────────────────────────────────────
// The welcome tour: a guided look around the real app.
//
// It navigates the actual pages rather than showing screenshots, because the point is that the
// interface stops being unfamiliar — a picture of a page you have never opened does not do that.
// What it adds on top is:
//
//   • a blur veil, so you see the *shape* of each page without being asked to read it;
//   • a spotlight cut out of that veil around the thing being talked about;
//   • every control blocked, so nothing can be triggered by accident mid-explanation;
//   • narration in whatever voice was chosen a minute ago, using the system voice by default
//     so it costs no AI requests;
//   • a deliberate pause on arrival, before "Next" becomes available, so there is time to look.
//
// It generates nothing and saves nothing except the fact that it was seen.
// ─────────────────────────────────────────────────────────────────────────────

const PAD = 8;          // breathing room around a spotlit element
const SETTLE_MS = 260;  // let the route render before measuring anything

/** Find the on-screen element for a stop, preferring whichever nav item is actually visible. */
function locate(route) {
  if (typeof document === "undefined") return null;
  const candidates = Array.from(document.querySelectorAll(`[data-tour-nav="${route}"]`));
  for (const el of candidates) {
    const r = el.getBoundingClientRect();
    // Both the desktop sidebar and the mobile bar exist in the DOM; only one has a size.
    if (r.width > 0 && r.height > 0) return el;
  }
  return null;
}

export default function WelcomeTour({ level: levelProp, onClose, onDone }) {
  const [i, setI] = useState(0);
  const [level, setLevel] = useState(levelProp || null);
  const [rect, setRect] = useState(null);
  const [ready, setReady] = useState(false);      // has the look-around pause elapsed?
  const [speaking, setSpeaking] = useState(false);
  const [muted, setMuted] = useState(() => !voicePrefs().speak);
  const nav = useNavigate();
  const stopId = useRef(0);                       // guards against a stale narration resolving late

  const stop = WELCOME_STOPS[i];
  const story = storyFor(stop, level || DEFAULT_LEVEL);
  const last = i === WELCOME_STOPS.length - 1;

  // The level may not have been passed in (tour reopened from the menu) — read it once.
  useEffect(() => {
    if (levelProp) return;
    let alive = true;
    api.getSettings()
      .then((s) => { if (alive) setLevel(s?.audience_level || DEFAULT_LEVEL); })
      .catch(() => { if (alive) setLevel(DEFAULT_LEVEL); });
    return () => { alive = false; };
  }, [levelProp]);

  const finish = useCallback(async (completed) => {
    stopSpeaking();
    try { await api.saveSettings({ welcome_tour_done: true }); } catch { /* never block on this */ }
    if (completed && onDone) onDone();
    else onClose?.();
  }, [onClose, onDone]);

  // ── Walk to the page, measure the thing we're talking about, and hold there a moment. ──
  useEffect(() => {
    if (!level) return;                            // wait for the level before saying anything
    const token = ++stopId.current;
    setReady(false);
    setRect(null);
    nav(stop.to);

    const settle = setTimeout(() => {
      if (stopId.current !== token) return;
      const el = locate(stop.to);
      if (el) {
        const r = el.getBoundingClientRect();
        setRect({ top: r.top, left: r.left, width: r.width, height: r.height });
      }
    }, SETTLE_MS);

    // The pause happens whether or not the voice is on: it is there to give the eye time, and a
    // muted tour that raced ahead would defeat the whole point of showing the page at all.
    const pause = setTimeout(() => { if (stopId.current === token) setReady(true); }, dwellMs(story.body));

    return () => { clearTimeout(settle); clearTimeout(pause); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [i, level]);

  // ── Narration, deliberately separate. ──
  // Muting is a decision about the voice, not about the stop: folding it into the effect above would
  // make "be quiet" also mean "navigate again and restart the pause", which reads as a glitch.
  useEffect(() => {
    if (!level || muted) return;
    const token = stopId.current;
    let cancelled = false;
    setSpeaking(true);
    (async () => {
      try {
        await speak(`${story.title}. ${story.body}`, { force: true });
      } catch { /* silence is always an acceptable outcome here */ }
      if (cancelled || stopId.current !== token) return;
      setSpeaking(false);
      setReady(true);                              // finished talking is also permission to move on
    })();
    return () => { cancelled = true; setSpeaking(false); stopSpeaking(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [i, level, muted]);

  // Keep the spotlight on target when the window changes shape under it.
  useEffect(() => {
    const remeasure = () => {
      const el = locate(stop.to);
      if (!el) return setRect(null);
      const r = el.getBoundingClientRect();
      setRect({ top: r.top, left: r.left, width: r.width, height: r.height });
    };
    window.addEventListener("resize", remeasure);
    window.addEventListener("scroll", remeasure, true);
    return () => {
      window.removeEventListener("resize", remeasure);
      window.removeEventListener("scroll", remeasure, true);
    };
  }, [stop.to]);

  useEffect(() => {
    const onKey = (e) => {
      if (e.key === "Escape") finish(false);
      else if (e.key === "ArrowRight" && ready && !last) setI((x) => x + 1);
      else if (e.key === "ArrowLeft" && i > 0) setI((x) => x - 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [i, last, ready, finish]);

  const toggleMute = () => {
    const next = !muted;
    setMuted(next);
    if (next) { stopSpeaking(); setSpeaking(false); }
    // Remember it: someone who silences the tour on stop two meant it for the whole app.
    setVoicePrefs({ speak: !next });
  };

  // ── The veil ───────────────────────────────────────────────────────────────
  // Four panels around the spotlight rather than one panel with a hole, because `backdrop-filter`
  // cannot be masked — a "hole" punched with a border-radius still blurs what is behind it.
  const veil = "fixed bg-background/25 backdrop-blur-[3px] transition-all duration-300";
  const panels = rect
    ? [
        { key: "t", style: { top: 0, left: 0, right: 0, height: Math.max(0, rect.top - PAD) } },
        { key: "b", style: { top: rect.top + rect.height + PAD, left: 0, right: 0, bottom: 0 } },
        { key: "l", style: { top: Math.max(0, rect.top - PAD), left: 0, width: Math.max(0, rect.left - PAD), height: rect.height + PAD * 2 } },
        { key: "r", style: { top: Math.max(0, rect.top - PAD), left: rect.left + rect.width + PAD, right: 0, height: rect.height + PAD * 2 } },
      ]
    : [{ key: "all", style: { inset: 0 } }];

  return (
    <>
      {/* Swallows every click and tap: during the tour the app is a diagram, not a control panel. */}
      <div className="fixed inset-0 z-[74]" onClick={(e) => e.preventDefault()} aria-hidden="true" />

      {panels.map((p) => (
        <div key={p.key} className={`${veil} z-[75] pointer-events-none`} style={p.style} />
      ))}

      {/* The spotlight ring — drawn, never filled, so the real element stays fully legible. */}
      {rect && (
        <div
          className="fixed z-[76] rounded-lg ring-2 ring-primary/70 shadow-[0_0_0_4px_hsl(var(--primary)/0.15)] pointer-events-none transition-all duration-300"
          style={{ top: rect.top - PAD, left: rect.left - PAD, width: rect.width + PAD * 2, height: rect.height + PAD * 2 }}
        />
      )}

      {/* Coach card, anchored low so the page stays the subject of the sentence. On a phone it sits
          above the fixed bottom nav bar rather than on top of it — that bar is often what is being
          spotlit. */}
      <div className="fixed inset-x-0 bottom-[calc(4.5rem+env(safe-area-inset-bottom))] md:bottom-4 z-[77] flex justify-center px-3 pointer-events-none">
        <div className="pointer-events-auto w-[min(94vw,640px)] rounded-2xl border border-border bg-card/95 backdrop-blur-md shadow-2xl overflow-hidden">
          <div className="h-1 bg-muted">
            <div className="h-full bg-primary transition-all duration-500"
                 style={{ width: `${((i + 1) / WELCOME_STOPS.length) * 100}%` }} />
          </div>

          <div className="p-5">
            <div className="flex items-center gap-2 mb-1.5">
              <span className="inline-flex items-center justify-center w-8 h-8 rounded-lg bg-primary/10 text-primary shrink-0">
                <Compass className="w-4 h-4" />
              </span>
              <div className="min-w-0">
                <div className="text-[10px] uppercase tracking-[0.2em] text-primary/80 font-semibold">
                  {story.tag} · {i + 1}/{WELCOME_STOPS.length}
                </div>
                <h3 className="font-semibold text-lg leading-tight tracking-tight truncate">{story.title}</h3>
              </div>
              <button onClick={toggleMute}
                      className="ml-auto p-1.5 rounded hover:bg-muted shrink-0"
                      title={muted ? "Read the tour aloud" : "Silence the tour"}
                      aria-label={muted ? "Turn narration on" : "Turn narration off"}>
                {muted ? <VolumeX className="w-4 h-4 text-muted-foreground" />
                       : <Volume2 className={`w-4 h-4 ${speaking ? "text-primary animate-pulse" : ""}`} />}
              </button>
              <button onClick={() => finish(false)} className="p-1.5 rounded hover:bg-muted shrink-0" aria-label="Close the tour">
                <X className="w-4 h-4" />
              </button>
            </div>

            <p className="text-sm text-muted-foreground leading-relaxed">{story.body}</p>

            {story.tip && (
              <div className="mt-2.5 flex items-start gap-2 text-xs text-foreground/80 bg-primary/5 border border-primary/15 rounded-lg px-2.5 py-1.5">
                <Sparkles className="w-3.5 h-3.5 text-primary shrink-0 mt-0.5" />
                <span>{story.tip}</span>
              </div>
            )}

            <div className="flex items-center justify-between mt-4 gap-2">
              <Button size="sm" variant="ghost" onClick={() => setI((x) => Math.max(0, x - 1))} disabled={i === 0}>
                <ArrowLeft className="w-3.5 h-3.5 mr-1.5" />Back
              </Button>

              <div className="flex items-center gap-1.5">
                {WELCOME_STOPS.map((_, k) => (
                  <span key={k} aria-hidden="true"
                        className={`w-1.5 h-1.5 rounded-full transition-colors ${k === i ? "bg-primary" : k < i ? "bg-primary/40" : "bg-muted"}`} />
                ))}
              </div>

              {last ? (
                <Button size="sm" onClick={() => finish(true)} disabled={!ready}>
                  Start using it<ArrowRight className="w-3.5 h-3.5 ml-1.5" />
                </Button>
              ) : (
                <Button size="sm" onClick={() => setI((x) => x + 1)} disabled={!ready}>
                  {ready ? <>Next<ArrowRight className="w-3.5 h-3.5 ml-1.5" /></> : <span className="opacity-70">have a look…</span>}
                </Button>
              )}
            </div>

            <button onClick={() => finish(false)}
                    className="mt-2 w-full text-center text-[11px] text-muted-foreground hover:text-foreground transition-colors">
              Skip the tour — I'll find my own way around
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
