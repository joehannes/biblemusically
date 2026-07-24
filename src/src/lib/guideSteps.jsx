import { useState, useRef, useEffect } from "react";
import { api } from "./api";
import { autoStartKaggleServer } from "./kaggleServerPipeline";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Label } from "../components/ui/label";
import { toast } from "sonner";
import {
  Bot, KeyRound, Cpu, FolderOpen, Youtube, CheckCircle2, ExternalLink,
  Loader2, Music2, Image as Img, Upload,
} from "lucide-react";
import YouTubeStepBody from "./guideYouTube";

// ─────────────────────────────────────────────────────────────────────────────
// Guided setup steps, defined ONCE and reusable in two places:
//   1. the first-run wizard (Onboarding.jsx) runs them in sequence, and
//   2. any page can re-open a single step on demand via <GuideStepDialog stepId=…>
//      — e.g. the workflow checks `isDone` for the YouTube step before publishing
//      and, if it isn't satisfied, guides the user through just that step.
//
// Each step owns its own state and persistence, so it behaves identically in both
// contexts. `isDone(settings)` is a pure check against the settings singleton so a
// caller can test a prerequisite without rendering anything.
// ─────────────────────────────────────────────────────────────────────────────

const MUSIC_ENGINES = [
  { id: "heartmula", label: "HeartMuLa", hint: "Free, Apache-2.0 — YouTube-safe. Recommended." },
  { id: "acestep", label: "ACE-Step", hint: "Free, self-hosted alternative." },
  { id: "suno", label: "Suno", hint: "Highest quality, needs a Suno account cookie." },
];
const IMAGE_ENGINES = [
  { id: "comfyui", label: "ComfyUI", hint: "Free, multi-model (SDXL, character consistency). Recommended." },
  { id: "flux", label: "FLUX", hint: "Free, top-tier photoreal stills (needs a HF token)." },
  { id: "midjourney", label: "Midjourney", hint: "Highest quality, needs a Midjourney account." },
];

const persist = async (patch) => {
  try { await api.saveSettings(patch); } catch (e) { console.warn("guide step save failed", e); }
};

// ── Step: AI provider ────────────────────────────────────────────────────────
function AiStepBody({ settings, onDone }) {
  const [provider, setProvider] = useState(settings?.ai_provider || "openrouter");
  const [orKey, setOrKey] = useState(settings?.openrouter_api_key || "");
  const [gemKey, setGemKey] = useState(settings?.gemini_api_key || "");
  const [saving, setSaving] = useState(false);

  const save = async () => {
    setSaving(true);
    await persist({
      ai_provider: provider,
      ...(provider === "openrouter" ? { openrouter_api_key: orKey.trim() } : { gemini_api_key: gemKey.trim() }),
    });
    setSaving(false);
    toast.success("AI provider saved.");
    onDone?.();
  };

  return (
    <div className="space-y-5">
      <p className="text-muted-foreground">
        The app writes lyrics, adapts musical styles, and plans imagery with a large language model.
        Pick a provider and paste a key — <b className="text-foreground">OpenRouter has capable free models</b>, so you can start at zero cost.
      </p>
      <div className="grid grid-cols-2 gap-3">
        {["openrouter", "gemini"].map((p) => (
          <button key={p} onClick={() => setProvider(p)}
            className={`text-left rounded-lg border p-3 transition-colors ${provider === p ? "border-primary bg-primary/10" : "border-border hover:bg-muted/40"}`}>
            <div className="font-medium">{p === "openrouter" ? "OpenRouter" : "Google Gemini"}</div>
            <div className="text-xs text-muted-foreground mt-0.5">{p === "openrouter" ? "Free models available" : "Fast, free tier"}</div>
          </button>
        ))}
      </div>
      {provider === "openrouter" ? (
        <div className="space-y-1.5">
          <Label className="text-xs">OpenRouter API key</Label>
          <Input type="password" value={orKey} onChange={(e) => setOrKey(e.target.value)} placeholder="sk-or-…" />
          <a href="https://openrouter.ai/keys" target="_blank" rel="noreferrer" className="text-xs text-primary inline-flex items-center gap-1">
            Get a free key <ExternalLink className="w-3 h-3" />
          </a>
        </div>
      ) : (
        <div className="space-y-1.5">
          <Label className="text-xs">Gemini API key</Label>
          <Input type="password" value={gemKey} onChange={(e) => setGemKey(e.target.value)} placeholder="AIza…" />
          <a href="https://aistudio.google.com/apikey" target="_blank" rel="noreferrer" className="text-xs text-primary inline-flex items-center gap-1">
            Get a free key <ExternalLink className="w-3 h-3" />
          </a>
        </div>
      )}
      <Button onClick={save} disabled={saving} data-testid="guide-ai-save">
        {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : "Save"}
      </Button>
    </div>
  );
}

