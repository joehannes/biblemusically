import { useState, useEffect, useCallback } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import { Badge } from "../components/ui/badge";
import { Switch } from "../components/ui/switch";
import { Textarea } from "../components/ui/textarea";
import { Music2, Wand2, Save, Trash2, Layers, Shuffle, Check, Sparkles } from "lucide-react";
import { toast } from "sonner";

// Rich, mixable genre catalog. Each entry is a Suno/ACE-Step-ready descriptor. Select several across
// categories and the mixer fuses them into one coherent style.
const GENRE_CATALOG = {
  "Playful EDM": {
    "Future Bounce": "future bounce, bright supersaw, bouncy plucks, vocal chops, festival energy, 126 bpm",
    "Electro Swing": "electro swing, vintage brass stabs, cheeky clarinet, four-on-the-floor house, hand claps, 122 bpm",
    "Playful Melodic House": "playful melodic house, sunny chords, catchy topline, warm bass, feel-good, 124 bpm",
    "Tropical Pop-EDM": "tropical house, steel drum plucks, marimba, breezy carefree pop, whistle hook, 108 bpm",
    "Kawaii Future Bass": "kawaii future bass, sparkly bells, pitched vocal chops, cute energy, euphoric drop, 150 bpm",
    "Moombahton Bounce": "moombahton, dembow bounce, big playful synths, party energy, 108 bpm",
    "Disco House": "nu-disco house, funky guitar, groovy bassline, string stabs, joyful dancefloor, 120 bpm",
    "Jersey Club Bounce": "jersey club, bouncy triplet kicks, chopped vocals, playful bed squeaks, 140 bpm",
  },
  "Playful (Acoustic/Retro)": {
    "Funk Pop": "funk pop, slap bass, bright brass, wah guitar, upbeat groove, playful ad-libs, 112 bpm",
    "Ska Punk": "ska punk, upstroke guitar, punchy horns, energetic offbeat, cheeky fun, 165 bpm",
    "Chiptune Dance": "chiptune, 8-bit arpeggios, punchy square leads, arcade energy, cheerful, 128 bpm",
    "Ukulele Pop": "ukulele pop, claps, whistles, sunny acoustic, wholesome playful, 110 bpm",
    "Circus Waltz": "playful circus waltz, oom-pah, glockenspiel, whimsical, mischievous, 3/4",
    "Swing Revival": "big band swing revival, brass section, walking bass, lively jive, 135 bpm",
  },
  "Electronic / EDM": {
    "Liquid DnB": "liquid drum and bass, lush pads, rolling breakbeats, soulful, uplifting, 174 bpm",
    "Melodic Dubstep": "melodic dubstep, emotional supersaws, huge drop, vocal hook, 140 bpm",
    "Future Bass": "future bass, detuned chord stacks, vocal chops, euphoric, 150 bpm",
    "Deep House": "deep house, warm chords, smooth bassline, late-night groove, 122 bpm",
    "Synthwave": "synthwave, retro analog synths, gated drums, neon nostalgia, 100 bpm",
    "Trance": "uplifting trance, supersaw leads, rolling bass, euphoric breakdown, 138 bpm",
    "Lo-Fi House": "lofi house, dusty samples, swung groove, hazy warmth, 118 bpm",
  },
  "Hip-Hop / Bass": {
    "Trap": "trap, booming 808s, crisp hats, dark melodic lead, 140 bpm",
    "Drill": "drill, sliding 808s, sinister melody, aggressive, 145 bpm",
    "Boom Bap": "boom bap, dusty drums, jazzy sample loop, head-nod groove, 90 bpm",
    "Phonk": "phonk, cowbell melody, distorted 808, memphis vocals, 130 bpm",
    "Emo Rap": "emo rap, melancholic guitar loop, raw vocals, deep 808, 80 bpm",
  },
  "Cinematic / Orchestral": {
    "Epic Cinematic": "epic cinematic orchestral, soaring strings, huge brass, big percussion, triumphant, 90 bpm",
    "Trailer": "hybrid trailer music, braams, rising tension, hits, dramatic, 100 bpm",
    "Ambient Score": "ambient film score, evolving pads, delicate piano, emotive, slow",
  },
  "Folk / World": {
    "Alpine Folk": "alpine folk, accordion, yodel accents, festive village energy, 120 bpm",
    "Klezmer": "klezmer, lively clarinet, hora rhythm, celebratory, 130 bpm",
    "Celtic": "celtic folk, fiddle, tin whistle, bodhran, spirited jig, 115 bpm",
    "Afrobeat": "afrobeat, polyrhythmic percussion, horn stabs, groovy guitar, 110 bpm",
    "Reggaeton": "reggaeton, dembow beat, catchy latin hook, club energy, 95 bpm",
    "Bossa Nova": "bossa nova, nylon guitar, soft brushes, warm jazz chords, 75 bpm",
  },
  "Chill / Lo-Fi": {
    "Lo-Fi Hip-Hop": "lofi hip hop, mellow keys, vinyl crackle, relaxed boom bap, 82 bpm",
    "Chillhop": "chillhop, jazzy chords, soft drums, cozy, 85 bpm",
    "Downtempo": "downtempo, dubby chords, deep bass, hazy atmosphere, 95 bpm",
    "Ambient": "ambient, warm drones, gentle textures, meditative, slow",
  },
  "Worship / Sacred": {
    "Gospel": "gospel, soulful choir, hammond organ, uplifting, 100 bpm",
    "Worship Pop": "modern worship pop, anthemic build, bright pads, emotional, 72 bpm",
    "Choir": "sacred choir, layered harmonies, cathedral reverb, majestic, slow",
  },
};

