import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Badge } from "../components/ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "../components/ui/dialog";
import { Plus, Trash2, KeyRound, ChevronDown, ChevronUp, Edit3, Languages, ShieldCheck, ShieldAlert, RefreshCw, Info } from "lucide-react";
import { toast } from "sonner";

/** Manage multiple Google OAuth clients to overcome per-client quota.
 * Each client binds to a list of languages; channel→client resolution prefers explicit binding then language match. */
export default function OAuthClientsPanel({ defaultOpen = false, onChange }) {
  const [open, setOpen] = useState(defaultOpen);
  const [clients, setClients] = useState([]);
  const [editing, setEditing] = useState(null); // {id?, label, client_id, client_secret, redirect_uri, languages[], notes}
  const [validating, setValidating] = useState({}); // { [id]: 'loading' | { ok, missing } }
  const blank = { label: "", client_id: "", client_secret: "", redirect_uri: "http://127.0.0.1:3335", languages: "", notes: "" };

  const load = async () => { const cs = await api.listOauthClients(); setClients(cs); onChange?.(cs); };
  useEffect(() => { load(); }, []);

  // Validate a specific client
  const validateClient = async (id) => {
    setValidating(prev => ({ ...prev, [id]: "loading" }));
    try {
      const result = await api.validateOauthClient(id);
      setValidating(prev => ({ ...prev, [id]: result }));
      if (!result.ok) {
        toast.error(`Client "${result.label || id}" is missing: ${result.missing?.join(", ") || "unknown fields"}`);
      } else {
        toast.success(`Client "${result.label || id}" is valid.`);
      }
    } catch (err) {
      setValidating(prev => ({ ...prev, [id]: { ok: false, error: err.toString() } }));
      toast.error("Validation failed: " + (err?.toString?.() || "Unknown error"));
    }
  };

  const startNew = () => setEditing(blank);
  const startEdit = (c) => setEditing({ ...c, languages: (c.languages || []).join(", "), client_secret: "" });

  const save = async () => {
    if (!editing.label || !editing.client_id || !editing.redirect_uri) return toast.error("Label, client_id, and redirect_uri are required");
    try {
      const redirect = new URL(editing.redirect_uri);
      if (redirect.protocol !== "http:" || !["127.0.0.1", "localhost"].includes(redirect.hostname)) {
        return toast.error("Redirect URI must be a local loopback URL such as http://127.0.0.1:3335");
      }
    } catch {
      return toast.error("Redirect URI must be a valid URL");
    }
    const payload = { ...editing, languages: (editing.languages || "").split(",").map(s=>s.trim()).filter(Boolean) };
    if (editing.id && !payload.client_secret) delete payload.client_secret;
    try {
      if (editing.id) {
        await api.updateOauthClient(editing.id, payload);
        toast.success("Client updated");
      } else {
        if (!editing.client_secret) return toast.error("client_secret required for new entries");
        await api.createOauthClient(payload);
        toast.success("Client added");
      }
      setEditing(null);
      await load();
    } catch (err) {
      toast.error("Save failed: " + (err?.toString?.() || "Unknown error"));
    }
  };

  const del = async (id) => { await api.deleteOauthClient(id); load(); };

  // Check if a client is valid based on its data
  const clientHasMissingFields = (c) => {
    return !c.client_id || !c.client_secret || !c.redirect_uri ||
      c.client_secret.startsWith('•') && (!c.client_id || !c.redirect_uri);
  };

  const getValidationIcon = (c) => {
    const v = validating[c.id];
    if (v === "loading") return <RefreshCw className="w-3 h-3 animate-spin text-muted-foreground" />;
    if (v?.ok) return <ShieldCheck className="w-3.5 h-3.5 text-green-400" />;
    // If we haven't validated yet, check obvious field presence
    if (c.client_id && c.client_secret && c.redirect_uri && !c.client_secret.startsWith('•')) {
      return <ShieldCheck className="w-3.5 h-3.5 text-green-400/60" />;
    }
    return <ShieldAlert className="w-3.5 h-3.5 text-amber-400" />;
  };

  return (
    <Card className="p-5 mb-6">
      <button onClick={()=>setOpen(o=>!o)} className="flex items-center justify-between w-full text-left">
        <div>
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">YouTube OAuth client pool</div>
          <div className="text-sm font-semibold mt-1">{clients.length} Google OAuth client{clients.length===1?"":"s"} registered</div>
        </div>
        {open ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
      </button>

      {open && (
        <div className="mt-5 fade-in">
          <p className="text-xs text-muted-foreground mb-3">Each Google OAuth client has a 10k unit/day quota. Bind multiple clients to language groups so channels auto-pick a free one when connecting.</p>

          {/* Prompt banner when any client appears to be missing credentials */}
          {clients.length > 0 && clients.some(c => clientHasMissingFields(c)) && (
            <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-500/5 border border-amber-500/20 text-xs text-muted-foreground mb-4">
              <ShieldAlert className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
              <div>
                <strong className="text-amber-300">Some OAuth clients have missing credentials.</strong>{" "}
                Clients with incomplete <code className="text-foreground">client_id</code>, <code className="text-foreground">client_secret</code>, or <code className="text-foreground">redirect_uri</code> will fail when used.
                Click <strong>Validate</strong> next to each client to check, then edit to fill in missing fields.
              </div>
            </div>
          )}

          <div className="space-y-2 mb-4">
            {clients.map(c => {
              const v = validating[c.id];
              const hasIssues = c.client_id && c.client_secret && c.redirect_uri && !c.client_secret.startsWith('•')
                ? false
                : true;
              return (
                <div key={c.id} data-testid={`oauth-client-row-${c.id}`} className={`border rounded-md p-3 flex flex-wrap items-center gap-3 ${hasIssues ? 'border-amber-500/30' : 'border-border'}`}>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <KeyRound className="w-4 h-4 text-primary shrink-0" />
                      <span className="font-medium truncate">{c.label}</span>
                      <span className="shrink-0">{getValidationIcon(c)}</span>
                    </div>
                    <div className="text-mono text-[10px] text-muted-foreground truncate">{c.client_id}</div>
                  </div>
                  <div className="flex items-center gap-1 flex-wrap">
                    {(c.languages || []).length ? c.languages.map(l => <Badge key={l} variant="secondary" className="text-[10px]"><Languages className="w-2.5 h-2.5 mr-1" />{l}</Badge>) : <Badge variant="outline" className="text-[10px]">all langs</Badge>}
                  </div>
                  {/* Validation button */}
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => validateClient(c.id)}
                    disabled={validating[c.id] === "loading"}
                    title={v?.ok ? "Valid" : v === "loading" ? "Validating..." : "Click to validate"}
                  >
                    <RefreshCw className={`w-3 h-3 ${validating[c.id] === "loading" ? "animate-spin" : ""}`} />
                  </Button>
                  <Button size="sm" variant="ghost" data-testid={`oauth-client-edit-${c.id}`} onClick={()=>startEdit(c)}><Edit3 className="w-3 h-3" /></Button>
                  <Button size="sm" variant="ghost" data-testid={`oauth-client-del-${c.id}`} onClick={()=>del(c.id)}><Trash2 className="w-3 h-3 text-destructive" /></Button>
                </div>
              );
            })}
            {!clients.length && <div className="text-xs text-muted-foreground italic">No clients yet — add one to enable real OAuth.</div>}
          </div>
          <Button size="sm" data-testid="oauth-client-new-btn" onClick={startNew}><Plus className="w-3 h-3 mr-2" />Add OAuth client</Button>
        </div>
      )}

      <Dialog open={!!editing} onOpenChange={v=>!v && setEditing(null)}>
        <DialogContent>
          <DialogHeader><DialogTitle>{editing?.id ? "Edit" : "New"} Google OAuth client</DialogTitle></DialogHeader>
          {editing && (
            <div className="space-y-3">
              <Input data-testid="oauth-client-label" placeholder="Label (e.g. 'EN-channels-pool-1')" value={editing.label} onChange={e=>setEditing({...editing, label:e.target.value})} />
              <Input data-testid="oauth-client-cid" placeholder="Client ID (xxxxx.apps.googleusercontent.com)" value={editing.client_id} onChange={e=>setEditing({...editing, client_id:e.target.value})} />
              <Input data-testid="oauth-client-secret" type="password" placeholder={editing.id ? "leave blank to keep existing secret" : "Client Secret (GOCSPX-...)"} value={editing.client_secret} onChange={e=>setEditing({...editing, client_secret:e.target.value})} />
              <Input data-testid="oauth-client-redirect" placeholder="Redirect URI (must match Google Console)" value={editing.redirect_uri} onChange={e=>setEditing({...editing, redirect_uri:e.target.value})} />
              <p className="text-xs text-muted-foreground">Use the same loopback redirect URI in Google Cloud Console, for example <code className="text-foreground">http://127.0.0.1:3335</code>.</p>
              <div className="flex items-start gap-2 p-2 rounded-md bg-amber-500/5 border border-amber-500/20 text-[11px] text-muted-foreground">
                <Info className="w-3.5 h-3.5 text-amber-400 shrink-0 mt-0.5" />
                <div>
                  <strong className="text-amber-300">Important:</strong> You must create a <strong className="text-foreground">Desktop App</strong> type credential in Google Cloud Console — not a "Web Application" credential.
                  Desktop App credentials are required for local loopback redirects. Also, if your app is in <strong>Testing</strong> mode,
                  add your Gmail account to the <strong>Test Users</strong> list in the OAuth consent screen.
                </div>
              </div>
              <Input data-testid="oauth-client-langs" placeholder="Languages (comma-separated, e.g. English, German)" value={editing.languages} onChange={e=>setEditing({...editing, languages:e.target.value})} />
              <Input placeholder="Notes (optional)" value={editing.notes} onChange={e=>setEditing({...editing, notes:e.target.value})} />
              <div className="flex justify-end gap-2 pt-2">
                <Button variant="ghost" onClick={()=>setEditing(null)}>Cancel</Button>
                <Button data-testid="oauth-client-save" onClick={save}>Save</Button>
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </Card>
  );
}