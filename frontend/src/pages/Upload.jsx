import { useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Badge } from "../components/ui/badge";
import { UploadCloud, Send, Plus, Lock, Globe } from "lucide-react";
import { toast } from "sonner";

export default function Upload() {
  const { songs } = useStudio();
  const [channels, setChannels] = useState([]);
  const [uploads, setUploads] = useState([]);
  const [songId, setSongId] = useState(""); const [chId, setChId] = useState("");
  const [title, setTitle] = useState(""); const [desc, setDesc] = useState(""); const [tags, setTags] = useState("");
  const [privacy, setPrivacy] = useState("private"); const [format, setFormat] = useState("youtube");

  const load = async () => { setChannels(await api.listChannels()); setUploads(await api.listUploads()); };
  useEffect(() => { load(); const t = setInterval(load, 3000); return () => clearInterval(t); }, []);

  const create = async () => {
    if (!songId || !chId || !title) return toast.error("Song, channel and title required");
    await api.createUpload({ song_id: songId, channel_id: chId, title, description: desc, tags: tags.split(",").map(s=>s.trim()).filter(Boolean), privacy, format });
    setTitle(""); setDesc(""); setTags(""); load(); toast.success("Added to upload queue");
  };
  const publish = async (id) => { await api.publish(id); toast.success("Publishing"); setTimeout(load, 1500); };
  const publishAll = async () => { const r = await api.publishAll(); toast.success(`Queued ${r.queued}`); setTimeout(load, 1500); };

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step 9</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Upload</h1>
        <Button data-testid="upload-publishall-btn" onClick={publishAll}><Send className="w-4 h-4 mr-2" />Publish all pending</Button>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">Compose video metadata per song-channel pair and publish to YouTube (Data API v3, resumable upload).</p>

      <Card className="p-5 mb-6">
        <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3">New upload</div>
        <div className="grid md:grid-cols-12 gap-3">
          <Select value={songId} onValueChange={setSongId}>
            <SelectTrigger data-testid="upload-song-select" className="md:col-span-3"><SelectValue placeholder="Song" /></SelectTrigger>
            <SelectContent>{songs.map(s => <SelectItem key={s.id} value={s.id}>{s.title}</SelectItem>)}</SelectContent>
          </Select>
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
          <Textarea data-testid="upload-desc-input" placeholder="Description" value={desc} onChange={e=>setDesc(e.target.value)} rows={2} className="md:col-span-8" />
          <Input data-testid="upload-tags-input" placeholder="tags, comma, separated" value={tags} onChange={e=>setTags(e.target.value)} className="md:col-span-3" />
          <Select value={format} onValueChange={setFormat}>
            <SelectTrigger className="md:col-span-1"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="youtube">YT</SelectItem>
              <SelectItem value="shorts">Shorts</SelectItem>
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
                <div className="font-medium truncate">{u.title}</div>
                <div className="text-xs text-muted-foreground truncate">{song?.title} → {ch?.name}</div>
              </div>
              <Badge variant="outline" className="text-[10px]">{u.format}</Badge>
              <Badge variant="secondary" className="text-[10px]">{u.privacy === "public" ? <Globe className="w-3 h-3 mr-1" /> : <Lock className="w-3 h-3 mr-1" />}{u.privacy}</Badge>
              <Badge variant={u.status==="published"?"default":"outline"} data-testid={`upload-status-${u.id}`}>{u.status}</Badge>
              {u.status === "pending" && <Button size="sm" data-testid={`upload-publish-${u.id}`} onClick={()=>publish(u.id)}><UploadCloud className="w-3 h-3 mr-1" />Publish</Button>}
            </Card>
          );
        })}
        {!uploads.length && <Card className="p-10 text-center text-muted-foreground border-dashed">No uploads queued.</Card>}
      </div>
    </div>
  );
}
