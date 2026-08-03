import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Palette, Loader2, Blend, Check, Eye } from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// Choosing a look, including one that does not exist yet.
//
// Three ways in, in increasing order of effort: a named mix (someone already decided these go
// together), a single style, or two styles the user blends themselves. The named mixes come first
// because they are the ones most likely to be right — a blend assembled from two random families
// usually produces mud, and the curated list is where that judgement lives.
//
// The resolved prompt is shown rather than hidden. A style *is* a prompt fragment; treating it as an
// opaque token means a disappointing picture gives you nothing to reason about, whereas seeing
// "photograph, photo of a painting" sitting in the negative explains immediately why the watercolour
// came out looking like a photo of a watercolour once that line was removed.
// ─────────────────────────────────────────────────────────────────────────────

export default function StylePicker({ value, onChange }) {
  const [cat, setCat] = useState(null);
  const [preview, setPreview] = useState(null);
  const [blend, setBlend] = useState([]);

  useEffect(() => {
    api.imageStyleCatalogue().then(setCat).catch((e) =>
      toast.error(`Could not load the style catalogue: ${e?.message || e}`));
  }, []);

  useEffect(() => {
    if (!value) return setPreview(null);
    let alive = true;
    api.resolveImageStyle(value).then((r) => { if (alive) setPreview(r?.ok ? r : null); }).catch(() => {});
    return () => { alive = false; };
  }, [value]);

  if (!cat) {
    return <div className="flex items-center gap-2 text-xs text-muted-foreground">
      <Loader2 className="w-3.5 h-3.5 animate-spin" />Loading styles…
    </div>;
  }

  const toggleBlend = (id) => {
    setBlend((prev) => {
      // Two is the limit on purpose: three styles at equal weight reliably average into mud, and the
      // curated mixes exist for the combinations worth having beyond that.
      const next = prev.includes(id) ? prev.filter((x) => x !== id)
                 : prev.length >= 2 ? [prev[1], id] : [...prev, id];
      if (next.length === 2) onChange?.(next.join("+"));
      return next;
    });
  };

  return (
    <div className="space-y-4">
      {/* ── Curated mixes ─────────────────────────────────────────────── */}
      <div className="space-y-2">
        <h3 className="text-xs font-medium flex items-center gap-1.5">
          <Blend className="w-3.5 h-3.5 text-primary" />Mixes
          <span className="text-muted-foreground font-normal">— combinations that work</span>
        </h3>
        <div className="grid sm:grid-cols-2 gap-2">
          {cat.mixes.map((m) => (
            <button key={m.id} onClick={() => { setBlend([]); onChange?.(m.id); }}
              className={`text-left rounded-lg border p-2.5 space-y-1 transition-colors ${
                value === m.id ? "border-primary bg-primary/10" : "border-border hover:bg-accent"}`}>
              <div className="flex items-center gap-1.5 flex-wrap">
                <span className="text-sm font-medium">{m.label}</span>
                {value === m.id && <Check className="w-3.5 h-3.5 text-primary" />}
              </div>
              <div className="text-[11px] text-muted-foreground">{m.note}</div>
              <div className="flex gap-1 flex-wrap pt-0.5">
                {m.parts.map((p) => (
                  <Badge key={p.style} variant="outline" className="text-[9px]">
                    {p.label} ×{p.weight}
                  </Badge>
                ))}
              </div>
            </button>
          ))}
        </div>
      </div>

      {/* ── Single styles, and ad-hoc blending ────────────────────────── */}
      <div className="space-y-2">
        <h3 className="text-xs font-medium flex items-center gap-1.5">
          <Palette className="w-3.5 h-3.5 text-primary" />Styles
          <span className="text-muted-foreground font-normal">
            — click one to use it, or pick two to blend
          </span>
        </h3>
        <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-2">
          {cat.styles.map((s) => {
            const inBlend = blend.includes(s.id);
            return (
              <div key={s.id}
                className={`rounded-lg border p-2.5 space-y-1 ${
                  value === s.id ? "border-primary bg-primary/10"
                  : inBlend ? "border-primary/60 bg-primary/5" : "border-border"}`}>
                <div className="flex items-center justify-between gap-2">
                  <span className="text-sm font-medium">{s.label}</span>
                  <Badge variant="outline" className="text-[9px]">{s.family}</Badge>
                </div>
                <div className="text-[11px] text-muted-foreground">{s.note}</div>
                <div className="flex gap-1 pt-1">
                  <Button size="sm" variant="ghost" className="h-6 text-[10px] px-2"
                          onClick={() => { setBlend([]); onChange?.(s.id); }}>
                    Use
                  </Button>
                  <Button size="sm" variant={inBlend ? "default" : "ghost"} className="h-6 text-[10px] px-2"
                          onClick={() => toggleBlend(s.id)}>
                    {inBlend ? "Blending" : "Blend"}
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
        {blend.length === 1 && (
          <p className="text-[11px] text-muted-foreground">
            Pick one more to blend with <b>{cat.styles.find((s) => s.id === blend[0])?.label}</b>.
          </p>
        )}
      </div>

      {/* ── What that actually sends ──────────────────────────────────── */}
      {preview && (
        <Card className="p-3 space-y-2">
          <div className="flex items-center gap-1.5 text-xs font-medium">
            <Eye className="w-3.5 h-3.5 text-primary" />What this sends
            <code className="text-[10px] text-muted-foreground" data-no-i18n>{value}</code>
          </div>
          <div className="space-y-1.5 text-[11px]">
            <div>
              <div className="text-[9px] uppercase tracking-widest text-muted-foreground">Prompt</div>
              <div className="font-mono leading-relaxed break-words" data-no-i18n>{preview.prompt}</div>
            </div>
            <div>
              <div className="text-[9px] uppercase tracking-widest text-muted-foreground">Negative</div>
              <div className="font-mono leading-relaxed break-words text-muted-foreground" data-no-i18n>
                {preview.negative}
              </div>
            </div>
          </div>
        </Card>
      )}
    </div>
  );
}
