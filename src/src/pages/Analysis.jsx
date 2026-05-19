import { useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Waves, BrainCircuit } from "lucide-react";
import { getStepForPath } from "../lib/pageSteps";
import { toast } from "sonner";

const MOOD_COLOR = {
  ethereal: "bg-indigo-500", radiant: "bg-amber-500", warm: "bg-orange-500", dramatic: "bg-rose-600",
  serene: "bg-sky-500", epic: "bg-fuchsia-600", soft: "bg-emerald-500", celestial: "bg-cyan-500"
};

export default function Analysis() {
  const { songs, activeSongId, selectSong } = useStudio();
  const [sections, setSections] = useState([]);
  const song = songs.find(s => s.id === activeSongId);

  const load = async () => { if (activeSongId) setSections(await api.listSections(activeSongId)); };
  useEffect(() => { load(); }, [activeSongId]);

  const analyze = async () => {
    if (!activeSongId) return toast.error("Pick a song");
    await api.analyze(activeSongId);
    toast.success("Audio analysis queued");
    setTimeout(load, 3500);
  };

  const readySongs = songs.filter(s => s.audio_url);

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step {getStepForPath("/analysis")}</div>
      <h1 className="text-4xl sm:text-5xl font-bold mb-2">Audio Analysis</h1>
      <p className="text-muted-foreground mb-6 max-w-2xl">FFmpeg + audio analysis CLI tools parse mood &amp; timing per section, feeding the Qwen prompter for FFmpeg effect suggestions.</p>

      <Card className="p-5 mb-6 flex flex-wrap items-center gap-3">
        <Select value={activeSongId || ""} onValueChange={selectSong}>
          <SelectTrigger data-testid="analysis-song-select" className="w-80">
            <SelectValue placeholder={readySongs.length ? "Select song" : "No generated songs available"} />
          </SelectTrigger>
          <SelectContent>
            {readySongs.map(s => <SelectItem key={s.id} value={s.id}>{s.title}</SelectItem>)}
          </SelectContent>
        </Select>
        <Button data-testid="analysis-run-btn" onClick={analyze} disabled={!activeSongId}><BrainCircuit className="w-4 h-4 mr-2" />Run analysis</Button>
        {song && <Badge variant="outline" className="text-mono">{song.duration ? `${song.duration.toFixed(1)}s` : "no audio"}</Badge>}
        {readySongs.length === 0 && (
          <Badge variant="destructive" className="animate-pulse">
            Please generate song audio in Step 3 first!
          </Badge>
        )}
      </Card>

      {sections.length > 0 && (
        <Card className="p-5">
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-4">Timeline ({sections.length} sections)</div>
          <div className="space-y-2">
            {sections.map((s) => (
              <div key={s.id} data-testid={`analysis-section-${s.index}`} className="flex items-center gap-3 text-sm">
                <div className="text-mono text-[11px] text-muted-foreground w-20 shrink-0">{s.start.toFixed(1)}–{s.end.toFixed(1)}s</div>
                <div className={`h-3 rounded-sm shrink-0 ${MOOD_COLOR[s.mood] || "bg-muted"}`} style={{ width: `${Math.max(6, (s.end - s.start) * 4)}px` }} />
                <div className="text-xs text-muted-foreground w-24 truncate uppercase tracking-wide">{s.mood}</div>
                <div className="text-sm truncate flex-1">{s.line}</div>
              </div>
            ))}
          </div>
        </Card>
      )}
      {!sections.length && <Card className="p-10 text-center text-muted-foreground border-dashed"><Waves className="w-6 h-6 mx-auto mb-3" />No analysis yet.</Card>}
    </div>
  );
}
