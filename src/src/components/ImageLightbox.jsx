import { useState, useEffect, useCallback } from "react";
import { Button } from "./ui/button";
import {
  X, ChevronLeft, ChevronRight, Columns2, Square, Sparkles, Loader2,
  Star, Trash2, Download,
} from "lucide-react";

// ─────────────────────────────────────────────────────────────────────────────
// Full-screen image explorer for experimenting before a project is flowing:
//   • big preview of any variant,
//   • SIDE-BY-SIDE compare (pick a second variant to hold against the first),
//   • one-click "generate an alternate", set-primary, discard, download,
//   • a thumbnail filmstrip of every variant.
//
// Purely presentational — the parent owns the variant list and the actions, so this drops into the
// section-image grid and the character gallery alike.
// ─────────────────────────────────────────────────────────────────────────────

export default function ImageLightbox({
  open, title, images = [], startIndex = 0, busy = false,
  onClose, onGenerateAlternate, onSetPrimary, onDiscard, primaryIndex = 0,
}) {
  const [idx, setIdx] = useState(startIndex);
  const [compare, setCompare] = useState(false);
  const [cmpIdx, setCmpIdx] = useState(startIndex);

  useEffect(() => { if (open) { setIdx(Math.min(startIndex, Math.max(0, images.length - 1))); } }, [open, startIndex, images.length]);

  const go = useCallback((d) => {
    setIdx((i) => (images.length ? (i + d + images.length) % images.length : 0));
  }, [images.length]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e) => {
      if (e.key === "Escape") onClose?.();
      else if (e.key === "ArrowRight") go(1);
      else if (e.key === "ArrowLeft") go(-1);
      else if (e.key.toLowerCase() === "c") setCompare((c) => !c);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, go, onClose]);

  if (!open) return null;
  const cur = images[idx];
  const cmp = images[cmpIdx];

  const download = (url) => {
    const a = document.createElement("a");
    a.href = url; a.download = ""; a.target = "_blank"; a.rel = "noreferrer";
    document.body.appendChild(a); a.click(); a.remove();
  };

  return (
    <div className="fixed inset-0 z-[80] bg-background/95 backdrop-blur-sm flex flex-col" data-no-i18n>
      {/* header */}
      <div className="flex items-center justify-between gap-3 px-4 h-14 border-b border-border shrink-0">
        <div className="flex items-center gap-2 min-w-0">
          <span className="font-semibold truncate">{title || "Preview"}</span>
          <span className="text-xs text-muted-foreground shrink-0">{images.length ? `${idx + 1}/${images.length}` : "empty"}</span>
        </div>
        <div className="flex items-center gap-1.5 flex-wrap justify-end">
          <Button size="sm" variant={compare ? "default" : "outline"} className="h-8" onClick={() => setCompare((c) => !c)}
            title="Compare two variants side by side (press C)">
            {compare ? <Square className="w-3.5 h-3.5 mr-1.5" /> : <Columns2 className="w-3.5 h-3.5 mr-1.5" />}
            {compare ? "Single" : "Compare"}
          </Button>
          {onGenerateAlternate && (
            <Button size="sm" className="h-8" onClick={onGenerateAlternate} disabled={busy}
              title="Generate another alternate version">
              {busy ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" /> : <Sparkles className="w-3.5 h-3.5 mr-1.5" />}
              Alternate
            </Button>
          )}
          <button onClick={onClose} className="p-2 rounded hover:bg-muted" aria-label="Close"><X className="w-4 h-4" /></button>
        </div>
      </div>

      {/* stage */}
      <div className="flex-1 min-h-0 relative flex items-center justify-center p-4 gap-4">
        {images.length > 1 && !compare && (
          <button onClick={() => go(-1)} className="absolute left-3 z-10 p-2 rounded-full bg-card/80 border border-border hover:bg-muted" aria-label="Previous"><ChevronLeft className="w-5 h-5" /></button>
        )}

        {compare ? (
          <div className="grid grid-cols-2 gap-3 w-full h-full">
            {[[cur, idx, setIdx], [cmp, cmpIdx, setCmpIdx]].map(([url, i], side) => (
              <div key={side} className="flex flex-col min-h-0">
                <div className="flex-1 min-h-0 rounded-lg border border-border bg-muted/20 overflow-hidden flex items-center justify-center">
                  {url ? <img src={url} alt="" className="max-w-full max-h-full object-contain" /> : <span className="text-xs text-muted-foreground">—</span>}
                </div>
                <div className="text-center text-[11px] text-muted-foreground mt-1">variant {i + 1}{i === primaryIndex ? " · primary" : ""} · {side === 0 ? "left" : "right"}</div>
              </div>
            ))}
          </div>
        ) : (
          <div className="max-w-full max-h-full flex items-center justify-center">
            {cur ? <img src={cur} alt="" className="max-w-full max-h-[calc(100vh-11rem)] object-contain rounded-lg" /> : <span className="text-muted-foreground">No image yet — generate one.</span>}
          </div>
        )}

        {images.length > 1 && !compare && (
          <button onClick={() => go(1)} className="absolute right-3 z-10 p-2 rounded-full bg-card/80 border border-border hover:bg-muted" aria-label="Next"><ChevronRight className="w-5 h-5" /></button>
        )}
      </div>

      {/* per-variant actions */}
      {cur && (
        <div className="flex items-center justify-center gap-2 px-4 pb-2 flex-wrap">
          {onSetPrimary && (
            <Button size="sm" variant={idx === primaryIndex ? "default" : "outline"} className="h-8" onClick={() => onSetPrimary(idx)}>
              <Star className="w-3.5 h-3.5 mr-1.5" />{idx === primaryIndex ? "Primary" : "Set primary"}
            </Button>
          )}
          <Button size="sm" variant="outline" className="h-8" onClick={() => download(cur)}><Download className="w-3.5 h-3.5 mr-1.5" />Download</Button>
          {onDiscard && images.length > 1 && (
            <Button size="sm" variant="ghost" className="h-8 text-destructive hover:text-destructive" onClick={() => { onDiscard(idx); setIdx((i) => Math.max(0, i - 1)); }}>
              <Trash2 className="w-3.5 h-3.5 mr-1.5" />Discard
            </Button>
          )}
        </div>
      )}

      {/* filmstrip */}
      {images.length > 1 && (
        <div className="shrink-0 border-t border-border px-3 py-2 overflow-x-auto scroll-thin">
          <div className="flex gap-2">
            {images.map((url, i) => (
              <button key={i}
                onClick={() => (compare ? setCmpIdx(i) : setIdx(i))}
                className={`relative w-16 h-16 rounded border-2 overflow-hidden shrink-0 transition-colors ${
                  (compare ? cmpIdx : idx) === i ? "border-primary" : "border-transparent hover:border-border"}`}>
                <img src={url} alt="" className="w-full h-full object-cover" />
                {i === primaryIndex && <Star className="w-3 h-3 text-primary absolute top-0.5 right-0.5 fill-primary" />}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
