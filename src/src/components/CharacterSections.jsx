import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { toast } from "sonner";
import { Users, Loader2, Link2, Unlink } from "lucide-react";

/**
 * Attach a character to the sections it appears in, and push its visual identity into those
 * sections' image prompts.
 *
 * "Consistent characters" used to stop at the portrait: the appearance tags kept the character's
 * own image stable, but getting that same face into the song's scenes meant copying the prompt into
 * every section by hand. This closes that gap — pick the sections, press apply.
 */
export default function CharacterSections({ character, songId, onApplied }) {
  const [open, setOpen] = useState(false);
  const [sections, setSections] = useState([]);
  const [selected, setSelected] = useState(() => new Set());
  const [mode, setMode] = useState("prepend");
  const [busy, setBusy] = useState(false);
  const [linkedCount, setLinkedCount] = useState(0);

  const load = useCallback(async () => {
    if (!songId) return;
    try {
      const res = await api.characterSectionLinks(songId);
      setSections(res?.sections || []);
      const mine = (res?.by_character || {})[character.id] || [];
      setLinkedCount(mine.length);
      setSelected(new Set(mine.map((s) => s.id)));
    } catch {
      setSections([]);
    }
  }, [songId, character.id]);

  useEffect(() => { load(); }, [load]);

  if (!songId) return null;

  const toggle = (id) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  const apply = async (detach = false) => {
    const ids = [...selected];
    if (ids.length === 0) return toast.error("Select at least one section");
    setBusy(true);
    try {
      const res = detach
        ? await api.detachCharacterFromSections(character.id, ids)
        : await api.applyCharacterToSections(character.id, ids, mode);
      toast.success(
        detach
          ? `Removed from ${res?.updated ?? 0} section(s)`
          : `Applied to ${res?.updated ?? 0} section(s)${res?.already_applied ? `, ${res.already_applied} already had it` : ""}`
      );
      await load();
      onApplied?.();
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mt-2">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
        data-testid={`character-sections-toggle-${character.id}`}
      >
        <Users className="h-3.5 w-3.5" />
        Appears in sections
        {linkedCount > 0 && <Badge variant="secondary" className="ml-1 h-4 px-1.5 text-[10px]">{linkedCount}</Badge>}
      </button>

      {open && (
        <div className="mt-2 rounded border border-border bg-muted/30 p-2 space-y-2">
          <div className="max-h-40 overflow-auto space-y-0.5">
            {sections.length === 0 && (
              <p className="text-xs text-muted-foreground">This song has no sections yet — run Analysis first.</p>
            )}
            {sections.map((s) => (
              <label key={s.id} className="flex items-start gap-2 text-xs cursor-pointer hover:bg-muted/50 rounded px-1 py-0.5">
                <input
                  type="checkbox"
                  className="accent-primary mt-0.5"
                  checked={selected.has(s.id)}
                  onChange={() => toggle(s.id)}
                />
                <span className="text-muted-foreground w-6 shrink-0">{s.index}</span>
                <span className="truncate">{s.line || s.image_prompt || "(empty)"}</span>
              </label>
            ))}
          </div>

          {sections.length > 0 && (
            <>
              <div className="flex items-center gap-2 text-xs">
                <span className="text-muted-foreground">Prompt position:</span>
                {["prepend", "append", "replace"].map((m) => (
                  <button
                    key={m}
                    onClick={() => setMode(m)}
                    className={`px-1.5 py-0.5 rounded ${mode === m ? "bg-primary text-primary-foreground" : "bg-muted hover:bg-secondary"}`}
                  >
                    {m}
                  </button>
                ))}
              </div>
              <div className="flex flex-wrap gap-2">
                <Button size="sm" disabled={busy} onClick={() => apply(false)}>
                  {busy ? <Loader2 className="h-3 w-3 mr-1.5 animate-spin" /> : <Link2 className="h-3 w-3 mr-1.5" />}
                  Apply to {selected.size} section(s)
                </Button>
                <Button size="sm" variant="ghost" disabled={busy} onClick={() => apply(true)}>
                  <Unlink className="h-3 w-3 mr-1.5" /> Remove
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setSelected(new Set(sections.map((s) => s.id)))}>
                  Select all
                </Button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
