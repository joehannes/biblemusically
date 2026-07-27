import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";
import { Type, ShieldAlert, Download } from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// Words on products, set in a real font.
//
// The preview is the actual SVG the backend produced, not an approximation — so what you see is
// literally what gets rasterised. That matters more than it sounds: the whole reason this exists
// rather than asking an image model for lettering is that a generated word is *occasionally* wrong,
// and on a mug that is a returned order rather than a retry.
//
// The refusals are the other half. Embroidery cannot hold a hairline serif or nine colours, and this
// panel says so before an order exists rather than after a customer complains.
// ─────────────────────────────────────────────────────────────────────────────

const DECORATIONS = [
  { id: "dtg", label: "Direct-to-garment (most shirts)" },
  { id: "dtf", label: "Direct-to-film (the most forgiving)" },
  { id: "embroidery", label: "Embroidery (thread — the fussiest)" },
  { id: "sublimation", label: "Mug or hard surface" },
];

export default function TypographyStudio({ initialPhrase = "", reference = "", onRendered }) {
  const [cat, setCat] = useState(null);
  const [phrase, setPhrase] = useState(initialPhrase);
  const [ref, setRef] = useState(reference);
  const [face, setFace] = useState("inter");
  const [layout, setLayout] = useState("verse_and_reference");
  const [decoration, setDecoration] = useState("dtg");
  const [colours, setColours] = useState(2);
  const [svg, setSvg] = useState("");
  const [fit, setFit] = useState(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => { api.typographyCatalogue().then(setCat).catch(() => {}); }, []);

  // Check first, draw second. Drawing something that cannot be printed and reporting it afterwards
  // teaches people to ignore the answer.
  useEffect(() => {
    if (!phrase.trim()) { setFit(null); return; }
    let live = true;
    api.typographyFit(phrase, face, decoration, Number(colours) || 1, 4000)
      .then((r) => { if (live) setFit(r); })
      .catch(() => {});
    return () => { live = false; };
  }, [phrase, face, decoration, colours]);

  useEffect(() => {
    if (!phrase.trim() || fit?.refused) { setSvg(""); return; }
    let live = true;
    api.renderTextArt({ phrase, reference: ref, layout, face, width: 4000, height: 4000 })
      .then((r) => { if (live) setSvg(r.svg || ""); })
      .catch(() => { if (live) setSvg(""); });
    return () => { live = false; };
  }, [phrase, ref, layout, face, fit?.refused]);

  const chosenFace = useMemo(() => cat?.faces?.find((f) => f.id === face), [cat, face]);
  if (!cat) return null;

  return (
    <Card className="p-5 space-y-4">
      <div className="flex items-center gap-2">
        <Type className="w-4 h-4 text-primary" />
        <h2 className="font-semibold">The words</h2>
        <Badge variant="outline" className="text-[9px]">set in a real font, never generated</Badge>
      </div>

      <div className="grid md:grid-cols-2 gap-3">
        <div className="space-y-1">
          <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Phrase</Label>
          <Input value={phrase} onChange={(e) => setPhrase(e.target.value)}
                 data-testid="typography-phrase" placeholder="Be still and know" />
        </div>
        <div className="space-y-1">
          <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Reference</Label>
          <Input value={ref} onChange={(e) => setRef(e.target.value)}
                 data-testid="typography-reference" placeholder="Psalm 46:10" />
        </div>
      </div>

      <div className="grid sm:grid-cols-2 lg:grid-cols-4 gap-3">
        <div className="space-y-1">
          <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Font</Label>
          <Select value={face} onValueChange={setFace}>
            <SelectTrigger data-testid="typography-face"><SelectValue /></SelectTrigger>
            <SelectContent>
              {cat.faces.map((f) => <SelectItem key={f.id} value={f.id}>{f.family}</SelectItem>)}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Layout</Label>
          <Select value={layout} onValueChange={setLayout}>
            <SelectTrigger data-testid="typography-layout"><SelectValue /></SelectTrigger>
            <SelectContent>
              {cat.layouts.map((l) => <SelectItem key={l.id} value={l.id}>{l.label}</SelectItem>)}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Printed how</Label>
          <Select value={decoration} onValueChange={setDecoration}>
            <SelectTrigger data-testid="typography-decoration"><SelectValue /></SelectTrigger>
            <SelectContent>
              {DECORATIONS.map((d) => <SelectItem key={d.id} value={d.id}>{d.label}</SelectItem>)}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Colours</Label>
          <Input type="number" min={1} max={64} value={colours}
                 data-testid="typography-colours"
                 onChange={(e) => setColours(e.target.value)} />
        </div>
      </div>

      {chosenFace && (
        <p className="text-[11px] text-muted-foreground">
          {chosenFace.use_for} <span className="opacity-70">Licensed {chosenFace.licence}, which
          permits commercial use including print.</span>
        </p>
      )}

      {/* A refusal, not a warning. At fifty products a day nobody reads a warning. */}
      {fit?.refused && (
        <p className="text-xs text-destructive flex items-start gap-1.5">
          <ShieldAlert className="w-4 h-4 shrink-0 mt-0.5" /><span>{fit.reason}</span>
        </p>
      )}
      {fit?.ok && (
        <p className="text-[11px] text-muted-foreground">
          {fit.words} words, strokes about {fit.stroke_px}px. This process holds up to {fit.colour_cap} colours
          and wants {fit.min_dpi} DPI.
        </p>
      )}

      {/* The preview is the real SVG, so what is shown is exactly what gets rasterised. */}
      {svg && (
        <div className="rounded-lg border border-border/60 bg-[repeating-conic-gradient(#0000_0%_25%,#8883_0%_50%)] bg-[length:16px_16px] p-4 flex justify-center">
          <div className="max-w-[320px] w-full" dangerouslySetInnerHTML={{ __html: svg }} />
        </div>
      )}

      <div className="flex gap-2">
        <Button size="sm" disabled={!svg || busy || fit?.refused}
                data-testid="typography-render"
                onClick={async () => {
                  setBusy(true);
                  try {
                    const r = await api.renderTextArt({
                      phrase, reference: ref, layout, face, width: 4000, height: 4000,
                      raster: true, decoration, colours: Number(colours) || 1,
                    });
                    onRendered?.(r);
                    toast.success(r.png_path ? "Rendered to a transparent PNG at 300 DPI."
                                             : "SVG saved. " + (r.raster_error || ""));
                  } catch (err) { toast.error(String(err), { duration: 10000 }); }
                  finally { setBusy(false); }
                }}>
          <Download className="w-3.5 h-3.5 mr-1.5" />Render for print
        </Button>
      </div>

      <p className="text-[10px] text-muted-foreground">{cat.licensing}</p>
    </Card>
  );
}
