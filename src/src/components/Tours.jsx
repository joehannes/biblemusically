import { useState, useEffect, useRef, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { GUIDE_STEPS } from "../lib/guideSteps";
import Onboarding from "../pages/Onboarding";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Badge } from "./ui/badge";
import { toast } from "sonner";
import {
  GraduationCap, X, ArrowRight, ArrowLeft, Sparkles, Wrench, Map as MapIcon, Lightbulb,
  Wand2, Mic, MicOff, Loader2, CheckCircle2, RefreshCw,
} from "lucide-react";

// ─────────────────────────────────────────────────────────────────────────────
// Guided tours, launched any time from the menubar (GraduationCap button):
//   • welcome  — the full welcome & setup wizard (same one as first run).
//   • tune     — every setup panel again, pre-filled, for adjusting what's already configured.
//   • workflow — an explanatory walkthrough of the production pipeline, page by page.
//   • daily    — the AI-guided daily generation flow: reads the project brief + channels, has the
//                AI suggest today's angles as multi-choice tiles (typed or spoken input welcome),
//                saves the pick as the daily topic, and hands off to the Workflow runner.
// ─────────────────────────────────────────────────────────────────────────────

// ── Feature tour: each stop is a real page you're taken to while a coach card explains it. ──
const WORKFLOW_STOPS = [
  { title: "Dashboard & Project Brief", to: "/", icon: MapIcon, tag: "start here",
    body: "Every project begins here. Create one, then fill its Brief — mood, audience, aims, personality, and today's topic. This is the project's creative DNA: it quietly steers lyrics, styles, imagery and characters everywhere downstream, adapted to each channel's region, culture and language.",
    tip: "The Style Sample Studio here lets you preview any genre/style on your own engine." },
  { title: "Sources", to: "/bible", icon: MapIcon, tag: "raw material",
    body: "Pick what a song is built from: a scripture chapter from Bible Sources, or free material from the Freeform Composer. Either becomes the source text the AI Composer works from.",
    tip: "Non-Bible projects hide this tab and work straight from Freeform." },
  { title: "AI Composer", to: "/composer", icon: MapIcon, tag: "make songs",
    body: "Turns the source into one song per channel — lyrics in each channel's language, engine-aware section tags, and a distinct image idea per section. Review, tweak, and import them in a click.",
    tip: "It reads your Brief + live channel research, so each version speaks its audience's dialect." },
  { title: "Music Gen", to: "/music", icon: MapIcon, tag: "render audio",
    body: "Each imported song renders on your chosen engine. HeartMuLa / ACE-Step run on free Kaggle GPUs — started automatically right before generation and stopped when idle, to protect your weekly quota.",
    tip: "Styles are re-tuned per engine + channel, biased by your Brief and today's topic." },
  { title: "Images & Characters", to: "/images", icon: MapIcon, tag: "visuals",
    body: "Section images render from the per-section ideas. Click any image to open the explorer — compare variants side by side, generate alternates, set a primary. Characters keeps recurring figures consistent, and can adapt the same figure to each channel's visual culture.",
    tip: "Setting a 'primary' image quietly teaches the app your taste." },
  { title: "Video & Upload", to: "/video", icon: MapIcon, tag: "publish",
    body: "The Video Composer assembles sections, transitions and overlays into the finished video; Upload publishes to the right YouTube channel. The Workflow page runs this whole chain hands-off.",
    tip: "Servers auto-start before generation and auto-stop after — a run is end-to-end." },
];

// Maps an AI-named page to a real route + label, so the daily plan's "Open" buttons always land somewhere valid.
const PAGE_ROUTES = {
  dashboard: { to: "/", label: "Dashboard" },
  sources: { to: "/bible", label: "Sources" },
  composer: { to: "/composer", label: "AI Composer" },
  music: { to: "/music", label: "Music Gen" },
  images: { to: "/images", label: "Images" },
  video: { to: "/video", label: "Video & Upload" },
  workflow: { to: "/workflow", label: "Workflow" },
};
const DEFAULT_PLAN = [
  { label: "Confirm the brief", why: "Make sure mood, audience and topic reflect today's angle.", page: "dashboard" },
  { label: "Compose the songs", why: "Turn your source into a per-channel song set.", page: "composer" },
  { label: "Render the music", why: "Generate audio on your chosen engine.", page: "music" },
  { label: "Make the visuals", why: "Section images and consistent characters.", page: "images" },
  { label: "Assemble & publish", why: "Build the video and upload to the right channel.", page: "video" },
];

