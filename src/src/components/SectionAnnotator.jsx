import { useState, useEffect, useMemo } from "react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Textarea } from "./ui/textarea";
import { Label } from "./ui/label";
import { Badge } from "./ui/badge";
import { Music2, Image as ImageIcon, Info, Save, Wand2, Loader2 } from "lucide-react";

// ─────────────────────────────────────────────────────────────────────────────
// Per-section annotation editor.
//
// Authoring annotations used to mean hand-writing a JSON blob: `lyrics` as one string with
// [section] headers, and `annotations` as alternating "[image prompt]" / lyric-line pairs. This
// edits the same two fields structurally instead — split the lyrics into sections, give each one
// an image idea (and, where the engine supports it, a performance hint), then rebuild both strings.
//
// It is engine-aware because the lyric text reaches the music engine VERBATIM and each engine
// parses section structure differently (mirrors engine_lyric_annotation_guide in ai.rs).
// ─────────────────────────────────────────────────────────────────────────────

export const ENGINE_ANNOTATION = {
  suno: {
    label: "Suno",
    headerCase: "Title",
    suggested: ["Intro", "Verse", "Pre-Chorus", "Chorus", "Bridge", "Outro"],
    allowsPerformance: true,
    hint: "Capitalised tags. Suno also honours short performance hints like [Instrumental] or [Soft female vocal] — use them sparingly.",
  },
  acestep: {
    label: "ACE-Step",
    headerCase: "lower",
    suggested: ["intro", "verse", "chorus", "bridge", "outro"],
    allowsPerformance: false,
    hint: "Plain lowercase structure tags only — ACE-Step does not interpret performance directions inside the lyric text.",
  },
  heartmula: {
    label: "HeartMuLa",
    headerCase: "Title",
    suggested: ["Verse", "Chorus", "Bridge", "Outro"],
    allowsPerformance: false,
    hint: "Bare capitalised section headers only. The text is written verbatim to a lyrics file — anything that isn't a header gets sung.",
  },
};

const titleCase = (s) => s.replace(/\w\S*/g, (w) => w[0].toUpperCase() + w.slice(1).toLowerCase());

/** Split a lyrics string into [{ header, lines[] }]. Text before any header becomes an untitled section. */
export function parseSections(lyrics) {
  const out = [];
  let cur = { header: "", lines: [] };
  for (const raw of String(lyrics || "").split("\n")) {
    const m = raw.match(/^\s*\[([^\]]+)\]\s*$/);
    if (m) {
      if (cur.header || cur.lines.length) out.push(cur);
      cur = { header: m[1].trim(), lines: [] };
    } else {
      cur.lines.push(raw);
    }
  }
  if (cur.header || cur.lines.length) out.push(cur);
  return out.length ? out : [{ header: "Verse", lines: [""] }];
}

/** Rebuild the lyrics string using the target engine's header dialect. */
export function buildLyrics(sections, engine) {
  const cfg = ENGINE_ANNOTATION[engine] || ENGINE_ANNOTATION.heartmula;
  return sections
    .map((s) => {
      const head = (s.header || "").trim();
      if (!head) return s.lines.join("\n").trim();
      const cased = cfg.headerCase === "lower" ? head.toLowerCase() : titleCase(head);
      const perf = cfg.allowsPerformance && s.performance?.trim() ? `\n[${s.performance.trim()}]` : "";
      return `[${cased}]${perf}\n${s.lines.join("\n").trim()}`;
    })
    .join("\n\n")
    .trim();
}

/**
 * Rebuild the `annotations` string: alternating lines — a bracketed image prompt, then the lyric
 * line it belongs to. Every non-empty line in a section inherits that section's image idea, which
 * is what gives each section its own visual identity in the finished video.
 */
export function buildAnnotations(sections) {
  const out = [];
  for (const s of sections) {
    const idea = (s.image || "").trim();
    if (!idea) continue;
    for (const line of s.lines) {
      if (!line.trim()) continue;
      out.push(`[${idea}]`);
      out.push(line.trim());
    }
  }
  return out.join("\n");
}