// ── Step: Kaggle free GPU servers ────────────────────────────────────────────
function KaggleStepBody({ settings, onDone }) {
  const [tokenJson, setTokenJson] = useState("");
  const [state, setState] = useState({ verifying: false, verified: !!settings?.kaggle_connected, username: settings?.kaggle_username || "", error: "" });
  const [musicEngine, setMusicEngine] = useState(settings?.music_engine || "heartmula");
  const [imageEngine, setImageEngine] = useState(settings?.image_engine || "comfyui");
  const [autostart, setAutostart] = useState(true);
  const fileRef = useRef(null);

  const verify = async (override) => {
    const token = typeof override === "string" ? override : tokenJson;
    setState((k) => ({ ...k, verifying: true, error: "" }));
    try {
      const r = await api.saveKaggleToken(token);
      setState({ verifying: false, verified: r.verified, username: r.username || "", error: r.verified ? "" : "Saved, but Kaggle didn't accept it — check you used the whole kaggle.json." });
      if (r.verified) {
        toast.success(`Kaggle connected as ${r.username}`);
        // Record it so `isDone` can tell later without re-reading the token file.
        await persist({ kaggle_connected: true, kaggle_username: r.username || "" });
      }
    } catch (e) {
      setState({ verifying: false, verified: false, username: "", error: String(e) });
    }
  };

  const onFile = async (e) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try { const text = await file.text(); setTokenJson(text); await verify(text); }
    catch (err) { setState({ verifying: false, verified: false, username: "", error: `Couldn't read that file: ${err}` }); }
    finally { e.target.value = ""; }
  };

  const finish = async () => {
    await persist({ music_engine: musicEngine, image_engine: imageEngine });
    if (autostart && state.verified) {
      autoStartKaggleServer(musicEngine);
      if (imageEngine === "comfyui" || imageEngine === "flux") autoStartKaggleServer(imageEngine);
      toast.success("Installing & starting your servers — track progress in Settings.");
    }
    onDone?.();
  };

  return (
    <div className="space-y-4">
      <p className="text-muted-foreground text-sm">
        Music and image generation run on free Kaggle GPUs. Kaggle can't be signed into from inside this app
        (Google blocks embedded logins), so sign in your normal browser, then connect with an API token.
      </p>
      <ol className="space-y-3 text-sm">
        <li className="flex gap-3 items-start">
          <span className="text-primary font-mono">1</span>
          <div className="flex-1">
            <div>Sign in to Kaggle (free account; "Continue with Google" is fine there).</div>
            <Button size="sm" variant="outline" className="mt-1.5 h-7 text-xs" onClick={() => api.openKaggleLogin()}>
              <ExternalLink className="w-3 h-3 mr-1.5" />Open Kaggle sign-in
            </Button>
          </div>
        </li>
        <li className="flex gap-3 items-start">
          <span className="text-primary font-mono">2</span>
          <div className="flex-1">
            <div>Account settings → <b className="text-foreground">Create New API Token</b> → save the <code>kaggle.json</code>.</div>
            <Button size="sm" variant="outline" className="mt-1.5 h-7 text-xs" onClick={() => api.openKaggleTokenPage()}>
              <KeyRound className="w-3 h-3 mr-1.5" />Open token page
            </Button>
          </div>
        </li>
        <li className="flex gap-3 items-start">
          <span className="text-primary font-mono">3</span>
          <div className="flex-1 space-y-1.5">
            <div className="flex items-center gap-2 flex-wrap">
              <span>Upload or paste your <code>kaggle.json</code>:</span>
              <input ref={fileRef} type="file" accept=".json,application/json" onChange={onFile} className="hidden" />
              <Button size="sm" variant="outline" className="h-7 text-xs" onClick={() => fileRef.current?.click()}>
                <Upload className="w-3 h-3 mr-1.5" />Upload kaggle.json
              </Button>
            </div>
            <Textarea value={tokenJson} onChange={(e) => setTokenJson(e.target.value)} rows={3}
              placeholder='{"username":"…","key":"…"}' className="font-mono text-xs" />
            <div className="flex items-center gap-2 flex-wrap">
              <Button size="sm" className="h-7 text-xs" onClick={() => verify()} disabled={state.verifying || !tokenJson.trim()}>
                {state.verifying ? <Loader2 className="w-3 h-3 animate-spin" /> : "Save & verify"}
              </Button>
              {state.verified && <span className="text-xs text-emerald-500 flex items-center gap-1"><CheckCircle2 className="w-3.5 h-3.5" />Connected{state.username ? ` as ${state.username}` : ""}</span>}
              {state.error && <span className="text-xs text-destructive">{state.error}</span>}
            </div>
          </div>
        </li>
      </ol>

      <div className="rounded-lg border border-border bg-muted/20 p-3 space-y-3">
        <div className="grid sm:grid-cols-2 gap-3">
          <div className="space-y-1.5">
            <Label className="text-[10px] uppercase tracking-widest text-muted-foreground flex items-center gap-1"><Music2 className="w-3 h-3" />Music engine</Label>
            <select value={musicEngine} onChange={(e) => setMusicEngine(e.target.value)} className="w-full bg-background border border-border rounded px-2 py-1.5 text-sm">
              {MUSIC_ENGINES.map((m) => <option key={m.id} value={m.id}>{m.label}</option>)}
            </select>
            <div className="text-[11px] text-muted-foreground">{MUSIC_ENGINES.find((m) => m.id === musicEngine)?.hint}</div>
          </div>
          <div className="space-y-1.5">
            <Label className="text-[10px] uppercase tracking-widest text-muted-foreground flex items-center gap-1"><Img className="w-3 h-3" />Image engine</Label>
            <select value={imageEngine} onChange={(e) => setImageEngine(e.target.value)} className="w-full bg-background border border-border rounded px-2 py-1.5 text-sm">
              {IMAGE_ENGINES.map((m) => <option key={m.id} value={m.id}>{m.label}</option>)}
            </select>
            <div className="text-[11px] text-muted-foreground">{IMAGE_ENGINES.find((m) => m.id === imageEngine)?.hint}</div>
          </div>
        </div>
        <label className="flex items-center gap-2 text-sm cursor-pointer">
          <input type="checkbox" className="accent-primary" checked={autostart} onChange={(e) => setAutostart(e.target.checked)} />
          Install &amp; start these servers now (~8–10 min in the background)
        </label>
      </div>
      <Button onClick={finish} data-testid="guide-kaggle-done">Save</Button>
    </div>
  );
}

