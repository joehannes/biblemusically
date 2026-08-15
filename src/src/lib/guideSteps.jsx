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
  Loader2, Music2, Image as Img, Upload, Languages, Target, Volume2, ShieldCheck, ScrollText,
} from "lucide-react";
import YouTubeStepBody from "./guideYouTube";
import VoicePicker from "../components/VoicePicker";
import { visibleMusicEngines, visibleImageEngines } from "./engineCapabilities";
import Markdown from "../components/Markdown";
import {
  UI_LANGUAGES, setUiLanguage, getUiLanguage, isBundledLanguage,
  customLanguages, addCustomLanguage,
} from "./uiTranslate";
import { openLoginUrl } from "./openLogin";
import { useNavigate } from "react-router-dom";

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

// The engine lists come from the catalogue rather than being written again here. They were
// duplicated, and the copy had gone stale in the worst possible place: the welcome wizard offered a
// brand-new user Suno and Midjourney by name — the two engines that are hidden everywhere else
// precisely because reaching them means driving an account the user would lose. The first screen
// somebody sees is not where to leak the thing every other picker withholds.
const engineOptions = (visible, settings, current) =>
  visible(settings, current).map(([id, engine]) => ({
    id, label: engine.label,
    hint: `${engine.note}. ${engine.strengths}`,
  }));

const persist = async (patch) => {
  try { await api.saveSettings(patch); } catch (e) { console.warn("guide step save failed", e); }
};

