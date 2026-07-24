import { useState } from "react";
import { Card } from "./ui/card";
import { Badge } from "./ui/badge";
import StyleSample from "./StyleSample";
import { useStyleSamples } from "../lib/styleSamples";
import { Palette, Music2, ChevronDown, ChevronUp, FlaskConical } from "lucide-react";

// Central place to generate example samples per art style / music genre, using the CURRENT engine.
// Each row has a StyleSample control (generate on demand → preview). Samples are saved globally and
// then appear next to the same styles inside Style Studio / Sound Studio automatically.

const IMAGE_STYLES = [
  { key: "style:photoreal", label: "Photoreal", spec: "ultra realistic photograph, natural lighting, sharp focus, 85mm, a serene landscape with a lone figure at golden hour" },
  { key: "style:comic", label: "Comic", spec: "comic book style, bold ink outlines, halftone shading, vibrant flat colors, a heroic figure on a hilltop" },
  { key: "style:graphic_novel", label: "Graphic novel", spec: "graphic novel illustration, moody cinematic lighting, painterly ink, muted palette, a lone figure at golden hour" },
  { key: "style:anime", label: "Anime", spec: "anime key visual, clean cel shading, expressive, studio quality, a figure under a wide sky" },
  { key: "style:oil_painting", label: "Oil painting", spec: "classical oil painting, visible brushstrokes, impasto texture, chiaroscuro, a serene pastoral scene" },
  { key: "style:watercolor", label: "Watercolor", spec: "delicate watercolor, soft washes, paper texture, gentle gradients, a quiet landscape" },
];

const MUSIC_GENRES = [
  { key: "genre:Epic Cinematic", label: "Epic Cinematic", spec: "epic cinematic orchestral, soaring strings, huge brass, triumphant, 90 bpm" },
  { key: "genre:Gospel", label: "Gospel", spec: "gospel, soulful choir, hammond organ, uplifting, 100 bpm" },
  { key: "genre:Worship Pop", label: "Worship Pop", spec: "modern worship pop, anthemic build, bright pads, emotional, 72 bpm" },
  { key: "genre:Future Bass", label: "Future Bass", spec: "future bass, detuned chord stacks, vocal chops, euphoric, 150 bpm" },
  { key: "genre:Lo-Fi Hip-Hop", label: "Lo-Fi Hip-Hop", spec: "lofi hip hop, mellow keys, vinyl crackle, relaxed boom bap, 82 bpm" },
  { key: "genre:Reggaeton", label: "Reggaeton", spec: "reggaeton, dembow beat, catchy latin hook, club energy, 95 bpm" },
  { key: "genre:Klezmer", label: "Klezmer", spec: "klezmer, lively clarinet, hora rhythm, celebratory, 130 bpm" },
];

export default function StyleSampleStudio() {
  const [open, setOpen] = useState(false);
  const map = useStyleSamples();
  // Keys are used verbatim so a sample made here matches the same style in Sound/Style Studio
  // (e.g. music "genre:Future Bass" is shared with the Sound Studio genre chip).
  const have = (rows, kind) => rows.filter((r) => map?.[`${kind}:${r.key}`]).length;

  return (
    <Card className="p-5 mb-6">
      <button className="w-full flex items-center justify-between gap-2" onClick={() => setOpen((o) => !o)}>
        <div className="flex items-center gap-2">
          <FlaskConical className="w-4 h-4 text-primary" />
          <h2 className="font-semibold">Style sample studio</h2>
          <Badge variant="secondary" className="text-[10px]">{have(IMAGE_STYLES, "image")} img · {have(MUSIC_GENRES, "music")} audio</Badge>
        </div>
        {open ? <ChevronUp className="w-4 h-4 text-muted-foreground" /> : <ChevronDown className="w-4 h-4 text-muted-foreground" />}
      </button>

      {open && (
        <div className="mt-4 space-y-5">
          <p className="text-xs text-muted-foreground max-w-3xl">
            Generate a quick example for any style or genre <b className="text-foreground">using your current engine</b> — to see what an art
            direction or genre-mix actually looks or sounds like. Samples are saved once and then appear next to these
            styles in Style Studio and Sound Studio. (Needs the relevant engine connected.)
          </p>

          <div>
            <div className="flex items-center gap-2 mb-2 text-sm font-medium"><Palette className="w-4 h-4 text-primary" />Image styles</div>
            <div className="flex flex-wrap gap-3">
              {IMAGE_STYLES.map((s) => (
                <div key={s.key} className="flex flex-col items-center gap-1 w-20 text-center">
                  <StyleSample kind="image" sampleKey={s.key} label={s.label} spec={s.spec} size={64} />
                  <span className="text-[10px] text-muted-foreground truncate w-full">{s.label}</span>
                </div>
              ))}
            </div>
          </div>

          <div>
            <div className="flex items-center gap-2 mb-2 text-sm font-medium"><Music2 className="w-4 h-4 text-primary" />Music genres</div>
            <div className="flex flex-wrap gap-2">
              {MUSIC_GENRES.map((g) => (
                <div key={g.key} className="flex items-center gap-1.5 rounded-full border border-border/60 pl-2 pr-1 py-0.5">
                  <span className="text-xs">{g.label}</span>
                  <StyleSample kind="music" sampleKey={g.key} label={g.label} spec={g.spec} />
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </Card>
  );
}
