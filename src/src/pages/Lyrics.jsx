import { useState, useRef } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Textarea } from "../components/ui/textarea";
import { Badge } from "../components/ui/badge";
import { Upload, FileJson, CheckCheck } from "lucide-react";
import { toast } from "sonner";

export default function Lyrics() {
  const { activeProjectId, refreshSongs, songs } = useStudio();
  const [items, setItems] = useState([]);
  const [raw, setRaw] = useState("");
  const fileRef = useRef();

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

  if (!activeProjectId) return <div className="p-8"><Card className="p-10 text-center text-muted-foreground border-dashed">Select a project on the Dashboard first.</Card></div>;

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step 2</div>
      <h1 className="text-4xl sm:text-5xl font-bold mb-2">Lyrics Import</h1>
      <p className="text-muted-foreground mb-8 max-w-2xl">Paste or upload a JSON file holding an array of songs — each with title, language, styles, lyrics, annotations (image prompts) and image_styles.</p>

      <div className="grid lg:grid-cols-2 gap-6">
        <Card className="p-6">
          <div className="flex items-center justify-between mb-3">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">JSON Source</div>
            <input ref={fileRef} type="file" accept=".json,application/json" onChange={onFile} className="hidden" />
            <Button data-testid="lyrics-upload-btn" size="sm" variant="secondary" onClick={()=>fileRef.current?.click()}><Upload className="w-3 h-3 mr-2" />Upload .json</Button>
          </div>
          <Textarea data-testid="lyrics-json-textarea" rows={20} value={raw} onChange={e=>setRaw(e.target.value)} placeholder='[{"title":"...","language":"English","styles":"...","lyrics":"...","annotations":"...","image_styles":"..."}]' className="text-mono text-xs" />
          <div className="flex gap-2 mt-3">
            <Button data-testid="lyrics-parse-btn" onClick={()=>parse(raw)} variant="secondary"><FileJson className="w-4 h-4 mr-2" />Parse</Button>
            <Button data-testid="lyrics-import-btn" onClick={importAll} disabled={!items.length}><CheckCheck className="w-4 h-4 mr-2" />Import {items.length || ""}</Button>
          </div>
        </Card>

        <Card className="p-6">
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3">Preview ({items.length})</div>
          <div className="space-y-3 max-h-[520px] overflow-auto scroll-thin pr-2">
            {items.map((it, idx) => (
              <div key={idx} data-testid={`lyrics-preview-${idx}`} className="border border-border rounded-md p-3 hover:bg-secondary/50">
                <div className="flex items-center justify-between gap-2 mb-2">
                  <div className="font-medium truncate">{it.title}</div>
                  <Badge variant="secondary" className="text-[10px]">{it.language}</Badge>
                </div>
                <div className="text-xs text-muted-foreground italic mb-1">{it.styles}</div>
                <div className="text-xs text-foreground/80 line-clamp-3">{it.lyrics}</div>
              </div>
            ))}
            {!items.length && <div className="text-muted-foreground text-sm">Nothing parsed yet.</div>}
          </div>
        </Card>
      </div>

      {songs.length > 0 && (
        <Card className="mt-8 p-5">
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3">Imported in this project ({songs.length})</div>
          <div className="flex flex-wrap gap-2">
            {songs.map(s => <Badge key={s.id} variant="outline" data-testid={`song-chip-${s.id}`}>{s.title}</Badge>)}
          </div>
        </Card>
      )}
    </div>
  );
}
