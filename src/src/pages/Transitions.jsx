import { useState, useEffect, useCallback } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Label } from "../components/ui/label";
import { Badge } from "../components/ui/badge";
import { Input } from "../components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Clapperboard, Wand2, Save, Sparkles, Info, PackagePlus, Trash2 } from "lucide-react";
import { toast } from "sonner";

const AUTOMATION = [
  ["manual", "Manual — one default for all (you edit)"],
  ["assist", "Assist — rule-based per scene (mood/pacing)"],
  ["full", "Full — AI picks each transition"],
];

// Superset of transitions the compositor accepts; per-scene dropdown falls back to this when a
// pack is narrower.
const ALL_TRANSITIONS = ["fade","fadeblack","fadewhite","dissolve","distance","slideleft","slideright","slideup","slidedown","wipeleft","wiperight","wipeup","wipedown","smoothleft","smoothright","smoothup","smoothdown","circleopen","circleclose","radial","pixelize","zoomin","hlslice","vuslice","diagtl","diagtr"];

export default function Transitions() {
  const { activeProjectId, songs, refreshSongs } = useStudio();
  const [songId, setSongId] = useState("");
  const [sections, setSections] = useState([]);
  const [automation, setAutomation] = useState("assist");
  const [packs, setPacks] = useState([]);
  const [packId, setPackId] = useState("");
  const [busy, setBusy] = useState(false);
  // A pack you tuned by hand was previously unkeepable: the backend has had save/delete since the
  // packs were introduced, and only the buttons were missing. Named here rather than prompt()ed
  // because the name is the only field a custom pack has that the sections cannot supply.
  const [newPackName, setNewPackName] = useState("");

  useEffect(() => { refreshSongs && refreshSongs(); }, [refreshSongs]);

  const loadPacks = useCallback(async (preferId) => {
    try {
      const r = await api.listTransitionPresets();
      const ps = r?.presets || [];
      setPacks(ps);
      setPackId((cur) => preferId || (ps.some((p) => p.id === cur) ? cur : ps[0]?.id || ""));
    } catch (e) { console.error(e); }
  }, []);
  useEffect(() => { loadPacks(); }, [loadPacks]);

  const loadSections = useCallback(async (sid) => {
    if (!sid) return setSections([]);
    try {
      const secs = await api.listSections(sid);
      const arr = (secs || []).slice().sort((a, b) => (a.index ?? 0) - (b.index ?? 0));
      setSections(arr);
    } catch (e) { console.error(e); setSections([]); }
  }, []);

  const selectSong = (sid) => { setSongId(sid); loadSections(sid); };
  const pack = packs.find((p) => p.id === packId);
  const vocab = pack?.transitions?.length ? pack.transitions : ALL_TRANSITIONS;

  const suggest = async () => {
    if (!songId) return toast.error("Select a song first.");
    setBusy(true);
    try {
      const r = await api.suggestTransitions({
        song_id: songId,
        automation,
        allowed: pack?.transitions || [],
        default_transition: pack?.default || "fade",
      });
      if (r.error) return toast.error(r.error);
      toast.success(`${automation === "full" ? "AI" : automation === "assist" ? "Rules" : "Default"} set ${r.count} transition(s) via ${r.model}.`);
      await loadSections(songId); // reflect persisted transitions
    } catch (e) { console.error(e); toast.error("Suggestion failed."); }
    finally { setBusy(false); }
  };

  const setSecTransition = (id, transition) =>
    setSections((prev) => prev.map((s) => (s.id === id ? { ...s, transition } : s)));

  const save = async () => {
    if (!sections.length) return;
    try {
      await api.setSectionTransitions(sections.map((s) => ({ id: s.id, transition: s.transition || "fade" })));
      toast.success("Per-scene transitions saved.");
    } catch (e) { console.error(e); toast.error("Save failed."); }
  };

  // What a custom pack is made of: the transitions this song actually ended up using, with the
  // commonest as the default. That is the shape a pack has, and it is exactly what somebody who
  // just hand-tuned twenty scenes wants to keep — rather than asking them to re-pick a vocabulary
  // they have already expressed by choosing.
  const packFromSections = () => {
    const used = sections.map((s) => s.transition).filter(Boolean);
    if (!used.length) return null;
    const counts = used.reduce((acc, t) => ({ ...acc, [t]: (acc[t] || 0) + 1 }), {});
    const transitions = Object.keys(counts).sort((a, b) => counts[b] - counts[a]);
    return { transitions, default: transitions[0] };
  };

  const saveAsPack = async () => {
    const name = newPackName.trim();
    if (!name) return toast.error("Give the pack a name first.");
    const built = packFromSections();
    if (!built) return toast.error("Set some per-scene transitions first — a pack is made from what you used.");
    try {
      const r = await api.saveTransitionPreset({ name, ...built });
      setNewPackName("");
      await loadPacks(r?.id);
      toast.success(`Saved “${name}” — ${built.transitions.length} transition(s), default ${built.default}.`);
    } catch (e) { console.error(e); toast.error(`Could not save the pack: ${e?.message || e}`); }
  };

  const deletePack = async () => {
    if (!pack || pack.builtin) return;
    if (!window.confirm(`Delete the pack “${pack.name}”? This cannot be undone.`)) return;
    try {
      await api.deleteTransitionPreset(pack.id);
      await loadPacks();
      toast.success(`Deleted “${pack.name}”.`);
    } catch (e) { console.error(e); toast.error(`Could not delete the pack: ${e?.message || e}`); }
  };

  return (
    <div className="p-4 sm:p-6 max-w-5xl mx-auto">
      <div className="flex items-center gap-2 mb-1">
        <Clapperboard className="w-5 h-5 text-primary" />
        <h1 className="text-4xl sm:text-5xl font-bold">Transitions &amp; FX</h1>
      </div>
      <p className="text-muted-foreground mb-4 max-w-3xl">
        Choose how much AI drives the scene-to-scene transitions, pick a preset pack, then let it suggest
        fitting transitions from each scene's mood and pacing (from audio analysis) — or hand-tune every
        one. Enable crossfades in <b>Settings → Video composition</b> for them to render.
      </p>

      <div className="flex items-center gap-2 text-xs text-muted-foreground mb-5 rounded-md border border-border/60 bg-muted/20 px-3 py-2">
        <Info className="w-3.5 h-3.5 shrink-0" /> Transitions apply when you compose the video with scene transitions turned on.
      </div>

      <Card className="p-5 mb-5">
        <div className="grid md:grid-cols-3 gap-4">
          <div className="space-y-1">
            <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Song</Label>
            <Select value={songId} onValueChange={selectSong}>
              <SelectTrigger className="w-full"><SelectValue placeholder={activeProjectId ? "Select a song…" : "Select a project first"} /></SelectTrigger>
              <SelectContent>{(songs || []).map((s) => <SelectItem key={s.id} value={s.id}>{s.title} · {s.language}</SelectItem>)}</SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Automation level</Label>
            <Select value={automation} onValueChange={setAutomation}>
              <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>{AUTOMATION.map(([v, l]) => <SelectItem key={v} value={v}>{l}</SelectItem>)}</SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Preset pack</Label>
            <div className="flex gap-1.5">
              <Select value={packId} onValueChange={setPackId}>
                <SelectTrigger className="w-full"><SelectValue placeholder="Pick a pack…" /></SelectTrigger>
                <SelectContent>{packs.map((p) => <SelectItem key={p.id} value={p.id}>{p.name}{p.builtin ? " (built-in)" : ""}</SelectItem>)}</SelectContent>
              </Select>
              {/* Built-ins are refused by the backend too — disabled here so the refusal is visible
                  before it is a toast. */}
              <Button variant="outline" size="icon" className="shrink-0" onClick={deletePack}
                      disabled={!pack || pack.builtin}
                      title={pack?.builtin ? "Built-in packs cannot be deleted" : "Delete this pack"}>
                <Trash2 className="w-4 h-4" />
              </Button>
            </div>
          </div>
        </div>
        <div className="flex flex-wrap gap-2 mt-4">
          <Button onClick={suggest} disabled={busy || !songId}>
            {automation === "full" ? <Sparkles className="w-4 h-4 mr-2" /> : <Wand2 className="w-4 h-4 mr-2" />}
            {busy ? "Working…" : automation === "full" ? "AI suggest transitions" : automation === "assist" ? "Auto-set (rules)" : "Apply default to all"}
          </Button>
          <Button variant="secondary" onClick={save} disabled={!sections.length}><Save className="w-4 h-4 mr-2" />Save edits</Button>
        </div>
        <div className="flex flex-wrap items-center gap-2 mt-3 pt-3 border-t border-border/50">
          <Input value={newPackName} onChange={(e) => setNewPackName(e.target.value)}
                 placeholder="Name a pack from the scenes below…" className="h-8 w-64 text-sm" />
          <Button variant="outline" size="sm" className="h-8" onClick={saveAsPack}
                  disabled={!sections.length || !newPackName.trim()}>
            <PackagePlus className="w-3.5 h-3.5 mr-1.5" />Save as pack
          </Button>
          <span className="text-[11px] text-muted-foreground">
            Keeps the transitions this song ended up using, commonest as the default.
          </span>
        </div>
        {pack?.transitions?.length ? (
          <div className="mt-3 text-xs text-muted-foreground">Pack vocabulary: {pack.transitions.join(", ")} · default <Badge variant="secondary">{pack.default}</Badge></div>
        ) : null}
      </Card>

      {sections.length > 0 && (
        <Card className="p-5">
          <div className="text-sm font-semibold mb-3">Per-scene transitions ({sections.length})</div>
          <div className="space-y-1.5 max-h-[420px] overflow-auto pr-1">
            {sections.map((s, i) => (
              <div key={s.id} className="flex items-center gap-3 rounded-md border border-border/50 px-3 py-2">
                <span className="text-xs text-muted-foreground w-6 shrink-0">{i}</span>
                <div className="flex-1 min-w-0">
                  <div className="text-sm truncate">{s.image_prompt || s.line || "—"}</div>
                  <div className="text-[11px] text-muted-foreground">mood: {s.mood || "?"}{s.mood_prev ? ` (from ${s.mood_prev})` : ""}</div>
                </div>
                <div className="w-44 shrink-0">
                  <Select value={s.transition || "fade"} onValueChange={(v) => setSecTransition(s.id, v)}>
                    <SelectTrigger className="w-full h-8"><SelectValue /></SelectTrigger>
                    <SelectContent>{vocab.map((t) => <SelectItem key={t} value={t}>{t}</SelectItem>)}</SelectContent>
                  </Select>
                </div>
              </div>
            ))}
          </div>
        </Card>
      )}
    </div>
  );
}
