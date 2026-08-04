import { useState, useRef } from "react";
import { useStyleSamples, sampleSrc, generateSample, deleteSample } from "../lib/styleSamples";
import { toast } from "sonner";
import { Sparkles, Loader2, Play, Pause, ImageIcon, X } from "lucide-react";

// A tiny, self-contained "what does this style look/sound like?" control shown next to a style or
// genre name. If a sample already exists it previews it (image thumbnail, or an audio play toggle);
// otherwise it offers a one-click "generate a sample with your current engine" button.
//
// Props: kind ("image"|"music"), sampleKey (stable id), label (human name), spec (the descriptor
// actually sent to the engine — usually the same string that drives real generation).

export default function StyleSample({ kind, sampleKey, label, spec, size = 40 }) {
  const map = useStyleSamples();
  const src = sampleSrc(map, kind, sampleKey);
  const [busy, setBusy] = useState(false);
  const [playing, setPlaying] = useState(false);
  const audioRef = useRef(null);

  const generate = async (e) => {
    e?.stopPropagation?.();
    setBusy(true);
    try {
      await generateSample(kind, sampleKey, label, spec);
      toast.success(`Sample ready for "${label}".`);
    } catch (err) {
      toast.error(`Couldn't make a sample: ${err}`);
    } finally { setBusy(false); }
  };

  const togglePlay = (e) => {
    e?.stopPropagation?.();
    const a = audioRef.current;
    if (!a) return;
    if (a.paused) { a.play(); setPlaying(true); } else { a.pause(); setPlaying(false); }
  };

  // Discarding a sample is how you get a better one: the next render of this control offers to
  // generate again, on whatever engine is current now. Without it a stale sample is permanent.
  const discard = async (e) => {
    e?.stopPropagation?.();
    e?.preventDefault?.();
    setBusy(true);
    try {
      await deleteSample(kind, sampleKey);
      if (playing) { audioRef.current?.pause(); setPlaying(false); }
    } catch (err) {
      toast.error(`Couldn't remove that sample: ${err}`);
    } finally { setBusy(false); }
  };

  if (src && kind === "image") {
    return (
      <span className="relative shrink-0 group" style={{ width: size, height: size }}>
        <img src={src} alt={`${label} sample`} title={`${label} — sample on your engine`}
          style={{ width: size, height: size }}
          className="rounded object-cover border border-border" />
        <button onClick={discard} disabled={busy} title={`Discard the "${label}" sample and offer a fresh one`}
          className="absolute -top-1 -right-1 rounded-full bg-background border border-border p-0.5 text-muted-foreground
                     opacity-0 group-hover:opacity-100 focus:opacity-100 hover:text-red-400 transition-opacity">
          {busy ? <Loader2 className="w-2.5 h-2.5 animate-spin" /> : <X className="w-2.5 h-2.5" />}
        </button>
      </span>
    );
  }
  if (src && kind === "music") {
    return (
      <>
        <span className="relative shrink-0 group inline-flex">
          <button onClick={togglePlay} title={`Play a ${label} sample`}
            className="rounded-full border border-border p-1.5 hover:bg-muted/50 text-primary">
            {playing ? <Pause className="w-3.5 h-3.5" /> : <Play className="w-3.5 h-3.5" />}
          </button>
          <button onClick={discard} disabled={busy} title={`Discard the "${label}" sample and offer a fresh one`}
            className="absolute -top-1 -right-1 rounded-full bg-background border border-border p-0.5 text-muted-foreground
                       opacity-0 group-hover:opacity-100 focus:opacity-100 hover:text-red-400 transition-opacity">
            {busy ? <Loader2 className="w-2.5 h-2.5 animate-spin" /> : <X className="w-2.5 h-2.5" />}
          </button>
        </span>
        <audio ref={audioRef} src={src} onEnded={() => setPlaying(false)} className="hidden" />
      </>
    );
  }
  // No sample yet.
  return (
    <button onClick={generate} disabled={busy} title={`Generate a ${kind} sample of "${label}" with your current engine`}
      className="shrink-0 rounded border border-dashed border-border/70 text-muted-foreground hover:text-foreground hover:border-primary/60 flex items-center justify-center gap-1 text-[10px] px-1.5 py-1">
      {busy ? <Loader2 className="w-3 h-3 animate-spin" /> : (kind === "music" ? <Sparkles className="w-3 h-3" /> : <ImageIcon className="w-3 h-3" />)}
      {busy ? "…" : "sample"}
    </button>
  );
}
