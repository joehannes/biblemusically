import { useEffect, useState, useCallback, memo } from "react";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Input } from "../components/ui/input";
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
  { id: "qwen/qwen-2.5-72b-instruct:free", name: "Qwen 2.5 72B Instruct (Free)" },
  { id: "deepseek/deepseek-chat:free", name: "DeepSeek V3 / R1 (Free)" },
  { id: "meta-llama/llama-3-8b-instruct:free", name: "Llama 3 8B Instruct (Free)" },
  { id: "google/gemma-2-9b-it:free", name: "Gemma 2 9B IT (Free)" },
  { id: "mistralai/mistral-7b-instruct:free", name: "Mistral 7B Instruct (Free)" }
];


const SettingsComponent = () => {
  const [s, setS] = useState({ suno_cookie:"", mj_cookie:"", mj_discord_token:"", mj_proxy_url:"", google_client_id:"", google_client_secret:"", google_redirect_uri:"", ffmpeg_path:"ffmpeg", ffprobe_path:"ffprobe", qwen_endpoint:"", openrouter_api_key:"", openrouter_model:"qwen/qwen-2.5-72b-instruct:free" });
  
  const updateS = useCallback((updates) => {
    setS(prev => ({ ...prev, ...updates }));
  }, []);
  const [status, setStatus] = useState({});
  const [loaded, setLoaded] = useState(false);
  const [oauthClients, setOauthClients] = useState([]);
  const [mjLogin, setMjLogin] = useState({ account: "", password: "", twofa: "" });
  const updateMjLogin = useCallback((updates) => {
    setMjLogin(prev => ({ ...prev, ...updates }));
  }, []);
  const [mjLoginStatus, setMjLoginStatus] = useState({});
  const [loginInProgress, setLoginInProgress] = useState(false);

  useEffect(() => {
    (async () => {
      const settings = await api.getSettings();
      setS(prev => ({ ...prev, ...settings }));

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

      setLoaded(true);
    })();
  }, []);

  // Auto-save settings (debounced) so casual edits persist without clicking Save
  const { status: asStatus, lastSaved } = useAutoSave("settings-mirror", s, { delay: 900, enabled: loaded });

  const save = async () => {
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
      if (r.ok && r.cookie) {
        setS(prev => ({ ...prev, mj_cookie: r.cookie }));
        toast.success("Midjourney session cookie captured and stored.");
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

  const Field = memo(({ k, label, placeholder, type="text", testid }) => {
    const handleChange = useCallback((e) => {
      updateS({ [k]: e.target.value });
    }, [k, updateS]);
    
    return (
      <div className="space-y-1">
        <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">{label}</Label>
        <Input data-testid={testid} type={type} value={s[k] || ""} onChange={handleChange} placeholder={placeholder} />
      </div>
    );
  });

  Field.displayName = 'Field';

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

  const StatusPill = ({ k }) => status[k] ? (
    <Badge variant={status[k].ok?"default":"destructive"} className="ml-2">{status[k].ok?<CheckCircle2 className="w-3 h-3 mr-1" />:<XCircle className="w-3 h-3 mr-1" />}{status[k].ok?"connected":"fail"}</Badge>
  ) : null;

  return (
    <div className="p-8 max-w-5xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step {getStepForPath("/settings")}</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Settings &amp; Connections</h1>
        <div className="flex items-center gap-3">
          <AutoSaveChip status={asStatus} lastSaved={lastSaved} />
          <Button data-testid="settings-save-btn" onClick={save}><Save className="w-4 h-4 mr-2" />Save now</Button>
        </div>
      </div>
      <p className="text-muted-foreground mb-8 max-w-2xl">All cookies, tokens and binary paths the engine uses live here. Nothing leaves your machine until you trigger an action.</p>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Music2 className="w-4 h-4 text-primary" /><h2 className="font-semibold">Suno (unofficial)</h2><StatusPill k="suno" /></div>
        <Field k="suno_cookie" label="studio-api.suno.com session cookie" placeholder="cookie string..." testid="settings-suno-cookie" />
        <div className="flex flex-wrap gap-2 mt-3">
          <Button size="sm" variant="secondary" onClick={openSunoLogin}><Cookie className="w-3 h-3 mr-2" />Open Suno login</Button>
          <Button size="sm" variant="secondary" data-testid="settings-test-suno" onClick={()=>testS("suno")}><Cookie className="w-3 h-3 mr-2" />Test</Button>
        </div>
        <div className="mt-3 text-xs text-muted-foreground">After you log in to Suno, use the browser capture flow to persist the <code>studio-api_key</code> cookie to Settings.</div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Bot className="w-4 h-4 text-primary" /><h2 className="font-semibold">AI Composer (OpenRouter Free Tier Models)</h2></div>
        <div className="grid md:grid-cols-2 gap-4">
          <Field k="openrouter_api_key" label="OpenRouter API key" placeholder="sk-or-..." type="password" testid="settings-openrouter-key" />
          
          <div className="space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Select AI Model</Label>
            <Select 
              value={FREE_MODELS.some(m => m.id === s.openrouter_model) ? s.openrouter_model : (s.openrouter_model ? "custom" : "qwen/qwen-2.5-72b-instruct:free")} 
              onValueChange={(val) => {
                if (val === "custom") {
                  setS({ ...s, openrouter_model: "" });
                } else {
                  setS({ ...s, openrouter_model: val });
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
              onChange={e => setS({ ...s, openrouter_model: e.target.value })} 
              placeholder="e.g. cognitivecomputations/dolphin-mixtral-8x7b" 
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
          <Field k="mj_cookie" label="midjourney.com session cookie" placeholder="cookie string..." testid="settings-mj-cookie" />
          <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="secondary" onClick={openMjLogin}><Bot className="w-3 h-3 mr-2" />Open Midjourney login</Button>
            <Button size="sm" variant="secondary" onClick={captureMjSession} disabled={loginInProgress}><Bot className="w-3 h-3 mr-2" />{loginInProgress ? "Capturing..." : "Capture session"}</Button>
          </div>
          <Field k="mj_discord_token" label="Discord wrapper token (optional fallback)" placeholder="bot/user token" testid="settings-mj-discord" />
          <Field k="mj_proxy_url" label="MJ proxy URL (e.g. self-hosted midjourney-proxy)" placeholder="https://your-mj-proxy/api" testid="settings-mj-proxy" />
        </div>
        <div className="mt-3 text-xs text-muted-foreground">Use the visible Midjourney browser flow to capture your site session cookie automatically. The legacy proxy/Discord token path remains as a fallback.</div>
        <div className="mt-4 rounded-xl border border-muted/50 bg-slate-950/10 p-4">
          <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
            <div className="space-y-1">
              <div className="text-sm font-semibold">Automated Discord login</div>
              <p className="text-xs text-muted-foreground">Use the configured Midjourney proxy to log in with Discord credentials and persist the wrapper token automatically.</p>
            </div>
            <div className="flex items-center gap-2">
              {mjLoginStatus.ok === true && <Badge variant="default">Success</Badge>}
              {mjLoginStatus.ok === false && <Badge variant="destructive">Failed</Badge>}
              <Button size="sm" variant="secondary" onClick={autoLogin} disabled={loginInProgress || !s.mj_proxy_url}>
                <Bot className="w-3 h-3 mr-2" />{loginInProgress ? "Working..." : "Auto-login"}
              </Button>
            </div>
          </div>
          <div className="grid md:grid-cols-3 gap-3 mt-4">
            <AuthField label="Discord email" placeholder="email@example.com" value={mjLogin.account} onChange={(value) => updateMjLogin({ account: value })} />
            <AuthField label="Discord password" placeholder="Discord password" type="password" value={mjLogin.password} onChange={(value) => updateMjLogin({ password: value })} />
            <AuthField label="Discord 2FA code" placeholder="6-digit code" value={mjLogin.twofa} onChange={(value) => updateMjLogin({ twofa: value })} />
          </div>
          {mjLoginStatus.detail && (
            <div className="mt-3 text-sm text-muted-foreground">{typeof mjLoginStatus.detail === 'string' ? mjLoginStatus.detail : JSON.stringify(mjLoginStatus.detail)}</div>
          )}
        </div>
        <Button size="sm" variant="secondary" data-testid="settings-test-mj" className="mt-3" onClick={()=>testS("mj")}><Cookie className="w-3 h-3 mr-2" />Test</Button>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Film className="w-4 h-4 text-primary" /><h2 className="font-semibold">FFmpeg &amp; audio analysis</h2><StatusPill k="ffmpeg" /></div>
        <div className="grid md:grid-cols-2 gap-3">
          <Field k="ffmpeg_path" label="ffmpeg binary path" placeholder="ffmpeg" testid="settings-ffmpeg-path" />
          <Field k="ffprobe_path" label="ffprobe binary path" placeholder="ffprobe" testid="settings-ffprobe-path" />
          <Field k="qwen_endpoint" label="Qwen prompt endpoint (effect suggestions)" placeholder="http://localhost:11434/api/generate" testid="settings-qwen" />
        </div>
        <Button size="sm" variant="secondary" data-testid="settings-test-ffmpeg" className="mt-3" onClick={()=>testS("ffmpeg")}><Film className="w-3 h-3 mr-2" />Probe</Button>
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
