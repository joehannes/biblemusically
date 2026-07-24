import { useState, useEffect, useRef } from "react";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Textarea } from "./ui/textarea";
import { Label } from "./ui/label";
import { Badge } from "./ui/badge";
import { toast } from "sonner";
import { Compass, Loader2, Save, Sparkles, ChevronDown, ChevronUp, BookOpenCheck } from "lucide-react";

// ─────────────────────────────────────────────────────────────────────────────
// Project Brief — the project's creative DNA, editable on the Dashboard.
//
// One structured place for who this project is FOR and how it should FEEL: mood, attitude,
// motivation, aims & goals, audience, storyline, humor, personality — plus a rotating daily topic.
// Stored on the project doc (`brief: {…}`) and read by every generation path: lyric composition,
// per-channel style adaptation, and character/avatar prompts — so a change made here shifts the
// whole output, not just one page.
//
// Saving a materially changed brief re-runs the per-channel background research pass, which caches
// regional/cultural/linguistic notes per channel for the composers to draw on.
// ─────────────────────────────────────────────────────────────────────────────

export const BRIEF_FIELDS = [
  { key: "mood", label: "Mood", ph: "e.g. hopeful, reverent, warm with moments of awe", rows: 1 },
  { key: "attitude", label: "Attitude", ph: "e.g. encouraging, never preachy; speaks WITH the listener", rows: 1 },
  { key: "motivation", label: "Motivation", ph: "why this project exists — what drives it", rows: 1 },
  { key: "goals", label: "Aims & goals", ph: "what success looks like — reach, impact, growth", rows: 2 },
  { key: "audience", label: "Audience", ph: "who it speaks to — age, region, faith background, mood when they arrive", rows: 2 },
  { key: "storyline", label: "Storyline", ph: "the arc across songs/videos — where this is heading", rows: 2 },
  { key: "humor", label: "Humor", ph: "how much and what kind — dry, playful, none…", rows: 1 },
  { key: "personality", label: "Personality", ph: "the channel's voice as if it were a person", rows: 2 },
];

/** Compact single-string rendering of a brief, used by prompts and change-detection. */
export const briefSummary = (b) =>
  BRIEF_FIELDS.map(({ key, label }) => (b?.[key]?.trim() ? `${label}: ${b[key].trim()}` : null))
    .filter(Boolean)
    .join(" · ");

export default function ProjectBrief({ project, onSaved }) {
  const [open, setOpen] = useState(true);
  const [brief, setBrief] = useState({});
  const [dailyTopic, setDailyTopic] = useState("");
  const [saving, setSaving] = useState(false);
  const [researching, setResearching] = useState(false);
  const savedSummaryRef = useRef("");

  useEffect(() => {
    const b = project?.brief || {};
    setBrief(b);
    setDailyTopic(project?.daily_topic || "");
    savedSummaryRef.current = briefSummary(b);
  }, [project?.id]);

  if (!project) return null;

  const set = (key, val) => setBrief((b) => ({ ...b, [key]: val }));

  const save = async () => {
    setSaving(true);
    try {
      await api.updateProject(project.id, {
        brief,
        daily_topic: dailyTopic.trim(),
        daily_topic_date: dailyTopic.trim() ? new Date().toISOString().slice(0, 10) : "",
      });
      toast.success("Project brief saved — it now steers lyrics, styles and characters.");
      // A materially different brief invalidates the cached per-channel research; refresh it in the
      // background so the next generation already speaks with the new voice.
      const now = briefSummary(brief);
      if (now !== savedSummaryRef.current && now.trim()) {
        savedSummaryRef.current = now;
        runResearch(true);
      }
      onSaved?.();
    } catch (e) {
      toast.error(`Save failed: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  const runResearch = async (silent = false) => {
    setResearching(true);
    try {
      const r = await api.researchProjectChannels(project.id);
      if (!silent) {
        if (r?.researched > 0) toast.success(`Refreshed research for ${r.researched} channel(s).`);
        else toast.info(r?.detail || "No channels to research yet — add channels first.");
      }
    } catch (e) {
      if (!silent) toast.error(`Research failed: ${e}`);
    } finally {
      setResearching(false);
    }
  };

  const filled = BRIEF_FIELDS.filter((f) => brief[f.key]?.trim()).length;

  return (
    <Card className="p-5 mb-6">
      <button className="w-full flex items-center justify-between gap-2" onClick={() => setOpen((o) => !o)}>
        <div className="flex items-center gap-2">
          <Compass className="w-4 h-4 text-primary" />
          <h2 className="font-semibold">Project brief</h2>
          <Badge variant="secondary" className="text-[10px]">{filled}/{BRIEF_FIELDS.length} set</Badge>
          {dailyTopic.trim() && <Badge variant="outline" className="text-[10px]">today: {dailyTopic.trim().slice(0, 32)}</Badge>}
        </div>
        {open ? <ChevronUp className="w-4 h-4 text-muted-foreground" /> : <ChevronDown className="w-4 h-4 text-muted-foreground" />}
      </button>

      {open && (
        <div className="mt-4 space-y-4">
          <p className="text-xs text-muted-foreground max-w-3xl">
            The creative DNA of <b className="text-foreground">{project.name}</b>. Everything generated —
            lyrics, per-channel musical styles, translations, characters — is steered by what you write here,
            adapted per channel's region, culture and language.
          </p>

          <div className="grid sm:grid-cols-2 gap-3">
            {BRIEF_FIELDS.map(({ key, label, ph, rows }) => (
              <div key={key} className="space-y-1">
                <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">{label}</Label>
                {rows > 1 ? (
                  <Textarea rows={rows} value={brief[key] || ""} onChange={(e) => set(key, e.target.value)}
                    placeholder={ph} className="text-sm" data-testid={`brief-${key}`} />
                ) : (
                  <Input value={brief[key] || ""} onChange={(e) => set(key, e.target.value)}
                    placeholder={ph} className="text-sm" data-testid={`brief-${key}`} />
                )}
              </div>
            ))}
          </div>

          <div className="space-y-1">
            <Label className="text-[10px] uppercase tracking-widest text-muted-foreground flex items-center gap-1">
              <Sparkles className="w-3 h-3" />Daily topic (today's angle — feeds every generation today)
            </Label>
            <Input value={dailyTopic} onChange={(e) => setDailyTopic(e.target.value)}
              placeholder="e.g. gratitude in hard seasons; Psalm 23 as shelter" className="text-sm" data-testid="brief-daily-topic" />
          </div>

          <div className="flex items-center gap-2 flex-wrap">
            <Button size="sm" onClick={save} disabled={saving} data-testid="brief-save">
              {saving ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" /> : <Save className="w-3.5 h-3.5 mr-1.5" />}
              Save brief
            </Button>
            <Button size="sm" variant="secondary" onClick={() => runResearch(false)} disabled={researching}
              title="AI researches each channel's region, culture, language and audience against this brief, and caches the notes for the composers"
              data-testid="brief-research">
              {researching ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" /> : <BookOpenCheck className="w-3.5 h-3.5 mr-1.5" />}
              Research channels now
            </Button>
          </div>
        </div>
      )}
    </Card>
  );
}
