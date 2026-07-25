import { useState, useEffect, useCallback } from "react";
import { api } from "./api";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import { toast } from "sonner";
import { CheckCircle2, ExternalLink, Loader2, Youtube, Plus, ShieldCheck, Copy, ShieldAlert } from "lucide-react";
import { DEFAULT_OAUTH_REDIRECT, isLoopbackRedirect } from "./oauthRedirect";

// Guided YouTube publishing setup, reusable both in the first-run wizard and on demand (e.g. the
// upload step checks this prerequisite and re-opens just this panel when it isn't satisfied).
//
// Two parts, in order:
//   1. Register a Google OAuth client (the "pool" of credentials the app authorizes against).
//      This is a one-time Google Cloud step; the app can't create it for you.
//   2. Authorize a Google account through Google's OWN consent screen and import its YouTube
//      channels. Because this is real OAuth, accounts with 2FA work normally and the app never
//      sees the password — only a refresh token.

export default function YouTubeStepBody({ onDone }) {
  const [clients, setClients] = useState([]);
  const [selected, setSelected] = useState("");
  const [creating, setCreating] = useState(false);
  const [busy, setBusy] = useState("");
  const [form, setForm] = useState({ label: "YouTube uploads", client_id: "", client_secret: "", redirect_uri: DEFAULT_OAUTH_REDIRECT });
  const [channels, setChannels] = useState([]);
  // Google's own verdict on the saved client, so a misregistered redirect URI is caught here rather
  // than on the "Access blocked" page after the browser has already opened.
  const [check, setCheck] = useState(null);

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

  /** Ask the backend (which asks Google) whether this client will actually work. */
  const verify = useCallback(async (id) => {
    if (!id) return null;
    setBusy("verify");
    try {
      const r = await api.validateOauthClient(id);
      setCheck(r);
      if (r?.google_ok === false) toast.error("Google rejected this OAuth client — see the details below.");
      else if (r?.ok) toast.success("Google accepted this client.");
      return r;
    } catch (e) { toast.error(String(e)); return null; }
    finally { setBusy(""); }
  }, []);

  const saveClient = async () => {
    if (!form.client_id.trim() || !form.client_secret.trim()) {
      return toast.error("Client ID and Client secret are both required.");
    }
    if (!isLoopbackRedirect(form.redirect_uri)) {
      return toast.error(`The redirect URI must be a loopback address, e.g. ${DEFAULT_OAUTH_REDIRECT}`);
    }
    setBusy("save");
    try {
      const r = await api.createOauthClient({ ...form, languages: [], notes: "" });
      toast.success("OAuth client saved.");
      setCreating(false);
      await loadClients();
      const newId = r?.id || r?._id;
      if (newId) { setSelected(newId); verify(newId); }
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
                <li>Credentials → Create credentials → <b className="text-foreground">OAuth client ID</b> → type <b className="text-foreground">Desktop app</b>. Desktop credentials accept any 127.0.0.1 port, so there is nothing to register.</li>
                <li>Copy the Client ID and Client secret into the fields here.</li>
              </ol>
              <div className="text-[11px] leading-relaxed pt-1">
                Picking <b className="text-foreground">Web application</b> instead? Then the redirect URI below has to be
                registered under "Authorised redirect URIs" <b className="text-foreground">character for character</b> —
                a missing or extra trailing slash is a different URI to Google, and the sign-in fails with
                <span className="font-mono"> redirect_uri_mismatch</span>.
              </div>
              <a href="https://console.cloud.google.com/apis/credentials" target="_blank" rel="noreferrer"
                className="text-primary inline-flex items-center gap-1 pt-0.5">
                Open Google Cloud Credentials <ExternalLink className="w-3 h-3" />
              </a>
            </div>
            <div className="grid sm:grid-cols-2 gap-2">
              <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Label</Label>
                <Input value={form.label} onChange={(e) => setForm({ ...form, label: e.target.value })} /></div>
              <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Redirect URI</Label>
                <div className="flex gap-1.5">
                  <Input value={form.redirect_uri} onChange={(e) => setForm({ ...form, redirect_uri: e.target.value })} className="font-mono text-xs" />
                  <Button size="sm" variant="outline" type="button" title="Copy the exact redirect URI"
                    onClick={() => { navigator.clipboard?.writeText(form.redirect_uri); toast.success("Redirect URI copied — paste it into Google Cloud Console unchanged."); }}>
                    <Copy className="w-3 h-3" />
                  </Button>
                </div></div>
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
        <div className="flex gap-2 flex-wrap">
          <Button size="sm" onClick={connect} disabled={!selected || busy === "connect"} data-testid="guide-yt-connect">
            {busy === "connect" ? <><Loader2 className="w-3 h-3 mr-1.5 animate-spin" />Waiting for Google…</> : "Connect Google account"}
          </Button>
          <Button size="sm" variant="outline" onClick={() => verify(selected)} disabled={!selected || busy === "verify"}>
            {busy === "verify" ? <Loader2 className="w-3 h-3 animate-spin" /> : "Check with Google first"}
          </Button>
        </div>
        {check?.google_ok === false && (
          <div className="flex items-start gap-2 rounded-md border border-amber-500/30 bg-amber-500/5 p-2.5 text-[11px] leading-relaxed">
            <ShieldAlert className="w-3.5 h-3.5 text-amber-400 shrink-0 mt-0.5" />
            <div className="whitespace-pre-wrap" data-no-i18n>{check.google_error}</div>
          </div>
        )}
        {check?.google_ok === true && (
          <div className="flex items-center gap-1.5 text-[11px] text-emerald-500">
            <ShieldCheck className="w-3.5 h-3.5" />Google accepted this client and redirect URI.
          </div>
        )}
        {(check?.missing?.length > 0) && (
          <div className="text-[11px] text-amber-400">Incomplete client — missing: {check.missing.join(", ")}.</div>
        )}
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
