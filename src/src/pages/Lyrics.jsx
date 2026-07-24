import { useState, useRef, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Textarea } from "../components/ui/textarea";
import { Badge } from "../components/ui/badge";
import { Upload, FileJson, CheckCheck, Trash2, Copy, Download, X, Mic2, PenLine } from "lucide-react";
import { toast } from "sonner";
import SectionAnnotator from "../components/SectionAnnotator";
import { getStepForPath } from "../lib/pageSteps";

export default function Lyrics() {
  const nav = useNavigate();
  const { activeProjectId, refreshSongs, songs } = useStudio();
  // Section-annotation editor state (which song, and which engine's dialect to author for).
  const [annotSongId, setAnnotSongId] = useState("");
  const [annotEngine, setAnnotEngine] = useState("heartmula");
  const [annotSaving, setAnnotSaving] = useState(false);
  const annotSong = songs.find((s) => s.id === annotSongId) || null;

  // Load the saved engine preference so the editor defaults to the engine actually in use.
  useEffect(() => {
    (async () => {
      try {
        const s = await api.getSettings();
        if (s?.music_engine) setAnnotEngine(s.music_engine);
      } catch { /* keep the default */ }
    })();
  }, []);

  const saveAnnotations = async ({ lyrics, annotations }) => {
    if (!annotSongId) return;
    setAnnotSaving(true);
    try {
      await api.updateSong(annotSongId, { lyrics, annotations });
      toast.success("Annotations saved.");
      refreshSongs && refreshSongs();
    } catch (e) {
      toast.error(`Save failed: ${e}`);
    } finally {
      setAnnotSaving(false);
    }
  };
  const [items, setItems] = useState([]);
  const [raw, setRaw] = useState("");
  const fileRef = useRef();

  // Lyrics should always be a plain string (see the fixed AI prompt in commands/ai.rs), but a
  // JSON file exported before that fix — or any other producer that didn't follow the shape —
  // can still hold an array of {section, lines} objects. Rendering that array directly as a
  // JSX child throws "Objects are not valid as a React child" and blanks the whole page, which
  // is exactly the reported crash; this keeps the preview safe regardless of shape.
  const lyricsText = (lyrics) => {
    if (typeof lyrics === "string") return lyrics;
    if (Array.isArray(lyrics)) {
      return lyrics.map((l) => `[${l?.section || ""}]\n${l?.lines || ""}`).join("\n\n");
    }
    return "";
  };

  const parse = (text) => {
    try {
      const data = JSON.parse(text);
      const arr = Array.isArray(data) ? data : [data];
      setItems(arr);
      toast.success(`Parsed ${arr.length} song variants`);
    } catch (e) {
      toast.error("Invalid JSON");
    }
  };

  const clearRaw = () => {
    setRaw("");
    toast.info("Textarea cleared");
  };

  const clearItems = () => {
    setItems([]);
    toast.info("Preview cleared");
  };

  const copyToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(raw || JSON.stringify(items, null, 2));
      toast.success("Copied to clipboard");
    } catch {
      toast.error("Failed to copy");
    }
  };

  // Saves straight into the active project's own ~/Documents folder — no save dialog (kept
  // deliberately simple), just a semantically named file instead of one dropped in Downloads.
  const downloadJSON = async () => {
    if (!items.length) return toast.error("Nothing to export");
    if (!activeProjectId) return toast.error("Select a project first — exports save into that project's own folder.");
    const stamp = new Date().toISOString().replace(/[:T]/g, "-").slice(0, 16);
    const filename = `lyrics-import_${items.length}-songs_${stamp}.json`;
    try {
      const res = await api.saveProjectFile(activeProjectId, filename, JSON.stringify(items, null, 2));
      toast.success(`Saved to ${res.path}`);
    } catch (err) {
      toast.error("Failed to save: " + err);
    }
  };

  const selectAllText = () => {
    const textarea = document.querySelector("[data-testid='lyrics-json-textarea']");
    if (textarea) {
      textarea.select();
      toast.info("Text selected");
    }
  };

  const onFile = async (e) => {
    const f = e.target.files?.[0]; if (!f) return;
    const text = await f.text(); setRaw(text); parse(text);
  };

  const importAll = async () => {
    if (!activeProjectId) return toast.error("Select a project first");
    if (!items.length) return toast.error("Nothing to import");
    const res = await api.importLyrics(activeProjectId, items);
    toast.success(`Imported ${res.created} songs`);
    await refreshSongs();
  };

  // Explicit "put these on the Music Generation tiles" action — imports, then jumps straight
  // to the Music Studio page where the resulting song tiles actually show up.
  const sendToMusicGen = async () => {
    if (!activeProjectId) return toast.error("Select a project first");
    if (!items.length) return toast.error("Nothing to import");
    const res = await api.importLyrics(activeProjectId, items);
    toast.success(`Sent ${res.created} song${res.created === 1 ? "" : "s"} to Music Generation`);
    await refreshSongs();
    nav("/music");
  };

  const clearImportedSongs = async () => {
    if (!activeProjectId) return toast.error("Select a project first");
    if (!songs.length) return toast.error("No imported songs to clear");
    if (!window.confirm("This will permanently delete all imported songs for the current project. Continue?")) return;

    try {
      await Promise.all(songs.map((song) => api.deleteSong(song.id)));
      toast.success(`Cleared ${songs.length} imported songs`);
      await refreshSongs();
    } catch (error) {
      console.error(error);
      toast.error("Failed to clear imported songs");
    }
  };

  if (!activeProjectId) return <div className="p-8"><Card className="p-10 text-center text-muted-foreground border-dashed">Select a project on the Dashboard first.</Card></div>;

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step {getStepForPath("/lyrics")}</div>
      <h1 className="text-4xl sm:text-5xl font-bold mb-2">Lyrics Import</h1>
      <p className="text-muted-foreground mb-8 max-w-2xl">Paste or upload a JSON file holding an array of songs — each with title, language, styles, lyrics, annotations (image prompts) and image_styles.</p>

      <div className="grid lg:grid-cols-2 gap-6">
        <Card className="p-6">
          <div className="flex items-center justify-between mb-3">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">JSON Source</div>
            <div className="flex gap-1">
              <input ref={fileRef} type="file" accept=".json,application/json" onChange={onFile} className="hidden" />
              <Button data-testid="lyrics-upload-btn" size="sm" variant="secondary" onClick={()=>fileRef.current?.click()}><Upload className="w-3 h-3 mr-2" />Upload .json</Button>
              <Button data-testid="lyrics-copy-btn" size="sm" variant="secondary" onClick={copyToClipboard} title="Copy JSON to clipboard"><Copy className="w-3 h-3" /></Button>
              <Button data-testid="lyrics-clear-raw-btn" size="sm" variant="secondary" onClick={clearRaw} title="Clear textarea"><Trash2 className="w-3 h-3" /></Button>
            </div>
          </div>
          <Textarea data-testid="lyrics-json-textarea" rows={20} value={raw} onChange={e=>setRaw(e.target.value)} placeholder='[{"title":"...","language":"English","styles":"...","lyrics":"...","annotations":"...","image_styles":"..."}]' className="text-mono text-xs" />
          <div className="flex flex-wrap gap-2 mt-3">
            <Button data-testid="lyrics-parse-btn" onClick={()=>parse(raw)} variant="secondary"><FileJson className="w-4 h-4 mr-2" />Parse</Button>
            <Button data-testid="lyrics-import-btn" onClick={importAll} disabled={!items.length}><CheckCheck className="w-4 h-4 mr-2" />Import {items.length || ""}</Button>
            <Button data-testid="lyrics-sendmusicgen-btn" onClick={sendToMusicGen} disabled={!items.length} title="Import and go straight to the tiles in Music Generation">
              <Mic2 className="w-4 h-4 mr-2" />Send to Music Generation
            </Button>
            <Button data-testid="lyrics-download-btn" onClick={downloadJSON} disabled={!items.length} variant="secondary"><Download className="w-4 h-4 mr-2" />Export</Button>
            <Button data-testid="lyrics-select-btn" onClick={selectAllText} size="sm" variant="ghost">Select All</Button>
          </div>
        </Card>

        <Card className="p-6">
          <div className="flex items-center justify-between mb-3">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Preview ({items.length})</div>
            <Button data-testid="lyrics-clear-items-btn" size="sm" variant="secondary" onClick={clearItems} disabled={!items.length} title="Clear preview items"><X className="w-3 h-3" /></Button>
          </div>
          <div className="space-y-3 max-h-[520px] overflow-auto scroll-thin pr-2">
            {items.map((it, idx) => (
              <div key={idx} data-testid={`lyrics-preview-${idx}`} className="border border-border rounded-md p-3 hover:bg-secondary/50">
                <div className="flex items-center justify-between gap-2 mb-2">
                  <div className="font-medium truncate">{it.title}</div>
                  <Badge variant="secondary" className="text-[10px]">{it.language}</Badge>
                </div>
                <div className="text-xs text-muted-foreground italic mb-1">{it.styles}</div>
                <div className="text-xs text-foreground/80 line-clamp-3">{lyricsText(it.lyrics)}</div>
              </div>
            ))}
            {!items.length && <div className="text-muted-foreground text-sm">Nothing parsed yet.</div>}
          </div>
        </Card>
      </div>

      {/* ── Per-section annotation editor ─────────────────────────────
          Authoring annotations by hand meant editing a JSON blob. This edits the same two fields
          (lyrics + annotations) structurally, per section, with hints for the selected engine. */}
      {songs.length > 0 && (
        <Card className="mt-8 p-5">
          <div className="flex items-center justify-between mb-3 gap-2 flex-wrap">
            <div className="flex items-center gap-2">
              <PenLine className="w-4 h-4 text-primary" />
              <span className="font-semibold">Annotate sections</span>
              <span className="text-xs text-muted-foreground">musical structure + per-section imagery</span>
            </div>
            <div className="flex items-center gap-2 flex-wrap">
              <select
                value={annotSongId}
                onChange={(e) => setAnnotSongId(e.target.value)}
                className="bg-background border border-border rounded px-2 py-1.5 text-sm max-w-[16rem]"
                data-testid="annotator-song-select"
              >
                <option value="">Pick a song…</option>
                {songs.map((s) => <option key={s.id} value={s.id}>{s.title || "(untitled)"}</option>)}
              </select>
              <select
                value={annotEngine}
                onChange={(e) => setAnnotEngine(e.target.value)}
                className="bg-background border border-border rounded px-2 py-1.5 text-sm"
                data-testid="annotator-engine-select"
                title="Each engine reads section structure differently"
              >
                <option value="heartmula">HeartMuLa</option>
                <option value="acestep">ACE-Step</option>
                <option value="suno">Suno</option>
              </select>
            </div>
          </div>
          {annotSong ? (
            <SectionAnnotator song={annotSong} engine={annotEngine} saving={annotSaving} onSave={saveAnnotations} />
          ) : (
            <div className="text-sm text-muted-foreground">Pick a song above to edit its sections, structure tags and per-section image ideas.</div>
          )}
        </Card>
      )}

      {songs.length > 0 && (
        <Card className="mt-8 p-5">
          <div className="flex items-center justify-between mb-3 gap-2">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Imported in this project ({songs.length})</div>
            <Button data-testid="lyrics-clear-imported-btn" size="sm" variant="secondary" onClick={clearImportedSongs} title="Delete all imported songs">Clear imported</Button>
          </div>
          <div className="flex flex-wrap gap-2">
            {songs.map(s => <Badge key={s.id} variant="outline" data-testid={`song-chip-${s.id}`}>{s.title}</Badge>)}
          </div>
        </Card>
      )}
    </div>
  );
}