export default function SoundStudio() {
  const { activeProjectId, songs, refreshSongs } = useStudio();

  const [selected, setSelected] = useState({});   // descriptor -> label
  const [mood, setMood] = useState("");
  const [useAi, setUseAi] = useState(true);
  const [mixed, setMixed] = useState("");
  const [mixing, setMixing] = useState(false);

  const [presets, setPresets] = useState([]);
  const [pName, setPName] = useState("");
  const [pPack, setPPack] = useState("Custom");

  const [selSongs, setSelSongs] = useState({});    // song id -> true

  const loadPresets = useCallback(async () => {
    try { const r = await api.listGenrePresets(); setPresets(r?.presets || []); }
    catch (e) { console.error(e); }
  }, []);
  useEffect(() => { loadPresets(); }, [loadPresets]);
  useEffect(() => { refreshSongs && refreshSongs(); }, [refreshSongs]);

  const toggleGenre = (descriptor, label) => {
    setSelected((prev) => {
      const next = { ...prev };
      if (next[descriptor]) delete next[descriptor]; else next[descriptor] = label;
      return next;
    });
  };

  const combine = async () => {
    const genres = Object.keys(selected);
    if (!genres.length) return toast.error("Select at least one genre to mix.");
    setMixing(true);
    try {
      const r = await api.mixGenres({ genres, mood, ai: useAi });
      if (r.error) return toast.error(r.error);
      setMixed(r.styles || "");
      toast.success(`Mixed via ${r.model || "combiner"}.`);
    } catch (e) { console.error(e); toast.error("Mixing failed."); }
    finally { setMixing(false); }
  };

  const savePreset = async () => {
    if (!mixed.trim()) return toast.error("Combine a mix first.");
    if (!pName.trim()) return toast.error("Name the preset.");
    try {
      await api.saveGenrePreset({ name: pName, pack: pPack || "Custom", styles: mixed, genres: Object.values(selected) });
      toast.success("Genre preset saved.");
      setPName("");
      loadPresets();
    } catch (e) { console.error(e); toast.error("Save failed."); }
  };

  const loadPreset = (p) => {
    setMixed(p.styles || "");
    setPName(p.name || "");
    setPPack(p.pack || "Custom");
    toast.success(`Loaded "${p.name}" — edit or apply.`);
  };

  const removePreset = async (p) => {
    if (p.builtin || String(p.id).startsWith("builtin-")) return toast.error("Built-in presets can't be deleted.");
    try { await api.deleteGenrePreset(p.id); loadPresets(); } catch (e) { toast.error("Delete failed."); }
  };

  const applyToSongs = async () => {
    const ids = Object.keys(selSongs).filter((k) => selSongs[k]);
    if (!mixed.trim()) return toast.error("Combine or load a mix first.");
    if (!ids.length) return toast.error("Select songs to apply the mix to.");
    try {
      const r = await api.updateSongStyles(ids, mixed);
      toast.success(`Applied to ${r?.updated ?? ids.length} song(s). Regenerate their music to hear it.`);
      refreshSongs && refreshSongs();
    } catch (e) { console.error(e); toast.error("Apply failed."); }
  };

  const packs = [...new Set(presets.map((p) => p.pack || "Custom"))].sort();
  const selectedCount = Object.keys(selected).length;

  return (
    <div className="p-6 max-w-6xl mx-auto">
      <div className="flex items-center gap-2 mb-1">
        <Music2 className="w-5 h-5 text-primary" />
        <h1 className="text-xl font-semibold">Sound Studio</h1>
      </div>
      <p className="text-muted-foreground mb-6 max-w-3xl">
        Pick and <span className="font-medium">mix any number of genres</span> — including playful and playful-EDM styles —
        and the mixer intelligently fuses them into one coherent, producible style (reconciling tempo and instrumentation).
        Save mixes as presets and apply them to songs before generating.
      </p>

      <div className="grid lg:grid-cols-2 gap-5">
        {/* ── Catalog + mixer ─────────────────────── */}
        <Card className="p-5">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2"><Shuffle className="w-4 h-4 text-primary" /><h2 className="font-semibold">Genre catalog</h2></div>
            <Badge variant="secondary">{selectedCount} selected</Badge>
          </div>
          <div className="space-y-3 max-h-[340px] overflow-auto pr-1">
            {Object.entries(GENRE_CATALOG).map(([cat, items]) => (
              <div key={cat}>
                <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1.5">{cat}</div>
                <div className="flex flex-wrap gap-1.5">
                  {Object.entries(items).map(([label, descriptor]) => {
                    const on = !!selected[descriptor];
                    return (
                      <button
                        key={label}
                        onClick={() => toggleGenre(descriptor, label)}
                        className={`text-xs rounded-full border px-2.5 py-1 transition-colors ${on ? "bg-primary text-primary-foreground border-primary" : "border-border/60 hover:border-primary/60"}`}
                        title={descriptor}
                      >
                        {on && <Check className="w-3 h-3 mr-1 inline" />}{label}
                      </button>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>

          <div className="mt-4 pt-4 border-t border-border/50 space-y-3">
            <div className="space-y-1">
              <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Mood / vibe (optional)</Label>
              <Input value={mood} onChange={(e) => setMood(e.target.value)} placeholder="e.g. playful, sunny, cheeky" />
            </div>
            <div className="flex items-center justify-between">
              <label className="flex items-center gap-2 text-sm"><Sparkles className="w-4 h-4 text-primary" /> Intelligent AI fusion</label>
              <Switch checked={useAi} onCheckedChange={setUseAi} />
            </div>
            <Button onClick={combine} disabled={mixing}><Wand2 className="w-4 h-4 mr-2" />{mixing ? "Mixing…" : "Combine intelligently"}</Button>
          </div>
        </Card>

        {/* ── Result + presets + apply ────────────── */}
        <Card className="p-5 space-y-4">
          <div>
            <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Fused style (editable)</Label>
            <Textarea rows={3} value={mixed} onChange={(e) => setMixed(e.target.value)} placeholder="Your combined genre mix appears here — tweak freely." />
          </div>

          <div className="flex items-end gap-2">
            <div className="space-y-1 flex-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Preset name</Label><Input value={pName} onChange={(e) => setPName(e.target.value)} placeholder="e.g. Sunny Playful Bounce" /></div>
            <div className="space-y-1 w-32"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Pack</Label><Input value={pPack} onChange={(e) => setPPack(e.target.value)} placeholder="Playful" /></div>
            <Button size="sm" variant="secondary" onClick={savePreset}><Save className="w-3 h-3 mr-2" />Save</Button>
          </div>

          <div>
            <div className="flex items-center gap-2 mb-2"><Layers className="w-4 h-4 text-primary" /><span className="text-sm font-semibold">Genre presets</span></div>
            <div className="space-y-3 max-h-[160px] overflow-auto pr-1">
              {packs.map((pack) => (
                <div key={pack}>
                  <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1">{pack}</div>
                  <div className="flex flex-wrap gap-1.5">
                    {presets.filter((p) => (p.pack || "Custom") === pack).map((p) => (
                      <span key={p.id} className="inline-flex items-center gap-1 text-xs rounded-md border border-border/60 pl-2 pr-1 py-1">
                        <button onClick={() => loadPreset(p)} title={p.styles}>{p.name}</button>
                        {!p.builtin && !String(p.id).startsWith("builtin-") && (
                          <button onClick={() => removePreset(p)} className="text-muted-foreground hover:text-red-400"><Trash2 className="w-3 h-3" /></button>
                        )}
                      </span>
                    ))}
                  </div>
                </div>
              ))}
              {!presets.length && <p className="text-xs text-muted-foreground">No presets yet.</p>}
            </div>
          </div>

          <div className="pt-3 border-t border-border/50">
            <div className="text-sm font-semibold mb-2">Apply to songs {activeProjectId ? "" : <span className="text-xs text-muted-foreground">(select a project)</span>}</div>
            <div className="space-y-1 max-h-[140px] overflow-auto pr-1">
              {(songs || []).map((s) => (
                <label key={s.id} className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={!!selSongs[s.id]} onChange={(e) => setSelSongs((p) => ({ ...p, [s.id]: e.target.checked }))} />
                  <span className="truncate flex-1">{s.title} <span className="text-muted-foreground">· {s.language}</span></span>
                </label>
              ))}
              {!(songs || []).length && <p className="text-xs text-muted-foreground">No songs in this project yet.</p>}
            </div>
            <Button size="sm" className="mt-3" onClick={applyToSongs} disabled={!mixed.trim()}><Check className="w-3 h-3 mr-2" />Apply mix to selected songs</Button>
          </div>
        </Card>
      </div>
    </div>
  );
}