function WorkflowTour({ onClose }) {
  const [i, setI] = useState(0);
  const nav = useNavigate();
  const stop = WORKFLOW_STOPS[i];
  const last = i === WORKFLOW_STOPS.length - 1;

  // Show the ACTUAL page behind the coach card as each feature is introduced.
  useEffect(() => { nav(stop.to); /* eslint-disable-next-line */ }, [i]);
  useEffect(() => {
    const onKey = (e) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowRight" && !last) setI((x) => x + 1);
      else if (e.key === "ArrowLeft" && i > 0) setI((x) => x - 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [i, last, onClose]);

  const Icon = stop.icon;
  return (
    <>
      {/* Barely-there veil — you should mostly see the real feature through it. Click to dismiss. */}
      <div className="fixed inset-0 z-[68] bg-background/[0.06] backdrop-blur-[1.5px]" onClick={onClose} />

      {/* Floating coach card, low so it doesn't cover the page. Rich typography, not a wall of text. */}
      <div className="fixed inset-x-0 bottom-5 z-[69] flex justify-center px-4 pointer-events-none">
        <div className="pointer-events-auto w-[min(94vw,660px)] rounded-2xl border border-border bg-card/95 backdrop-blur-md shadow-2xl overflow-hidden">
          <div className="h-1 bg-muted">
            <div className="h-full bg-primary transition-all" style={{ width: `${((i + 1) / WORKFLOW_STOPS.length) * 100}%` }} />
          </div>
          <div className="p-5">
            <div className="flex items-center gap-2 mb-1.5">
              <span className="inline-flex items-center justify-center w-8 h-8 rounded-lg bg-primary/10 text-primary shrink-0"><Icon className="w-4 h-4" /></span>
              <div>
                <div className="text-[10px] uppercase tracking-[0.2em] text-primary/80 font-semibold">{stop.tag} · {i + 1}/{WORKFLOW_STOPS.length}</div>
                <h3 className="font-semibold text-lg leading-tight tracking-tight">{stop.title}</h3>
              </div>
              <button onClick={onClose} className="ml-auto p-1.5 rounded hover:bg-muted shrink-0" aria-label="Close tour"><X className="w-4 h-4" /></button>
            </div>
            <p className="text-sm text-muted-foreground leading-relaxed">{stop.body}</p>
            {stop.tip && (
              <div className="mt-2.5 flex items-start gap-2 text-xs text-foreground/80 bg-primary/5 border border-primary/15 rounded-lg px-2.5 py-1.5">
                <Sparkles className="w-3.5 h-3.5 text-primary shrink-0 mt-0.5" />
                <span>{stop.tip}</span>
              </div>
            )}
            <div className="flex items-center justify-between mt-4">
              <Button size="sm" variant="ghost" onClick={() => setI((x) => Math.max(0, x - 1))} disabled={i === 0}>
                <ArrowLeft className="w-3.5 h-3.5 mr-1.5" />Back
              </Button>
              <div className="flex items-center gap-1.5">
                {WORKFLOW_STOPS.map((_, k) => (
                  <button key={k} onClick={() => setI(k)} aria-label={`Step ${k + 1}`}
                    className={`w-1.5 h-1.5 rounded-full transition-colors ${k === i ? "bg-primary" : "bg-muted hover:bg-muted-foreground/40"}`} />
                ))}
              </div>
              {last ? (
                <Button size="sm" onClick={() => { onClose(); nav("/workflow"); }}>Run it<ArrowRight className="w-3.5 h-3.5 ml-1.5" /></Button>
              ) : (
                <Button size="sm" onClick={() => setI((x) => x + 1)}>Next<ArrowRight className="w-3.5 h-3.5 ml-1.5" /></Button>
              )}
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

// ── AI-guided daily generation ───────────────────────────────────────────────
function getRecognition() {
  const SR = typeof window !== "undefined" && (window.SpeechRecognition || window.webkitSpeechRecognition);
  if (!SR) return null;
  const rec = new SR();
  rec.continuous = false;
  rec.interimResults = false;
  return rec;
}

function DailyGuide({ onClose }) {
  const nav = useNavigate();
  const [phase, setPhase] = useState("loading"); // loading | pick | done | plan
  const [project, setProject] = useState(null);
  const [options, setOptions] = useState([]);
  const [note, setNote] = useState("");
  const [chosen, setChosen] = useState(null);
  const [busy, setBusy] = useState(false);
  const [listening, setListening] = useState(false);
  const [plan, setPlan] = useState([]);      // AI-proposed ordered steps for today
  const [planBusy, setPlanBusy] = useState(false);
  const recRef = useRef(null);

  // The AI suggests today's angles FROM the project's own brief + channels, so suggestions are
  // on-brand rather than generic. Free-text (typed or spoken) steers a re-roll.
  const suggest = useCallback(async (proj, userNote) => {
    setBusy(true);
    try {
      const r = await api.composeAssist({
        preset: "daily_angles",
        json_mode: true,
        temperature: 0.8,
        task:
          'Suggest 4 distinct "angles" for today\'s song/video generation for this project. ' +
          'Each angle: a short title (≤6 words), a one-sentence pitch, and a concrete daily_topic string usable as a generation steer. ' +
          'Vary tone across options (e.g. one comforting, one energetic, one reflective, one bold). ' +
          'Return JSON exactly: {"angles":[{"title":"…","pitch":"…","daily_topic":"…"}]}',
        context: {
          project_name: proj?.name,
          project_topic: proj?.topic,
          brief: proj?.brief || {},
          previous_daily_topic: proj?.daily_topic || "",
          user_direction: userNote || "",
          date: new Date().toDateString(),
        },
      });
      // compose_assist returns { text, json, model } — `json` is the extracted JSON value.
      let angles = r?.json?.angles;
      if (!angles?.length && r?.text) {
        try { angles = JSON.parse(r.text)?.angles; } catch { /* fall through */ }
      }
      if (!angles?.length) throw new Error(r?.error || "no angles returned");
      setOptions(angles.slice(0, 4));
      setPhase("pick");
    } catch (e) {
      toast.error(`Couldn't get suggestions: ${e}`);
      setPhase("pick"); // still let the user type their own
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const projects = (await api.listProjects()) || [];
        const pid = localStorage.getItem("studio:project");
        const proj = projects.find((p) => p.id === pid) || projects[0] || null;
        setProject(proj);
        if (!proj) { setPhase("pick"); return; }
        await suggest(proj, "");
      } catch { setPhase("pick"); }
    })();
  }, [suggest]);

  const toggleMic = () => {
    if (listening) { recRef.current?.stop(); setListening(false); return; }
    const rec = getRecognition();
    if (!rec) return toast.error("Speech input isn't available in this webview.");
    recRef.current = rec;
    rec.onresult = (ev) => {
      const text = Array.from(ev.results).map((r) => r[0].transcript).join(" ");
      setNote((n) => (n ? `${n} ${text}` : text));
    };
    rec.onend = () => setListening(false);
    rec.start();
    setListening(true);
  };

  const choose = async (opt) => {
    if (!project) return toast.error("Create a project first.");
    setChosen(opt);
    setBusy(true);
    try {
      await api.updateProject(project.id, {
        daily_topic: opt.daily_topic || opt.title,
        daily_topic_date: new Date().toISOString().slice(0, 10),
      });
      setPhase("done");
    } catch (e) {
      toast.error(`Couldn't save the topic: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  // Ask the AI to turn today's topic + this project into a short, ordered game plan — each step a
  // real page you can jump to, with a one-line reason. Whole-process guidance without taking over.
  const makePlan = useCallback(async (proj, topic) => {
    setPlanBusy(true);
    setPhase("plan");
    try {
      const r = await api.composeAssist({
        preset: "daily_plan",
        json_mode: true,
        temperature: 0.5,
        task:
          "Given today's topic and this project, produce a concise ordered plan (4–6 steps) to take it " +
          "from idea to a published music video. Each step: a short imperative `label` (≤6 words), a " +
          "one-sentence `why`, and a `page` chosen ONLY from this set: " +
          "dashboard, sources, composer, music, images, video, workflow. " +
          'Return JSON exactly: {"steps":[{"label":"…","why":"…","page":"composer"}]}',
        context: {
          project_name: proj?.name,
          brief: proj?.brief || {},
          channels: (proj?.channels || []).map((c) => c?.name).filter(Boolean),
          daily_topic: topic,
          source_type: proj?.source_type || (proj?.is_bible ? "bible" : "freeform"),
        },
      });
      let steps = r?.json?.steps;
      if (!steps?.length && r?.text) { try { steps = JSON.parse(r.text)?.steps; } catch { /* fall */ } }
      if (!steps?.length) throw new Error(r?.error || "no plan returned");
      setPlan(steps.slice(0, 6).map((s) => ({ ...s, done: false })));
    } catch (e) {
      // Fall back to a sensible fixed plan so the guide is still useful when the provider is down.
      toast.error(`Using a default plan (${e}).`);
      setPlan(DEFAULT_PLAN.map((s) => ({ ...s, done: false })));
    } finally {
      setPlanBusy(false);
    }
  }, []);

  return (
    <div className="space-y-4">
      {phase === "loading" && (
        <div className="flex items-center gap-2 text-sm text-muted-foreground py-8 justify-center">
          <Loader2 className="w-4 h-4 animate-spin" />Reading your project brief and drafting today's angles…
        </div>
      )}

      {phase === "pick" && (
        <>
          <p className="text-sm text-muted-foreground">
            {project
              ? <>Today's angles for <b className="text-foreground">{project.name}</b> — grounded in your brief. Pick one, or steer and re-roll.</>
              : "No project yet — create one on the Dashboard first, then run this guide."}
          </p>
          <div className="grid sm:grid-cols-2 gap-2.5">
            {options.map((o, idx) => (
              <button key={idx} onClick={() => choose(o)} disabled={busy}
                className="text-left rounded-lg border border-border p-3 hover:border-primary hover:bg-primary/5 transition-colors disabled:opacity-50">
                <div className="font-medium text-sm">{o.title}</div>
                <div className="text-xs text-muted-foreground mt-1">{o.pitch}</div>
              </button>
            ))}
            {!options.length && !busy && (
              <div className="text-xs text-muted-foreground col-span-2">No suggestions yet — type a direction below and re-roll, or just write your own topic and press Use.</div>
            )}
          </div>
          <div className="flex items-center gap-2">
            <Input value={note} onChange={(e) => setNote(e.target.value)}
              placeholder="Steer it (typed or spoken): e.g. something for a rainy Monday, focus on Psalms…"
              className="text-sm" />
            <Button size="sm" variant="outline" onClick={toggleMic} title="Speak your direction">
              {listening ? <MicOff className="w-3.5 h-3.5 text-destructive" /> : <Mic className="w-3.5 h-3.5" />}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => suggest(project, note)} disabled={busy || !project}>
              {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <RefreshCw className="w-3.5 h-3.5 mr-1.5" />}Re-roll
            </Button>
            <Button size="sm" onClick={() => choose({ title: note, daily_topic: note })} disabled={busy || !note.trim() || !project}>
              Use
            </Button>
          </div>
        </>
      )}

      {phase === "done" && (
        <div className="space-y-4 text-center py-4">
          <CheckCircle2 className="w-10 h-10 text-emerald-500 mx-auto" />
          <div>
            <div className="font-semibold">Today's topic is set</div>
            <div className="text-sm text-muted-foreground mt-1">“{chosen?.daily_topic || chosen?.title}”</div>
          </div>
          <p className="text-xs text-muted-foreground max-w-sm mx-auto">
            Every generation today — lyrics, styles, imagery — will lean into this angle, adapted per channel.
          </p>
          <div className="flex flex-col sm:flex-row items-center justify-center gap-2">
            <Button variant="outline" onClick={() => makePlan(project, chosen?.daily_topic || chosen?.title)}>
              <Wand2 className="w-4 h-4 mr-2" />Guide me step by step
            </Button>
            <Button onClick={() => { onClose(); nav("/workflow"); }}>
              Run it hands-off<ArrowRight className="w-4 h-4 ml-2" />
            </Button>
          </div>
        </div>
      )}

      {phase === "plan" && (
        <div className="space-y-3">
          <div className="flex items-start gap-2">
            <Wand2 className="w-4 h-4 text-primary mt-0.5 shrink-0" />
            <p className="text-sm text-muted-foreground">
              Your plan for <b className="text-foreground">“{chosen?.daily_topic || chosen?.title}”</b>. Do them in order, or jump to any — I'll take you there.
            </p>
          </div>
          {planBusy && (
            <div className="flex items-center gap-2 text-sm text-muted-foreground py-6 justify-center">
              <Loader2 className="w-4 h-4 animate-spin" />Thinking through the best order for today…
            </div>
          )}
          {!planBusy && (
            <ol className="space-y-2">
              {plan.map((s, idx) => {
                const route = PAGE_ROUTES[s.page] || PAGE_ROUTES.workflow;
                return (
                  <li key={idx} className="flex items-start gap-3 rounded-lg border border-border p-3">
                    <button
                      onClick={() => setPlan((p) => p.map((x, k) => (k === idx ? { ...x, done: !x.done } : x)))}
                      className={`mt-0.5 w-5 h-5 rounded-full border flex items-center justify-center shrink-0 transition-colors ${s.done ? "bg-emerald-500 border-emerald-500 text-white" : "border-muted-foreground/40 text-transparent hover:border-primary"}`}
                      aria-label="Toggle done">
                      <CheckCircle2 className="w-3.5 h-3.5" />
                    </button>
                    <div className="min-w-0 flex-1">
                      <div className={`font-medium text-sm ${s.done ? "line-through text-muted-foreground" : ""}`}>{idx + 1}. {s.label}</div>
                      <div className="text-xs text-muted-foreground mt-0.5">{s.why}</div>
                    </div>
                    <Button size="sm" variant="outline" className="shrink-0"
                      onClick={() => { onClose(); nav(route.to); }}>
                      {route.label}<ArrowRight className="w-3.5 h-3.5 ml-1.5" />
                    </Button>
                  </li>
                );
              })}
            </ol>
          )}
          {!planBusy && (
            <div className="flex items-center justify-between pt-1">
              <Button size="sm" variant="ghost" onClick={() => makePlan(project, chosen?.daily_topic || chosen?.title)}>
                <RefreshCw className="w-3.5 h-3.5 mr-1.5" />Re-plan
              </Button>
              <Button size="sm" onClick={() => { onClose(); nav("/workflow"); }}>
                Run it all hands-off<ArrowRight className="w-3.5 h-3.5 ml-1.5" />
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── AI-suggestions toggle (menubar) ──────────────────────────────────────────
// When enabled, the guide proactively pops up with contextual suggestions — currently: the AI
// daily-generation guide, once per day when a project exists and today's topic isn't set yet.
// The toggle persists in settings; off = the AI never pops up on its own.
export function SuggestionsToggle({ onAutoPop }) {
  const [enabled, setEnabled] = useState(false);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const s = (await api.getSettings()) || {};
        const on = s.ai_suggestions_enabled === true;
        setEnabled(on);
        setLoaded(true);
        // Auto-pop the daily guide at most once per calendar day, and only when it's useful:
        // a project exists and today's topic hasn't been chosen yet.
        if (!on) return;
        const today = new Date().toISOString().slice(0, 10);
        if (localStorage.getItem("studio:daily-pop") === today) return;
        const projects = (await api.listProjects()) || [];
        const pid = localStorage.getItem("studio:project");
        const proj = projects.find((p) => p.id === pid) || projects[0];
        if (!proj) return;
        if (proj.daily_topic_date === today) return; // already chosen today
        localStorage.setItem("studio:daily-pop", today);
        setTimeout(() => onAutoPop?.(), 1200); // after first paint, not in the user's face instantly
      } catch { setLoaded(true); }
    })();
  }, [onAutoPop]);

  const toggle = async () => {
    const next = !enabled;
    setEnabled(next);
    try { await api.saveSettings({ ai_suggestions_enabled: next }); } catch { /* ignore */ }
    toast.success(next ? "AI suggestions on — the guide may pop up with contextual ideas." : "AI suggestions off — the guide only opens manually.");
  };

  if (!loaded) return null;
  return (
    <button
      data-testid="suggestions-toggle"
      onClick={toggle}
      className={`p-1.5 rounded shrink-0 transition-colors ${enabled ? "text-primary hover:bg-primary/10" : "text-muted-foreground hover:bg-muted/30"}`}
      title={enabled ? "AI suggestions: ON (auto-pops contextual guidance) — click to disable" : "AI suggestions: OFF — click to enable"}
      aria-label="Toggle AI suggestions"
    >
      <Lightbulb className="w-4 h-4" />
    </button>
  );
}

// ── Registry + menubar button ────────────────────────────────────────────────
const TOURS = [
  { id: "welcome", title: "Welcome & setup", icon: Sparkles, blurb: "The first-run guide: AI key, GPU servers, YouTube, folders." },
  { id: "tune", title: "Tune & adjust", icon: Wrench, blurb: "Revisit every setup panel, pre-filled, to adjust what's configured." },
  { id: "workflow", title: "How a project flows", icon: MapIcon, blurb: "A guided explanation of the pipeline, page by page." },
  { id: "daily", title: "Today's generation (AI)", icon: Wand2, blurb: "AI suggests today's angles from your brief — pick, speak, or steer." },
];

export default function ToursFab() {
  const [menuOpen, setMenuOpen] = useState(false);
  const [active, setActive] = useState(null); // tour id
  const ref = useRef(null);

  useEffect(() => {
    if (!menuOpen) return;
    const onClick = (e) => { if (ref.current && !ref.current.contains(e.target)) setMenuOpen(false); };
    const onKey = (e) => { if (e.key === "Escape") setMenuOpen(false); };
    document.addEventListener("mousedown", onClick);
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("mousedown", onClick); document.removeEventListener("keydown", onKey); };
  }, [menuOpen]);

  const close = () => setActive(null);
  const tour = TOURS.find((t) => t.id === active);

  return (
    <div className="relative flex items-center" ref={ref}>
      <SuggestionsToggle onAutoPop={() => setActive("daily")} />
      <button
        data-testid="tours-fab"
        onClick={() => setMenuOpen((o) => !o)}
        className="p-1.5 rounded hover:bg-muted/30 shrink-0"
        title="Guided tours"
        aria-label="Guided tours"
      >
        <GraduationCap className="w-4 h-4" />
      </button>

      {menuOpen && (
        <div className="absolute right-0 top-full mt-2 z-50 w-72 rounded-lg border border-border bg-card shadow-xl p-1.5">
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground px-1.5 pb-1 flex items-center gap-1.5">
            <GraduationCap className="w-3 h-3" /> Guided tours
          </div>
          {TOURS.map((t) => (
            <button key={t.id} onClick={() => { setMenuOpen(false); setActive(t.id); }}
              className="w-full text-left px-2 py-2 rounded hover:bg-muted/50 flex gap-2.5 items-start">
              <t.icon className="w-4 h-4 text-primary shrink-0 mt-0.5" />
              <span>
                <span className="block text-sm font-medium">{t.title}</span>
                <span className="block text-[11px] text-muted-foreground">{t.blurb}</span>
              </span>
            </button>
          ))}
        </div>
      )}

      {/* Full-screen wizard tours reuse the Onboarding component as-is. */}
      {(active === "welcome" || active === "tune") && (
        <div className="fixed inset-0 z-[70] bg-background overflow-y-auto">
          <button onClick={close} className="fixed top-4 right-4 z-[71] p-2 rounded-full border border-border bg-card hover:bg-muted" aria-label="Close tour">
            <X className="w-4 h-4" />
          </button>
          <Onboarding
            steps={GUIDE_STEPS}
            resume={active === "tune"}
            onDone={close}
            onSkip={close}
          />
        </div>
      )}

      {/* Feature tour: navigates the real pages and floats a coach card over a barely-there blur,
          so you actually SEE each feature as it's introduced. */}
      {active === "workflow" && <WorkflowTour onClose={close} />}

      {/* Daily guide stays a focused centered dialog. */}
      {active === "daily" && (
        <div className="fixed inset-0 z-[70] flex items-center justify-center p-4">
          <button className="absolute inset-0 bg-background/70 backdrop-blur-sm" onClick={close} aria-label="Close tour" />
          <div className="relative w-full max-w-xl rounded-xl border border-border bg-card shadow-2xl p-5 max-h-[85vh] overflow-y-auto">
            <div className="flex items-center justify-between mb-4">
              <div className="flex items-center gap-2">
                {tour && <tour.icon className="w-4 h-4 text-primary" />}
                <h2 className="font-semibold">{tour?.title}</h2>
              </div>
              <button onClick={close} className="p-1.5 rounded hover:bg-muted" aria-label="Close"><X className="w-4 h-4" /></button>
            </div>
            <DailyGuide onClose={close} />
          </div>
        </div>
      )}
    </div>
  );
}
