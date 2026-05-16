import { useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Mic2, Play, Sparkles } from "lucide-react";
import { toast } from "sonner";

export default function MusicGen() {
  const { activeProjectId, songs, refreshSongs, selectSong, activeSongId } = useStudio();

  useEffect(() => { refreshSongs(); }, [refreshSongs]);

  const trigger = async (sid) => { await api.genMusic(sid); toast.success("Music generation queued (Suno)"); setTimeout(refreshSongs, 3000); };
  const triggerAll = async () => { for (const s of songs.filter(x=>!x.audio_url)) await api.genMusic(s.id); toast.success(`Queued ${songs.length} jobs`); setTimeout(refreshSongs, 3000); };

  if (!activeProjectId) return <div className="p-8"><Card className="p-10 text-center text-muted-foreground border-dashed">Select a project first.</Card></div>;

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step 3</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Music Generation</h1>
        <Button data-testid="musicgen-batch-btn" onClick={triggerAll} disabled={!songs.length}><Sparkles className="w-4 h-4 mr-2" />Generate all</Button>
      </div>
      <p className="text-muted-foreground mb-8 max-w-2xl">Each song is sent to Suno via the studio-api endpoint (uses the cookie configured in Settings). Previews appear once ready.</p>

      <div className="grid md:grid-cols-2 gap-4">
        {songs.map(s => (
          <Card key={s.id} data-testid={`song-card-${s.id}`}
            className={`p-5 transition-all cursor-pointer ${activeSongId===s.id ? "ring-2 ring-primary" : "hover:border-primary/40"}`}
            onClick={() => selectSong(s.id)}>
            <div className="flex items-start justify-between mb-2">
              <div className="min-w-0">
                <div className="font-medium truncate">{s.title}</div>
                <div className="text-xs text-muted-foreground italic">{s.styles}</div>
              </div>
              <Badge variant="secondary" data-testid={`song-lang-${s.id}`}>{s.language}</Badge>
            </div>

            <div className="my-4 h-10 rounded-md overflow-hidden border border-border bg-muted/30 relative">
              {s.audio_url ? <div className="absolute inset-0 waveform opacity-80" /> : <div className="absolute inset-0 flex items-center justify-center text-xs text-muted-foreground text-mono">— no audio yet —</div>}
            </div>

            <div className="flex items-center justify-between gap-2">
              <Badge variant={s.audio_url ? "default" : "outline"} data-testid={`song-status-${s.id}`}>{s.status}</Badge>
              <div className="flex gap-2">
                {s.audio_url && <Button size="sm" variant="secondary" onClick={(e)=>{e.stopPropagation(); toast.info("Preview is mocked");}}><Play className="w-3 h-3 mr-1" />Preview</Button>}
                <Button size="sm" data-testid={`song-genmusic-${s.id}`} onClick={(e)=>{e.stopPropagation(); trigger(s.id);}}><Mic2 className="w-3 h-3 mr-1" />{s.audio_url?"Re-gen":"Generate"}</Button>
              </div>
            </div>
          </Card>
        ))}
        {!songs.length && <Card className="p-10 col-span-full text-center text-muted-foreground border-dashed">No songs yet — import lyrics first.</Card>}
      </div>
    </div>
  );
}
