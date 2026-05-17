import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Badge } from "../components/ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "../components/ui/dialog";
import { Plus, Trash2, KeyRound, ChevronDown, ChevronUp, Edit3, Languages } from "lucide-react";
import { toast } from "sonner";

/** Manage multiple Google OAuth clients to overcome per-client quota.
 * Each client binds to a list of languages; channel→client resolution prefers explicit binding then language match. */
export default function OAuthClientsPanel({ defaultOpen = false, onChange }) {
  const [open, setOpen] = useState(defaultOpen);
  const [clients, setClients] = useState([]);
  const [editing, setEditing] = useState(null); // {id?, label, client_id, client_secret, redirect_uri, languages[], notes}
  const blank = { label: "", client_id: "", client_secret: "", redirect_uri: "", languages: "", notes: "" };

  const load = async () => { const cs = await api.listOauthClients(); setClients(cs); onChange?.(cs); };
  useEffect(() => { load(); }, []);

  const startNew = () => setEditing(blank);
  const startEdit = (c) => setEditing({ ...c, languages: (c.languages || []).join(", "), client_secret: "" });

  const save = async () => {
    if (!editing.label || !editing.client_id) return toast.error("Label and client_id required");
    const payload = { ...editing, languages: (editing.languages || "").split(",").map(s=>s.trim()).filter(Boolean) };
    if (editing.id) {
      await api.updateOauthClient(editing.id, payload);
      toast.success("Client updated");
    } else {
      if (!editing.client_secret) return toast.error("client_secret required for new entries");
      await api.createOauthClient(payload);
      toast.success("Client added");
    }
    setEditing(null); load();
  };

  const del = async (id) => { await api.deleteOauthClient(id); load(); };

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
          <div className="space-y-2 mb-4">
            {clients.map(c => (
              <div key={c.id} data-testid={`oauth-client-row-${c.id}`} className="border border-border rounded-md p-3 flex flex-wrap items-center gap-3">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <KeyRound className="w-4 h-4 text-primary shrink-0" />
                    <span className="font-medium truncate">{c.label}</span>
                  </div>
                  <div className="text-mono text-[10px] text-muted-foreground truncate">{c.client_id}</div>
                </div>
                <div className="flex items-center gap-1 flex-wrap">
                  {(c.languages || []).length ? c.languages.map(l => <Badge key={l} variant="secondary" className="text-[10px]"><Languages className="w-2.5 h-2.5 mr-1" />{l}</Badge>) : <Badge variant="outline" className="text-[10px]">all langs</Badge>}
                </div>
                <Button size="sm" variant="ghost" data-testid={`oauth-client-edit-${c.id}`} onClick={()=>startEdit(c)}><Edit3 className="w-3 h-3" /></Button>
                <Button size="sm" variant="ghost" data-testid={`oauth-client-del-${c.id}`} onClick={()=>del(c.id)}><Trash2 className="w-3 h-3 text-destructive" /></Button>
              </div>
            ))}
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
