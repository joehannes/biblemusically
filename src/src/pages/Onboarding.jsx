import { useState, useRef } from "react";
import { api } from "../lib/api";
import { autoStartKaggleServer } from "../lib/kaggleServerPipeline";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Label } from "../components/ui/label";
import { toast } from "sonner";
import {
  Sparkles, Bot, KeyRound, Cpu, FolderOpen, Youtube, CheckCircle2, ExternalLink,
  ArrowRight, ArrowLeft, Rocket, Loader2, Music2, Image as Img, Upload,
} from "lucide-react";

// First-run guided setup. Walks a new user through the few things the app genuinely needs before
// it can do anything useful — an AI provider key, the free Kaggle GPU servers, and where files
// live — then drops them at the Dashboard. Persists to the settings singleton as it goes, and
// flips `onboarded:true` at the end so this never shows again (re-runnable from Settings later).

const STEPS = ["Welcome", "AI brain", "Free GPU servers", "YouTube", "Files", "Done"];

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

export default function Onboarding({ onDone }) {
  const [step, setStep] = useState(0);
  const [saving, setSaving] = useState(false);

  // Collected settings (persisted incrementally so nothing is lost on a back/forward).
  const [aiProvider, setAiProvider] = useState("openrouter");
  const [openrouterKey, setOpenrouterKey] = useState("");
  const [geminiKey, setGeminiKey] = useState("");
  const [tokenJson, setTokenJson] = useState("");
  const [kaggleState, setKaggleState] = useState({ verifying: false, verified: false, username: "", error: "" });
  const [musicEngine, setMusicEngine] = useState("heartmula");
  const [imageEngine, setImageEngine] = useState("comfyui");
  const [projectDir, setProjectDir] = useState("");
  const [autostart, setAutostart] = useState(true);
  const tokenFileRef = useRef(null);

  // Accept an uploaded kaggle.json file: read it, drop it into the textarea, and verify straight
  // away so the user doesn't have to open + copy + paste the file's contents by hand.
  const onTokenFile = async (e) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      setTokenJson(text);
      await verifyToken(text);
    } catch (err) {
      setKaggleState({ verifying: false, verified: false, username: "", error: `Couldn't read that file: ${err}` });
    } finally {
      e.target.value = ""; // allow re-selecting the same file
    }
  };

  const persist = async (patch) => {
    try { await api.saveSettings(patch); } catch (e) { console.warn("onboarding save failed", e); }
  };

  const next = () => setStep((s) => Math.min(s + 1, STEPS.length - 1));
  const back = () => setStep((s) => Math.max(s - 1, 0));

  const saveAi = async () => {
    setSaving(true);
    await persist({
      ai_provider: aiProvider,
      ...(aiProvider === "openrouter" ? { openrouter_api_key: openrouterKey.trim() } : { gemini_api_key: geminiKey.trim() }),
    });
    setSaving(false);
    next();
  };

  const verifyToken = async (tokenOverride) => {
    const token = typeof tokenOverride === "string" ? tokenOverride : tokenJson;
    setKaggleState((k) => ({ ...k, verifying: true, error: "" }));
    try {
      const r = await api.saveKaggleToken(token);
      setKaggleState({ verifying: false, verified: r.verified, username: r.username || "", error: r.verified ? "" : "Saved, but Kaggle didn't accept it — double-check you pasted the whole kaggle.json." });
      if (r.verified) toast.success(`Kaggle connected as ${r.username}`);
    } catch (e) {
      setKaggleState({ verifying: false, verified: false, username: "", error: String(e) });
    }
  };

  const finishKaggleStep = async () => {
    await persist({ music_engine: musicEngine, image_engine: imageEngine });
    // Kick off provisioning of the preferred engines in the background — the user can watch it in
    // Settings. Only fire if Kaggle actually verified, otherwise it would just error.
    if (autostart && kaggleState.verified) {
      autoStartKaggleServer(musicEngine);
      if (imageEngine === "comfyui" || imageEngine === "flux") autoStartKaggleServer(imageEngine);
      toast.success("Installing & starting your servers — track progress in Settings.");
    }
    next();
  };

  const chooseFolder = async () => {
    try {
      const dir = await api.pickDirectory("Choose where BibleMusically stores project files");
      if (dir) { setProjectDir(dir); await persist({ project_files_dir: dir }); }
    } catch (e) { toast.error(`Couldn't open the folder picker: ${e}`); }
  };

  const finishAll = async () => {
    setSaving(true);
    await persist({ onboarded: true });
    setSaving(false);
    onDone?.();
  };

  return (
    <div className="min-h-screen w-full bg-background text-foreground flex flex-col items-center overflow-y-auto">
      <div className="w-full max-w-2xl px-6 py-10">
        {/* progress rail */}
        <div className="flex items-center gap-1.5 mb-8">
          {STEPS.map((label, i) => (
            <div key={label} className="flex items-center gap-1.5 flex-1 last:flex-none">
              <div className={`h-1.5 rounded-full flex-1 transition-colors ${i <= step ? "bg-primary" : "bg-muted"}`} />
            </div>
          ))}
        </div>
        <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">
          Setup · step {step + 1}/{STEPS.length}
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
                <li className="flex gap-2"><Bot className="w-4 h-4 text-primary shrink-0 mt-0.5" />An <b className="text-foreground">AI provider</b> for lyrics &amp; style writing (a free key works).</li>
                <li className="flex gap-2"><Cpu className="w-4 h-4 text-primary shrink-0 mt-0.5" />Your <b className="text-foreground">free Kaggle GPU servers</b> for music &amp; images.</li>
                <li className="flex gap-2"><FolderOpen className="w-4 h-4 text-primary shrink-0 mt-0.5" />Where your <b className="text-foreground">project files</b> live.</li>
              </ul>
            </div>
            <p className="text-xs text-muted-foreground">You can change all of this later in Settings. Nothing leaves your machine except calls you trigger.</p>
            <div className="flex justify-end">
              <Button onClick={next}>Let's go <ArrowRight className="w-4 h-4 ml-2" /></Button>
            </div>
          </div>
        )}

        {/* ── AI brain ────────────────────────────────────────── */}
        {step === 1 && (
          <div className="fade-in space-y-5">
            <div className="flex items-center gap-3"><Bot className="w-7 h-7 text-primary" /><h1 className="text-3xl font-bold">The AI brain</h1></div>
            <p className="text-muted-foreground">
              The app writes lyrics, adapts musical styles, and plans imagery with a large language model.
              Pick a provider and paste a key — <b className="text-foreground">OpenRouter has capable free models</b>, so you can start at zero cost.
            </p>
            <div className="grid grid-cols-2 gap-3">
              {["openrouter", "gemini"].map((p) => (
                <button key={p} onClick={() => setAiProvider(p)}
                  className={`text-left rounded-lg border p-3 transition-colors ${aiProvider === p ? "border-primary bg-primary/10" : "border-border hover:bg-muted/40"}`}>
                  <div className="font-medium capitalize">{p === "openrouter" ? "OpenRouter" : "Google Gemini"}</div>
                  <div className="text-xs text-muted-foreground mt-0.5">{p === "openrouter" ? "Free models available" : "Fast, free tier"}</div>
                </button>
              ))}
            </div>
            {aiProvider === "openrouter" ? (
              <div className="space-y-1.5">
                <Label className="text-xs">OpenRouter API key</Label>
                <Input type="password" value={openrouterKey} onChange={(e) => setOpenrouterKey(e.target.value)} placeholder="sk-or-…" />
                <a href="https://openrouter.ai/keys" target="_blank" rel="noreferrer" className="text-xs text-primary inline-flex items-center gap-1">
                  Get a free key <ExternalLink className="w-3 h-3" />
                </a>
              </div>
            ) : (
              <div className="space-y-1.5">
                <Label className="text-xs">Gemini API key</Label>
                <Input type="password" value={geminiKey} onChange={(e) => setGeminiKey(e.target.value)} placeholder="AIza…" />
                <a href="https://aistudio.google.com/apikey" target="_blank" rel="noreferrer" className="text-xs text-primary inline-flex items-center gap-1">
                  Get a free key <ExternalLink className="w-3 h-3" />
                </a>
              </div>
            )}
            <div className="flex justify-between">
              <Button variant="ghost" onClick={back}><ArrowLeft className="w-4 h-4 mr-2" />Back</Button>
              <div className="flex gap-2">
                <Button variant="ghost" onClick={next}>Skip for now</Button>
                <Button onClick={saveAi} disabled={saving}>{saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <>Save &amp; continue<ArrowRight className="w-4 h-4 ml-2" /></>}</Button>
              </div>
            </div>
          </div>
        )}

        {/* ── Kaggle / GPU servers ────────────────────────────── */}
        {step === 2 && (
          <div className="fade-in space-y-4">
            <div className="flex items-center gap-3"><Cpu className="w-7 h-7 text-primary" /><h1 className="text-3xl font-bold">Free GPU servers</h1></div>
            <p className="text-muted-foreground text-sm">
              Music and image generation run on free Kaggle GPUs. Kaggle can't be signed into from inside this app
              (Google blocks embedded logins), so we do it in your normal browser, then connect with an API token.
            </p>
            <ol className="space-y-3 text-sm">
              <li className="flex gap-3 items-start">
                <span className="text-primary font-mono">1</span>
                <div className="flex-1">
                  <div>Sign in to Kaggle (a free account; "Continue with Google" is fine here).</div>
                  <Button size="sm" variant="outline" className="mt-1.5 h-7 text-xs" onClick={() => api.openKaggleLogin()}>
                    <ExternalLink className="w-3 h-3 mr-1.5" />Open Kaggle sign-in
                  </Button>
                </div>
              </li>
              <li className="flex gap-3 items-start">
                <span className="text-primary font-mono">2</span>
                <div className="flex-1">
                  <div>Open your account settings, click <b className="text-foreground">Create New API Token</b>, and copy the downloaded <code>kaggle.json</code>.</div>
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
                    <input ref={tokenFileRef} type="file" accept=".json,application/json" onChange={onTokenFile} className="hidden" />
                    <Button size="sm" variant="outline" className="h-7 text-xs" onClick={() => tokenFileRef.current?.click()}>
                      <Upload className="w-3 h-3 mr-1.5" />Upload kaggle.json
                    </Button>
                  </div>
                  <Textarea value={tokenJson} onChange={(e) => setTokenJson(e.target.value)} rows={3}
                    placeholder='{"username":"…","key":"…"}' className="font-mono text-xs" />
                  <div className="flex items-center gap-2 flex-wrap">
                    <Button size="sm" className="h-7 text-xs" onClick={() => verifyToken()} disabled={kaggleState.verifying || !tokenJson.trim()}>
                      {kaggleState.verifying ? <Loader2 className="w-3 h-3 animate-spin" /> : "Save & verify"}
                    </Button>
                    {kaggleState.verified && <span className="text-xs text-emerald-500 flex items-center gap-1"><CheckCircle2 className="w-3.5 h-3.5" />Connected as {kaggleState.username}</span>}
                    {kaggleState.error && <span className="text-xs text-destructive">{kaggleState.error}</span>}
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
                Install &amp; start these servers now (takes ~8–10 min in the background)
              </label>
            </div>

            <div className="flex justify-between">
              <Button variant="ghost" onClick={back}><ArrowLeft className="w-4 h-4 mr-2" />Back</Button>
              <div className="flex gap-2">
                <Button variant="ghost" onClick={next}>Skip for now</Button>
                <Button onClick={finishKaggleStep}>Continue<ArrowRight className="w-4 h-4 ml-2" /></Button>
              </div>
            </div>
          </div>
        )}

        {/* ── YouTube (optional) ──────────────────────────────── */}
        {step === 3 && (
          <div className="fade-in space-y-5">
            <div className="flex items-center gap-3"><Youtube className="w-7 h-7 text-primary" /><h1 className="text-3xl font-bold">Publishing to YouTube</h1></div>
            <p className="text-muted-foreground">
              To upload finished videos, the app connects to your YouTube channel with Google sign-in.
              This uses a proper Google consent screen (the app never sees your password) and needs a
              one-time Google API client — so it's easiest to set up on the <b className="text-foreground">Channels</b> page after setup, whenever you're ready to publish.
            </p>
            <div className="rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted-foreground">
              You can skip this now and add channels later — none of the creation steps need it.
            </div>
            <div className="flex justify-between">
              <Button variant="ghost" onClick={back}><ArrowLeft className="w-4 h-4 mr-2" />Back</Button>
              <Button onClick={next}>Continue<ArrowRight className="w-4 h-4 ml-2" /></Button>
            </div>
          </div>
        )}

        {/* ── Files ───────────────────────────────────────────── */}
        {step === 4 && (
          <div className="fade-in space-y-5">
            <div className="flex items-center gap-3"><FolderOpen className="w-7 h-7 text-primary" /><h1 className="text-3xl font-bold">Where your files live</h1></div>
            <p className="text-muted-foreground">
              Pick a folder for generated audio, images and video exports. Leave it unset to use the app's default location.
            </p>
            <div className="flex items-center gap-2">
              <Button variant="outline" onClick={chooseFolder}><FolderOpen className="w-4 h-4 mr-2" />Choose folder</Button>
              {projectDir && <span className="text-xs text-mono text-muted-foreground truncate">{projectDir}</span>}
            </div>
            <div className="flex justify-between">
              <Button variant="ghost" onClick={back}><ArrowLeft className="w-4 h-4 mr-2" />Back</Button>
              <Button onClick={next}>Continue<ArrowRight className="w-4 h-4 ml-2" /></Button>
            </div>
          </div>
        )}

        {/* ── Done ────────────────────────────────────────────── */}
        {step === 5 && (
          <div className="fade-in space-y-5 text-center">
            <Rocket className="w-12 h-12 text-primary mx-auto" />
            <h1 className="text-4xl font-bold">You're all set</h1>
            <p className="text-muted-foreground max-w-md mx-auto">
              {kaggleState.verified && autostart
                ? "Your GPU servers are provisioning in the background — you can watch them in Settings. Meanwhile, start your first project."
                : "You can start creating right away. Connect servers any time from Settings."}
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
