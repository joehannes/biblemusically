import { useEffect, useState, useCallback, memo, useRef } from "react";
import { useNavigate } from "react-router-dom";
import appPkg from "../../package.json";
import { api } from "../lib/api";
import { useBackgroundSave, restore } from "../lib/hooks";
import { Card } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Button } from "../components/ui/button";
import { Label } from "../components/ui/label";
import { Badge } from "../components/ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Switch } from "../components/ui/switch";
import VoicePicker from "../components/VoicePicker";
import { PowerOff, UserCog, Cookie, KeyRound, Music2, Image as Img, Film, ShieldCheck, CheckCircle2, XCircle, Save, Bot, HelpCircle, ExternalLink, DownloadCloud, Sparkles, Gauge, Loader2 } from "lucide-react";
import { getStepForPath } from "../lib/pageSteps";
import { autoStartKaggleServer, subscribeKaggle } from "../lib/kaggleServerPipeline";
import { markStopped } from "../lib/serverLifecycle";
import { useRequireGuideStep } from "../components/GuideStepDialog";
import OAuthClientsPanel from "../components/OAuthClientsPanel";
import { toast } from "sonner";
import { AutoSaveChip } from "../lib/hooks";
import { useStudio } from "../lib/store";

const FREE_MODELS = [
  { id: "qwen/qwen3-next-80b-a3b-instruct:free", name: "Qwen 3 Next 80B (Free)" },
  { id: "nvidia/nemotron-3-super-120b-a12b:free", name: "Nemotron 3 Super 120B (Free)" },
  { id: "nvidia/nemotron-3-ultra-550b-a55b:free", name: "Nemotron 3 Ultra 550B (Free)" },
  { id: "google/gemma-4-31b-it:free", name: "Gemma 4 31B IT (Free)" },
  { id: "meta-llama/llama-3.3-70b-instruct:free", name: "Llama 3.3 70B (Free)" },
  { id: "nousresearch/hermes-3-llama-3.1-405b:free", name: "Hermes 3 405B (Free)" },
  { id: "openai/gpt-oss-120b:free", name: "GPT-OSS 120B (Free)" },
  { id: "cognitivecomputations/dolphin-mistral-24b-venice-edition:free", name: "Dolphin Mistral 24B (Free)" },
];

// Quick-setup engine combos: bundles the two engine choices most people actually want together.
// `browserFree` flags whether the combo needs the embedded Browser tab kept open — only Suno
// does (it has no public API, so it's driven by DOM automation); every other engine here is a
// plain REST job the app talks to directly, so a combo without Suno can run through Workflow's
// "Run full pipeline" completely unattended. This is exactly the distinction the Workflow page
// itself surfaces, so the two are meant to be read together.
const ENGINE_PRESETS = [
  {
    id: "free-heartmula-comfyui",
    label: "Free & unattended — HeartMuLa + ComfyUI",
    music_engine: "heartmula", image_engine: "comfyui",
    browserFree: true,
    hint: "Both engines are free, self-hosted REST servers (Kaggle/Colab or local GPU). No Browser-tab babysitting — safe to run unattended from Workflow's \"Run full pipeline\".",
  },
  {
    id: "free-acestep-flux",
    label: "Free & unattended — ACE-Step + FLUX",
    music_engine: "acestep", image_engine: "flux",
    browserFree: true,
    hint: "The other free pair — ACE-Step for music, FLUX.1 schnell for images. FLUX needs a one-time Hugging Face token (see the FLUX section below).",
  },
  {
    id: "premium-suno-mj",
    label: "Premium — Suno + Midjourney",
    music_engine: "suno", image_engine: "midjourney",
    browserFree: false,
    hint: "Best output quality, but Suno has no public API — it's driven by the embedded Browser tab, so expect the app to switch you there during music generation. Midjourney itself runs through a proxy, not the browser.",
  },
  {
    id: "hybrid-suno-comfyui",
    label: "Hybrid — Suno + ComfyUI",
    music_engine: "suno", image_engine: "comfyui",
    browserFree: false,
    hint: "Suno for music quality, ComfyUI for fully-unattended images and character consistency. Only the music stage needs the Browser tab.",
  },
];

// ComfyUI speed/quality tradeoff — steps and CFG are the two knobs that most directly trade
// render time for detail; width/height/checkpoint/style are left alone since those are
// compositional choices, not a quality dial.
const IMAGE_QUALITY_PRESETS = [
  { id: "draft", label: "Draft (fast)", comfyui_steps: 15, comfyui_cfg: 5.5, hint: "Fewer steps, lower CFG — quick iteration, rougher detail." },
  { id: "balanced", label: "Balanced (default)", comfyui_steps: 30, comfyui_cfg: 6.5, hint: "Good detail at a reasonable render time." },
  { id: "quality", label: "Quality (slow)", comfyui_steps: 45, comfyui_cfg: 7.5, hint: "More steps, higher CFG — best detail, noticeably slower per image." },
];

// Field component moved OUTSIDE of SettingsComponent to prevent focus-loss on re-render.
const Field = memo(({ k, label, placeholder, type="text", testid, value, onValueChange }) => {
  const handleChange = useCallback((e) => {
    onValueChange({ [k]: e.target.value });
  }, [k, onValueChange]);
  
  return (
    <div className="space-y-1">
      <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">{label}</Label>
      <Input data-testid={testid} type={type} value={value || ""} onChange={handleChange} placeholder={placeholder} />
    </div>
  );
});
Field.displayName = 'Field';

// AuthField moved OUTSIDE too.
const AuthField = memo(({ label, placeholder, type="text", value, onChange }) => {
  const handleChange = useCallback((e) => {
    onChange(e.target.value);
  }, [onChange]);
  
  return (
    <div className="space-y-1">
      <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">{label}</Label>
      <Input type={type} value={value} onChange={handleChange} placeholder={placeholder} />
    </div>
  );
});
AuthField.displayName = 'AuthField';


const initialSettings = {
  music_engine: "suno",
  music_engine_fallback: "none",
  acestep_api_url: "",
  acestep_api_key: "",
  acestep_duration: 240,
  heartmula_api_url: "",
  heartmula_api_key: "",
  ai_provider: "openrouter",
  anthropic_api_key: "",
  anthropic_model: "claude-opus-5",
  openai_api_key: "",
  openai_model: "gpt-5.1",
  // Opt-in engine tuning: blank/0 keeps the built-in per-task behaviour.
  ai_temperature: "",
  ai_max_tokens: "",
  ai_timeout_s: "",
  gemini_api_key: "",
  gemini_model: "gemini-3.5-flash",
  image_engine: "midjourney",
  flux_api_url: "",
  flux_api_key: "",
  comfyui_api_url: "",
  comfyui_api_key: "",
  comfyui_ckpt: "sd_xl_base_1.0.safetensors",
  comfyui_auto_config: true,
  comfyui_style: "photoreal",
  comfyui_steps: 30,
  comfyui_cfg: 6.5,
  comfyui_width: 1024,
  comfyui_height: 1024,
  comfyui_negative: "",
  comfyui_ip_weight: 0.7,
  comfyui_prompt_prefix: "",
  comfyui_workflow_mode: "auto",
  comfyui_custom_workflow: "",
  video_width: 1280,
  video_height: 720,
  video_overlay_url: "",
  video_overlay_mode: "screen",
  video_overlay_opacity: 0.35,
  overlay_auto_use: true,
  overlay_style: "cqt",
  overlay_engine: "ffmpeg",
  projectm_path: "",
  video_transitions_enabled: false,
  video_transition: "fade",
  video_transition_dur: 0.7,
  suno_cookie: "",
  mj_profile_dir: "",
  mj_discord_token: "",
  google_client_id: "",
  google_client_secret: "",
  google_redirect_uri: "",
  remote_render_provider: "local",
  modal_endpoint: "",
  render_worker_url: "",
  ffmpeg_path: "ffmpeg",
  ffprobe_path: "ffprobe",
  theme: "obsidian",
  qwen_endpoint: "",
  openrouter_api_key: "",
  openrouter_email: "",
  openrouter_model: "qwen/qwen3-next-80b-a3b-instruct:free",
  mj_proxy_url: "",
  gdrive_connected: false,
  gdrive_email: "",
  google_oauth_client_id: "",
  max_concurrent_jobs: 2,
};

