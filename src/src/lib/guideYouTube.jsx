import { useState, useEffect, useCallback } from "react";
import { api } from "./api";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import { toast } from "sonner";
import { CheckCircle2, ExternalLink, Loader2, Youtube, Plus, ShieldCheck } from "lucide-react";

// Guided YouTube publishing setup, reusable both in the first-run wizard and on demand (e.g. the
// upload step checks this prerequisite and re-opens just this panel when it isn't satisfied).
//
// Two parts, in order:
//   1. Register a Google OAuth client (the "pool" of credentials the app authorizes against).
//      This is a one-time Google Cloud step; the app can't create it for you.
//   2. Authorize a Google account through Google's OWN consent screen and import its YouTube
//      channels. Because this is real OAuth, accounts with 2FA work normally and the app never
//      sees the password — only a refresh token.

const DEFAULT_REDIRECT = "http://127.0.0.1:8765/";

export default function YouTubeStepBody({ onDone }) {
  const [clients, setClients] = useState([]);
  const [selected, setSelected] = useState("");
  const [creating, setCreating] = useState(false);
  const [busy, setBusy] = useState("");
  const [form, setForm] = useState({ label: "YouTube uploads", client_id: "", client_secret: "", redirect_uri: DEFAULT_REDIRECT });
  const [channels, setChannels] = useState([]);

  const loadClients = useCallback(async () => {
    try {
      const r = await api.listOauthClients();
      const list = Array.isArray(r) ? r : r?.clients || [];
      setClients(list);
      if (list.length && !selected) setSelected(list[0].id || list[0]._id || "");
      setCreating(list.length === 0);
    } catch (e) { console.warn(e); setCreating(true); }
  }, [selected]);

  useEffect(() => { loadClients(); }, [loadClients]);

  const saveClient = async () => {
    if (!form.client_id.trim() || !form.client_secret.trim()) {
      return toast.error("Client ID and Client secret are both required.");
    }
    setBusy("save");
    try {
      const r = await api.createOauthClient({ ...form, languages: [], notes: "" });
      toast.success("OAuth client saved.");
      setCreating(false);
      await loadClients();
      const newId = r?.id || r?._id;
      if (newId) setSelected(newId);
    } catch (e) { toast.error(String(e)); }
    finally { setBusy(""); }
  };

  const connect = async () => {
    if (!selected) return toast.error("Pick or create an OAuth client first.");
    setBusy("connect");
    try {
      // Opens Google's real consent screen; 2FA is handled by Google itself.
      const r = await api.importFromGoogleAccount(selected);
      const found = r?.channels || r?.imported || [];
      setChannels(Array.isArray(found) ? found : []);
      await api.saveSettings({ youtube_connected: true });
      toast.success(`Connected — ${Array.isArray(found) ? found.length : 0} channel(s) imported.`);
    } catch (e) { toast.error(String(e)); }
    finally { setBusy(""); }
  };

  return (
    <div className="space-y-5">
      <p className="text-muted-foreground text-sm">
        To upload finished videos the app signs in with Google. It uses a proper OAuth consent
        screen — <b className="text-foreground">your password never reaches this app</b>, and accounts with
        2-factor auth work normally because you authenticate on Google's own page.
      </p>

      {/* ── 1. OAuth client ─────────────────────────────── */}
      <div className="rounded-lg border border-border p-3 space-y-3">
        <div className="flex items-center gap-2 text-sm font-medium">
          <ShieldCheck className="w-4 h-4 text-primary" /> 1. Google OAuth client
        </div>

        {clients.length > 0 && !creating && (
          <div className="space-y-2">
            <Label className="text-xs">Use an existing client</Label>
            <div className="flex gap-2 flex-wrap">
              <select value={selected} onChange={(e) => setSelected(e.target.value)}
                className="flex-1 min-w-[12rem] bg-background border border-border rounded px-2 py-1.5 text-sm">
                {clients.map((c) => (
                  <option key={c.id || c._id} value={c.id || c._id}>{c.label || c.client_id}</option>
                ))}
              </select>
              <Button size="sm" variant="outline" onClick={() => setCreating(true)}>
                <Plus className="w-3 h-3 mr-1.5" />Add another
              </Button>
            </div>
          </div>
        )}

        {creating && (
          <div className="space-y-2.5">
            <div className="text-xs text-muted-foreground space-y-1">
              <div>Create one once in Google Cloud Console — it takes a couple of minutes:</div>
              <ol className="list-decimal ml-4 space-y-0.5">
                <li>APIs &amp; Services → <b className="text-foreground">Enable</b> the "YouTube Data API v3".</li>
                <li>Credentials → Create credentials → <b className="text-foreground">OAuth client ID</b> → type <b className="text-foreground">Desktop app</b> (or Web app with the redirect below).</li>
                <li>Copy the Client ID and Client secret into the fields here.</li>
              </ol>
              <a href="https://console.cloud.google.com/apis/credentials" target="_blank" rel="noreferrer"
                className="text-primary inline-flex items-center gap-1 pt-0.5">
                Open Google Cloud Credentials <ExternalLink className="w-3 h-3" />
              </a>
            </div>
            <div className="grid sm:grid-cols-2 gap-2">
              <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Label</Label>
                <Input value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} /></div>
              <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Redirect URI</Label>
                <Input value={form.redirect_uri} onChange={(e) => setForm({ ...form, redirect_uri: e.target.value })} className="font-mono text-xs" /></div>
              <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Client ID</Label>
                <Input value={form.client_id} onChange={(e) => setForm({ ...form, client_id: e.target.value })} placeholder="…apps.googleusercontent.com" className="font-mono text-xs" /></div>
              <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Client secret</Label>
                <Input type="password" value={form.client_secret} onChange={(e) => setForm({ ...form, client_secret: e.target.value })} className="font-mono text-xs" /></div>
            </div>
            <div className="text-[11px] text-muted-foreground">
              The redirect URI must be a loopback address and must match what you registered in Google Cloud.
            </div>
            <div className="flex gap-2">
              <Button size="sm" onClick={saveClient} disabled={busy === "save"}>
                {busy === "save" ? <Loader2 className="w-3 h-3 animate-spin" /> : "Save client"}
              </Button>
              {clients.length > 0 && <Button size="sm" variant="ghost" onClick={() => setCreating(false)}>Cancel</Button>}
            </div>
          </div>
        )}
      </div>

      {/* ── 2. Authorize + import channels ──────────────── */}
      <div className="rounded-lg border border-border p-3 space-y-3">
        <div className="flex items-center gap-2 text-sm font-medium">
          <Youtube className="w-4 h-4 text-primary" /> 2. Authorize &amp; import channels
        </div>
        <p className="text-xs text-muted-foreground">
          Opens Google's sign-in in your browser, then pulls in the YouTube channels on that account
          and stores a refreshable upload token for each.
        </p>
        <Button size="sm" onClick={connect} disabled={!selected || busy === "connect"} data-testid="guide-yt-connect">
          {busy === "connect" ? <><Loader2 className="w-3 h-3 mr-1.5 animate-spin" />Waiting for Google…</> : "Connect Google account"}
        </Button>
        {channels.length > 0 && (
          <div className="space-y-1 pt-1">
            {channels.map((c, i) => (
              <div key={c.id || i} className="text-xs flex items-center gap-1.5 text-emerald-500">
                <CheckCircle2 className="w-3.5 h-3.5" />{c.title || c.name || c.id || `channel ${i + 1}`}
              </div>
            ))}
          </div>
        )}
      </div>

      <Button onClick={() => onDone?.()}>Done</Button>
    </div>
  );
}
