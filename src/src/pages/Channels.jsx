import { useEffect, useState } from "react";
import { api } from "../lib/api";
import OAuthClientsPanel from "../components/OAuthClientsPanel";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Badge } from "../components/ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from "../components/ui/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Tv, Plus, Link as LinkIcon, Trash2, ShieldCheck, KeyRound } from "lucide-react";
import { toast } from "sonner";

export default function Channels() {
  const [channels, setChannels] = useState([]);
  const [oauthClients, setOauthClients] = useState([]);
  const [name, setName] = useState(""); const [ytId, setYtId] = useState(""); const [lang, setLang] = useState(""); const [region, setRegion] = useState("");
  const [oauthDialog, setOauthDialog] = useState(null);
  const [refresh, setRefresh] = useState(""); const [yt, setYt] = useState(""); const [subs, setSubs] = useState("");
  const [pickedClient, setPickedClient] = useState({}); // channelId -> client info

  const load = async () => setChannels(await api.listChannels());
  useEffect(() => { load(); }, []);

  // re-resolve picked clients for each channel whenever oauthClients change
  useEffect(() => {
    (async () => {
      const next = {};
      for (const c of channels) {
        try { const r = await api.channelPickedClient(c.id); next[c.id] = r.client; } catch {}
      }
      setPickedClient(next);
    })();
  }, [channels, oauthClients]);

  const create = async () => {
    if (!name) return toast.error("Channel name required");
    await api.createChannel({ name, youtube_channel_id: ytId, language: lang || "English", region });
    setName(""); setYtId(""); setLang(""); setRegion(""); load(); toast.success("Channel added");
  };

  const startOauth = async (c, forceClientId) => {
    const r = await api.oauthStart(c.id, forceClientId);
    if (r.error) return toast.error(r.error);
    setOauthDialog(c);
    window.open(r.url, "_blank");
  };

  const completeOauth = async () => {
    if (!oauthDialog) return;
    await api.oauthComplete(oauthDialog.id, { refresh_token: refresh, youtube_channel_id: yt, subscriber_count: subs });
    setOauthDialog(null); setRefresh(""); setYt(""); setSubs(""); load(); toast.success("Channel connected");
  };

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step 8</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Channel Manager</h1>
        <Button data-testid="channels-connect-all-btn" onClick={async ()=>{
          const r = await api.connectAllUrls();
          const items = (r.items || []).filter(x => x.url);
          if (!items.length) return toast.success("All channels already connected");
          toast.info(`Opening ${items.length} OAuth popups one at a time…`);
          let i = 0;
          const next = () => {
            if (i >= items.length) { toast.success("Done"); return; }
            const it = items[i++];
            const win = window.open(it.url, "_blank", "width=720,height=820");
            let attempts = 0;
            const poll = setInterval(async () => {
              attempts++;
              const all = await api.listChannels();
              const ch = all.find(x => x.id === it.channel_id);
              if ((ch && ch.connected) || (win && win.closed) || attempts > 120) {
                clearInterval(poll); try { win && !win.closed && win.close(); } catch {}
                load(); setTimeout(next, 300);
              }
            }, 1500);
          };
          next();
        }}><ShieldCheck className="w-4 h-4 mr-2" />Connect all</Button>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">Add YouTube channels and connect each via Google OAuth. Manage multiple OAuth clients below to spread upload quota across language groups.</p>

      <OAuthClientsPanel defaultOpen={false} onChange={setOauthClients} />

      <Card className="p-5 mb-6">
        <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3">Add channel</div>
        <div className="grid md:grid-cols-12 gap-3">
          <Input data-testid="channel-name-input" placeholder="Channel name" value={name} onChange={e=>setName(e.target.value)} className="md:col-span-4" />
          <Input data-testid="channel-ytid-input" placeholder="YouTube channel id (UC...)" value={ytId} onChange={e=>setYtId(e.target.value)} className="md:col-span-3" />
          <Input data-testid="channel-lang-input" placeholder="Language" value={lang} onChange={e=>setLang(e.target.value)} className="md:col-span-2" />
          <Input data-testid="channel-region-input" placeholder="Region" value={region} onChange={e=>setRegion(e.target.value)} className="md:col-span-2" />
          <Button data-testid="channel-add-btn" onClick={create} className="md:col-span-1"><Plus className="w-4 h-4" /></Button>
        </div>
      </Card>

      <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
        {channels.map(c => {
          const picked = pickedClient[c.id];
          return (
            <Card key={c.id} data-testid={`channel-card-${c.id}`} className="p-5">
              <div className="flex items-start justify-between mb-3">
                <div className="min-w-0">
                  <div className="font-semibold truncate flex items-center gap-2"><Tv className="w-4 h-4 text-primary" />{c.name}</div>
                  <div className="text-xs text-muted-foreground truncate text-mono">{c.youtube_channel_id || "no id"}</div>
                </div>
                <button onClick={()=>{api.deleteChannel(c.id).then(load);}} className="text-muted-foreground hover:text-destructive"><Trash2 className="w-4 h-4" /></button>
              </div>
              <div className="flex flex-wrap gap-2 mb-3">
                <Badge variant="secondary">{c.language}</Badge>
                {c.region && <Badge variant="outline">{c.region}</Badge>}
                <Badge variant={c.connected?"default":"outline"} data-testid={`channel-status-${c.id}`}>{c.connected?"connected":"not connected"}</Badge>
              </div>
              {picked && (
                <div className="mb-3 text-[10px] text-mono text-muted-foreground flex items-center gap-1"><KeyRound className="w-3 h-3" />will use: <span className="text-foreground">{picked.label}</span></div>
              )}
              {oauthClients.length > 1 ? (
                <div className="flex gap-2">
                  <Select onValueChange={(v)=>startOauth(c, v === "_auto" ? undefined : v)}>
                    <SelectTrigger data-testid={`channel-pickclient-${c.id}`} className="flex-1"><SelectValue placeholder="Connect via..." /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="_auto">Auto-pick by language</SelectItem>
                      {oauthClients.map(oc => <SelectItem key={oc.id} value={oc.id}>{oc.label}</SelectItem>)}
                    </SelectContent>
                  </Select>
                </div>
              ) : (
                <Button size="sm" variant={c.connected?"secondary":"default"} data-testid={`channel-oauth-${c.id}`} onClick={()=>startOauth(c)} className="w-full">
                  <ShieldCheck className="w-3 h-3 mr-2" />{c.connected?"Re-connect":"Connect OAuth"}
                </Button>
              )}
            </Card>
          );
        })}
        {!channels.length && <Card className="p-10 col-span-full text-center text-muted-foreground border-dashed">No channels yet.</Card>}
      </div>

      <Dialog open={!!oauthDialog} onOpenChange={(v)=>!v && setOauthDialog(null)}>
        <DialogContent>
          <DialogHeader><DialogTitle>Complete OAuth for {oauthDialog?.name}</DialogTitle></DialogHeader>
          <p className="text-sm text-muted-foreground">After authorizing in the opened tab, the callback updates this channel automatically. If your redirect_uri doesn't return here, paste the refresh token manually below.</p>
          <Input data-testid="oauth-refresh-input" placeholder="refresh_token (manual fallback)" value={refresh} onChange={e=>setRefresh(e.target.value)} />
          <Input data-testid="oauth-ytid-input" placeholder="youtube channel id" value={yt} onChange={e=>setYt(e.target.value)} />
          <Input data-testid="oauth-subs-input" placeholder="subscriber count" value={subs} onChange={e=>setSubs(e.target.value)} />
          <Button data-testid="oauth-complete-btn" onClick={completeOauth}><LinkIcon className="w-4 h-4 mr-2" />Save manual token</Button>
        </DialogContent>
      </Dialog>
    </div>
  );
}

