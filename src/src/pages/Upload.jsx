import { useEffect, useState, useRef } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Badge } from "../components/ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "../components/ui/dialog";
import { Checkbox } from "../components/ui/checkbox";
import { UploadCloud, Send, Plus, Lock, Globe, Sparkles, Zap, Wand2, ShieldCheck, Loader2, CheckCheck } from "lucide-react";
import { toast } from "sonner";
import { getStepForPath } from "../lib/pageSteps";

const ALL_FORMATS = [
  { id: "youtube", label: "YouTube 16:9" },
  { id: "shorts", label: "YT Shorts 9:16" },
  { id: "tiktok", label: "TikTok 9:16" },
];

export default function Upload() {
  const { songs, activeProjectId } = useStudio();
  const readySongs = songs.filter(s => s.audio_url);
  const [channels, setChannels] = useState([]);
  const [uploads, setUploads] = useState([]);
  const [songId, setSongId] = useState(""); const [chId, setChId] = useState("");
  const [title, setTitle] = useState(""); const [desc, setDesc] = useState(""); const [tags, setTags] = useState("");
  const [privacy, setPrivacy] = useState("public"); const [format, setFormat] = useState("youtube");
  // bulk
  const [bulkOpen, setBulkOpen] = useState(false);
  const [bulkFormats, setBulkFormats] = useState(["youtube"]);
  const [bulkPrivacy, setBulkPrivacy] = useState("public");
  const [globalDesc, setGlobalDesc] = useState("");
  const [busy, setBusy] = useState("");
  const [oauthQueue, setOauthQueue] = useState([]); // [{channel_id, name, url, label}]
  const [currentOauth, setCurrentOauth] = useState(null);
  const pollRef = useRef();

  const load = async () => { setChannels(await api.listChannels()); setUploads(await api.listUploads()); };
  useEffect(() => { load(); const t = setInterval(load, 3000); return () => clearInterval(t); }, []);

  const create = async () => {
    if (!songId || !chId || !title) return toast.error("Song, channel and title required");
    await api.createUpload({ song_id: songId, channel_id: chId, title, description: desc, tags: tags.split(",").map(s=>s.trim()).filter(Boolean), privacy, format });
    setTitle(""); setDesc(""); setTags(""); load(); toast.success("Added to upload queue");
  };

  const publish = async (id) => { await api.publish(id); toast.success("Publishing"); setTimeout(load, 1500); };
  const publishAll = async () => { const r = await api.publishAll(); toast.success(`Queued ${r.queued}`); setTimeout(load, 1500); };

  const toggleBulkFmt = (f) => setBulkFormats(arr => arr.includes(f) ? arr.filter(x=>x!==f) : [...arr, f]);
  const runBulkCreate = async () => {
    setBusy("create");
    try {
      const r = await api.bulkFromVideos({ project_id: activeProjectId || undefined, formats: bulkFormats, privacy: bulkPrivacy, match_by: "language" });
      toast.success(`Created ${r.created} uploads (${r.songs} videos × ${r.channels} channels × ${bulkFormats.length} formats)`);
      setBulkOpen(false); load();
    } catch (e) { toast.error(e?.message || "failed"); }
    finally { setBusy(""); }
  };

  const runAiEnrich = async (regenerate=false) => {
    setBusy("ai");
    try {
      const r = await api.aiEnrich({ global_description: globalDesc, regenerate });
      toast.success(`AI enriched ${r.updated} of ${r.total_pending} pending uploads`);
      load();
    } finally { setBusy(""); }
  };

  // sequential OAuth flow: open one, poll channels for connected=true, then open next
  const startConnectFlow = async (queue) => {
    if (!queue.length) { runPublishAll(); return; }
    const next = queue[0];
    setCurrentOauth(next);
    const win = window.open(next.url, "_blank", "width=720,height=820");
    if (pollRef.current) clearInterval(pollRef.current);
    let attempts = 0;
    pollRef.current = setInterval(async () => {
      attempts++;
      const all = await api.listChannels();
      const ch = all.find(x => x.id === next.channel_id);
      if ((ch && ch.connected) || (win && win.closed) || attempts > 120) {
        clearInterval(pollRef.current); pollRef.current = null;
        try { win && !win.closed && win.close(); } catch {}
        const rest = queue.slice(1);
        setOauthQueue(rest);
        setCurrentOauth(null);
        setTimeout(() => startConnectFlow(rest), 400);
      }
    }, 1500);
  };

  const runPublishAll = async () => {
    setBusy("publish");
    try {
      const r = await api.publishAll();
      toast.success(`Queued ${r.queued} uploads`);
    } finally { setBusy(""); setTimeout(load, 1500); }
  };

  const runConnectAll = async (alsoPublish = false) => {
    const pre = alsoPublish ? await api.uploadsPreflight() : await api.connectAllUrls().then(d=>({ need_oauth: d.items, ready: [], pending_uploads: 0 }));
    const need = (pre.need_oauth || []).filter(x => x.url);
    if (!need.length) {
      toast.success("All channels already connected");
      if (alsoPublish) runPublishAll();
      return;
    }
    toast.info(`Connecting ${need.length} channel${need.length>1?"s":""} sequentially — popups will open one by one`);
    setOauthQueue(need);
    startConnectFlow(need);
  };

  // Smart bulk: bulk-create → AI enrich → connect-all → publish
  const smartBulk = async () => {
    if (!bulkFormats.length) return toast.error("Pick at least one format");
    setBusy("smart");
    try {
      const c = await api.bulkFromVideos({ project_id: activeProjectId || undefined, formats: bulkFormats, privacy: bulkPrivacy });
      toast.success(`Created ${c.created} upload rows`);
      const e = await api.aiEnrich({ global_description: globalDesc, regenerate: false });
      toast.success(`AI enriched ${e.updated}`);
      setBulkOpen(false);
      const pre = await api.uploadsPreflight();
      const need = (pre.need_oauth || []).filter(x => x.url);
      if (need.length) {
        toast.info(`Pre-connecting ${need.length} channel${need.length>1?"s":""} before upload`);
        setOauthQueue(need); startConnectFlow(need);
      } else {
        await runPublishAll();
      }
    } finally { setBusy(""); }
  };

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step {getStepForPath("/upload")}</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Upload</h1>
        <div className="flex gap-2 flex-wrap">
          <Button data-testid="upload-smartbulk-btn" onClick={()=>setBulkOpen(true)} disabled={!!busy}><Zap className="w-4 h-4 mr-2" />Smart Bulk Upload</Button>
          <Button variant="secondary" data-testid="upload-aienrich-btn" onClick={()=>runAiEnrich(false)} disabled={!!busy}>{busy==="ai"?<Loader2 className="w-4 h-4 mr-2 animate-spin"/>:<Wand2 className="w-4 h-4 mr-2" />}AI enrich</Button>
          <Button variant="secondary" data-testid="upload-connectall-btn" onClick={()=>runConnectAll(false)} disabled={!!busy}><ShieldCheck className="w-4 h-4 mr-2" />Connect all</Button>
          <Button data-testid="upload-publishall-btn" onClick={()=>runConnectAll(true)} disabled={!!busy}>{busy==="publish"?<Loader2 className="w-4 h-4 mr-2 animate-spin"/>:<Send className="w-4 h-4 mr-2" />}Publish all</Button>
        </div>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">Smart Bulk creates one upload per video × matching channel × format, AI-enriches title/description/tags via Qwen, walks through every needed OAuth one popup at a time, then publishes everything. No babysitting.</p>

      <Card className="p-5 mb-5">
        <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2 flex items-center gap-2"><Sparkles className="w-3 h-3" />Global description (used by AI enrich)</div>
        <Textarea data-testid="upload-globaldesc-input" value={globalDesc} onChange={e=>setGlobalDesc(e.target.value)} rows={3} placeholder="Write one master description — Qwen adapts it for each channel's language and music style." />
      </Card>

      <Card className="p-5 mb-6">
        <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3">Manual upload (single)</div>
        <div className="grid md:grid-cols-12 gap-3">
          <div className="md:col-span-3 flex flex-col gap-1.5">
            <Select value={songId} onValueChange={setSongId}>
              <SelectTrigger data-testid="upload-song-select">
                <SelectValue placeholder={readySongs.length ? "Select song" : "No generated songs available"} />
              </SelectTrigger>
              <SelectContent>
                {readySongs.map(s => <SelectItem key={s.id} value={s.id}>{s.title}</SelectItem>)}
              </SelectContent>
            </Select>
            {readySongs.length === 0 && (
              <span className="text-[11px] text-destructive animate-pulse font-mono pl-1">
                * Generate song audio in Step 3 first!
              </span>
            )}
          </div>
          <Select value={chId} onValueChange={setChId}>
            <SelectTrigger data-testid="upload-channel-select" className="md:col-span-3"><SelectValue placeholder="Channel" /></SelectTrigger>
            <SelectContent>{channels.map(c => <SelectItem key={c.id} value={c.id}>{c.name}</SelectItem>)}</SelectContent>
          </Select>
          <Input data-testid="upload-title-input" placeholder="Title" value={title} onChange={e=>setTitle(e.target.value)} className="md:col-span-4" />
          <Select value={privacy} onValueChange={setPrivacy}>
            <SelectTrigger className="md:col-span-1"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="private">Private</SelectItem>
              <SelectItem value="unlisted">Unlisted</SelectItem>
              <SelectItem value="public">Public</SelectItem>
            </SelectContent>
          </Select>
          <Button data-testid="upload-add-btn" onClick={create} className="md:col-span-1"><Plus className="w-4 h-4" /></Button>
          <Textarea data-testid="upload-desc-input" placeholder="Description (optional — global + AI enrich preferred)" value={desc} onChange={e=>setDesc(e.target.value)} rows={2} className="md:col-span-8" />
          <Input data-testid="upload-tags-input" placeholder="tags, comma, separated" value={tags} onChange={e=>setTags(e.target.value)} className="md:col-span-3" />
          <Select value={format} onValueChange={setFormat}>
            <SelectTrigger className="md:col-span-1"><SelectValue /></SelectTrigger>
            <SelectContent>
              {ALL_FORMATS.map(f => <SelectItem key={f.id} value={f.id}>{f.label}</SelectItem>)}
            </SelectContent>
          </Select>
        </div>
      </Card>

      <div className="space-y-3">
        {uploads.map(u => {
          const song = songs.find(s => s.id === u.song_id);
          const ch = channels.find(c => c.id === u.channel_id);
          return (
            <Card key={u.id} data-testid={`upload-row-${u.id}`} className="p-4 flex flex-wrap items-center gap-3">
              <div className="flex-1 min-w-0">
                <div className="font-medium truncate">{u.title || "(no title)"}</div>
                <div className="text-xs text-muted-foreground truncate">{song?.title || u.song_id} → {ch?.name || u.channel_id}</div>
                {u.description && <div className="text-[11px] text-muted-foreground line-clamp-1 italic mt-0.5">{u.description.slice(0, 140)}</div>}
                {(u.tags||[]).length > 0 && <div className="flex flex-wrap gap-1 mt-1">{u.tags.slice(0,6).map(t => <span key={t} className="text-[10px] text-mono bg-muted px-1.5 py-0.5 rounded">{t}</span>)}</div>}
              </div>
              <Badge variant="outline" className="text-[10px]">{u.format}</Badge>
              <Badge variant="secondary" className="text-[10px]">{u.privacy === "public" ? <Globe className="w-3 h-3 mr-1" /> : <Lock className="w-3 h-3 mr-1" />}{u.privacy}</Badge>
              <Badge variant={u.status==="published"?"default":"outline"} data-testid={`upload-status-${u.id}`}>{u.status}</Badge>
              {u.status === "pending" && <Button size="sm" data-testid={`upload-publish-${u.id}`} onClick={()=>publish(u.id)}><UploadCloud className="w-3 h-3 mr-1" />Publish</Button>}
            </Card>
          );
        })}
        {!uploads.length && <Card className="p-10 text-center text-muted-foreground border-dashed">No uploads queued. Try "Smart Bulk Upload" above.</Card>}
      </div>

      {/* Smart Bulk dialog */}
      <Dialog open={bulkOpen} onOpenChange={setBulkOpen}>
        <DialogContent>
          <DialogHeader><DialogTitle><Zap className="w-4 h-4 inline mr-2" />Smart Bulk Upload</DialogTitle></DialogHeader>
          <p className="text-sm text-muted-foreground">Auto-creates upload rows for every <b>video_ready</b> song × matching channel × selected format(s), then Qwen-enriches titles/descriptions/tags, pre-connects every missing OAuth in sequence, and finally publishes the lot.</p>
          <div className="space-y-3 mt-2">
            <div>
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2">Formats</div>
              <div className="flex flex-wrap gap-3">
                {ALL_FORMATS.map(f => (
                  <label key={f.id} className="flex items-center gap-2 text-sm">
                    <Checkbox data-testid={`bulk-fmt-${f.id}`} checked={bulkFormats.includes(f.id)} onCheckedChange={()=>toggleBulkFmt(f.id)} />{f.label}
                  </label>
                ))}
              </div>
            </div>
            <div>
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2">Privacy</div>
              <Select value={bulkPrivacy} onValueChange={setBulkPrivacy}>
                <SelectTrigger data-testid="bulk-privacy"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="public">Public</SelectItem>
                  <SelectItem value="unlisted">Unlisted</SelectItem>
                  <SelectItem value="private">Private</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div>
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2">Global description (will be AI-adapted per channel)</div>
              <Textarea rows={3} value={globalDesc} onChange={e=>setGlobalDesc(e.target.value)} placeholder="e.g. 'A multilingual reimagining of John 1...'" />
            </div>
            <div className="flex justify-between gap-2 pt-3">
              <Button variant="secondary" data-testid="bulk-create-only" onClick={runBulkCreate} disabled={!!busy}>Create rows only</Button>
              <Button data-testid="bulk-smart-run" onClick={smartBulk} disabled={!!busy}>{busy==="smart"?<Loader2 className="w-4 h-4 mr-2 animate-spin"/>:<CheckCheck className="w-4 h-4 mr-2" />}Run full flow</Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Sequential OAuth dialog */}
      <Dialog open={!!currentOauth} onOpenChange={()=>{}}>
        <DialogContent>
          <DialogHeader><DialogTitle>Authorizing channel</DialogTitle></DialogHeader>
          {currentOauth && (
            <div className="text-sm space-y-2">
              <div><b>{currentOauth.name}</b> via <span className="text-mono text-xs">{currentOauth.label}</span></div>
              <p className="text-muted-foreground">A Google authorization popup is open. Approve it — this dialog will advance automatically once the channel reports connected (or when you close the popup).</p>
              <div className="text-xs text-muted-foreground text-mono">Queue remaining: {oauthQueue.length}</div>
              <div className="flex items-center gap-2 text-primary"><Loader2 className="w-4 h-4 animate-spin" /> waiting for callback…</div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
