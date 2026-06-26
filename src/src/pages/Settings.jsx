import { useEffect, useState, useCallback, memo, useRef } from "react";
import appPkg from "../../package.json";
import { api } from "../lib/api";
import { useBackgroundSave } from "../lib/hooks";
import { Card } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Button } from "../components/ui/button";
import { Label } from "../components/ui/label";
import { Badge } from "../components/ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Cookie, KeyRound, Music2, Image as Img, Film, ShieldCheck, CheckCircle2, XCircle, Save, Bot, HelpCircle } from "lucide-react";
import { getStepForPath } from "../lib/pageSteps";
import OAuthClientsPanel from "../components/OAuthClientsPanel";
import { toast } from "sonner";
import { useAutoSave, AutoSaveChip } from "../lib/hooks";

const FREE_MODELS = [
  { id: "qwen/qwen3-next-80b-a3b-instruct:free", name: "Qwen 3 Next 80B (Free)" },
  { id: "google/gemma-4-31b-it:free", name: "Gemma 4 31B IT (Free)" },
  { id: "meta-llama/llama-3.3-70b-instruct:free", name: "Llama 3.3 70B (Free)" },
  { id: "nousresearch/hermes-3-llama-3.1-405b:free", name: "Hermes 3 405B (Free)" },
  { id: "openai/gpt-oss-120b:free", name: "GPT-OSS 120B (Free)" },
  { id: "cognitivecomputations/dolphin-mistral-24b-venice-edition:free", name: "Dolphin Mistral 24B (Free)" },
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
  suno_cookie: "",
  mj_profile_dir: "",
  mj_discord_token: "",
  google_client_id: "",
  google_client_secret: "",
  google_redirect_uri: "",
  ffmpeg_path: "ffmpeg",
  ffprobe_path: "ffprobe",
  theme: "obsidian",
  qwen_endpoint: "",
  openrouter_api_key: "",
  openrouter_email: "",
  openrouter_model: "qwen/qwen3-next-80b-a3b-instruct:free",
  mj_proxy_url: "",
};

const SettingsComponent = () => {
  const [s, setS] = useState(initialSettings);
  const appVersion = appPkg.version || "0.6.1";
  
  const updateS = useCallback((updates) => {
    setS(prev => ({ ...prev, ...updates }));
  }, []);
  const [status, setStatus] = useState({});
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
  const [mjPrompt, setMjPrompt] = useState("");
  const [mjGenerateError, setMjGenerateError] = useState(null);
  const [mjResults, setMjResults] = useState([]);

  useEffect(() => {
    (async () => {
      try {
        const settings = await api.getSettings();
        setS(prev => ({ ...prev, ...initialSettings, ...settings }));

        let clients = await api.listOauthClients();
        setOauthClients(clients || []);

        // If no oauth clients exist but legacy settings contain credentials, migrate into a new oauth client
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
          // Mirror first client into settings (no secret auto-populated)
          setS(prev => ({ ...prev, google_client_id: clients[0].client_id || prev.google_client_id, google_redirect_uri: clients[0].redirect_uri || prev.google_redirect_uri }));
        }
      } catch (error) {
        console.error("Failed to load settings", error);
        toast.error("Unable to load settings. Please restart the app.");
      } finally {
        setLoaded(true);
      }
    })();
  }, []);

  // Auto-save settings (debounced) so casual edits persist without clicking Save
  const { status: asStatus, lastSaved } = useAutoSave("settings-mirror", s, { delay: 900, enabled: loaded });

  const save = async () => {
    try {
      await api.saveSettings(s);
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
    const r = kind === "suno" ? await api.testSuno() : kind === "mj" ? await api.testMj() : await api.testFfmpeg();
    setStatus(p => ({ ...p, [kind]: r }));
    r.ok ? toast.success(r.detail || "ok") : toast.error(r.detail || "fail");
  };

  const openSunoLogin = async () => {
    try {
      const r = await api.openSunoLogin();
      setStatus(p => ({ ...p, sunoLogin: r }));
      toast.success("Suno login page opened. Complete login in the browser and then test the cookie.");
    } catch (err) {
      console.error(err);
      toast.error("Unable to open Suno login page.");
    }
  };

  const openMjLogin = async () => {
    try {
      const r = await api.openMjLogin();
      setStatus(p => ({ ...p, mjLoginPage: r }));
      toast.success("Midjourney login page opened in a visible browser.");
    } catch (err) {
      console.error(err);
      toast.error("Unable to open Midjourney login page.");
    }
  };

  const captureMjSession = async () => {
    setLoginInProgress(true);
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
      const r = await api.bulkGenerateAll();
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

  return (
    <div className="p-8 max-w-5xl mx-auto fade-in">
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
        <div className="flex items-center gap-2 mb-4"><Music2 className="w-4 h-4 text-primary" /><h2 className="font-semibold">Suno (unofficial)</h2><StatusPill k="suno" /></div>
        <Field k="suno_cookie" label="Suno session cookie (studio-api_key / __session / session_id)" placeholder="cookie string..." testid="settings-suno-cookie" value={s.suno_cookie} onValueChange={updateS} />
        <div className="flex flex-wrap gap-2 mt-3">
          <Button size="sm" variant="secondary" onClick={openSunoLogin}><Cookie className="w-3 h-3 mr-2" />Open Suno login</Button>
          <Button size="sm" variant="secondary" data-testid="settings-test-suno" onClick={()=>testS("suno")}><Cookie className="w-3 h-3 mr-2" />Test</Button>
          <Button size="sm" variant="secondary" onClick={async ()=>{
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
        <div className="flex items-center gap-2 mb-4"><Bot className="w-4 h-4 text-primary" /><h2 className="font-semibold">AI Composer (OpenRouter Free Tier Models)</h2></div>
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
        <div className="flex items-center gap-2 mb-4"><Img className="w-4 h-4 text-primary" /><h2 className="font-semibold">Midjourney</h2><StatusPill k="mj" /></div>
        <div className="space-y-3">
          <div className="text-sm text-muted-foreground">Use the visible Midjourney browser flow to capture a Playwright profile. The app no longer requires a manual session cookie entry.</div>
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