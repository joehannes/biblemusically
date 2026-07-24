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

// ── Workflow explainer content ───────────────────────────────────────────────
const WORKFLOW_STOPS = [
  { title: "Project & brief", to: "/", body: "Everything starts on the Dashboard: create a project and fill its Brief — mood, audience, goals, personality, and today's topic. The brief steers every generation downstream, adapted per channel's region, culture and language." },
  { title: "Sources", to: "/bible", body: "Pick raw material: a Bible chapter from Bible Sources, or free material from the Freeform Composer. Either lands in the AI Composer as the source text." },
  { title: "Compose", to: "/composer", body: "The AI Composer turns the source into per-channel songs — lyrics in each channel's language, engine-aware section tags, and per-section image ideas. Review and import." },
  { title: "Music", to: "/music", body: "Music Gen renders each song on your selected engine (HeartMuLa/ACE-Step run on free Kaggle GPUs — started automatically when needed, stopped when idle to save your quota)." },
  { title: "Images & characters", to: "/images", body: "Section images render per the annotations; Characters keeps recurring figures consistent across images via reference-driven generation." },
  { title: "Video & upload", to: "/video", body: "The Video Composer assembles sections, transitions and overlays into the final video; Upload publishes to the right YouTube channel. The Workflow page runs this whole chain hands-off." },
];

function WorkflowTour({ onClose }) {
  const [i, setI] = useState(0);
  const nav = useNavigate();
  const stop = WORKFLOW_STOPS[i];
  return (
    <div className="space-y-4">
      <div className="flex items-center gap-1.5">
        {WORKFLOW_STOPS.map((_, k) => (
          <div key={k} className={`h-1 rounded-full flex-1 ${k <= i ? "bg-primary" : "bg-muted"}`} />
        ))}
      </div>
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <Badge variant="secondary" className="text-[10px]">{i + 1}/{WORKFLOW_STOPS.length}</Badge>
          <h3 className="font-semibold text-lg">{stop.title}</h3>
        </div>
        <p className="text-sm text-muted-foreground leading-relaxed">{stop.body}</p>
      </div>
      <div className="flex items-center justify-between">
        <Button size="sm" variant="ghost" onClick={() => setI((x) => Math.max(0, x - 1))} disabled={i === 0}>
          <ArrowLeft className="w-3.5 h-3.5 mr-1.5" />Back
        </Button>
        <Button size="sm" variant="outline" onClick={() => { onClose(); nav(stop.to); }}>
          Open this page
        </Button>
        {i < WORKFLOW_STOPS.length - 1 ? (
          <Button size="sm" onClick={() => setI((x) => x + 1)}>Next<ArrowRight className="w-3.5 h-3.5 ml-1.5" /></Button>
        ) : (
          <Button size="sm" onClick={() => { onClose(); nav("/workflow"); }}>Go run it<ArrowRight className="w-3.5 h-3.5 ml-1.5" /></Button>
        )}
      </div>
    </div>
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
  const [phase, setPhase] = useState("loading"); // loading | pick | refine | done
  const [project, setProject] = useState(null);
  const [options, setOptions] = useState([]);
  const [note, setNote] = useState("");
  const [chosen, setChosen] = useState(null);
  const [busy, setBusy] = useState(false);
  const [listening, setListening] = useState(false);
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
          <Button onClick={() => { onClose(); nav("/workflow"); }}>
            Run today's workflow<ArrowRight className="w-4 h-4 ml-2" />
          </Button>
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
        <div className="absolute right-0 mt-2 z-40 w-72 rounded-lg border border-border bg-card shadow-xl p-1.5">
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

      {/* Dialog tours */}
      {(active === "workflow" || active === "daily") && (
        <div className="fixed inset-0 z-[70] flex items-center justify-center p-4">
          <button className="absolute inset-0 bg-background/70 backdrop-blur-sm" onClick={close} aria-label="Close tour" />
          <div className="relative w-full max-w-xl rounded-xl border border-border bg-card shadow-2xl p-5">
            <div className="flex items-center justify-between mb-4">
              <div className="flex items-center gap-2">
                {tour && <tour.icon className="w-4 h-4 text-primary" />}
                <h2 className="font-semibold">{tour?.title}</h2>
              </div>
              <button onClick={close} className="p-1.5 rounded hover:bg-muted" aria-label="Close"><X className="w-4 h-4" /></button>
            </div>
            {active === "workflow" && <WorkflowTour onClose={close} />}
            {active === "daily" && <DailyGuide onClose={close} />}
          </div>
        </div>
      )}
    </div>
  );
}
