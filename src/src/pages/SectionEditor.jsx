import { useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Badge } from "../components/ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Scissors, Save } from "lucide-react";
import { toast } from "sonner";

export default function SectionEditor() {
  const { songs, activeSongId, selectSong } = useStudio();
  const [sections, setSections] = useState([]);
  const [editingId, setEditingId] = useState(null);
  const [draft, setDraft] = useState({});

  const load = async () => { if (activeSongId) setSections(await api.listSections(activeSongId)); };
  useEffect(() => { load(); }, [activeSongId]);

  const startEdit = (s) => { setEditingId(s.id); setDraft({ start: s.start, end: s.end, mood: s.mood, image_prompt: s.image_prompt, line: s.line }); };
  const save = async () => {
    await api.updateSection(editingId, { ...draft, start: parseFloat(draft.start), end: parseFloat(draft.end) });
    setEditingId(null); toast.success("Section updated"); load();
  };

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step 5</div>
      <h1 className="text-4xl sm:text-5xl font-bold mb-2">Section Editor</h1>
      <p className="text-muted-foreground mb-6 max-w-2xl">Each lyric line → a section: timing, mood, image prompt. Tweak anything — these drive image generation.</p>

      <Card className="p-5 mb-6">
        <Select value={activeSongId || ""} onValueChange={selectSong}>
          <SelectTrigger data-testid="sectionedit-song-select" className="w-80"><SelectValue placeholder="Select song" /></SelectTrigger>
          <SelectContent>{songs.map(s => <SelectItem key={s.id} value={s.id}>{s.title}</SelectItem>)}</SelectContent>
        </Select>
      </Card>

      <div className="space-y-3">
        {sections.map(s => (
          <Card key={s.id} data-testid={`section-row-${s.index}`} className="p-4">
            {editingId === s.id ? (
              <div className="grid md:grid-cols-12 gap-3">
                <Input data-testid={`section-start-${s.index}`} value={draft.start} onChange={e=>setDraft({...draft,start:e.target.value})} className="md:col-span-1 text-mono" placeholder="start" />
                <Input data-testid={`section-end-${s.index}`} value={draft.end} onChange={e=>setDraft({...draft,end:e.target.value})} className="md:col-span-1 text-mono" placeholder="end" />
                <Input value={draft.mood} onChange={e=>setDraft({...draft,mood:e.target.value})} className="md:col-span-2" placeholder="mood" />
                <Input value={draft.line} onChange={e=>setDraft({...draft,line:e.target.value})} className="md:col-span-4" placeholder="lyric line" />
                <Textarea data-testid={`section-prompt-${s.index}`} value={draft.image_prompt} onChange={e=>setDraft({...draft,image_prompt:e.target.value})} rows={2} className="md:col-span-3" placeholder="image prompt" />
                <Button data-testid={`section-save-${s.index}`} onClick={save} className="md:col-span-1"><Save className="w-4 h-4" /></Button>
              </div>
            ) : (
              <div className="grid md:grid-cols-12 gap-3 items-center">
                <div className="md:col-span-2 text-mono text-xs text-muted-foreground">{s.start.toFixed(1)}–{s.end.toFixed(1)}s</div>
                <Badge variant="secondary" className="md:col-span-1 justify-center">{s.mood}</Badge>
                <div className="md:col-span-3 text-sm truncate">{s.line}</div>
                <div className="md:col-span-5 text-xs text-muted-foreground italic line-clamp-2">{s.image_prompt}</div>
                <Button size="sm" variant="secondary" data-testid={`section-edit-${s.index}`} onClick={()=>startEdit(s)} className="md:col-span-1"><Scissors className="w-3 h-3" /></Button>
              </div>
            )}
          </Card>
        ))}
        {!sections.length && <Card className="p-10 text-center text-muted-foreground border-dashed">Run audio analysis first.</Card>}
      </div>
    </div>
  );
}