export default function SectionAnnotator({ song, engine = "heartmula", onSave, saving = false }) {
  const cfg = ENGINE_ANNOTATION[engine] || ENGINE_ANNOTATION.heartmula;
  const [sections, setSections] = useState([]);

  // Seed from the song, folding any existing annotations back onto their sections so a second
  // pass edits what's there instead of starting blank.
  useEffect(() => {
    const parsed = parseSections(song?.lyrics);
    const existing = String(song?.annotations || "");
    const firstIdea = existing.match(/^\s*\[([^\]]+)\]/m)?.[1] || "";
    setSections(parsed.map((s, i) => ({ ...s, image: i === 0 ? firstIdea : "", performance: "" })));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [song?.id]);

  const setSection = (i, patch) =>
    setSections((arr) => arr.map((s, idx) => (idx === i ? { ...s, ...patch } : s)));

  const preview = useMemo(() => buildLyrics(sections, engine), [sections, engine]);
  const annotated = useMemo(() => sections.filter((s) => s.image?.trim()).length, [sections]);

  const save = () =>
    onSave?.({
      lyrics: buildLyrics(sections, engine),
      annotations: buildAnnotations(sections),
    });

  if (!song) {
    return <div className="text-sm text-muted-foreground">Pick a song to annotate.</div>;
  }

  return (
    <div className="space-y-4">
      <div className="flex items-start gap-2 rounded-md border border-border bg-muted/30 p-2.5 text-xs">
        <Info className="w-3.5 h-3.5 text-primary shrink-0 mt-0.5" />
        <div>
          <span className="font-medium">{cfg.label} format: </span>
          <span className="text-muted-foreground">{cfg.hint}</span>
        </div>
      </div>

      <div className="flex items-center justify-between">
        <div className="text-xs text-muted-foreground">
          {sections.length} section{sections.length === 1 ? "" : "s"} · {annotated} with an image idea
        </div>
        <Button size="sm" onClick={save} disabled={saving} data-testid="annotator-save">
          {saving ? <Loader2 className="w-3 h-3 mr-1.5 animate-spin" /> : <Save className="w-3 h-3 mr-1.5" />}
          Save annotations
        </Button>
      </div>

      <div className="space-y-3 max-h-[520px] overflow-y-auto scroll-thin pr-1">
        {sections.map((s, i) => (
          <div key={i} className="rounded-lg border border-border/70 p-3 space-y-2">
            <div className="flex items-center gap-2 flex-wrap">
              <Music2 className="w-3.5 h-3.5 text-primary shrink-0" />
              <Input
                value={s.header}
                onChange={(e) => setSection(i, { header: e.target.value })}
                placeholder="Section (e.g. Verse)"
                className="h-7 text-xs max-w-[12rem]"
                data-testid={`annotator-header-${i}`}
              />
              <div className="flex gap-1 flex-wrap">
                {cfg.suggested.map((tag) => (
                  <button key={tag} onClick={() => setSection(i, { header: tag })}
                    className="text-[10px] px-1.5 py-0.5 rounded bg-muted hover:bg-muted/70 text-muted-foreground">
                    {tag}
                  </button>
                ))}
              </div>
            </div>

            <div className="text-[11px] text-muted-foreground whitespace-pre-wrap line-clamp-3 pl-5">
              {s.lines.join("\n").trim() || <span className="italic">(no lines)</span>}
            </div>

            <div className="space-y-1 pl-5">
              <Label className="text-[10px] uppercase tracking-widest text-muted-foreground flex items-center gap-1">
                <ImageIcon className="w-3 h-3" />Image idea for this section
              </Label>
              <Input
                value={s.image || ""}
                onChange={(e) => setSection(i, { image: e.target.value })}
                placeholder="e.g. dawn over a stone chapel, volumetric light"
                className="h-7 text-xs"
                data-testid={`annotator-image-${i}`}
              />
            </div>

            {cfg.allowsPerformance && (
              <div className="space-y-1 pl-5">
                <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">
                  Performance hint (optional, {cfg.label} only)
                </Label>
                <Input
                  value={s.performance || ""}
                  onChange={(e) => setSection(i, { performance: e.target.value })}
                  placeholder="e.g. Soft female vocal"
                  className="h-7 text-xs"
                />
              </div>
            )}
          </div>
        ))}
      </div>

      <div className="space-y-1">
        <Label className="text-[10px] uppercase tracking-widest text-muted-foreground flex items-center gap-1">
          <Wand2 className="w-3 h-3" />Lyrics as {cfg.label} will receive them
          <Badge variant="secondary" className="ml-1 text-[9px]">preview</Badge>
        </Label>
        <Textarea value={preview} readOnly rows={6} className="font-mono text-[11px]" />
      </div>
    </div>
  );
}