// ── Step: project files folder ───────────────────────────────────────────────
function FilesStepBody({ settings, onDone }) {
  const [dir, setDir] = useState(settings?.project_files_dir || "");
  const choose = async () => {
    try {
      const picked = await api.pickDirectory("Choose where project files are stored");
      if (picked) { setDir(picked); await persist({ project_files_dir: picked }); toast.success("Folder saved."); }
    } catch (e) { toast.error(`Couldn't open the folder picker: ${e}`); }
  };
  return (
    <div className="space-y-5">
      <p className="text-muted-foreground">
        Pick a folder for generated audio, images and video exports. Leave it unset to use the app's default location.
      </p>
      <div className="flex items-center gap-2 flex-wrap">
        <Button variant="outline" onClick={choose}><FolderOpen className="w-4 h-4 mr-2" />Choose folder</Button>
        {dir && <span className="text-xs text-mono text-muted-foreground truncate max-w-full">{dir}</span>}
      </div>
      <Button onClick={() => onDone?.()}>Done</Button>
    </div>
  );
}

// ── The registry ─────────────────────────────────────────────────────────────
export const GUIDE_STEPS = [
  {
    id: "ai",
    title: "The AI brain",
    icon: Bot,
    blurb: "An AI provider key for lyrics, styles and imagery planning.",
    isDone: (s) => !!(s?.openrouter_api_key?.trim() || s?.gemini_api_key?.trim()),
    Body: AiStepBody,
  },
  {
    id: "kaggle",
    title: "Free GPU servers",
    icon: Cpu,
    blurb: "Connect Kaggle so music and images can render on free GPUs.",
    isDone: (s) => !!s?.kaggle_connected,
    Body: KaggleStepBody,
  },
  {
    id: "youtube",
    title: "Publishing to YouTube",
    icon: Youtube,
    blurb: "Authorize a Google account so finished videos can be uploaded.",
    // Satisfied once a Google account has been authorized and its channels imported.
    isDone: (s) => !!s?.youtube_connected,
    Body: YouTubeStepBody,
  },
  {
    id: "files",
    title: "Where your files live",
    icon: FolderOpen,
    blurb: "Choose the folder for generated audio, images and video.",
    isDone: (s) => !!s?.project_files_dir,
    Body: FilesStepBody,
  },
];

export const getGuideStep = (id) => GUIDE_STEPS.find((s) => s.id === id);

/** Load settings once and report which steps are still outstanding. */
export async function pendingGuideSteps() {
  let settings = {};
  try { settings = (await api.getSettings()) || {}; } catch { /* fail open */ }
  return GUIDE_STEPS.filter((s) => { try { return !s.isDone(settings); } catch { return false; } });
}

/** Convenience hook: load settings for a step body. */
export function useGuideSettings() {
  const [settings, setSettings] = useState(null);
  useEffect(() => {
    (async () => {
      try { setSettings((await api.getSettings()) || {}); } catch { setSettings({}); }
    })();
  }, []);
  return settings;
}
