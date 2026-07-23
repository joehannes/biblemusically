import { useEffect, useState, useCallback } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import { Switch } from "../components/ui/switch";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Clapperboard, Sparkles, RefreshCw, Loader2, CheckCircle2, FileAudio, Wand2 } from "lucide-react";
import { toast } from "sonner";

const STYLES = [
  { id: "cqt", name: "CQT spectrum (musical, piano-roll glow)" },
  { id: "spectrum", name: "Scrolling spectrum (fire)" },
  { id: "waves", name: "Waveform lines" },
  { id: "vectorscope", name: "Vectorscope (lissajous lines)" },
  { id: "freqs", name: "Frequency bars" },
];

export default function OverlayStudio() {
  const { activeProjectId, songs, refreshSongs } = useStudio();
  const [jobs, setJobs] = useState([]);
  const [s, setS] = useState(null); // relevant settings subset
  const [bulkRunning, setBulkRunning] = useState(false);

  useEffect(() => { refreshSongs(); }, [refreshSongs]);

  // Poll jobs for live overlay-generation status.
  useEffect(() => {
    const fetchJobs = () => api.listJobs().then(setJobs).catch(() => {});
    fetchJobs();
    const t = setInterval(fetchJobs, 2500);
    return () => clearInterval(t);
  }, []);

  // Poll songs while any overlay job is active so "ready" chips appear without a manual refresh.
  const activeOverlayJobs = jobs.filter(j => j.kind === "overlay" && (j.status === "queued" || j.status === "running"));
  useEffect(() => {
    if (activeOverlayJobs.length === 0) return;
    const t = setInterval(() => refreshSongs(), 4000);
    return () => clearInterval(t);
  }, [activeOverlayJobs.length, refreshSongs]);

  useEffect(() => {
    (async () => {
      try { setS(await api.getSettings(activeProjectId)); } catch { /* settings card just stays hidden */ }
    })();
  }, [activeProjectId]);

  const saveSetting = useCallback(async (updates) => {
    setS(prev => ({ ...prev, ...updates }));
    try {
      await api.saveSettings({ ...s, ...updates }, activeProjectId);
    } catch {
      toast.error("Could not save overlay settings.");
    }
  }, [s, activeProjectId]);

  const jobFor = (sid) => activeOverlayJobs.find(j => j.target_id === sid);

  const generateOne = async (sid) => {
    try {
      await api.generateOverlay(sid);
      toast.success("Overlay job queued.");
      api.listJobs().then(setJobs).catch(() => {});
    } catch (err) {
      toast.error(`Failed to queue overlay: ${err}`);
    }
  };

  const generateAll = async (force) => {
    setBulkRunning(true);
    try {
      const r = await api.generateOverlaysBulk(activeProjectId, force);
      const n = r?.queued ?? 0;
      n > 0 ? toast.success(`${n} overlay job(s) queued.`) : toast.message("Nothing to do — every song with audio already has an overlay. Use Re-render all to redo them.");
      api.listJobs().then(setJobs).catch(() => {});
    } catch (err) {
      toast.error(`Bulk queue failed: ${err}`);
    } finally {
      setBulkRunning(false);
    }
  };

  const withAudio = (songs || []).filter(x => x.audio_url);

  return (
    <div className="p-6 max-w-5xl mx-auto" data-testid="overlay-studio-page">
      <div className="flex items-center gap-2 mb-1">
        <Clapperboard className="w-5 h-5 text-primary" />
        <h1 className="text-xl font-bold">Overlay Studio</h1>
      </div>
      <p className="text-sm text-muted-foreground mb-6">
        Generates a cheap, CPU-only audio-reactive animation for each song (ffmpeg visualizers; optional projectM tier).
        The video composer automatically blends a song's companion overlay over its images — no manual wiring.
      </p>

      {s && (
        <Card className="p-6 mb-5">
          <div className="flex items-center gap-2 mb-4"><Wand2 className="w-4 h-4 text-primary" /><h2 className="font-semibold">Overlay style & mixing</h2></div>
          <div className="grid md:grid-cols-3 gap-4">
            <div className="space-y-1">
              <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Animation style</Label>
              <Select value={s.overlay_style || "cqt"} onValueChange={(v) => saveSetting({ overlay_style: v })}>
                <SelectTrigger data-testid="overlay-style"><SelectValue /></SelectTrigger>
                <SelectContent>{STYLES.map(x => <SelectItem key={x.id} value={x.id}>{x.name}</SelectItem>)}</SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Blend mode</Label>
              <Select value={s.video_overlay_mode || "screen"} onValueChange={(v) => saveSetting({ video_overlay_mode: v })}>
                <SelectTrigger data-testid="overlay-blend"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="screen">screen — additive glow (black disappears)</SelectItem>
                  <SelectItem value="overlay">overlay — translucent layer</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Opacity (0–1)</Label>
              <Input data-testid="overlay-opacity" type="number" step="0.05" min="0" max="1"
                value={s.video_overlay_opacity ?? 0.35}
                onChange={(ev) => saveSetting({ video_overlay_opacity: parseFloat(ev.target.value) || 0 })} />
            </div>
            <div className="space-y-1">
              <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Engine</Label>
              <Select value={s.overlay_engine || "ffmpeg"} onValueChange={(v) => saveSetting({ overlay_engine: v })}>
                <SelectTrigger data-testid="overlay-engine"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="ffmpeg">ffmpeg visualizers (fast, CPU-only)</SelectItem>
                  <SelectItem value="projectm">projectM / Milkdrop (opt-in, needs external renderer)</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {(s.overlay_engine === "projectm") && (
              <div className="space-y-1 md:col-span-2">
                <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">projectM render command</Label>
                <Input data-testid="overlay-projectm" placeholder='e.g. my-projectm-render {audio} {out}' value={s.projectm_path || ""}
                  onChange={(ev) => saveSetting({ projectm_path: ev.target.value })} />
                <p className="text-[11px] text-muted-foreground">Use {"{audio}"} and {"{out}"} placeholders. If the command fails, the job falls back to the ffmpeg style.</p>
              </div>
            )}
            <div className="flex items-center gap-3 md:col-span-3">
              <Switch checked={s.overlay_auto_use !== false} onCheckedChange={(v) => saveSetting({ overlay_auto_use: v })} data-testid="overlay-auto-use" />
              <div>
                <div className="text-sm font-medium">Auto-use companion overlays in composed videos</div>
                <div className="text-xs text-muted-foreground">When no global overlay URL is set in Settings, each video automatically uses its song's generated overlay.</div>
              </div>
            </div>
          </div>
        </Card>
      )}

      <div className="flex items-center justify-between mb-3">
        <h2 className="font-semibold">Songs ({withAudio.length} with audio)</h2>
        <div className="flex gap-2">
          <Button size="sm" variant="secondary" disabled={bulkRunning} onClick={() => generateAll(false)} data-testid="overlay-generate-all">
            {bulkRunning ? <Loader2 className="w-3 h-3 mr-2 animate-spin" /> : <Sparkles className="w-3 h-3 mr-2" />}Generate missing
          </Button>
          <Button size="sm" variant="outline" disabled={bulkRunning} onClick={() => generateAll(true)} data-testid="overlay-rerender-all">
            <RefreshCw className="w-3 h-3 mr-2" />Re-render all
          </Button>
        </div>
      </div>

      {withAudio.length === 0 && (
        <Card className="p-8 text-center text-sm text-muted-foreground">
          No songs with audio yet — generate music first (Music Gen / Sound Studio), then come back here.
        </Card>
      )}

      <div className="space-y-2">
        {withAudio.map(song => {
          const j = jobFor(song.id);
          const ready = !!song.overlay_local_path;
          return (
            <Card key={song.id} className="p-4 flex items-center gap-4" data-testid={`overlay-row-${song.id}`}>
              <FileAudio className="w-4 h-4 text-muted-foreground shrink-0" />
              <div className="flex-1 min-w-0">
                <div className="font-medium truncate">{song.title || song.id}</div>
                <div className="text-xs text-muted-foreground truncate">
                  {song.language} · {Math.round(song.duration || 0)}s
                  {ready && <> · overlay: {song.overlay_style || "cqt"}</>}
                </div>
              </div>
              {j ? (
                <Badge variant="secondary" className="shrink-0"><Loader2 className="w-3 h-3 mr-1 animate-spin" />{j.status}</Badge>
              ) : ready ? (
                <Badge className="shrink-0 bg-emerald-600/15 text-emerald-500 border-emerald-600/20"><CheckCircle2 className="w-3 h-3 mr-1" />ready</Badge>
              ) : (
                <Badge variant="outline" className="shrink-0">none</Badge>
              )}
              <Button size="sm" variant={ready ? "outline" : "secondary"} disabled={!!j}
                onClick={() => generateOne(song.id)} data-testid={`overlay-generate-${song.id}`}>
                <Sparkles className="w-3 h-3 mr-2" />{ready ? "Re-render" : "Generate"}
              </Button>
            </Card>
          );
        })}
      </div>
    </div>
  );
}
