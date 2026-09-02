import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Textarea } from "./ui/textarea";
import { Loader2, Wand2, Check, X, PenLine } from "lucide-react";
import { toast } from "sonner";
import { checkSingability } from "../lib/singability";

// ─────────────────────────────────────────────────────────────────────────────
// Rewriting one section, which is what writing actually is.
//
// Every path into a lyric in this app replaced all of it: compose a song, get a song. So the
// ordinary move — this verse is right, that chorus is not — had no button, and the only way to fix
// the chorus was to roll the whole song and lose the verse.
//
// The rewrite sees the entire song and is told to change one section, which is a far easier request
// than writing one: it can match the metre the other verses set, keep the rhyme scheme, and not
// restate what the chorus already says. It comes back with a few genuinely different options,
// because the point of rewriting one section is choosing between versions of it — and each option
// carries the whole lyric with it spliced in, so what you preview is exactly what you apply.
// ─────────────────────────────────────────────────────────────────────────────

export default function SectionRewrite({
  lyrics, projectId, craft, universeId, sourceText, onApply, compact = false,
}) {
  const [sections, setSections] = useState([]);
  const [open, setOpen] = useState(null);
  const [note, setNote] = useState("");
  const [options, setOptions] = useState([]);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    if (!lyrics?.trim()) { setSections([]); return; }
    try {
      const r = await api.lyricSections(lyrics);
      setSections(r?.sections || []);
    } catch { setSections([]); }
  }, [lyrics]);

  useEffect(() => { load(); }, [load]);
  useEffect(() => { setOptions([]); setNote(""); }, [open]);

  const rewrite = async () => {
    if (open === null) return;
    setBusy(true);
    setOptions([]);
    try {
      const r = await api.rewriteSection({
        lyrics, index: open, note, project_id: projectId || "",
        craft: craft || {}, universe_id: universeId || null,
        source_text: sourceText || "", count: 3,
      });
      setOptions(r?.options || []);
    } catch (err) { toast.error(`${err}`, { duration: 9000 }); }
    finally { setBusy(false); }
  };

  const apply = (option) => {
    onApply?.(option.lyrics);
    setOpen(null);
    setOptions([]);
    toast.success("Applied. Everything else in the song is untouched.");
  };

  if (!sections.length) return null;

  return (
    <div className="space-y-2">
      <div className="text-[9px] uppercase tracking-widest text-muted-foreground flex items-center gap-1.5">
        <PenLine className="w-3 h-3" />
        <span>Rewrite one section</span>
      </div>

      <div className="flex flex-wrap gap-1.5">
        {sections.map((s) => (
          <button key={s.index} onClick={() => setOpen(open === s.index ? null : s.index)}
                  className={`text-xs rounded-md border px-2 py-1 transition-all
                              ${open === s.index ? "border-primary/60 bg-primary/10 text-primary"
                                                 : "border-border text-muted-foreground hover:border-primary/40"}`}>
            {/* As a text node, not a string inside the expression: a literal in `{…}` is invisible
                to the extractor and would be paid for at runtime by the translation budget. */}
            {s.header ? <span>{s.header}</span> : <span>opening</span>}
            <span className="text-mono opacity-60 ml-1.5">{s.lines}</span>
          </button>
        ))}
      </div>

      {open !== null && (
        <div className="rounded-lg border border-border p-2.5 space-y-2">
          <pre className="text-[11px] font-mono whitespace-pre-wrap text-muted-foreground max-h-32 overflow-y-auto">
            {sections[open]?.text}
          </pre>
          <Textarea rows={2} value={note} onChange={(e) => setNote(e.target.value)}
                    data-testid="rewrite-note"
                    placeholder="What's wrong with it? Or leave it blank and just see it done differently."
                    className="text-xs" />
          <div className="flex items-center gap-1.5">
            <Button size="sm" onClick={rewrite} disabled={busy}>
              {busy ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                    : <Wand2 className="w-3.5 h-3.5 mr-1.5" />}
              Rewrite it
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setOpen(null)} className="text-muted-foreground">
              <X className="w-3.5 h-3.5" />
            </Button>
          </div>

          {options.map((o, i) => {
            // Measured against the song it is going into, not against itself: a rewrite that scans
            // beautifully on its own and not with the verses around it is the failure mode here.
            const scan = checkSingability(o.lyrics);
            return (
              <div key={i} className="rounded-lg border border-border p-2 space-y-1.5">
                <div className="flex items-start justify-between gap-2">
                  <pre className="text-[11px] font-mono whitespace-pre-wrap flex-1 min-w-0">{o.text}</pre>
                  <Button size="sm" variant="secondary" onClick={() => apply(o)} className="shrink-0">
                    <Check className="w-3.5 h-3.5" />
                  </Button>
                </div>
                <div className="flex items-center gap-2 flex-wrap">
                  {o.what_changed && (
                    <span className="text-[10px] text-muted-foreground">{o.what_changed}</span>
                  )}
                  {!scan.ok && !scan.uneven && (
                    <Badge variant="outline" className="text-[9px] text-amber-600 dark:text-amber-400">
                      {/* No parenthetical: the extractor reads a leading "word(s)" as an unbalanced
                          bracket and skips the string, which would put it on the runtime budget. */}
                      <span>{scan.outliers.length}</span>
                      <span className="ml-1">lines outside the song's metre</span>
                    </Badge>
                  )}
                </div>
              </div>
            );
          })}

          {!compact && !options.length && !busy && (
            <p className="text-[10px] text-muted-foreground">
              The rest of the song goes along as context, so a rewrite can match the metre and the
              rhymes the other sections already set. Nothing outside this section changes.
            </p>
          )}
        </div>
      )}
    </div>
  );
}
