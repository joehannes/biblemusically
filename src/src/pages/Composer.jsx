import { useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Checkbox } from "../components/ui/checkbox";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Film, Wand } from "lucide-react";
import { toast } from "sonner";
import { getStepForPath } from "../lib/pageSteps";

const FORMATS = ["youtube_16x9_1080p", "shorts_9x16_1080p", "tiktok_9x16_1080p"];

export default function Composer() {
  const { songs, activeSongId, selectSong } = useStudio();
  const [presets, setPresets] = useState([]);
  const [sections, setSections] = useState([]);
  const [formats, setFormats] = useState(["youtube_16x9_1080p"]);

  useEffect(() => { api.effectsPresets().then(setPresets); }, []);
  useEffect(() => { if (activeSongId) api.listSections(activeSongId).then(setSections); }, [activeSongId]);

  const toggleFmt = (f) => setFormats(arr => arr.includes(f) ? arr.filter(x=>x!==f) : [...arr, f]);

  const toggleEffect = async (sec, eid) => {
    const ef = sec.effects?.includes(eid) ? sec.effects.filter(x=>x!==eid) : [...(sec.effects||[]), eid];
    await api.updateSection(sec.id, { effects: ef });
    setSections(await api.listSections(activeSongId));
  };

  const compose = async () => { if (!activeSongId) return; await api.compose(activeSongId); toast.success(`Composing for ${formats.length} format(s)`); };

  const readySongs = songs.filter(s => s.audio_url);

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step {getStepForPath("/video")}</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Video Composer</h1>
        <Button data-testid="composer-render-btn" onClick={compose} disabled={!activeSongId}><Film className="w-4 h-4 mr-2" />Render</Button>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">FFmpeg stitches images to audio, applying mood-matched effects &amp; transitions. Choose target formats.</p>

      <Card className="p-5 mb-6 grid md:grid-cols-2 gap-4 items-center">
        <div className="flex flex-col gap-1.5">
          <Select value={activeSongId || ""} onValueChange={selectSong}>
            <SelectTrigger data-testid="composer-song-select">
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
        <div className="flex flex-wrap gap-3">
          {FORMATS.map(f => (
            <label key={f} className="flex items-center gap-2 text-xs text-mono">
              <Checkbox data-testid={`composer-fmt-${f}`} checked={formats.includes(f)} onCheckedChange={()=>toggleFmt(f)} />{f}
            </label>
          ))}
        </div>
      </Card>

      <div className="space-y-3">
        {sections.map(s => (
          <Card key={s.id} data-testid={`composer-section-${s.index}`} className="p-4">
            <div className="flex items-start justify-between gap-3 mb-3">
              <div className="min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <Badge variant="secondary">{s.mood}</Badge>
                  <span className="text-mono text-[10px] text-muted-foreground">{s.start.toFixed(1)}–{s.end.toFixed(1)}s</span>
                </div>
                <div className="text-sm truncate">{s.line}</div>
              </div>
              <Wand className="w-4 h-4 text-primary shrink-0" />
            </div>
            <div className="flex flex-wrap gap-2">
              {presets.map(p => {
                const on = s.effects?.includes(p.id);
                return (
                  <button key={p.id} data-testid={`composer-effect-${s.index}-${p.id}`}
                    onClick={()=>toggleEffect(s, p.id)}
                    className={`text-[11px] px-2 py-1 rounded-md text-mono transition-colors ${on?"bg-primary text-primary-foreground":"bg-muted text-muted-foreground hover:bg-secondary"}`}>
                    {p.label}
                  </button>
                );
              })}
            </div>
          </Card>
        ))}
        {!sections.length && <Card className="p-10 text-center text-muted-foreground border-dashed">Select a song with analyzed sections.</Card>}
      </div>
    </div>
  );
}