const SettingsComponent = () => {
  const { activeProjectId } = useStudio();
  const [s, setS] = useState(initialSettings);
  const appVersion = appPkg.version || "0.6.1";
  
  const updateS = useCallback((updates) => {
    setS(prev => ({ ...prev, ...updates }));
  }, []);
  const [status, setStatus] = useState({});
  const [kaggleState, setKaggleState] = useState({});
  const [loaded, setLoaded] = useState(false);
  const [nodeInfo, setNodeInfo] = useState(null);
  const [oauthClients, setOauthClients] = useState([]);
  const [mjLogin, setMjLogin] = useState({ account: "", password: "", twofa: "" });
  const updateMjLogin = useCallback((updates) => {
    setMjLogin(prev => ({ ...prev, ...updates }));
  }, []);
  const [mjLoginStatus, setMjLoginStatus] = useState({});
  const [loginInProgress, setLoginInProgress] = useState(false);
  const [mjGenerating, setMjGenerating] = useState(false);
  const navigate = useNavigate();
  const { dialog: kaggleGuideDialog, openStep: openKaggleGuide } = useRequireGuideStep("kaggle");
  const [mjPrompt, setMjPrompt] = useState("");
  const [mjGenerateError, setMjGenerateError] = useState(null);
  const [mjResults, setMjResults] = useState([]);

  useEffect(() => {
    setLoaded(false);
    (async () => {
      try {
        const settings = await api.getSettings(activeProjectId);

        // Try to restore any auto-saved draft first (for persistence across tab switches)
        let restored = null;
        try {
          restored = await restore("settings-mirror");
        } catch (e) {
          console.error("Failed to restore settings draft", e);
        }

        // Merge: initialSettings -> saved settings -> restored draft (draft takes precedence)
        setS(prev => ({ ...prev, ...initialSettings, ...settings, ...(restored || {}) }));

        let clients = await api.listOauthClients();
        setOauthClients(clients || []);

        if ((!clients || clients.length === 0) && settings.google_client_id) {
          try {
            await api.createOauthClient({
              label: "Settings: default",
              client_id: settings.google_client_id,
              client_secret: settings.google_client_secret || "",
              redirect_uri: settings.google_redirect_uri || "",
              languages: [],
            });
            clients = await api.listOauthClients();
            setOauthClients(clients || []);
          } catch (e) {
            console.error("Failed to migrate settings into oauth client", e);
          }
        } else if (clients && clients.length > 0) {
          setS(prev => ({ ...prev, google_client_id: clients[0].client_id || prev.google_client_id, google_redirect_uri: clients[0].redirect_uri || prev.google_redirect_uri }));
        }
      } catch (error) {
        console.error("Failed to load settings", error);
        toast.error("Unable to load settings. Please restart the app.");
      } finally {
        setLoaded(true);
      }
    })();
  }, [activeProjectId]);

  const [asStatus, setAsStatus] = useState("idle");
  const [lastSaved, setLastSaved] = useState(null);

  useEffect(() => {
    if (!loaded) return;
    setAsStatus("saving");
    const timer = setTimeout(async () => {
      try {
        await api.saveSettings(s, activeProjectId);
        setAsStatus("saved");
        setLastSaved(Date.now());
      } catch (e) {
        setAsStatus("error");
      }
    }, 900);
    return () => clearTimeout(timer);
  }, [s, activeProjectId, loaded]);

  // The provider catalogue (labels, costs, prerequisites) comes from the backend so the facts live
  // in one place next to the launchers that implement them.
  const [renderProviders, setRenderProviders] = useState([]);
  // Live model list for the selected paid provider (empty until "List models" is pressed).
  const [paidModels, setPaidModels] = useState([]);
  const [loadingModels, setLoadingModels] = useState(false);
  useEffect(() => { api.listRenderProviders().then((r) => setRenderProviders(r?.providers || [])).catch(() => {}); }, []);

  const save = async () => {
    try {
      await api.saveSettings(s, activeProjectId);
      try {
        const clients = await api.listOauthClients();
        if (!clients || clients.length === 0) {
          if (s.google_client_id) {
            await api.createOauthClient({
              label: "Settings: default",
              client_id: s.google_client_id,
              client_secret: s.google_client_secret || "",
              redirect_uri: s.google_redirect_uri || "",
              languages: [],
            });
          }
        } else {
          // update first client so settings reflect the same data source
          const first = clients[0];
          const payload = {
            label: first.label || "Settings: default",
            client_id: s.google_client_id || first.client_id,
            redirect_uri: s.google_redirect_uri || first.redirect_uri,
            languages: first.languages || [],
            notes: first.notes || "",
          };
          if (s.google_client_secret) payload.client_secret = s.google_client_secret;
          await api.updateOauthClient(first.id, payload);
        }
        const refreshed = await api.listOauthClients();
        setOauthClients(refreshed || []);
      } catch (e) {
        console.error("Failed to sync settings to oauth pool", e);
      }
      // Clear the auto-saved draft after successful explicit save
      try { localStorage.removeItem("draft:settings-mirror"); } catch {}
      toast.success("Settings saved");
    } catch (err) {
      console.error("Failed to save settings", err);
      toast.error(err?.message || "Settings save failed.");
    }
  };

  const autoLogin = async () => {
    if (!mjLogin.account || !mjLogin.password || !mjLogin.twofa) {
      toast.error("Discord email, password and 2FA are required for auto-login.");
      return;
    }
    setLoginInProgress(true);
    try {
      const r = await api.mjAutoLogin(mjLogin.account, mjLogin.password, mjLogin.twofa);
      setMjLoginStatus(r || {});
      if (r.ok) {
        toast.success("Discord login succeeded and token has been stored.");
        const refreshed = await api.getSettings();
        setS(prev => ({ ...prev, ...refreshed }));
        setMjLogin({ account: "", password: "", twofa: "" });
      } else {
        toast.error(r.detail || r.error || "Discord auto-login failed.");
      }
    } catch (err) {
      console.error("Auto-login failed", err);
      toast.error("Discord auto-login failed. See console for details.");
    } finally {
      setLoginInProgress(false);
    }
  };

  const testS = async (kind) => {
    const r = kind === "suno" ? await api.testSuno()
      : kind === "acestep" ? await api.testAcestep()
      : kind === "heartmula" ? await api.testHeartmula()
      : kind === "flux" ? await api.testFlux()
      : kind === "comfy" ? await api.testComfy()
      : kind === "mj" ? await api.testMj()
      : await api.testFfmpeg();
    setStatus(p => ({ ...p, [kind]: r }));
    r.ok ? toast.success(r.detail || "ok") : toast.error(r.detail || "fail");
  };

  // Engine session logins run in the embedded webview (Web Browser view): it establishes
  // a reusable Google session first, then opens the engine's login page so "Continue with
  // Google" works — all pages share the webview's cookie store, and the session then also
  // drives the in-webview automations.
  const openSunoLogin = () => {
    navigate("/browser?site=suno&auto=login");
  };

  // ── Kaggle server helpers ──────────────────────────────────────
  // These engines run as notebooks on the user's Kaggle account and expose an
  // ephemeral trycloudflare tunnel that rotates each run — so instead of hand-
  // copying a URL, "Fetch live URL" pulls the current one from the kernel log.
  useEffect(() => subscribeKaggle(setKaggleState), []);

  // Opens the notebook in the user's real system browser rather than the embedded webview.
  // Kaggle's own "Continue with Google" sign-in reliably gets rejected by Google inside the
  // embedded WebKit webview ("This browser or app may not be secure") no matter how the UA is
  // spoofed — Google deliberately blocks sign-in from embedded/automated webviews, which showed
  // up as the login page reloading in a loop and never reaching Kaggle. None of the CLI-driven
  // Start/Fetch flow below needs a browser session at all (it authenticates via ~/.kaggle/
  // kaggle.json), so this is only for the parts that do need the Kaggle web UI — signing in,
  // watching a run, clicking "Stop Session" on a stuck GPU slot, or adding a missing secret like
  // flux's HF_TOKEN — and the system browser, where Google sign-in already works normally, does
  // all of that without hitting the block.
  // Open the engine's notebook in the IN-APP browser rather than launching an external window
  // (the external opener was silently failing — the button just flashed). Falls back to the
  // system browser only if we can't resolve the URL.
  const openKaggleNb = async (engine) => {
    try {
      const r = await api.kaggleNotebookUrl(engine);
      if (r?.url) {
        navigate(`/browser?url=${encodeURIComponent(r.url)}&label=${encodeURIComponent(engine + " notebook")}`);
        return;
      }
      await api.openKaggleNotebook(engine);
    } catch (e) {
      toast.error(String(e));
    }
  };

  // One click: skip straight to testing if a server's already live, otherwise start a fresh
  // Kaggle run, poll until it's up, pull its tunnel URL and verify the connection — see
  // kaggleServerPipeline.js for why this can be plain client-side polling instead of a new
  // backend job (every step already runs headlessly via the local kaggle CLI).
  const autoStartServer = (engine) => {
    autoStartKaggleServer(engine);
    toast.message(`Bringing up ${engine} on Kaggle — this can take ~8-10 min the first time.`);
  };

  const openKaggleToken = async () => {
    try {
      await api.openKaggleTokenPage();
      toast.success("Kaggle settings opened. Create New API Token → save it as ~/.kaggle/kaggle.json.");
    } catch (err) {
      console.error(err);
      toast.error("Could not open Kaggle settings.");
    }
  };

  const fetchKaggleUrl = async (engine) => {
    try {
      toast.message(`Fetching the live URL from the ${engine} Kaggle kernel…`);
      const r = await api.fetchKaggleUrl(engine);
      setStatus(p => ({ ...p, [`${engine}Kaggle`]: r }));
      if (r.ok && r.url) {
        const key = `${engine === "comfy" ? "comfyui" : engine}_api_url`;
        updateS({ [key]: r.url });
        toast.success(r.detail || `Live URL saved: ${r.url}`);
      } else {
        toast.error(r.detail || "No live URL found.");
        if (r.next_step) toast.message(r.next_step);
      }
    } catch (err) {
      console.error(err);
      toast.error("Fetch failed — is the kaggle CLI installed and ~/.kaggle/kaggle.json present?");
    }
  };

  // Connected Kaggle accounts (GPU quota is per-account; the app rotates between them on
  // exhaustion). Keys never leave the backend — this only ever shows usernames + which is active.
  const [kaggleAccounts, setKaggleAccounts] = useState([]);
  const loadKaggleAccounts = async () => {
    try { const r = await api.listKaggleAccounts(); setKaggleAccounts(r?.accounts || []); }
    catch { setKaggleAccounts([]); }
  };
  useEffect(() => { loadKaggleAccounts(); }, []);
  const activateAccount = async (username) => {
    try { await api.activateKaggleAccount(username); toast.success(`Now using ${username}.`); loadKaggleAccounts(); }
    catch (e) { toast.error(String(e)); }
  };
  const removeAccount = async (username) => {
    try { await api.removeKaggleAccount(username); loadKaggleAccounts(); }
    catch (e) { toast.error(String(e)); }
  };

  // Add another Google/Kaggle account. Free GPU quota is per-account, so connecting more accounts
  // lets the app rotate to a fresh one when a quota runs out — it re-opens the SAME guided Kaggle
  // step from first-run setup, which also deploys the engines to the new account.
  const switchKaggleAccount = () => { openKaggleGuide(); };

  // Manually end a session so it stops consuming the free weekly GPU quota.
  const stopKaggleServer = async (engine) => {
    try {
      const r = await api.stopKaggleServer(engine);
      markStopped(engine);
      r?.ok ? toast.success(r.detail || `${engine} stopped.`) : toast.error(r?.detail || "Could not stop the session.");
    } catch (e) { toast.error(String(e)); }
  };

  const startKaggleServer = async (engine) => {
    try {
      toast.message(`Starting the ${engine} server on Kaggle…`);
      const r = await api.startKaggleServer(engine);
      setStatus(p => ({ ...p, [`${engine}KaggleStart`]: r }));
      if (r.ok) toast.success(r.detail);
      else { toast.error(r.detail || "Start failed."); if (r.next_step) toast.message(r.next_step); }
    } catch (err) {
      console.error(err);
      toast.error("Start failed — is the kaggle CLI installed and ~/.kaggle/kaggle.json present?");
    }
  };

  const openMjLogin = () => {
    navigate("/browser?site=midjourney&auto=login");
  };

  const captureMjSession = async () => {
    setLoginInProgress(true);
    // Webview first: if a Midjourney session lives in the embedded browser, seed the
    // Playwright automation profile from its cookies; else run the visible browser flow.
    try {
      const w = await api.webviewCaptureMjSession(false);
      if (w?.ok && w.profile_dir) {
        setS(prev => ({ ...prev, mj_profile_dir: w.profile_dir }));
        toast.success("Midjourney session captured from the webview into the automation profile.");
        setLoginInProgress(false);
        return;
      }
    } catch { /* no webview session — fall through */ }
    try {
      const r = await api.captureMjSession();
      setStatus(p => ({ ...p, mj: r }));
      if (r.ok && r.profile_dir) {
        setS(prev => ({ ...prev, mj_profile_dir: r.profile_dir }));
        toast.success("Midjourney Playwright profile captured and stored.");
      } else {
        toast.error(r.detail || r.error || "Midjourney session capture failed.");
      }
    } catch (err) {
      console.error(err);
      toast.error("Midjourney session capture failed. See console for details.");
    } finally {
      setLoginInProgress(false);
    }
  };

  const generateMidjourneyImage = async () => {
    if (!mjPrompt.trim()) {
      toast.error("Please enter a prompt first.");
      return;
    }
    setMjGenerating(true);
    setMjGenerateError(null);
    setMjResults([]);
    try {
      const r = await api.generateMjNow(mjPrompt);
      if (r.ok && r.paths) {
        setMjResults(r.paths);
        toast.success("Midjourney image generation complete.");
      } else {
        const errMsg = r.detail || r.error || "Generation failed.";
        setMjGenerateError(errMsg);
        toast.error(errMsg);
      }
    } catch (err) {
      console.error("Midjourney generation failed", err);
      setMjGenerateError(err.message || "Midjourney generation failed");
      toast.error("Midjourney generation failed. See console for details.");
    } finally {
      setMjGenerating(false);
    }
  };

  const bulkGenerateAll = async () => {
    setMjGenerating(true);
    try {
      const r = await api.bulkGenerateAll(activeProjectId);
      if (r && r.queued !== undefined) {
        toast.success(`Queued ${r.queued} image jobs.`);
      } else if (r && r.ok === false) {
        toast.error(r.detail || r.error || 'Bulk generation failed');
      } else {
        toast.success('Bulk generation queued.');
      }
    } catch (err) {
      console.error('Bulk generate failed', err);
      toast.error('Bulk generate failed. See console.');
    } finally {
      setMjGenerating(false);
    }
  };

  // Auto-sync first OAuth client when the legacy settings fields change (debounced)
  useEffect(() => {
    if (!loaded) return;
    const first = oauthClients && oauthClients[0];
    if (!first) return;
    const timer = setTimeout(async () => {
      try {
        const payload = {
          label: first.label || "Settings: default",
          client_id: s.google_client_id || first.client_id,
          redirect_uri: s.google_redirect_uri || first.redirect_uri,
          languages: first.languages || [],
          notes: first.notes || "",
        };
        if (s.google_client_secret) payload.client_secret = s.google_client_secret;
        await api.updateOauthClient(first.id, payload);
        const refreshed = await api.listOauthClients();
        setOauthClients(refreshed || []);
      } catch (e) {
        console.error("Failed to auto-sync settings -> oauth client", e);
      }
    }, 900);
    return () => clearTimeout(timer);
  }, [s.google_client_id, s.google_redirect_uri, s.google_client_secret, oauthClients, loaded]);

  const StatusPill = ({ k }) => status[k] ? (
    <Badge variant={status[k].ok?"default":"destructive"} className="ml-2">{status[k].ok?<CheckCircle2 className="w-3 h-3 mr-1" />:<XCircle className="w-3 h-3 mr-1" />}{status[k].ok?"connected":"fail"}</Badge>
  ) : null;

  // Live progress panel for the one-click Kaggle provisioning flow: phase steps, an elapsed timer,
  // the streaming notebook log, and — when a run fails — the actual cause plus an actionable button.
  const KAGGLE_STEPS = [
    { key: "queued", label: "Queue" },
    { key: "installing", label: "Install" },
    { key: "downloading", label: "Download" },
    { key: "loading", label: "Load" },
    { key: "serving", label: "Serve" },
    { key: "ready", label: "Ready" },
  ];
  const kagglePhaseIndex = (phase) => {
    switch (phase) {
      case "checking": case "starting": case "queued": case "waiting": return 0;
      case "installing": return 1;
      case "downloading": return 2;
      case "loading": return 3;
      // "tunneling" = cloudflared started but the URL isn't routable yet; same step as serving.
      case "tunneling": case "serving": case "testing": return 4;
      case "ready": return 5;
      default: return -1; // idle/error/stopped
    }
  };
  const fmtElapsed = (s) => {
    if (!s && s !== 0) return null;
    const m = Math.floor(s / 60), sec = s % 60;
    return `${m}:${String(sec).padStart(2, "0")}`;
  };
  const KaggleAutoStatus = ({ engine }) => {
    const st = kaggleState[engine];
    if (!st) return null;
    const busy = ["checking", "starting", "waiting", "testing"].includes(st.status);
    const isError = st.status === "error";
    const isReady = st.status === "ready";
    const idx = kagglePhaseIndex(st.phase || st.status);
    const elapsed = fmtElapsed(st.monitor?.elapsed_s);
    // Prefer the rich streaming log from the Rust monitor; fall back to the orchestrator log lines.
    const monLog = st.monitor?.log || [];
    const orchLog = (st.log || []).map((l) => ({ level: l.level, text: l.message }));
    const lines = (monLog.length ? monLog : orchLog).slice(-8);
    const variant = isReady ? "default" : isError ? "destructive" : "secondary";
    const canFix = isError || busy;

    return (
      <div className="mt-3 rounded-lg border border-border bg-muted/30 p-3 text-xs" data-testid={`kaggle-progress-${engine}`}>
        <div className="flex items-center gap-2 flex-wrap">
          <Badge variant={variant} className="capitalize">{st.status}{busy && <span className="ml-1 animate-pulse">…</span>}</Badge>
          {st.monitor?.kernel_status && <span className="text-muted-foreground">kernel: {st.monitor.kernel_status}</span>}
          {elapsed && <span className="text-muted-foreground ml-auto tabular-nums">{elapsed}</span>}
        </div>

        {/* phase steps */}
        <div className="flex items-center gap-1 mt-2 flex-wrap">
          {KAGGLE_STEPS.map((step, i) => {
            const done = idx > i, active = idx === i;
            return (
              <div key={step.key} className="flex items-center gap-1">
                <span className={
                  "px-2 py-0.5 rounded text-[10px] " +
                  (isError && active ? "bg-destructive/20 text-destructive" :
                   done ? "bg-primary/20 text-primary" :
                   active ? "bg-primary text-primary-foreground" :
                   "bg-muted text-muted-foreground")
                }>{step.label}</span>
                {i < KAGGLE_STEPS.length - 1 && <span className="text-muted-foreground">›</span>}
              </div>
            );
          })}
        </div>

        {/* streaming log tail */}
        {lines.length > 0 && (
          <pre className="mt-2 max-h-28 overflow-y-auto scroll-thin whitespace-pre-wrap break-words font-mono text-[10px] leading-snug bg-background/60 rounded p-2">
            {lines.map((l, i) => (
              <div key={i} className={l.level === "error" ? "text-destructive" : l.level === "milestone" || l.level === "success" ? "text-primary" : "text-muted-foreground"}>
                {l.text}
              </div>
            ))}
          </pre>
        )}

        {/* error / next step + action */}
        {isError && st.detail && <div className="mt-2 text-destructive">{st.detail}</div>}
        {st.next_step && <div className="mt-1 text-muted-foreground">{st.next_step}</div>}

        {(canFix) && (
          <div className="flex flex-wrap gap-2 mt-2">
            <Button size="sm" variant="outline" className="h-7 text-[11px]" onClick={() => openKaggleNb(engine)}>
              <ExternalLink className="w-3 h-3 mr-1" />Open notebook
            </Button>
            {isError && (
              <Button size="sm" variant="secondary" className="h-7 text-[11px]" onClick={() => autoStartServer(engine)}>
                <Bot className="w-3 h-3 mr-1" />Retry
              </Button>
            )}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="p-8 max-w-5xl mx-auto fade-in">
      {kaggleGuideDialog}
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step {getStepForPath("/settings")}</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <div>
          <h1 className="text-4xl sm:text-5xl font-bold">Settings & Connections</h1>
          <div className="text-sm text-muted-foreground mt-2">Version {appVersion}</div>
        </div>
        <div className="flex items-center gap-3">
          <AutoSaveChip status={asStatus} lastSaved={lastSaved} />
          <Button data-testid="settings-save-btn" onClick={save}><Save className="w-4 h-4 mr-2" />Save now</Button>
        </div>
      </div>
      <p className="text-muted-foreground mb-8 max-w-2xl">All cookies, tokens and binary paths the engine uses live here. Nothing leaves your machine until you trigger an action.</p>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Sparkles className="w-4 h-4 text-primary" /><h2 className="font-semibold">Quick setup — engine presets</h2></div>
        <p className="text-xs text-muted-foreground mb-3">One click sets both the music and image engine below to a combo that makes sense together. You still need to fill in each engine's own URL/key/cookie — this just picks which ones to configure.</p>
        <div className="grid sm:grid-cols-2 gap-3">
          {ENGINE_PRESETS.map((p) => {
            const active = s.music_engine === p.music_engine && s.image_engine === p.image_engine;
            return (
              <button
                key={p.id}
                onClick={() => { updateS({ music_engine: p.music_engine, image_engine: p.image_engine }); toast.success(`Set music/image engine to: ${p.label}`); }}
                className={`text-left rounded-lg border p-3 transition-colors ${active ? "border-primary bg-primary/10" : "border-border hover:bg-muted/40"}`}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="text-sm font-medium">{p.label}</span>
                  <Badge variant={p.browserFree ? "secondary" : "outline"} className="text-[9px] shrink-0">
                    {p.browserFree ? "Browser-free" : "Needs Browser tab"}
                  </Badge>
                </div>
                <p className="text-[11px] text-muted-foreground mt-1">{p.hint}</p>
              </button>
            );
          })}
        </div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center justify-between gap-2 mb-3 flex-wrap">
          <div className="flex items-center gap-2"><UserCog className="w-4 h-4 text-primary" /><h2 className="font-semibold">Kaggle accounts</h2></div>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-add-account" onClick={switchKaggleAccount}><UserCog className="w-3 h-3 mr-2" />Add another account</Button>
        </div>
        <p className="text-xs text-muted-foreground mb-3">Free GPU time is per Kaggle account. Connect several and the app automatically rotates to the next one when a run is denied a GPU (quota spent). Keys stay on your machine — only usernames are shown here.</p>
        {kaggleAccounts.length === 0 ? (
          <div className="text-xs text-muted-foreground">No Kaggle account connected yet. Use "Add another account" to connect one.</div>
        ) : (
          <div className="space-y-1.5">
            {kaggleAccounts.map((a) => (
              <div key={a.username} className="flex items-center justify-between gap-2 rounded-md border border-border/70 px-3 py-2 text-sm">
                <div className="flex items-center gap-2 min-w-0">
                  <span className="truncate">{a.username}</span>
                  {a.active && <Badge variant="default" className="text-[9px]">active</Badge>}
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  {!a.active && <Button size="sm" variant="ghost" className="h-7 text-[11px]" onClick={() => activateAccount(a.username)}>Use</Button>}
                  <Button size="sm" variant="ghost" className="h-7 text-[11px] text-destructive hover:text-destructive" onClick={() => removeAccount(a.username)}>Remove</Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Music2 className="w-4 h-4 text-primary" /><h2 className="font-semibold">Music engine</h2></div>
        <div className="space-y-1 max-w-sm">
          <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Which backend generates songs</Label>
          <Select value={s.music_engine || "suno"} onValueChange={(val) => updateS({ music_engine: val })}>
            <SelectTrigger className="w-full" data-testid="settings-music-engine"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="suno">Suno (unofficial, needs cookie)</SelectItem>
              <SelectItem value="acestep">ACE-Step 1.5 (free, open source, self-hosted)</SelectItem>
              <SelectItem value="heartmula">HeartMuLa (free, Apache-2.0 Suno competitor)</SelectItem>
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground mt-1">ACE-Step is a free MIT-licensed alternative that runs on your own GPU (locally or a free Kaggle/Colab notebook) and generates up to 10-minute songs with vocals. Configure whichever engine you select below.</p>
        </div>
        <div className="space-y-1 max-w-sm mt-4">
          <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">If it fails, retry with</Label>
          <Select value={s.music_engine_fallback || "none"} onValueChange={(val) => updateS({ music_engine_fallback: val })}>
            <SelectTrigger className="w-full" data-testid="settings-music-engine-fallback"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="none">Nothing — fail and tell me</SelectItem>
              <SelectItem value="suno">Suno</SelectItem>
              <SelectItem value="acestep">ACE-Step 1.5</SelectItem>
              <SelectItem value="heartmula">HeartMuLa</SelectItem>
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground mt-1">A Suno cookie that expires overnight, or a Kaggle notebook that times out, otherwise fails every song queued behind it. With a fallback set, the job retries once on the other engine before giving up — the job log records which engine actually produced the audio.</p>
        </div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Music2 className="w-4 h-4 text-primary" /><h2 className="font-semibold">ACE-Step 1.5 (free, open source)</h2><StatusPill k="acestep" /></div>
        <div className="grid md:grid-cols-2 gap-4">
          <Field k="acestep_api_url" label="ACE-Step server URL" placeholder="https://xxxx.gradio.live or http://localhost:8001" testid="settings-acestep-url" value={s.acestep_api_url} onValueChange={updateS} />
          <Field k="acestep_api_key" label="API key (optional)" placeholder="only if the server sets ACESTEP_API_KEY" type="password" testid="settings-acestep-key" value={s.acestep_api_key} onValueChange={updateS} />
          <Field k="acestep_duration" label="Song length in seconds (10–600)" placeholder="240" type="number" testid="settings-acestep-duration" value={s.acestep_duration} onValueChange={updateS} />
        </div>
        <div className="flex flex-wrap gap-2 mt-3">
          <Button size="sm" data-testid="settings-kaggle-auto-acestep" onClick={()=>autoStartServer("acestep")} disabled={kaggleState.acestep && ["checking","starting","waiting","testing"].includes(kaggleState.acestep.status)}><Bot className="w-3 h-3 mr-2" />Start & connect</Button>
          <Button size="sm" variant="outline" data-testid="settings-kaggle-open-acestep" onClick={()=>openKaggleNb("acestep")}><ExternalLink className="w-3 h-3 mr-2" />Open notebook</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-start-acestep" onClick={()=>startKaggleServer("acestep")}><Bot className="w-3 h-3 mr-2" />Start server</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-stop-acestep" title="End the Kaggle session now so it stops using your free weekly GPU hours" onClick={()=>stopKaggleServer("acestep")}><PowerOff className="w-3 h-3 mr-2" />Stop server</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-fetch-acestep" onClick={()=>fetchKaggleUrl("acestep")}><DownloadCloud className="w-3 h-3 mr-2" />Fetch live URL</Button>
          <Button size="sm" variant="secondary" data-testid="settings-test-acestep" onClick={()=>testS("acestep")}><KeyRound className="w-3 h-3 mr-2" />Test connection</Button>
          <Button size="sm" variant="ghost" data-testid="settings-kaggle-token" onClick={openKaggleToken}><KeyRound className="w-3 h-3 mr-2" />Kaggle API token</Button>
          <Button size="sm" variant="ghost" data-testid="settings-kaggle-switch-account" title="Connect a different free Google/Kaggle account — GPU quota is per account" onClick={switchKaggleAccount}><UserCog className="w-3 h-3 mr-2" />Switch account</Button>
        </div>
        <KaggleAutoStatus engine="acestep" />
        <div className="mt-3 text-xs text-muted-foreground">Run the ACE-Step notebook (<code>scripts/kaggle_acestep/</code>) on a free Kaggle/Colab GPU — it prints a public URL you paste above. Note: Kaggle/Colab share URLs rotate roughly every 72 hours, so re-paste when it expires. For a permanent setup, run <code>acestep-api</code> on a local GPU and use <code>http://localhost:8001</code>.</div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Music2 className="w-4 h-4 text-primary" /><h2 className="font-semibold">HeartMuLa (free, Apache-2.0)</h2><StatusPill k="heartmula" /></div>
        <div className="grid md:grid-cols-2 gap-4">
          <Field k="heartmula_api_url" label="HeartMuLa server URL" placeholder="https://xxxx.trycloudflare.com or http://localhost:8003" testid="settings-heartmula-url" value={s.heartmula_api_url} onValueChange={updateS} />
          <Field k="heartmula_api_key" label="API key (optional)" placeholder="only if the server sets API_KEY" type="password" testid="settings-heartmula-key" value={s.heartmula_api_key} onValueChange={updateS} />
        </div>
        <div className="flex flex-wrap gap-2 mt-3">
          <Button size="sm" data-testid="settings-kaggle-auto-heartmula" onClick={()=>autoStartServer("heartmula")} disabled={kaggleState.heartmula && ["checking","starting","waiting","testing"].includes(kaggleState.heartmula.status)}><Bot className="w-3 h-3 mr-2" />Start & connect</Button>
          <Button size="sm" variant="outline" data-testid="settings-kaggle-open-heartmula" onClick={()=>openKaggleNb("heartmula")}><ExternalLink className="w-3 h-3 mr-2" />Open notebook</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-start-heartmula" onClick={()=>startKaggleServer("heartmula")}><Bot className="w-3 h-3 mr-2" />Start server</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-stop-heartmula" title="End the Kaggle session now so it stops using your free weekly GPU hours" onClick={()=>stopKaggleServer("heartmula")}><PowerOff className="w-3 h-3 mr-2" />Stop server</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-fetch-heartmula" onClick={()=>fetchKaggleUrl("heartmula")}><DownloadCloud className="w-3 h-3 mr-2" />Fetch live URL</Button>
          <Button size="sm" variant="secondary" data-testid="settings-test-heartmula" onClick={()=>testS("heartmula")}><KeyRound className="w-3 h-3 mr-2" />Test connection</Button>
        </div>
        <KaggleAutoStatus engine="heartmula" />
        <div className="mt-3 text-xs text-muted-foreground">Run the HeartMuLa notebook (<code>scripts/kaggle_heartmula/</code>, pushed as <code>biblemusically-heartmula-server</code>). Apache-2.0, commercially usable. Shares the ACE-Step song-length setting above. Full songs on a free T4 can take a few minutes — keep length modest.</div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Music2 className="w-4 h-4 text-primary" /><h2 className="font-semibold">Suno (unofficial)</h2><StatusPill k="suno" /></div>
        <Field k="suno_cookie" label="Suno session cookie (studio-api_key / __session / session_id)" placeholder="cookie string..." testid="settings-suno-cookie" value={s.suno_cookie} onValueChange={updateS} />
        <div className="flex flex-wrap gap-2 mt-3">
          <Button size="sm" variant="secondary" onClick={openSunoLogin}><Cookie className="w-3 h-3 mr-2" />Open Suno login</Button>
          <Button size="sm" variant="secondary" data-testid="settings-test-suno" onClick={()=>testS("suno")}><Cookie className="w-3 h-3 mr-2" />Test</Button>
          <Button size="sm" variant="secondary" onClick={async ()=>{
            // Webview first: if the user signed in via the embedded browser, lift the
            // cookie straight from its cookie store; else fall back to the Playwright flow.
            try {
              const w = await api.webviewCaptureSunoSession();
              if (w?.ok && w.cookie) {
                setS(prev => ({ ...prev, suno_cookie: w.cookie }));
                toast.success('Suno session captured from the webview session');
                return;
              }
            } catch { /* no webview page open — use the visible browser flow */ }
            try {
              const r = await api.captureSunoSession();
              if (r.ok && r.cookie) {
                setS(prev => ({ ...prev, suno_cookie: r.cookie }));
                toast.success('Suno session cookie captured');
              } else {
                toast.error(r.detail || 'Capture failed');
              }
            } catch (e) {
              console.error(e);
              toast.error('Suno capture failed');
            }
          }}><Cookie className="w-3 h-3 mr-2" />Capture session</Button>
        </div>
        <div className="mt-3 text-xs text-muted-foreground">After you log in to Suno, use the browser capture flow to persist the <code>studio-api_key</code>, <code>studio-api_key_local</code>, <code>__session</code> or <code>session_id</code> cookie to Settings. If you paste a bare token, it will be normalized to <code>studio-api_key=...</code>.</div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Bot className="w-4 h-4 text-primary" /><h2 className="font-semibold">AI text provider (lyrics &amp; assist)</h2></div>
        <div className="space-y-1 max-w-sm mb-5">
          <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Which LLM generates lyrics &amp; assist text</Label>
          <Select value={s.ai_provider || "openrouter"} onValueChange={(val) => updateS({ ai_provider: val })}>
            <SelectTrigger className="w-full" data-testid="settings-ai-provider"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="openrouter">OpenRouter — free models</SelectItem>
              <SelectItem value="gemini">Google Gemini — free tier or billed key</SelectItem>
              <SelectItem value="anthropic">Claude (Anthropic) — paid per token</SelectItem>
              <SelectItem value="openai">ChatGPT (OpenAI) — paid per token</SelectItem>
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground mt-1">
            Only the selected provider is used; the others can stay configured and idle. Whichever you pick,
            an overload or rate-limit falls back to a free model for that one request and tells you it did.
          </p>
        </div>

        {/* Tunables applied to whichever provider is active. Each is opt-in — leave blank/0 to keep
            the built-in behaviour (per-task temperatures, 120s timeout, no token cap). */}
        <div className="mb-6 rounded-md border border-border p-4">
          <div className="text-xs font-semibold uppercase tracking-widest text-muted-foreground mb-3">Engine tuning (applies to the selected provider)</div>
          <div className="grid md:grid-cols-3 gap-4">
            <Field k="ai_temperature" label="Temperature override" placeholder="blank = per-task default" type="number"
              testid="settings-ai-temperature" value={s.ai_temperature} onValueChange={updateS} />
            <Field k="ai_max_tokens" label="Max output tokens" placeholder="0 = provider default" type="number"
              testid="settings-ai-maxtokens" value={s.ai_max_tokens} onValueChange={updateS} />
            <Field k="ai_timeout_s" label="Request timeout (s)" placeholder="120" type="number"
              testid="settings-ai-timeout" value={s.ai_timeout_s} onValueChange={updateS} />
          </div>
          <p className="text-xs text-muted-foreground mt-2">
            Leave <b>Temperature</b> blank so each task keeps its own tuned value (lyrics are written more
            conservatively than style variations). Raise <b>Max output tokens</b> if long lyrics get truncated.
          </p>
        </div>

        {(s.ai_provider === "anthropic" || s.ai_provider === "openai") && (
          <div className="mb-6 rounded-md border border-border p-4">
            <div className="text-xs font-semibold uppercase tracking-widest text-muted-foreground mb-3">
              {s.ai_provider === "anthropic" ? "Claude (Anthropic)" : "ChatGPT (OpenAI)"}
            </div>
            <div className="grid md:grid-cols-2 gap-4">
              {s.ai_provider === "anthropic" ? (
                <Field k="anthropic_api_key" label="Anthropic API key" placeholder="sk-ant-..." type="password"
                  testid="settings-anthropic-key" value={s.anthropic_api_key} onValueChange={updateS} />
              ) : (
                <Field k="openai_api_key" label="OpenAI API key" placeholder="sk-..." type="password"
                  testid="settings-openai-key" value={s.openai_api_key} onValueChange={updateS} />
              )}
              <div className="space-y-1">
                <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Model</Label>
                <div className="flex gap-2">
                  {paidModels.length > 0 ? (
                    <select
                      value={s.ai_provider === "anthropic" ? (s.anthropic_model || "") : (s.openai_model || "")}
                      onChange={(e) => updateS(s.ai_provider === "anthropic"
                        ? { anthropic_model: e.target.value } : { openai_model: e.target.value })}
                      className="flex-1 bg-background border border-border rounded px-2 py-1.5 text-sm">
                      {paidModels.map((m) => <option key={m.id} value={m.id}>{m.label}</option>)}
                    </select>
                  ) : (
                    <Input
                      value={s.ai_provider === "anthropic" ? (s.anthropic_model || "") : (s.openai_model || "")}
                      onChange={(e) => updateS(s.ai_provider === "anthropic"
                        ? { anthropic_model: e.target.value } : { openai_model: e.target.value })}
                      placeholder={s.ai_provider === "anthropic" ? "claude-opus-5" : "gpt-5.1"} />
                  )}
                  {/* Asks the provider what this key can actually reach — model names move faster
                      than any list baked into the app, and a stale id is an unexplainable 404. */}
                  <Button size="sm" variant="secondary" disabled={loadingModels} onClick={async () => {
                    setLoadingModels(true);
                    try {
                      const r = await api.listAiModels(s.ai_provider);
                      setPaidModels(r?.models || []);
                      toast[(r?.models || []).length ? "success" : "error"]((r?.models || []).length
                        ? `${r.models.length} models available on this key.`
                        : "That key returned no models.");
                    } catch (err) { toast.error(String(err)); }
                    finally { setLoadingModels(false); }
                  }}>
                    {loadingModels ? <Loader2 className="w-3 h-3 animate-spin" /> : "List models"}
                  </Button>
                </div>
              </div>
            </div>
            <div className="mt-3 text-xs text-muted-foreground">
              Save the key first, then "List models" — the check runs server-side with the stored key.
              {s.ai_provider === "openai" && " An OpenAI API key is separate from a ChatGPT subscription; a Plus plan does not include API access."}
            </div>
          </div>
        )}

        {s.ai_provider === "gemini" && (
          <div className="mb-6 rounded-md border border-border p-4">
            <div className="text-xs font-semibold uppercase tracking-widest text-muted-foreground mb-3">Google Gemini</div>
            <div className="grid md:grid-cols-2 gap-4">
              <Field k="gemini_api_key" label="Gemini API key" placeholder="AIza..." type="password" testid="settings-gemini-key" value={s.gemini_api_key} onValueChange={updateS} />
              <div className="space-y-1">
                <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Gemini model</Label>
                <Select value={s.gemini_model || "gemini-3.5-flash"} onValueChange={(val) => updateS({ gemini_model: val })}>
                  <SelectTrigger className="w-full" data-testid="settings-gemini-model"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="gemini-3.5-flash">gemini-3.5-flash (best free-tier model)</SelectItem>
                    <SelectItem value="gemini-3.1-flash-lite">gemini-3.1-flash-lite (fastest, highest free quota)</SelectItem>
                    <SelectItem value="gemini-3-flash-preview">gemini-3-flash-preview</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
            <div className="mt-4 text-xs text-muted-foreground flex items-center gap-1.5">
              <HelpCircle className="w-3.5 h-3.5 text-primary" />
              Get a free key at <a href="https://aistudio.google.com/apikey" target="_blank" rel="noopener noreferrer" className="text-primary hover:underline font-semibold">aistudio.google.com/apikey</a> — no credit card required.
            </div>
          </div>
        )}

        <div className="text-xs font-semibold uppercase tracking-widest text-muted-foreground mb-3">OpenRouter</div>
        <div className="grid md:grid-cols-2 gap-4">
          <Field k="openrouter_api_key" label="OpenRouter API key" placeholder="sk-or-..." type="password" testid="settings-openrouter-key" value={s.openrouter_api_key} onValueChange={updateS} />
          <Field k="openrouter_email" label="Your email (for API attribution)" placeholder="you@example.com" type="email" testid="settings-openrouter-email" value={s.openrouter_email} onValueChange={updateS} />
          
          <div className="space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Select AI Model</Label>
            <Select 
              value={FREE_MODELS.some(m => m.id === s.openrouter_model) ? s.openrouter_model : (s.openrouter_model ? "custom" : "qwen/qwen3-next-80b-a3b-instruct:free")} 
              onValueChange={(val) => {
                if (val === "custom") {
                  updateS({ openrouter_model: "" });
                } else {
                  updateS({ openrouter_model: val });
                }
              }}
            >
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Choose a model..." />
              </SelectTrigger>
              <SelectContent>
                {FREE_MODELS.map(m => (
                  <SelectItem key={m.id} value={m.id}>{m.name}</SelectItem>
                ))}
                <SelectItem value="custom" className="font-semibold text-primary">Custom OpenRouter Model ID...</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>

        {(!FREE_MODELS.some(m => m.id === s.openrouter_model) || s.openrouter_model === "") && (
          <div className="mt-3.5 space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Custom Model Identifier</Label>
            <Input 
              data-testid="settings-openrouter-model" 
              value={s.openrouter_model || ""} 
              onChange={e => updateS({ openrouter_model: e.target.value })} 
              placeholder="e.g. google/gemma-4-31b-it:free" 
            />
          </div>
        )}

        <div className="mt-4 text-xs text-muted-foreground flex items-center gap-1.5">
          <HelpCircle className="w-3.5 h-3.5 text-primary" />
          Get your free API key at <a href="https://openrouter.ai/keys" target="_blank" rel="noopener noreferrer" className="text-primary hover:underline font-semibold">openrouter.ai/keys</a>. Free models have generous request limits.
        </div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Img className="w-4 h-4 text-primary" /><h2 className="font-semibold">Image engine</h2></div>
        <div className="space-y-1 max-w-sm">
          <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Which backend generates images</Label>
          <Select value={s.image_engine || "midjourney"} onValueChange={(val) => updateS({ image_engine: val })}>
            <SelectTrigger className="w-full" data-testid="settings-image-engine"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="midjourney">Midjourney (browser automation)</SelectItem>
              <SelectItem value="flux">FLUX.1 [schnell] (free, single-model, self-hosted)</SelectItem>
              <SelectItem value="comfyui">ComfyUI (free, multi-model: styles, character consistency, animation)</SelectItem>
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground mt-1">FLUX is a simple single-model server; ComfyUI is the multi-model engine for style presets, character consistency (IP-Adapter) and animation. Both run free on Kaggle/Colab or a local GPU. Configure whichever engine you select below.</p>
        </div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Img className="w-4 h-4 text-primary" /><h2 className="font-semibold">ComfyUI (free, multi-model)</h2><StatusPill k="comfy" /></div>
        <label className="flex items-start gap-2 mb-4 cursor-pointer" title="When on, the server is asked which checkpoints it actually has; the best installed one is chosen and its ideal steps/CFG/sampler are applied automatically (e.g. few-step settings for a Lightning/Turbo model). Turn off to force the manual values below.">
          <input type="checkbox" className="accent-primary mt-0.5" checked={s.comfyui_auto_config !== false}
            data-testid="settings-comfy-autoconfig"
            onChange={(e) => updateS({ comfyui_auto_config: e.target.checked })} />
          <span className="text-xs">
            <span className="font-medium">Auto model config</span>
            <span className="text-muted-foreground"> — detect installed checkpoints and pick the best one with ideal settings. When on, the manual Quality preset / Steps / CFG below are overridden per model.</span>
          </span>
        </label>
        <div className={`flex items-center gap-2 mb-4 flex-wrap ${s.comfyui_auto_config !== false ? "opacity-50 pointer-events-none" : ""}`}>
          <Gauge className="w-3.5 h-3.5 text-muted-foreground shrink-0" />
          <span className="text-[10px] uppercase tracking-widest text-muted-foreground shrink-0">Quality preset</span>
          {IMAGE_QUALITY_PRESETS.map((p) => {
            const active = Number(s.comfyui_steps) === p.comfyui_steps && Number(s.comfyui_cfg) === p.comfyui_cfg;
            return (
              <Button
                key={p.id} size="sm" variant={active ? "default" : "outline"} className="h-7 text-xs"
                title={p.hint}
                onClick={() => { updateS({ comfyui_steps: p.comfyui_steps, comfyui_cfg: p.comfyui_cfg }); toast.success(`ComfyUI set to ${p.label} (${p.comfyui_steps} steps, CFG ${p.comfyui_cfg}).`); }}
              >
                {p.label}
              </Button>
            );
          })}
        </div>
        <div className="grid md:grid-cols-2 gap-4">
          <Field k="comfyui_api_url" label="ComfyUI server URL" placeholder="https://xxxx.trycloudflare.com or http://localhost:8188" testid="settings-comfy-url" value={s.comfyui_api_url} onValueChange={updateS} />
          <Field k="comfyui_api_key" label="API key (optional)" placeholder="only if fronted by an auth proxy" type="password" testid="settings-comfy-key" value={s.comfyui_api_key} onValueChange={updateS} />
          <div className="space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Style preset</Label>
            <Select value={s.comfyui_style || "photoreal"} onValueChange={(val) => updateS({ comfyui_style: val })}>
              <SelectTrigger className="w-full" data-testid="settings-comfy-style"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="photoreal">Photoreal</SelectItem>
                <SelectItem value="comic">Comic</SelectItem>
                <SelectItem value="graphic_novel">Graphic novel</SelectItem>
                <SelectItem value="anime">Anime</SelectItem>
                <SelectItem value="oil_painting">Oil painting</SelectItem>
                <SelectItem value="watercolor">Watercolor</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <Field k="comfyui_ckpt" label="SDXL checkpoint file" placeholder="sd_xl_base_1.0.safetensors" testid="settings-comfy-ckpt" value={s.comfyui_ckpt} onValueChange={updateS} />
          <Field k="comfyui_steps" label="Steps" type="number" testid="settings-comfy-steps" value={s.comfyui_steps} onValueChange={updateS} />
          <Field k="comfyui_cfg" label="CFG scale" type="number" testid="settings-comfy-cfg" value={s.comfyui_cfg} onValueChange={updateS} />
          <Field k="comfyui_width" label="Width" type="number" testid="settings-comfy-width" value={s.comfyui_width} onValueChange={updateS} />
          <Field k="comfyui_height" label="Height" type="number" testid="settings-comfy-height" value={s.comfyui_height} onValueChange={updateS} />
          <Field k="comfyui_ip_weight" label="Character reference strength (0–1)" type="number" testid="settings-comfy-ipweight" value={s.comfyui_ip_weight} onValueChange={updateS} />
          <Field k="comfyui_negative" label="Extra negative prompt" placeholder="appended to the style's negative" testid="settings-comfy-negative" value={s.comfyui_negative} onValueChange={updateS} />
        </div>
        <div className="mt-4 grid md:grid-cols-2 gap-4">
          <div className="space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Workflow mode</Label>
            <Select value={s.comfyui_workflow_mode || "auto"} onValueChange={(val) => updateS({ comfyui_workflow_mode: val })}>
              <SelectTrigger className="w-full" data-testid="settings-comfy-wfmode"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="auto">Auto (bundled photoreal / character graphs)</SelectItem>
                <SelectItem value="custom">Custom workflow (paste your own — e.g. AnimateDiff)</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
        {s.comfyui_workflow_mode === "custom" && (
          <div className="mt-3 space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Custom ComfyUI workflow (API format JSON)</Label>
            <textarea
              data-testid="settings-comfy-custom-wf"
              className="w-full h-40 rounded-md border border-input bg-background px-3 py-2 text-xs font-mono"
              value={s.comfyui_custom_workflow || ""}
              onChange={(e) => updateS({ comfyui_custom_workflow: e.target.value })}
              placeholder='Paste a ComfyUI "Save (API Format)" workflow. Use tokens the app fills: __PROMPT__, __NEGATIVE__, __SEED__, __WIDTH__, __HEIGHT__, __STEPS__, __CFG__, __CKPT__, __REF_IMAGE__'
            />
            <p className="text-xs text-muted-foreground">For animation: build an AnimateDiff graph in the ComfyUI web UI (same server URL above), <b>Save (API Format)</b>, paste it here, and keep the <code>__TOKEN__</code> placeholders. The app collects outputs from SaveImage / VHS_VideoCombine nodes (images, gifs, mp4). A starter is in <code>src-tauri/comfy_workflows/animate_sdxl.json.example</code>.</p>
          </div>
        )}
        <div className="flex flex-wrap gap-2 mt-3">
          <Button size="sm" data-testid="settings-kaggle-auto-comfy" onClick={()=>autoStartServer("comfyui")} disabled={kaggleState.comfyui && ["checking","starting","waiting","testing"].includes(kaggleState.comfyui.status)}><Bot className="w-3 h-3 mr-2" />Start & connect</Button>
          <Button size="sm" variant="outline" data-testid="settings-kaggle-open-comfy" onClick={()=>openKaggleNb("comfyui")}><ExternalLink className="w-3 h-3 mr-2" />Open notebook</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-start-comfy" onClick={()=>startKaggleServer("comfyui")}><Bot className="w-3 h-3 mr-2" />Start server</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-stop-comfy" title="End the Kaggle session now so it stops using your free weekly GPU hours" onClick={()=>stopKaggleServer("comfyui")}><PowerOff className="w-3 h-3 mr-2" />Stop server</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-fetch-comfy" onClick={()=>fetchKaggleUrl("comfyui")}><DownloadCloud className="w-3 h-3 mr-2" />Fetch live URL</Button>
          <Button size="sm" variant="secondary" data-testid="settings-test-comfy" onClick={()=>testS("comfy")}><KeyRound className="w-3 h-3 mr-2" />Test connection</Button>
        </div>
        <KaggleAutoStatus engine="comfyui" />
        <div className="mt-3 text-xs text-muted-foreground">Run the ComfyUI notebook (<code>scripts/kaggle_comfyui/</code>, pushed to Kaggle as <code>biblemusically-comfyui-server</code>). Character images automatically use the character's avatar as an IP-Adapter reference for consistency. Manage style presets &amp; per-channel sticky styles in <b>Style Studio</b>; mix music genres in <b>Sound Studio</b>.</div>
      </Card>

      {/* ── Assistant voice ─────────────────────────────────────────────────── */}
      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Sparkles className="w-4 h-4 text-primary" /><h2 className="font-semibold">Assistant voice</h2></div>
        <div className="text-sm text-muted-foreground mb-3">
          The guided flows can read their questions aloud and take spoken answers. Voices come from your
          Gemini key and every line is cached, so a repeated question costs nothing.
        </div>
        <VoicePicker />
      </Card>

      {/* ── Remote rendering ─────────────────────────────────────────────────
          Video assembly and the YouTube upload are the two steps that cost real CPU and real
          bandwidth. At 50 channels they are ~750 videos a month, so they belong on somebody else's
          computer — see docs/REMOTE_RENDER.md for where the numbers come from. */}
      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Film className="w-4 h-4 text-primary" /><h2 className="font-semibold">Remote rendering (ffmpeg + upload off this machine)</h2></div>
        <div className="text-sm text-muted-foreground mb-3">
          The worker fetches this project's audio and images from its sync remote, encodes the video and uploads it
          to YouTube itself — your connection only carries the job description. The project must be synced
          (Data &amp; Sync) for any remote provider to reach the assets.
        </div>
        <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-2 mb-3">
          {(renderProviders || []).map((p) => (
            <button
              key={p.id}
              onClick={() => updateS("remote_render_provider", p.id)}
              className={`text-left rounded-lg border p-2.5 transition-all hover:border-primary/60 ${s.remote_render_provider === p.id ? "border-primary/60 bg-primary/5" : "border-border"}`}
            >
              <div className="flex items-center gap-1.5">
                <span className="text-sm font-medium">{p.label}</span>
                <span className="text-[9px] uppercase tracking-wide text-muted-foreground">{p.cost}</span>
              </div>
              <div className="text-[11px] text-muted-foreground mt-0.5 leading-snug">{p.detail}</div>
            </button>
          ))}
        </div>
        {s.remote_render_provider === "modal" && (
          <Field k="modal_endpoint" label="Modal web endpoint" placeholder="https://you--bm-render-submit.modal.run"
            testid="settings-modal-endpoint" value={s.modal_endpoint} onValueChange={updateS} />
        )}
        {s.remote_render_provider === "http" && (
          <Field k="render_worker_url" label="Worker URL" placeholder="https://render.example.com/jobs"
            testid="settings-render-worker-url" value={s.render_worker_url} onValueChange={updateS} />
        )}
        {s.remote_render_provider === "actions" && (
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" variant="secondary" data-testid="settings-install-workflow"
              onClick={async () => {
                try {
                  const r = await api.writeRenderWorkflow(activeProjectId);
                  toast.success("Workflow installed into the project repo.", { description: r?.next_step, duration: 12000 });
                } catch (err) { toast.error(String(err)); }
              }}>
              <Bot className="w-3 h-3 mr-2" />Install the workflow
            </Button>
            <span className="text-xs text-muted-foreground">Writes <code>.github/workflows/bm-render.yml</code> plus the worker, then sync to push it.</span>
          </div>
        )}
        <div className="mt-3 text-xs text-muted-foreground">
          Deploy Modal once with <code>modal deploy scripts/remote/modal_app.py</code>; run your own worker with
          <code> python3 scripts/remote/render_worker.py job.json</code>. Kaggle needs nothing beyond the token the
          music engines already use.
        </div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Img className="w-4 h-4 text-primary" /><h2 className="font-semibold">FLUX.1 [schnell] (free, open source)</h2><StatusPill k="flux" /></div>
        <div className="grid md:grid-cols-2 gap-4">
          <Field k="flux_api_url" label="FLUX server URL" placeholder="https://xxxx.trycloudflare.com or http://localhost:8002" testid="settings-flux-url" value={s.flux_api_url} onValueChange={updateS} />
          <Field k="flux_api_key" label="API key (optional)" placeholder="only if the server sets FLUX_API_KEY" type="password" testid="settings-flux-key" value={s.flux_api_key} onValueChange={updateS} />
        </div>
        <div className="flex flex-wrap gap-2 mt-3">
          <Button size="sm" data-testid="settings-kaggle-auto-flux" onClick={()=>autoStartServer("flux")} disabled={kaggleState.flux && ["checking","starting","waiting","testing"].includes(kaggleState.flux.status)}><Bot className="w-3 h-3 mr-2" />Start & connect</Button>
          <Button size="sm" variant="outline" data-testid="settings-kaggle-open-flux" onClick={()=>openKaggleNb("flux")}><ExternalLink className="w-3 h-3 mr-2" />Open notebook</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-start-flux" onClick={()=>startKaggleServer("flux")}><Bot className="w-3 h-3 mr-2" />Start server</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-stop-flux" title="End the Kaggle session now so it stops using your free weekly GPU hours" onClick={()=>stopKaggleServer("flux")}><PowerOff className="w-3 h-3 mr-2" />Stop server</Button>
          <Button size="sm" variant="secondary" data-testid="settings-kaggle-fetch-flux" onClick={()=>fetchKaggleUrl("flux")}><DownloadCloud className="w-3 h-3 mr-2" />Fetch live URL</Button>
          <Button size="sm" variant="secondary" data-testid="settings-test-flux" onClick={()=>testS("flux")}><KeyRound className="w-3 h-3 mr-2" />Test connection</Button>
        </div>
        <KaggleAutoStatus engine="flux" />
        <div className="mt-3 text-xs text-muted-foreground">Run the FLUX notebook (<code>scripts/kaggle_flux/</code>) on a free Kaggle/Colab GPU — it prints a public URL you paste above. Kaggle/Colab share URLs rotate roughly every 72 hours, so re-paste when it expires. For a permanent setup, run the server on a local GPU and use <code>http://localhost:8002</code>.</div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Img className="w-4 h-4 text-primary" /><h2 className="font-semibold">Midjourney</h2><StatusPill k="mj" /></div>
        <div className="space-y-3">
          <div className="text-sm text-muted-foreground">"Open Midjourney login" signs in inside the embedded webview (Google session first, then Midjourney — use "Continue with Google"). "Capture session" still uses the visible Playwright browser flow for the proxy profile.</div>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="secondary" onClick={openMjLogin}><Bot className="w-3 h-3 mr-2" />Open Midjourney login</Button>
            <Button size="sm" variant="secondary" onClick={captureMjSession} disabled={loginInProgress}><Bot className="w-3 h-3 mr-2" />{loginInProgress ? "Capturing..." : "Capture session"}</Button>
          </div>
          <div className="mt-4 rounded-xl border border-muted/50 bg-slate-950/10 p-4 space-y-4">
            <div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
              <div>
                <div className="text-sm font-semibold">Generate a Midjourney image now</div>
                <p className="text-xs text-muted-foreground">Submit a prompt using the captured Playwright profile and download the results locally.</p>
              </div>
              <div className="flex items-center gap-2">
                <Button size="sm" variant="secondary" onClick={generateMidjourneyImage} disabled={mjGenerating}>
                  <Img className="w-3 h-3 mr-2" />{mjGenerating ? "Generating..." : "Generate now"}
                </Button>
                <Button size="sm" variant="secondary" onClick={bulkGenerateAll} disabled={mjGenerating}>
                  <Img className="w-3 h-3 mr-2" />{mjGenerating ? "Working..." : "Generate All Images"}
                </Button>
              </div>
            </div>
            <div className="space-y-2">
              <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Midjourney prompt</Label>
              <Textarea
                rows={3}
                value={mjPrompt}
                onChange={(e) => setMjPrompt(e.target.value)}
                placeholder="A cinematic landscape with neon lights, highly detailed"
              />
            </div>
            {mjGenerateError && <div className="text-sm text-destructive">{mjGenerateError}</div>}
            {mjResults.length > 0 && (
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
                {mjResults.map((path, idx) => {
                  const fileUrl = `file://${path.replace(/ /g, "%20")}`;
                  return (
                    <Card key={`${path}-${idx}`} className="overflow-hidden border-muted/50">
                      <div className="h-40 overflow-hidden bg-muted/20">
                        <img src={fileUrl} alt={`MJ result ${idx + 1}`} className="w-full h-full object-cover" />
                      </div>
                      <div className="p-3 space-y-2">
                        <div className="text-xs text-muted-foreground">Saved locally</div>
                        <div className="text-[10px] text-mono break-words">{path}</div>
                        <a href={fileUrl} target="_blank" rel="noreferrer" className="text-primary text-xs hover:underline">Open file</a>
                      </div>
                    </Card>
                  );
                })}
              </div>
            )}
          </div>
        </div>
        <div className="mt-3 text-xs text-muted-foreground">Use the visible Midjourney browser flow to capture a Playwright profile directory. No legacy session cookie entry is required.</div>
        <Button size="sm" variant="secondary" data-testid="settings-test-mj" className="mt-3" onClick={()=>testS("mj")}><Cookie className="w-3 h-3 mr-2" />Test</Button>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Film className="w-4 h-4 text-primary" /><h2 className="font-semibold">Video composition</h2></div>
        <div className="grid md:grid-cols-2 gap-3">
          <Field k="video_width" label="Video width" type="number" testid="settings-video-width" value={s.video_width} onValueChange={updateS} />
          <Field k="video_height" label="Video height" type="number" testid="settings-video-height" value={s.video_height} onValueChange={updateS} />
          <Field k="video_overlay_url" label="Animation overlay (URL or path, optional)" placeholder="looping light-leak / particle clip" testid="settings-overlay-url" value={s.video_overlay_url} onValueChange={updateS} />
          <div className="space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Overlay blend</Label>
            <Select value={s.video_overlay_mode || "screen"} onValueChange={(val) => updateS({ video_overlay_mode: val })}>
              <SelectTrigger className="w-full" data-testid="settings-overlay-mode"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="screen">Screen (additive glow — light leaks/particles)</SelectItem>
                <SelectItem value="overlay">Overlay (alpha blend)</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <Field k="video_overlay_opacity" label="Overlay opacity (0–1)" type="number" testid="settings-overlay-opacity" value={s.video_overlay_opacity} onValueChange={updateS} />
        </div>
        <div className="mt-3 flex items-center gap-3 rounded-md border border-border bg-muted/20 px-3 py-2">
          <Switch checked={s.overlay_auto_use !== false} onCheckedChange={(v) => updateS({ overlay_auto_use: v })} data-testid="settings-overlay-auto-use" />
          <div>
            <div className="text-sm font-medium">Auto-use companion overlays</div>
            <div className="text-xs text-muted-foreground">When the overlay URL above is empty, each composed video automatically blends its song's own audio-reactive overlay (generated in <b>Overlay Studio</b>).</div>
          </div>
        </div>
        <div className="mt-4 grid md:grid-cols-3 gap-3 items-end">
          <label className="flex items-center justify-between gap-3 rounded-md border border-border bg-muted/20 px-3 py-2">
            <span className="text-sm font-medium">Scene transitions</span>
            <Switch checked={!!s.video_transitions_enabled} onCheckedChange={(v) => updateS({ video_transitions_enabled: v })} data-testid="settings-transitions-enabled" />
          </label>
          <div className="space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Default transition</Label>
            <Select value={s.video_transition || "fade"} onValueChange={(val) => updateS({ video_transition: val })}>
              <SelectTrigger className="w-full" data-testid="settings-transition"><SelectValue /></SelectTrigger>
              <SelectContent>
                {["fade","fadeblack","fadewhite","dissolve","slideleft","slideright","slideup","wipeleft","wipeup","circleopen","circleclose","radial","pixelize","zoomin","smoothleft"].map(t => <SelectItem key={t} value={t}>{t}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          <Field k="video_transition_dur" label="Transition length (s)" type="number" testid="settings-transition-dur" value={s.video_transition_dur} onValueChange={updateS} />
        </div>
        <div className="mt-3 text-xs text-muted-foreground">Each section renders as its own segment (stills held for their duration; <b>animated clips</b> looped to fit) with an optional overlay composited on top. Turn on <b>scene transitions</b> for crossfades between scenes — set them per-scene and let AI pick fitting ones in <b>Transitions</b> (nav). Sizes should match your image aspect (16:9 → 1280×720 or 1920×1080).</div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Film className="w-4 h-4 text-primary" /><h2 className="font-semibold">FFmpeg & audio analysis</h2><StatusPill k="ffmpeg" /></div>
        <div className="grid md:grid-cols-2 gap-3">
          <Field k="ffmpeg_path" label="ffmpeg binary path" placeholder="ffmpeg" testid="settings-ffmpeg-path" value={s.ffmpeg_path} onValueChange={updateS} />
          <Field k="ffprobe_path" label="ffprobe binary path" placeholder="ffprobe" testid="settings-ffprobe-path" value={s.ffprobe_path} onValueChange={updateS} />
          <Field k="qwen_endpoint" label="Qwen prompt endpoint (effect suggestions)" placeholder="http://localhost:11434/api/generate" testid="settings-qwen" value={s.qwen_endpoint} onValueChange={updateS} />
        </div>
        <div className="flex items-center gap-2 mt-3">
          <Button size="sm" variant="secondary" data-testid="settings-test-ffmpeg" onClick={()=>testS("ffmpeg")}><Film className="w-3 h-3 mr-2" />Probe</Button>
          <Button size="sm" variant="secondary" onClick={async ()=>{
            try {
              const r = await api.getNodePath();
              setNodeInfo(r);
              if (r.ok) toast.success(`Node found: ${r.path}`);
              else toast.error(r.error || 'Node not found');
            } catch (e) {
              console.error(e);
              toast.error('Node probe failed');
            }
          }}><Bot className="w-3 h-3 mr-2" />Check Node</Button>
        </div>
        {nodeInfo && (
          <div className="mt-3 text-xs text-muted-foreground">Node probe: {nodeInfo.ok ? nodeInfo.path : (nodeInfo.error || 'not found')}</div>
        )}
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Bot className="w-4 h-4 text-primary" /><h2 className="font-semibold">Job Queue</h2></div>
        <div className="grid md:grid-cols-2 gap-3">
          <div className="space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Max concurrent jobs</Label>
            <Input
              type="number"
              min={1}
              max={10}
              value={s.max_concurrent_jobs ?? 2}
              onChange={(e) => updateS({ max_concurrent_jobs: Math.max(1, parseInt(e.target.value, 10) || 1) })}
            />
          </div>
        </div>
        <div className="mt-3 text-xs text-muted-foreground">
          Caps how many Suno/Midjourney/FFmpeg/YouTube-upload jobs run at once — each Midjourney job launches a full browser, and Suno/YouTube share account-level rate limits, so unbounded concurrency risks getting your session throttled or your cookie/token flagged. Jobs beyond the limit wait, visibly, as "queued" in the Jobs Monitor. Takes effect on the next app restart.
        </div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4">
          <ShieldCheck className="w-4 h-4 text-primary" />
          <h2 className="font-semibold">Google Drive Integration (Per Project)</h2>
        </div>
        {!activeProjectId ? (
          <div className="text-sm text-muted-foreground p-4 border border-dashed rounded-lg text-center">
            Select a project from the dashboard to enable and configure Google Drive integration.
          </div>
        ) : (
          <div className="space-y-4">
            <div className="flex items-center justify-between p-3 rounded-lg bg-muted/40 border">
              <div>
                <div className="text-sm font-semibold">Google Drive Connection</div>
                <div className="text-xs text-muted-foreground">
                  {s.gdrive_connected ? `Connected as ${s.gdrive_email}` : "Not connected"}
                </div>
              </div>
              <Button
                size="sm"
                variant={s.gdrive_connected ? "secondary" : "default"}
                onClick={async () => {
                  try {
                    toast.loading("Opening Google authorization in your browser...", { id: "gdrive-auth" });
                    const res = await api.authorizeProjectGDrive(activeProjectId);
                    if (res.ok) {
                      setS(prev => ({ ...prev, gdrive_connected: true, gdrive_email: res.email }));
                      toast.success(`Successfully authorized Google Drive as ${res.email}`, { id: "gdrive-auth" });
                    }
                  } catch (err) {
                    toast.error(err || "Authorization failed", { id: "gdrive-auth" });
                  }
                }}
              >
                {s.gdrive_connected ? "Reconnect" : "Connect Google Drive"}
              </Button>
            </div>
            <div className="space-y-1">
              <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">
                Google OAuth Client ID Override
              </Label>
              <Input
                placeholder="Use default client ID (or enter client ID override)"
                value={s.google_oauth_client_id || ""}
                onChange={(e) => updateS({ google_oauth_client_id: e.target.value })}
              />
              <p className="text-[11px] text-muted-foreground">
                Leave empty to use the default configured OAuth Client ID from the pool below.
              </p>
            </div>
          </div>
        )}
      </Card>

      <Card className="p-6">
        <div className="flex items-center gap-2 mb-4"><ShieldCheck className="w-4 h-4 text-primary" /><h2 className="font-semibold">Google OAuth (YouTube Data API v3)</h2></div>
        <div className="mb-4">
          <p className="text-sm text-muted-foreground">Manage your OAuth client pool below. Channels will auto-pick a client by explicit binding or language match when connecting. The legacy single-client fields above are synced to the first client in this pool.</p>
          <OAuthClientsPanel defaultOpen={false} onChange={(cs)=>{
            setOauthClients(cs || []);
            if (cs && cs.length) setS(prev => ({ ...prev, google_client_id: cs[0].client_id || prev.google_client_id, google_redirect_uri: cs[0].redirect_uri || prev.google_redirect_uri }));
          }} />
        </div>
        <div className="mt-3 text-xs text-muted-foreground flex items-center gap-2"><KeyRound className="w-3 h-3" />Channels are connected individually under <span className="text-foreground">Channel Manager</span>. Refresh tokens are stored per channel and never expire (unless revoked).</div>
      </Card>
    </div>
  );
};

export default memo(SettingsComponent);