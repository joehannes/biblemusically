import { useState, useRef, useEffect, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { usePageActions } from "../lib/pageActions";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import { Textarea } from "../components/ui/textarea";
import { Badge } from "../components/ui/badge";
import { Switch } from "../components/ui/switch";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Sparkles, Mic, MicOff, Plus, Trash2, Wand2, Import, Globe2, Palette, Music2, Image as ImageIcon, ArrowRight } from "lucide-react";
import { toast } from "sonner";

// Web Speech API — supported in Chromium (which Tauri's webview uses on Linux/Windows).
function getRecognition() {
  const SR = typeof window !== "undefined" && (window.SpeechRecognition || window.webkitSpeechRecognition);
  if (!SR) return null;
  const rec = new SR();
  rec.continuous = true;
  rec.interimResults = false;
  rec.lang = "en-US";
  return rec;
}

const DEFAULT_TARGETS = [
  { language: "English", genre: "cinematic pop" },
];

export default function FreeformComposer() {
  const nav = useNavigate();
  const { activeProjectId, projects, selectProject, refreshSongs } = useStudio();

  const [topic, setTopic] = useState("");
  const [moodDriver, setMoodDriver] = useState("");
  const [cultural, setCultural] = useState(true);
  const [targets, setTargets] = useState(DEFAULT_TARGETS);
  const [titlePattern, setTitlePattern] = useState("{topic} — {genre}");
  const [artist, setArtist] = useState("");
  const [imageSuffix, setImageSuffix] = useState("cinematic lighting, highly detailed --ar 16:9");

  const [busy, setBusy] = useState(false);
  const [items, setItems] = useState([]);
  const [channels, setChannels] = useState([]);
  const [listening, setListening] = useState(false);
  const recRef = useRef(null);
  const micSupported = useRef(false);

  useEffect(() => {
    micSupported.current = !!getRecognition();
    return () => { try { recRef.current && recRef.current.stop(); } catch (_) {} };
  }, []);

  // Channels drive multi-version cultural adaptation (one target per channel).
  useEffect(() => {
    (async () => {
      try {
        const r = await api.listChannels();
        setChannels(Array.isArray(r) ? r : r?.channels || []);
      } catch { setChannels([]); }
    })();
  }, [activeProjectId]);

  const toggleMic = () => {
    if (listening) {
      try { recRef.current && recRef.current.stop(); } catch (_) {}
      setListening(false);
      return;
    }
    const rec = getRecognition();
    if (!rec) {
      toast.error("Speech recognition isn't available in this webview. Type the topic instead.");
      return;
    }
    recRef.current = rec;
    rec.onresult = (e) => {
      let chunk = "";
      for (let i = e.resultIndex; i < e.results.length; i++) {
        if (e.results[i].isFinal) chunk += e.results[i][0].transcript;
      }
      if (chunk) setTopic((prev) => (prev ? prev.trim() + " " : "") + chunk.trim());
    };
    rec.onerror = (e) => { toast.error(`Mic error: ${e.error || "unknown"}`); setListening(false); };
    rec.onend = () => setListening(false);
    try { rec.start(); setListening(true); toast.success("Listening… describe your topic."); }
    catch (_) { setListening(false); }
  };

  const setTarget = (i, patch) => setTargets((t) => t.map((row, idx) => (idx === i ? { ...row, ...patch } : row)));
  const addTarget = () => setTargets((t) => [...t, { language: "English", genre: "" }]);

  // Build one target per channel in the project so cultural adaptation produces a distinct version
  // for each audience. Passing the channel's name/region/styles through gives the AI the context it
  // needs to adapt language, genre and imagery per channel rather than producing one generic song.
  const targetsFromChannels = () => {
    if (!channels.length) return toast.error("No channels in this project yet — add them in Channel Manager.");
    const next = channels.map((c) => ({
      language: c.language || "English",
      genre: (c.styles || "").trim(),
      channel: c.name || "",
      region: c.region || "",
    }));
    setTargets(next);
    toast.success(`Loaded ${next.length} target(s) from your channels.`);
  };
  const removeTarget = (i) => setTargets((t) => t.filter((_, idx) => idx !== i));

  const generate = async () => {
    if (!topic.trim()) return toast.error("Describe a topic first (type it or use the mic).");
    setBusy(true);
    setItems([]);
    try {
      const r = await api.composeFreeform({
        topic,
        mood_driver: moodDriver,
        cultural_influence: cultural,
        targets: targets.filter((t) => t.language?.trim()),
        title_pattern: titlePattern,
        artist,
        image_style_suffix: imageSuffix,
        sections: [],
      });
      if (r.error) {
        toast.error(r.error);
        if (r.raw) console.warn("Freeform raw response:", r.raw);
        return;
      }
      if (!r.items?.length) return toast.error("The AI returned no songs. Try again or refine the topic.");
      setItems(r.items);
      toast.success(`Generated ${r.items.length} song(s) via ${r.model || "AI"}.`);
    } catch (err) {
      console.error(err);
      toast.error("Generation failed. See console.");
    } finally {
      setBusy(false);
    }
  };

  // Hand this freeform material to the AI Composer as a *source*, the same way Bible Sources does,
  // so non-scripture projects can drive the composer's section/lyric workflow instead of only
  // importing finished songs.
  const sendToComposer = () => {
    const body = items.length
      ? items.map((it) => {
          const lyr = typeof it.lyrics === "string" ? it.lyrics : "";
          return `## ${it.title || "Untitled"}${it.language ? ` (${it.language})` : ""}\n${lyr}`.trim();
        }).join("\n\n")
      : "";
    const text = [
      topic.trim() && `Topic: ${topic.trim()}`,
      moodDriver.trim() && `Mood: ${moodDriver.trim()}`,
      body,
    ].filter(Boolean).join("\n\n");
    if (!text.trim()) return toast.error("Describe a topic (or generate songs) first.");
    localStorage.setItem("studio:composer-source", JSON.stringify({
      reference: topic.trim() ? `Freeform — ${topic.trim().slice(0, 60)}` : "Freeform material",
      text,
      language: targets[0]?.language || "English",
    }));
    toast.success("Sent to the AI Composer as a source.");
    nav("/composer");
  };

  const importAll = async () => {
    if (!activeProjectId) return toast.error("Select a project first.");
    if (!items.length) return toast.error("Generate songs first.");
    try {
      const res = await api.importLyrics(activeProjectId, items);
      toast.success(`Imported ${res?.created?.length ?? items.length} song(s) into the project.`);
      refreshSongs && refreshSongs();
    } catch (err) {
      console.error(err);
      toast.error("Import failed. See console.");
    }
  };

  usePageActions(useMemo(() => (
    <>
      <Button size="sm" onClick={generate} disabled={busy} data-testid="freeform-generate">
        <Wand2 className="w-4 h-4 mr-2" />{busy ? "Composing…" : "Compose songs"}
      </Button>
      <Button size="sm" variant="secondary" onClick={sendToComposer} data-testid="freeform-to-composer"
        title="Use this material as the AI Composer's source, like a Bible chapter">
        <ArrowRight className="w-4 h-4 mr-2" />To AI Composer
      </Button>
      <Button size="sm" variant="secondary" onClick={importAll} disabled={!items.length} data-testid="freeform-import">
        <Import className="w-4 h-4 mr-2" />Import {items.length ? `(${items.length})` : ""}
      </Button>
    </>
    // eslint-disable-next-line react-hooks/exhaustive-deps
  ), [busy, items.length, topic, moodDriver, targets]));

  return (
    <div className="p-4 sm:p-6 max-w-6xl mx-auto">
      <div className="flex items-center gap-2 mb-1">
        <Sparkles className="w-5 h-5 text-primary" />
        <h1 className="text-4xl sm:text-5xl font-bold">Freeform Composer</h1>
        <Badge variant="secondary" className="ml-1">non-Bible</Badge>
      </div>
      <p className="text-muted-foreground mb-6 max-w-3xl">
        Roughly describe any topic (type or speak it), set today's mood/vibe, and let AI write
        culture- and genre-adapted songs per target — each ultimately its own channel. The cultural
        adaptation is baked into the styles and image prompts so it steers music and image generation.
      </p>

      <div className="grid lg:grid-cols-2 gap-5">
        {/* ── Left: inputs ───────────────────────────── */}
        <Card className="p-5 space-y-4">
          <div className="space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground flex items-center justify-between">
              <span>Topic</span>
              <button
                type="button"
                onClick={toggleMic}
                className={`inline-flex items-center gap-1 rounded px-2 py-0.5 text-[11px] ${listening ? "bg-red-500/20 text-red-400" : "bg-muted text-muted-foreground hover:text-foreground"}`}
                data-testid="freeform-mic"
              >
                {listening ? <MicOff className="w-3 h-3" /> : <Mic className="w-3 h-3" />}
                {listening ? "Stop" : "Speak"}
              </button>
            </Label>
            <Textarea
              rows={4}
              value={topic}
              onChange={(e) => setTopic(e.target.value)}
              placeholder="e.g. A hopeful anthem about starting over after a hard year, with a wink of humor…"
              data-testid="freeform-topic"
            />
          </div>

          <div className="space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Daily mood / vibe driver</Label>
            <Input
              value={moodDriver}
              onChange={(e) => setMoodDriver(e.target.value)}
              placeholder="e.g. bittersweet but playful; end-of-summer restlessness"
              data-testid="freeform-mood"
            />
            <p className="text-xs text-muted-foreground">How the theme should vibe right now — shade of humor, wellbeing, state of mind.</p>
          </div>

          <label className="flex items-center justify-between gap-3 rounded-md border border-border bg-muted/20 px-3 py-2">
            <span className="flex items-center gap-2 text-sm font-medium"><Globe2 className="w-4 h-4 text-primary" /> Cultural influence</span>
            <Switch checked={cultural} onCheckedChange={setCultural} data-testid="freeform-cultural" />
          </label>
          <p className="text-xs text-muted-foreground -mt-2">When on, each translation + genre is adapted to its regional/genre culture so it speaks directly to that audience.</p>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Title pattern</Label>
              <Input value={titlePattern} onChange={(e) => setTitlePattern(e.target.value)} placeholder="{topic} — {genre}" />
            </div>
            <div className="space-y-1">
              <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Artist (optional)</Label>
              <Input value={artist} onChange={(e) => setArtist(e.target.value)} placeholder="channel / artist name" />
            </div>
          </div>

          <div className="space-y-1">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Image style suffix</Label>
            <Input value={imageSuffix} onChange={(e) => setImageSuffix(e.target.value)} placeholder="cinematic lighting, --ar 16:9" />
          </div>
        </Card>

        {/* ── Right: targets + project + actions ─────── */}
        <Card className="p-5 space-y-4">
          <div className="flex items-center justify-between">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Targets (language × genre → channel)</Label>
            <div className="flex gap-1.5">
              <Button size="sm" variant="secondary" onClick={targetsFromChannels} disabled={!channels.length}
                title="One target per channel in this project — each gets its own culturally adapted version">
                <Globe2 className="w-3 h-3 mr-1" />From channels{channels.length ? ` (${channels.length})` : ""}
              </Button>
              <Button size="sm" variant="secondary" onClick={addTarget}><Plus className="w-3 h-3 mr-1" />Add</Button>
            </div>
          </div>
          <div className="space-y-2">
            {targets.map((t, i) => (
              <div key={i} className="flex items-center gap-2">
                <Input
                  className="flex-1"
                  value={t.language}
                  onChange={(e) => setTarget(i, { language: e.target.value })}
                  placeholder="Language (e.g. Spanish)"
                />
                <Input
                  className="flex-1"
                  value={t.genre}
                  onChange={(e) => setTarget(i, { genre: e.target.value })}
                  placeholder="Genre (e.g. reggaeton)"
                />
                <button type="button" onClick={() => removeTarget(i)} className="text-muted-foreground hover:text-red-400 p-1" title="Remove">
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
            ))}
            {!targets.length && <p className="text-xs text-muted-foreground">Add at least one target.</p>}
          </div>

          <div className="space-y-1 pt-2">
            <Label className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Import into project</Label>
            <Select value={activeProjectId || ""} onValueChange={(v) => selectProject && selectProject(v)}>
              <SelectTrigger className="w-full"><SelectValue placeholder="Select a project…" /></SelectTrigger>
              <SelectContent>
                {projects.map((p) => (
                  <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

        </Card>
      </div>

      {/* ── Results ───────────────────────────────── */}
      {items.length > 0 && (
        <div className="mt-6 space-y-4">
          <h2 className="font-semibold text-sm text-muted-foreground uppercase tracking-widest">Generated songs & intel</h2>
          {items.map((it, i) => (
            <Card key={i} className="p-5 space-y-3">
              <div className="flex items-center gap-2 flex-wrap">
                <span className="font-semibold">{it.title || "Untitled"}</span>
                <Badge variant="secondary">{it.language || "?"}</Badge>
                {it.styles && <Badge variant="outline" className="flex items-center gap-1"><Music2 className="w-3 h-3" />{it.styles}</Badge>}
              </div>

              {it.cultural_influence && (
                <div className="text-sm flex gap-2">
                  <Globe2 className="w-4 h-4 text-primary shrink-0 mt-0.5" />
                  <span><span className="font-medium">Cultural adaptation: </span>{it.cultural_influence}</span>
                </div>
              )}
              {it.mood_context && (
                <div className="text-sm flex gap-2">
                  <Sparkles className="w-4 h-4 text-primary shrink-0 mt-0.5" />
                  <span><span className="font-medium">Mood: </span>{it.mood_context}</span>
                </div>
              )}
              {it.image_intel && (
                <div className="text-sm flex gap-2">
                  <ImageIcon className="w-4 h-4 text-primary shrink-0 mt-0.5" />
                  <span><span className="font-medium">Image direction: </span>{it.image_intel}</span>
                </div>
              )}

              {Array.isArray(it.section_intel) && it.section_intel.length > 0 && (
                <div className="rounded-md border border-border/60 divide-y divide-border/40">
                  {it.section_intel.map((s, j) => (
                    <div key={j} className="px-3 py-2 text-sm">
                      <div className="flex items-center gap-2">
                        <Palette className="w-3.5 h-3.5 text-muted-foreground" />
                        <span className="font-medium">{s.section || `Section ${j + 1}`}</span>
                      </div>
                      {s.note && <div className="text-muted-foreground text-xs mt-0.5">{s.note}</div>}
                      {s.image_intel && <div className="text-xs mt-0.5"><span className="text-muted-foreground">image: </span>{s.image_intel}</div>}
                    </div>
                  ))}
                </div>
              )}

              {it.lyrics && (
                <details className="text-sm">
                  <summary className="cursor-pointer text-muted-foreground hover:text-foreground">Lyrics</summary>
                  <pre className="whitespace-pre-wrap mt-2 text-xs bg-muted/30 rounded p-3 max-h-64 overflow-auto">{typeof it.lyrics === "string" ? it.lyrics : JSON.stringify(it.lyrics, null, 2)}</pre>
                </details>
              )}
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