// ── Step: AI provider ────────────────────────────────────────────────────────
// Comes early — almost everything after it (the setup plan, the guided flows, translation, lyrics)
// asks the AI something, so a key here makes the rest of setup smarter.
//
// Free and paid are shown side by side rather than hiding the paid options behind a toggle: a
// creator publishing daily has a real reason to pay, and finding that out three screens later is
// worse than seeing it now. The model list is fetched from the provider using the key that was just
// pasted, so it is never a stale hardcoded list.
function AiStepBody({ settings, onDone }) {
  const nav = useNavigate();
  const [providers, setProviders] = useState([]);
  const [provider, setProvider] = useState(settings?.ai_provider || "openrouter");
  const [keys, setKeys] = useState({
    openrouter_api_key: settings?.openrouter_api_key || "",
    gemini_api_key: settings?.gemini_api_key || "",
    anthropic_api_key: settings?.anthropic_api_key || "",
    openai_api_key: settings?.openai_api_key || "",
  });
  const [models, setModels] = useState({ list: [], loading: false, error: "" });
  const [model, setModel] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => { api.listAiProviders().then((r) => setProviders(r?.providers || [])).catch(() => {}); }, []);

  const current = providers.find((p) => p.id === provider);
  const keyField = current?.key_field || `${provider}_api_key`;
  const modelField = current?.model_field || `${provider}_model`;

  useEffect(() => { setModel(settings?.[modelField] || ""); }, [modelField, settings]);

  /** Ask the provider what this key can reach. Saves the key first — the backend reads it from settings. */
  const loadModels = async () => {
    setModels({ list: [], loading: true, error: "" });
    try {
      await persist({ [keyField]: (keys[keyField] || "").trim() });
      const r = await api.listAiModels(provider);
      const list = r?.models || [];
      setModels({ list, loading: false, error: list.length ? "" : "No models came back — check the key." });
      if (list.length && !list.some((m) => m.id === model)) setModel(list[0].id);
    } catch (err) {
      setModels({ list: [], loading: false, error: String(err) });
    }
  };

  const openKeyPage = () => {
    if (!current?.key_url) return;
    const where = openLoginUrl(current.key_url, {
      navigate: nav,
      label: `${current.label} API key`,
      recommended: current.login,
    });
    toast.message(where === "internal"
      ? "Opened inside the app — copy the key and come back to this tab."
      : "Opened in your normal browser (this site refuses embedded sign-in).");
  };

  const save = async () => {
    setSaving(true);
    await persist({
      ai_provider: provider,
      [keyField]: (keys[keyField] || "").trim(),
      ...(model ? { [modelField]: model } : {}),
    });
    setSaving(false);
    toast.success(`${current?.label || provider} saved.`);
    onDone?.();
  };

  const free = providers.filter((p) => p.tier === "free");
  const paid = providers.filter((p) => p.tier === "paid");

  const ProviderCard = ({ p }) => (
    <button onClick={() => setProvider(p.id)}
      className={`text-left rounded-lg border p-3 transition-colors ${provider === p.id ? "border-primary bg-primary/10" : "border-border hover:bg-muted/40"}`}>
      <div className="font-medium flex items-center gap-1.5">
        {p.label}
        {settings?.[p.key_field] ? <CheckCircle2 className="w-3.5 h-3.5 text-emerald-500" /> : null}
      </div>
      <div className="text-[11px] text-muted-foreground mt-0.5 leading-snug">{p.detail}</div>
    </button>
  );

  return (
    <div className="space-y-4">
      <p className="text-muted-foreground text-sm">
        The app writes lyrics, adapts styles, plans imagery, translates the interface and drives the
        guides with a language model. You can run the whole thing on a free key — or pay per token for
        stronger results. This is changeable at any time, and an overloaded provider automatically
        falls back to a free one.
      </p>

      <div className="space-y-1.5">
        <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Free</Label>
        <div className="grid sm:grid-cols-2 gap-2">{free.map((p) => <ProviderCard key={p.id} p={p} />)}</div>
      </div>
      <div className="space-y-1.5">
        <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Paid — per token, no subscription</Label>
        <div className="grid sm:grid-cols-2 gap-2">{paid.map((p) => <ProviderCard key={p.id} p={p} />)}</div>
      </div>

      {current && (
        <div className="rounded-lg border border-border p-3 space-y-2.5">
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <Label className="text-xs">{current.label} API key</Label>
            <Button size="sm" variant="ghost" onClick={openKeyPage} className="h-6 text-[11px]">
              <ExternalLink className="w-3 h-3 mr-1" />Get a key
            </Button>
          </div>
          <Input type="password" value={keys[keyField] || ""}
            onChange={(e) => setKeys({ ...keys, [keyField]: e.target.value })}
            placeholder={provider === "anthropic" ? "sk-ant-…" : provider === "openai" ? "sk-…" : provider === "gemini" ? "AIza…" : "sk-or-…"} />
          {current.login_note && <div className="text-[11px] text-amber-400">{current.login_note}</div>}

          <div className="flex items-center gap-2 flex-wrap">
            <Button size="sm" variant="secondary" onClick={loadModels} disabled={models.loading || !(keys[keyField] || "").trim()}>
              {models.loading ? <Loader2 className="w-3 h-3 mr-1.5 animate-spin" /> : null}
              Check the key and list models
            </Button>
            {models.list.length > 0 && (
              <select value={model} onChange={(e) => setModel(e.target.value)}
                className="flex-1 min-w-[14rem] bg-background border border-border rounded px-2 py-1.5 text-xs">
                {models.list.map((m) => (
                  <option key={m.id} value={m.id}>{m.label}{m.free ? " — free" : ""}</option>
                ))}
              </select>
            )}
          </div>
          {models.error && <div className="text-[11px] text-amber-400">{models.error}</div>}
          {models.list.length > 0 && (
            <div className="text-[11px] text-emerald-500">
              Key works — {models.list.length} model{models.list.length === 1 ? "" : "s"} available.
            </div>
          )}
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
  // `state.verified` means "a token was accepted in THIS sitting" — not "an account exists". The two
  // were conflated, and this step is also how you add a SECOND account (quota is per-account): it
  // opened already showing a green "Connected as <the old account>" against an empty token box, which
  // reads as "nothing to do here". The account already on file is shown separately, as context.
  const connected = settings?.kaggle_connected ? (settings?.kaggle_username || "your Kaggle account") : "";
  const [state, setState] = useState({ verifying: false, verified: false, username: "", error: "" });
  const [musicEngine, setMusicEngine] = useState(settings?.music_engine || "heartmula");
  const [imageEngine, setImageEngine] = useState(settings?.image_engine || "comfyui");
  const [autostart, setAutostart] = useState(true);
  const fileRef = useRef(null);

  const verify = async (override) => {
    const token = typeof override === "string" ? override : tokenJson;
    setState((k) => ({ ...k, verifying: true, error: "" }));
    try {
      const r = await api.saveKaggleToken(token);
      // The backend now distinguishes "Kaggle rejected this" from "Kaggle accepted it, as somebody
      // else" — the second one used to be reported as success and sent every run to the wrong
      // account. Its explanation is specific, so show it instead of the generic guess.
      setState({ verifying: false, verified: r.verified, username: r.username || "",
        error: r.verified ? ""
          : (r.detail || "Saved, but Kaggle didn't accept it — check you used the whole kaggle.json.") });
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
    if (autostart && (state.verified || connected)) {
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
      {connected && (
        <div className="rounded-lg border border-border bg-muted/20 p-3 text-xs text-muted-foreground space-y-1">
          <div>
            <span className="uppercase tracking-widest text-[10px]">Already connected</span>{" "}
            <b className="text-foreground normal-case tracking-normal" data-no-i18n>{connected}</b>
          </div>
          <p>
            Free GPU time is granted per account, so connecting a second one gives the app somewhere to
            go when this one's weekly quota runs out — it switches over on its own. Sign in below as the
            account you want to add: sign out of Kaggle in your browser first, or the token page will
            just hand you the same account's key again.
          </p>
        </div>
      )}
      <ol className="space-y-3 text-sm">
        <li className="flex gap-3 items-start">
          <span className="text-primary font-mono">1</span>
          <div className="flex-1">
            {/* One sentence, unconditional. Which account to sign in as is said in the box above,
                where there is room to say why — and a sentence spliced around {connected} is a
                sentence no catalogue can carry: extract-ui-strings.mjs keys on whole text nodes. */}
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
              {engineOptions(visibleMusicEngines, settings, musicEngine).map((m) => <option key={m.id} value={m.id}>{m.label}</option>)}
            </select>
            <div className="text-[11px] text-muted-foreground">{engineOptions(visibleMusicEngines, settings, musicEngine).find((m) => m.id === musicEngine)?.hint}</div>
          </div>
          <div className="space-y-1.5">
            <Label className="text-[10px] uppercase tracking-widest text-muted-foreground flex items-center gap-1"><Img className="w-3 h-3" />Image engine</Label>
            <select value={imageEngine} onChange={(e) => setImageEngine(e.target.value)} className="w-full bg-background border border-border rounded px-2 py-1.5 text-sm">
              {engineOptions(visibleImageEngines, settings, imageEngine).map((m) => <option key={m.id} value={m.id}>{m.label}</option>)}
            </select>
            <div className="text-[11px] text-muted-foreground">{engineOptions(visibleImageEngines, settings, imageEngine).find((m) => m.id === imageEngine)?.hint}</div>
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

// ── Step: interface language ─────────────────────────────────────────────────
// First, because everything after it is easier to read in your own language. Every listed language
// ships a catalogue and switches instantly and offline; a language you type yourself is translated
// once by the AI and cached.
function LanguageStepBody({ onDone }) {
  const [current, setCurrent] = useState(getUiLanguage());
  const [busy, setBusy] = useState("");
  const [custom, setCustom] = useState(() => customLanguages());
  const [typed, setTyped] = useState("");

  const pick = async (code) => {
    setBusy(code);
    try {
      const r = await setUiLanguage(code);
      setCurrent(code);
      await persist({ ui_language: code });
      if (r?.error && !r.applied) toast.error(`Couldn't translate: ${r.error}`);
      else if (code !== "en") toast.success(r?.bundled ? "Interface translated (built-in)." : "Interface translated.");
    } finally { setBusy(""); }
  };

  const addOwn = async () => {
    const r = addCustomLanguage(typed);
    if (!r.ok) { toast.error(r.error); return; }
    setTyped("");
    setCustom(customLanguages());
    await pick(r.code);
  };

  return (
    <div className="space-y-3">
      <p className="text-muted-foreground text-sm">
        Pick the language you want the app in. All of these ship with the app, so they switch instantly
        and work offline. If yours isn't here, type it below — the AI translates the interface into it
        once and then remembers it.
      </p>
      <div className="grid sm:grid-cols-3 gap-2">
        {[...UI_LANGUAGES, ...custom].map((l) => (
          <button key={l.code} onClick={() => pick(l.code)} disabled={busy === l.code}
            className={`text-left rounded-lg border p-2.5 transition-all hover:border-primary/60 ${current === l.code ? "border-primary/60 bg-primary/5" : "border-border"}`}>
            <div className="text-sm font-medium flex items-center gap-1.5">
              {busy === l.code ? <Loader2 className="w-3 h-3 animate-spin" /> : null}
              {l.native}
              {current === l.code && <CheckCircle2 className="w-3 h-3 text-primary" />}
            </div>
            <div className="text-[10px] uppercase tracking-wide text-muted-foreground">
              {isBundledLanguage(l.code) ? "built-in" : l.code === "en" ? "original" : "AI translated"}
            </div>
          </button>
        ))}
      </div>
      {/* Sixteen languages is not every language, and the missing ones are the ones nobody ships. */}
      <div className="rounded-lg border border-border p-2.5 space-y-1.5">
        <div className="text-xs font-medium">Your own language</div>
        <div className="flex gap-2">
          <input
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") addOwn(); }}
            placeholder="Name any language — e.g. Tagalog, Swiss German, Afrikaans"
            className="flex-1 bg-muted/30 border border-border rounded px-2 py-1.5 text-sm"
          />
          <Button variant="secondary" onClick={addOwn} disabled={!!busy || typed.trim().length < 2}>Translate</Button>
        </div>
        <div className="text-[10px] text-muted-foreground">
          Needs an AI provider (the next step) and takes a minute the first time.
        </div>
      </div>
      <Button onClick={() => onDone?.()}>Continue</Button>
    </div>
  );
}

// ── Step: what are you here to do? ───────────────────────────────────────────
// The step that makes the rest of setup shorter. Rather than presenting everything the app can connect
// to, it asks for the goal and then works out what that goal actually needs — presetting what it can
// and marking the rest required, optional or unnecessary.
const GOAL_CHOICES = [
  { id: "try", label: "Try it out first", hint: "See what it makes before connecting anything else." },
  { id: "one_channel", label: "Run one channel well", hint: "A steady stream of videos on a single channel." },
  { id: "many_channels", label: "Publish to many channels", hint: "Several languages or brands, on a schedule." },
  { id: "scale", label: "Fifty channels, hands-off", hint: "High volume, rendering and uploading off this machine." },
];

function GoalStepBody({ settings, onDone }) {
  const [choice, setChoice] = useState(settings?.user_goal_kind || "");
  const [own, setOwn] = useState(settings?.user_goal || "");
  const [rec, setRec] = useState(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);

  const ask = async (goalText, kind) => {
    setBusy(true);
    setChoice(kind);
    try {
      const done = [];
      for (const s of GUIDE_STEPS) { try { if (s.isDone(settings || {})) done.push(s.id); } catch { /* ignore */ } }
      const r = await api.setupRecommendation({
        goal: goalText,
        done,
        context: { has_ai_key: Boolean(settings?.openrouter_api_key || settings?.gemini_api_key), kaggle: !!settings?.kaggle_connected },
      });
      setRec(r);
    } catch (err) {
      // Without an AI key yet this is expected on a fresh install — the fixed step order still works.
      setRec({ summary: "", steps: [], presets: {}, unavailable: String(err) });
    } finally { setBusy(false); }
  };

  const accept = async () => {
    const patch = { user_goal: own || GOAL_CHOICES.find((g) => g.id === choice)?.label || "", user_goal_kind: choice };
    if (rec?.presets) Object.assign(patch, rec.presets);
    if (rec?.steps?.length) patch.setup_plan = rec.steps;
    await persist(patch);
    setSaved(true);
    toast.success("Setup tailored to that.");
    onDone?.();
  };

  const NEED_STYLE = {
    required: "border-primary/50 bg-primary/5 text-foreground",
    optional: "border-border text-muted-foreground",
    skip: "border-border/50 text-muted-foreground/60 line-through",
  };

  return (
    <div className="space-y-3">
      <p className="text-muted-foreground text-sm">
        There are a lot of things this app <i>can</i> connect to. Tell me what you're after and I'll only
        set up what that actually needs — you can add the rest whenever you want it.
      </p>
      <div className="grid sm:grid-cols-2 gap-2">
        {GOAL_CHOICES.map((g) => (
          <button key={g.id} onClick={() => ask(g.label + " — " + g.hint, g.id)}
            className={`text-left rounded-lg border p-2.5 transition-all hover:border-primary/60 ${choice === g.id ? "border-primary/60 bg-primary/5" : "border-border"}`}>
            <div className="text-sm font-medium">{g.label}</div>
            <div className="text-[11px] text-muted-foreground mt-0.5">{g.hint}</div>
          </button>
        ))}
      </div>

      <div className="space-y-1.5">
        <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Or say it in your own words</Label>
        <div className="flex gap-2">
          <Input value={own} onChange={(e) => setOwn(e.target.value)}
            placeholder="e.g. daily psalms in German and Hebrew for two channels" />
          <Button size="sm" variant="secondary" onClick={() => ask(own, "custom")} disabled={!own.trim() || busy}>
            {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : "Plan it"}
          </Button>
        </div>
      </div>

      {busy && <div className="text-[11px] text-muted-foreground flex items-center gap-1.5"><Loader2 className="w-3 h-3 animate-spin" />Working out what that needs…</div>}

      {rec && !busy && (
        <div className="rounded-lg border border-border p-3 space-y-2">
          {rec.summary && <div className="text-sm">{rec.summary}</div>}
          {rec.unavailable && (
            <div className="text-[11px] text-amber-400">
              I can't plan this yet — that needs an AI key, which is the next step. The standard order works fine too.
            </div>
          )}
          {rec.steps?.length > 0 && (
            <div className="space-y-1">
              {rec.steps.filter((s) => s.id !== "goal").map((s) => (
                <div key={s.id} className={`text-[11px] rounded border px-2 py-1 flex items-baseline gap-2 ${NEED_STYLE[s.need]}`}>
                  <span className="uppercase tracking-wide text-[9px] shrink-0 w-16">{s.need}</span>
                  <span className="font-medium shrink-0">{getGuideStep(s.id)?.title || s.id}</span>
                  <span className="text-muted-foreground">{s.why}</span>
                </div>
              ))}
            </div>
          )}
          {Object.keys(rec.presets || {}).length > 0 && (
            <div className="text-[11px] text-muted-foreground">
              Presetting: {Object.entries(rec.presets).map(([k, v]) => `${k} = ${v}`).join(", ")}.
            </div>
          )}
        </div>
      )}

      <div className="flex gap-2">
        <Button onClick={accept} disabled={!choice && !own.trim()}>
          {saved ? <><CheckCircle2 className="w-3.5 h-3.5 mr-1.5" />Saved</> : "Use this plan"}
        </Button>
        <Button variant="ghost" onClick={() => onDone?.()}>Skip</Button>
      </div>
    </div>
  );
}

// ── Step: the assistant's voice ──────────────────────────────────────────────
function VoiceStepBody({ onDone }) {
  return (
    <div className="space-y-3">
      <p className="text-muted-foreground text-sm">
        The guides can talk you through each step and take spoken answers — useful when your hands are
        busy. Your device's own voice is picked to start with: it works offline, costs nothing and
        needs no key. Listen to it, pick a different one, or turn speech off entirely.
      </p>
      <VoicePicker compact preferSystem />
      <Button onClick={() => onDone?.()}>Continue</Button>
    </div>
  );
}

// ── Step: permissions ────────────────────────────────────────────────────────
// Asked once, explicitly, with the reason for each — and each request is triggered by the button next
// to it, so nothing is ever demanded silently in the background.
function PermissionsStepBody({ settings, onDone }) {
  const [mic, setMic] = useState(null);          // null unknown · true granted · false refused
  const [folder, setFolder] = useState(settings?.project_files_dir || "");
  const [busy, setBusy] = useState("");
  // Installing updates: a phone-only row. `supported: false` on desktop, where the OS package
  // manager does this and there is nothing to ask for — so the row simply is not shown there.
  const [install, setInstall] = useState(null);
  // Android has no folder picker available to this app — see pick_directory in settings.rs. Knowing
  // that here is the difference between a button that quietly does something else and a sentence
  // saying what actually happens.
  const [caps, setCaps] = useState(null);
  // The real folders this platform will let the app write to. On Android these are
  // getExternalFilesDir(...) paths — browsable in a file manager, no permission required — which is
  // a genuine choice, unlike the picker Android cannot offer. See list_storage_locations.
  const [storage, setStorage] = useState(null);
  const [showFolders, setShowFolders] = useState(false);

  useEffect(() => {
    api.updateInstallState().then(setInstall).catch(() => setInstall(null));
    api.platformCapabilities().then(setCaps).catch(() => setCaps(null));
    api.listStorageLocations().then(setStorage).catch(() => setStorage(null));
  }, []);

  const chooseLocation = async (loc) => {
    setFolder(loc.path);
    await persist({ project_files_dir: loc.path });
    setShowFolders(false);
    toast.success(`Files will go to ${loc.label}.`);
  };

  const askInstall = async () => {
    setBusy("install");
    try {
      await api.requestInstallPermission();
      // Android hands back no result for that screen — it just opens. So the honest thing is to
      // re-ask rather than to assume the trip through Settings ended in a yes.
      toast.message("Turn on \"Allow from this source\", then come back here.", { duration: 9000 });
      setTimeout(() => { api.updateInstallState().then(setInstall).catch(() => {}); }, 1200);
    } catch (err) { toast.error(String(err)); }
    finally { setBusy(""); }
  };

  const askMic = async () => {
    setBusy("mic");
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      stream.getTracks().forEach((t) => t.stop());
      setMic(true);
      toast.success("Microphone access granted.");
    } catch {
      setMic(false);
      toast.error("Microphone access was refused. Spoken answers stay off; everything else works.");
    } finally { setBusy(""); }
  };

  const askFolder = async () => {
    setBusy("folder");
    try {
      const dir = await api.pickDirectory("Where should generated audio, images and video go?");
      if (dir) {
        setFolder(dir);
        await persist({ project_files_dir: dir });
        toast.success(caps?.mobile ? "Using the app's own folder." : "Folder chosen.");
      } else {
        // A cancelled picker is not a failure, but silence here reads as one.
        toast.message("No folder chosen — the default is still in use.");
      }
    } catch (err) { toast.error(String(err)); }
    finally { setBusy(""); }
  };

  const rows = [
    {
      id: "mic", label: "Microphone", state: mic,
      why: "Only used while you hold a guide's \"Say it\" button. Nothing is recorded otherwise.",
      action: askMic, actionLabel: "Allow microphone",
    },
    // On a phone this is a statement, not a choice. Android's scoped storage means an app writes to
    // its own directory or asks for a document through a picker neither rfd nor tauri-plugin-dialog
    // implements on mobile — so `pick_directory` returns the app's own folder. Offering "Choose
    // folder" there is a button that appears to do nothing, which is exactly what was reported.
    caps?.mobile ? {
      id: "folder", label: "Where your files go", state: true,
      why: "Android keeps each app's files in its own folder and does not let one write elsewhere "
         + "without a document picker. Nothing to choose — generated audio, images and video are "
         + "kept there, and Data & Sync can copy them out.",
      action: () => setShowFolders((v) => !v), actionLabel: showFolders ? "Hide" : "Choose folder",
      detail: folder,
    } : {
      id: "folder", label: "A folder for your files", state: folder ? true : null,
      why: "Generated audio, images and video are written there. Nothing outside it is touched.",
      action: askFolder, actionLabel: folder ? "Change folder" : "Choose folder",
      detail: folder,
    },
    // Only where it means anything. On desktop an update is handed to the package manager, which
    // needs no permission from the app, so offering a switch would be inventing a decision.
    ...(storage?.can_browse === false && (storage?.locations || []).length > 1 ? [] : []),
    ...(install?.supported ? [{
      id: "install", label: "Installing updates", state: install.allowed ? true : null,
      why: "When a new version is released, the app can download it and hand it to Android's "
         + "installer instead of leaving a file you have to find yourself. Android still shows its "
         + "own confirmation every time, and it still refuses anything not signed by us — this only "
         + "allows the app to ask.",
      action: askInstall, actionLabel: "Allow installing",
    }] : []),
  ];

  return (
    <div className="space-y-3">
      <p className="text-muted-foreground text-sm">
        What the app may need from your system. All of it is optional, and all of it is asked for
        here rather than surprising you later.
      </p>
      {rows.map((r) => (
        <div key={r.id} className="rounded-lg border border-border p-2.5 flex items-start gap-2">
          <div className="flex-1 min-w-0">
            <div className="text-sm font-medium flex items-center gap-1.5">
              {r.label}
              {r.state === true && <CheckCircle2 className="w-3.5 h-3.5 text-emerald-500" />}
              {r.state === false && <span className="text-[10px] uppercase tracking-wide text-amber-400">refused</span>}
            </div>
            <div className="text-[11px] text-muted-foreground mt-0.5">{r.why}</div>
            {r.detail && <div className="text-[10px] text-mono text-muted-foreground/80 mt-0.5 truncate">{r.detail}</div>}
          </div>
          <Button size="sm" variant="secondary" onClick={r.action} disabled={busy === r.id}>
            {busy === r.id ? <Loader2 className="w-3 h-3 animate-spin" /> : r.actionLabel}
          </Button>
        </div>
      ))}
      {showFolders && (storage?.locations || []).length > 0 && (
        <div className="rounded-lg border border-border p-2.5 space-y-1.5">
          {storage.note && <p className="text-[11px] text-muted-foreground">{storage.note}</p>}
          {storage.locations.map((loc) => (
            <button key={loc.id} type="button" onClick={() => chooseLocation(loc)}
              className={`w-full text-left rounded-md border p-2 transition-colors ${
                folder === loc.path ? "border-primary bg-primary/10" : "border-border hover:bg-muted/40"}`}>
              <div className="text-xs font-medium flex items-center gap-1.5">
                {loc.label}
                {folder === loc.path && <CheckCircle2 className="w-3 h-3 text-emerald-500" />}
              </div>
              {loc.note && <div className="text-[10px] text-muted-foreground mt-0.5">{loc.note}</div>}
              <div className="text-[10px] text-mono text-muted-foreground/70 mt-0.5 truncate">{loc.path}</div>
            </button>
          ))}
        </div>
      )}
      <Button onClick={async () => { await persist({ permissions_reviewed: true }); onDone?.(); }}>Continue</Button>
    </div>
  );
}


// ── Step: who is reading ─────────────────────────────────────────────────────
//
// Asked before anything technical, and answered in one tap. Everything the guides say afterwards is
// written four times over — the same fact, pitched differently — and this is what picks which one.
//
// The levels are about *prior knowledge*, deliberately not about age. A twelve-year-old who has made
// videos before is not a beginner, and a professional composer meeting a diffusion model for the
// first time is. Age is offered separately and only to soften wording, never to withhold anything:
// an app that decides what somebody may attempt based on how old they say they are would be making
// a different promise than this one.
function AudienceStepBody({ settings, onDone }) {
  const [level, setLevel] = useState(settings?.audience_level || "");
  const [saving, setSaving] = useState(false);

  const LEVELS = [
    { id: "kid", label: "Show me simply",
      hint: "Short sentences, plain words, one idea at a time. Nothing is hidden — it is just explained more slowly." },
    { id: "beginner", label: "I'm new to this",
      hint: "Every term explained the first time it appears, and why a step exists before how to do it." },
    { id: "adult", label: "I've used creative apps",
      hint: "Assumes you know what a project, a track and an export are. Explains only what is specific to this app." },
    { id: "pro", label: "I do this professionally",
      hint: "Dense and fast. Names the model, the sampler and the trade-off, and skips the reassurance." },
  ];

  const save = async (id) => {
    setLevel(id);
    setSaving(true);
    await persist({ audience_level: id });
    setSaving(false);
    onDone?.();
  };

  return (
    <div className="space-y-3">
      <p className="text-muted-foreground text-sm">
        How much should the guides explain? This changes the wording everywhere, not what you are
        allowed to do — every feature is available at every level, and you can change this whenever
        you like.
      </p>
      <div className="grid sm:grid-cols-2 gap-2">
        {LEVELS.map((l) => (
          <button key={l.id} type="button" onClick={() => save(l.id)} disabled={saving}
            className={`text-left rounded-lg border p-3 transition-colors ${
              level === l.id ? "border-primary bg-primary/10" : "border-border hover:bg-muted/40"}`}>
            <div className="font-medium text-sm flex items-center gap-1.5">
              {l.label}
              {level === l.id && <CheckCircle2 className="w-3.5 h-3.5 text-emerald-500" />}
            </div>
            <div className="text-[11px] text-muted-foreground mt-0.5 leading-snug">{l.hint}</div>
          </button>
        ))}
      </div>
      <p className="text-xs text-muted-foreground">
        Nothing here is sent anywhere. It only decides which wording the app reaches for.
      </p>
    </div>
  );
}

// ── The registry ─────────────────────────────────────────────────────────────
/// The terms, read from the server so they are never a stale copy compiled into the binary.
///
/// First in the guide on purpose. Everything here is about what leaves the machine, and asking
/// somebody to agree to that *after* they have set up four API keys is asking at the point where
/// saying no is expensive.
function TermsStepBody({ settings, save }) {
  const [terms, setTerms] = useState("");
  useEffect(() => {
    api.subsTerms()
      .then((t) => setTerms(t?.markdown || t?.text || "The terms could not be loaded right now."))
      .catch(() => setTerms("The terms could not be loaded — check your connection."));
  }, []);
  return (
    <div className="space-y-3">
      <div className="max-h-64 overflow-y-auto rounded-lg border border-border/60 p-3 text-xs leading-relaxed">
        <Markdown text={terms} />
      </div>
      <label className="flex items-start gap-2 text-sm cursor-pointer">
        <input type="checkbox" className="accent-primary mt-1"
               data-testid="guide-terms-accept"
               checked={!!settings?.terms_accepted_at}
               onChange={(e) => save({ terms_accepted_at: e.target.checked ? new Date().toISOString() : "" })} />
        <span>
          <b>I have read these.</b>
          <span className="text-muted-foreground"> The short version: your work stays on your
          machine, crashes are reported automatically so they can be fixed, and during the free week
          which screens you open is counted. Nothing you write is ever sent unless you press send.</span>
        </span>
      </label>
      <label className="flex items-start gap-2 text-sm cursor-pointer">
        <input type="checkbox" className="accent-primary mt-1"
               data-testid="guide-hotjar-optin"
               checked={settings?.hotjar_opt_in === true}
               onChange={(e) => save({ hotjar_opt_in: e.target.checked })} />
        <span>
          <b>Record how I use the app, to help fix it.</b>
          <span className="text-muted-foreground"> Off unless you switch it on, and off by default
          forever. This one loads a third-party script (Hotjar) that watches clicks and scrolling in
          the interface — which is why it is a question rather than a default.</span>
        </span>
      </label>
    </div>
  );
}

export const GUIDE_STEPS = [
  {
    id: "terms",
    title: "What this does with what it sees",
    icon: ScrollText,
    blurb: "The terms, in the order that matters — before anything is connected.",
    isDone: (s) => !!s?.terms_accepted_at,
    Body: TermsStepBody,
  },
  {
    id: "audience",
    title: "How much should I explain?",
    icon: Target,
    blurb: "Pitch every explanation at the level you want — changeable any time.",
    isDone: (s) => !!s?.audience_level,
    Body: AudienceStepBody,
  },
  {
    id: "language",
    title: "Your language",
    icon: Languages,
    blurb: "Which language the interface should be in.",
    isDone: (s) => !!s?.ui_language,
    Body: LanguageStepBody,
  },
  {
    id: "voice",
    title: "The assistant's voice",
    icon: Volume2,
    blurb: "Have the guides speak, and answer them out loud.",
    isDone: (s) => !!s?.voice_engine,
    Body: VoiceStepBody,
  },
  {
    id: "ai",
    title: "The AI brain",
    icon: Bot,
    blurb: "A provider key — free or paid — for lyrics, styles, translation and the guides.",
    // Any of the four counts: whichever provider is configured is the one that gets used.
    isDone: (s) => !!(s?.openrouter_api_key?.trim() || s?.gemini_api_key?.trim()
      || s?.anthropic_api_key?.trim() || s?.openai_api_key?.trim()),
    Body: AiStepBody,
  },
  {
    id: "permissions",
    title: "Permissions",
    icon: ShieldCheck,
    blurb: "Microphone, a folder for your files, and — on a phone — installing updates. All optional.",
    isDone: (s) => s?.permissions_reviewed === true,
    Body: PermissionsStepBody,
  },
  {
    id: "goal",
    title: "What you're here to do",
    icon: Target,
    blurb: "So setup only asks for what your goal actually needs.",
    isDone: (s) => !!s?.user_goal,
    Body: GoalStepBody,
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

/**
 * What first run is allowed to ask, and in what order.
 *
 * Everything else — API keys, GPU accounts, OAuth, folders — was moved out of the way. Asking a
 * stranger for four provider keys before they have seen a single screen of the app puts the most
 * technical minutes of the whole product in front of the person least equipped to sit through them,
 * and the honest answer to "why do I need this?" at that moment is "you don't know yet".
 *
 * So first run asks only what genuinely has to come first: what the app does with what it sees, how
 * much to explain, which language, and which voice. Then it shows the app. The rest lives in
 * "Set up & configure" under the graduation-cap menu, and is asked for when something needs it.
 */
export const FIRST_RUN_STEP_IDS = ["terms", "audience", "language", "voice"];
export const FIRST_RUN_STEPS = GUIDE_STEPS.filter((s) => FIRST_RUN_STEP_IDS.includes(s.id));
export const SETUP_STEPS = GUIDE_STEPS.filter((s) => !FIRST_RUN_STEP_IDS.includes(s.id));

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
