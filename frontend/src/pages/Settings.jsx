import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Button } from "../components/ui/button";
import { Label } from "../components/ui/label";
import { Badge } from "../components/ui/badge";
import { Cookie, KeyRound, Music2, Image as Img, Film, ShieldCheck, CheckCircle2, XCircle, Save, Bot } from "lucide-react";
import { toast } from "sonner";

export default function Settings() {
  const [s, setS] = useState({ suno_cookie:"", mj_cookie:"", mj_discord_token:"", google_client_id:"", google_client_secret:"", google_redirect_uri:"", ffmpeg_path:"ffmpeg", ffprobe_path:"ffprobe", qwen_endpoint:"" });
  const [status, setStatus] = useState({});

  useEffect(() => { api.getSettings().then(r => setS(prev => ({ ...prev, ...r }))); }, []);

  const save = async () => { await api.saveSettings(s); toast.success("Settings saved"); };
  const testS = async (kind) => {
    const r = kind === "suno" ? await api.testSuno() : kind === "mj" ? await api.testMj() : await api.testFfmpeg();
    setStatus(p => ({ ...p, [kind]: r }));
    r.ok ? toast.success(r.detail || "ok") : toast.error(r.detail || "fail");
  };

  const Field = ({ k, label, placeholder, type="text", testid }) => (
    <div className="space-y-1">
      <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">{label}</Label>
      <Input data-testid={testid} type={type} value={s[k] || ""} onChange={e=>setS({...s, [k]: e.target.value})} placeholder={placeholder} />
    </div>
  );

  const StatusPill = ({ k }) => status[k] ? (
    <Badge variant={status[k].ok?"default":"destructive"} className="ml-2">{status[k].ok?<CheckCircle2 className="w-3 h-3 mr-1" />:<XCircle className="w-3 h-3 mr-1" />}{status[k].ok?"connected":"fail"}</Badge>
  ) : null;

  return (
    <div className="p-8 max-w-5xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step 0</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Settings &amp; Connections</h1>
        <Button data-testid="settings-save-btn" onClick={save}><Save className="w-4 h-4 mr-2" />Save</Button>
      </div>
      <p className="text-muted-foreground mb-8 max-w-2xl">All cookies, tokens and binary paths the engine uses live here. Nothing leaves your machine until you trigger an action.</p>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Music2 className="w-4 h-4 text-primary" /><h2 className="font-semibold">Suno (unofficial)</h2><StatusPill k="suno" /></div>
        <Field k="suno_cookie" label="studio-api.suno.com session cookie" placeholder="cookie string..." testid="settings-suno-cookie" />
        <Button size="sm" variant="secondary" data-testid="settings-test-suno" className="mt-3" onClick={()=>testS("suno")}><Cookie className="w-3 h-3 mr-2" />Test</Button>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Bot className="w-4 h-4 text-primary" /><h2 className="font-semibold">AI Composer (OpenRouter / Qwen)</h2></div>
        <div className="grid md:grid-cols-2 gap-3">
          <Field k="openrouter_api_key" label="OpenRouter API key" placeholder="sk-or-..." type="password" testid="settings-openrouter-key" />
          <Field k="openrouter_model" label="Model" placeholder="qwen/qwen-2.5-72b-instruct:free" testid="settings-openrouter-model" />
        </div>
        <div className="mt-3 text-xs text-muted-foreground">Get a free key at <span className="text-foreground">openrouter.ai/keys</span> — the free Qwen model has generous limits.</div>
      </Card>

      <Card className="p-6 mb-5">
        <div className="flex items-center gap-2 mb-4"><Img className="w-4 h-4 text-primary" /><h2 className="font-semibold">Midjourney</h2><StatusPill k="mj" /></div>
        <div className="space-y-3">
          <Field k="mj_cookie" label="midjourney.com session cookie" placeholder="cookie string..." testid="settings-mj-cookie" />
          <Field k="mj_discord_token" label="Discord wrapper token (optional fallback)" placeholder="bot/user token" testid="settings-mj-discord" />
          <Field k="mj_proxy_url" label="MJ proxy URL (e.g. self-hosted midjourney-proxy)" placeholder="https://your-mj-proxy/api" testid="settings-mj-proxy" />
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
        <div className="grid md:grid-cols-2 gap-3">
          <Field k="google_client_id" label="Client ID" placeholder="xxxxx.apps.googleusercontent.com" testid="settings-google-cid" />
          <Field k="google_client_secret" label="Client Secret" placeholder="GOCSPX-..." type="password" testid="settings-google-secret" />
          <Field k="google_redirect_uri" label="Redirect URI" placeholder="http://localhost/oauth/callback" testid="settings-google-redirect" />
        </div>
        <div className="mt-3 text-xs text-muted-foreground flex items-center gap-2"><KeyRound className="w-3 h-3" />Channels are connected individually under <span className="text-foreground">Channel Manager</span>. Refresh tokens are stored per channel and never expire (unless revoked).</div>
      </Card>
    </div>
  );
}
