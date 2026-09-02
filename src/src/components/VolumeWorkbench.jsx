import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Input } from "./ui/input";
import { Textarea } from "./ui/textarea";
import {
  Library, Loader2, Sparkles, Download, Trash2, Wand2, AlertTriangle, CheckCircle2,
  ChevronUp, ChevronDown, X, Plus, ListTree,
} from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// The manuscript.
//
// A single edition is one song, twelve pages, one EPUB. A volume is the book: metadata a store
// sorts on, an ordered contents of chapters and parts, and the front and back matter that separates
// a book from an export.
//
// Automated first: "Assemble from this project" turns every song that has an edition into a chapter
// in the project's own order and lays in the standard matter pages, empty. Controllable after: what
// it produced is an ordinary list — reorder it, drop a chapter, add a part, write your own preface.
// The AI drafts any one matter page on request and what comes back is text in a box, not a decision.
//
// Preflight is the part that makes this publishing rather than exporting: it names what a retailer
// will reject and what will merely make the book worse, and it never refuses to build. A draft is a
// legitimate thing to want to hold.
// ─────────────────────────────────────────────────────────────────────────────

const FIELDS = [
  ["title", "Title", "text"],
  ["subtitle", "Subtitle", "text"],
  ["author", "Author", "text"],
  ["illustrator", "Illustrator", "text"],
  ["publisher", "Publisher", "text"],
  ["language", "Language", "text"],
  ["isbn", "ISBN", "text"],
  ["series", "Series", "text"],
  ["series_index", "Number in series", "number"],
  ["pubdate", "Publication date", "text"],
  ["rights", "Rights", "text"],
  ["cover_url", "Cover image path", "text"],
];

export default function VolumeWorkbench({ projectId }) {
  const [volumes, setVolumes] = useState([]);
  const [active, setActive] = useState(null);
  const [kinds, setKinds] = useState([]);
  const [check, setCheck] = useState(null);
  const [busy, setBusy] = useState("");

  const load = useCallback(async () => {
    try {
      const r = await api.volumeList(projectId || null);
      const list = r?.volumes || [];
      setVolumes(list);
      setActive((a) => list.find((v) => v.id === a?.id) || list[0] || null);
    } catch { setVolumes([]); }
  }, [projectId]);

  useEffect(() => { api.bookMatterKinds().then((r) => setKinds(r?.matter || [])).catch(() => {}); }, []);
  useEffect(() => { load(); }, [load]);
  useEffect(() => { setCheck(null); }, [active?.id]);

  const patch = (fields) => setActive((v) => ({ ...v, ...fields }));

  const save = async (extra = {}) => {
    if (!active) return null;
    setBusy("save");
    try {
      const body = { ...active, ...extra };
      const p = {};
      for (const k of ["title", "subtitle", "author", "illustrator", "translator", "publisher",
                       "description", "rights", "subjects", "isbn", "series", "series_index",
                       "pubdate", "language", "cover_url", "contents"]) {
        if (body[k] !== undefined) p[k] = body[k];
      }
      if (p.series_index === "" || p.series_index === null) p.series_index = null;
      else if (p.series_index !== undefined) p.series_index = Number(p.series_index);
      const saved = await api.volumeSave({ id: active.id, project_id: projectId || "", patch: p });
      await load();
      return saved;
    } catch (err) { toast.error(`${err}`); return null; }
    finally { setBusy(""); }
  };

  const assemble = async (group_by) => {
    setBusy("assemble");
    try {
      const r = await api.volumeAutofill({ project_id: projectId || "", id: active?.id || null, group_by });
      toast.success(`${r.chapters} chapters assembled.`);
      if (r.songs_without_an_edition?.length) {
        toast.message(`Not in the book yet: ${r.songs_without_an_edition.slice(0, 5).join(", ")}`,
                      { duration: 9000 });
      }
      await load();
      setActive(r.volume);
    } catch (err) { toast.error(`${err}`, { duration: 9000 }); }
    finally { setBusy(""); }
  };

  const newVolume = async () => {
    setBusy("new");
    try {
      const v = await api.volumeSave({ project_id: projectId || "", patch: { title: "Untitled volume" } });
      await load();
      setActive(v);
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const remove = async () => {
    if (!active) return;
    if (!window.confirm(`Delete the volume “${active.title || "Untitled"}”?\n\nThe editions in it are kept — only the manuscript goes.`)) return;
    try {
      await api.volumeDelete(active.id);
      await load();
    } catch (err) { toast.error(`${err}`); }
  };

  const writeMatter = async (role) => {
    if (!active) return;
    setBusy(`matter:${role}`);
    try {
      await save();
      const r = await api.volumeWriteMatter({ volume_id: active.id, role });
      patch({ contents: r.contents });
      toast.success("Drafted — edit it however you like.");
      await load();
    } catch (err) { toast.error(`${err}`, { duration: 9000 }); }
    finally { setBusy(""); }
  };

  const preflight = async () => {
    if (!active) return;
    setBusy("check");
    try {
      await save();
      setCheck(await api.volumePreflight(active.id));
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const bind = async () => {
    if (!active) return;
    setBusy("bind");
    try {
      await save();
      const r = await api.buildVolumeEpub({ id: active.id, include_audio: true });
      toast.success(`${r.chapters} chapters, ${r.pages} pages, ${(r.bytes / 1048576).toFixed(1)} MB.`);
      if (r.missing_art) toast.message(`${r.missing_art} pages have a prompt but no art yet.`);
      if (r.blocking) toast.warning(`${r.blocking} thing(s) would stop a store taking this — see the check.`);
      setCheck({ findings: r.findings, blocking: r.blocking,
                 warnings: r.findings.filter((f) => f.severity === "warning").length,
                 pages: r.pages, chapters: r.chapters });
      await load();
    } catch (err) { toast.error(`${err}`, { duration: 9000 }); }
    finally { setBusy(""); }
  };

  // ── contents editing ─────────────────────────────────────────────────────
  const contents = active?.contents || [];
  const setContents = (next) => patch({ contents: next });
  const move = (i, by) => {
    const next = [...contents];
    const j = i + by;
    if (j < 0 || j >= next.length) return;
    [next[i], next[j]] = [next[j], next[i]];
    setContents(next);
  };
  const drop = (i) => setContents(contents.filter((_, x) => x !== i));
  const editEntry = (i, fields) =>
    setContents(contents.map((c, x) => (x === i ? { ...c, ...fields } : c)));
  const addPart = () => setContents([...contents, { kind: "part", title: "", note: "" }]);
  const addMatter = (role) => {
    const k = kinds.find((m) => m.id === role);
    setContents([...contents, { kind: "matter", role, heading: k?.label || role, body: "" }]);
  };

  const label = (entry) => {
    if (entry.kind === "part") return entry.title || "Untitled part";
    if (entry.kind === "matter") return kinds.find((m) => m.id === entry.role)?.label || entry.role;
    return entry.title || entry.edition_id;
  };

  return (
    <div className="space-y-3">
      <Card className="p-4 space-y-3">
        <div className="flex items-start justify-between gap-3 flex-wrap">
          <div>
            <div className="text-sm font-semibold flex items-center gap-2">
              <Library className="w-4 h-4 text-primary" />The manuscript
            </div>
            <p className="text-[11px] text-muted-foreground max-w-2xl">
              A volume is many editions bound as one book — with the metadata a store sorts on, an
              ordered contents, and the pages that are not the book. Assemble it from what this
              project already has, then change any of it.
            </p>
          </div>
          <div className="flex items-center gap-1.5 flex-wrap">
            <Button size="sm" variant="secondary" onClick={newVolume} disabled={!!busy}>
              <Plus className="w-3.5 h-3.5 mr-1.5" />Empty volume
            </Button>
            <Button size="sm" onClick={() => assemble("none")} disabled={busy === "assemble"}>
              {busy === "assemble" ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                                   : <Sparkles className="w-3.5 h-3.5 mr-1.5" />}
              Assemble from this project
            </Button>
            <Button size="sm" variant="secondary" onClick={() => assemble("language")} disabled={!!busy}
                    title="One part per language">
              <ListTree className="w-3.5 h-3.5 mr-1.5" />By language
            </Button>
          </div>
        </div>

        {volumes.length > 1 && (
          <div className="flex flex-wrap gap-1.5">
            {volumes.map((v) => (
              <button key={v.id} onClick={() => setActive(v)}
                      className={`text-xs rounded-md border px-2 py-1 transition-all
                                  ${active?.id === v.id ? "border-primary/60 bg-primary/10 text-primary" : "border-border text-muted-foreground hover:border-primary/40"}`}>
                {v.title || "Untitled"}
              </button>
            ))}
          </div>
        )}
      </Card>

      {!active && (
        <Card className="p-6 text-center text-sm text-muted-foreground">
          No volume yet. Assembling from this project is one click and takes every song that already
          has an edition.
        </Card>
      )}

      {active && (
        <>
          <Card className="p-4 space-y-3">
            <div className="flex items-center justify-between gap-2">
              <div className="text-sm font-semibold">The book itself</div>
              <Button size="sm" variant="ghost" onClick={remove} className="text-destructive">
                <Trash2 className="w-3.5 h-3.5" />
              </Button>
            </div>
            <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-2.5">
              {FIELDS.map(([key, name, type]) => (
                <div key={key} className="space-y-1">
                  <label className="text-[11px] text-muted-foreground">{name}</label>
                  <Input type={type} value={active[key] ?? ""} onBlur={() => save()}
                         onChange={(ev) => patch({ [key]: ev.target.value })} className="h-8 text-sm" />
                </div>
              ))}
            </div>
            <div className="space-y-1">
              <label className="text-[11px] text-muted-foreground">
                Description — this is the blurb on the store page
              </label>
              <Textarea rows={2} value={active.description ?? ""} onBlur={() => save()}
                        onChange={(ev) => patch({ description: ev.target.value })} className="text-sm" />
            </div>
            <div className="space-y-1">
              <label className="text-[11px] text-muted-foreground">
                Subjects, comma separated — the categories it is browsable under
              </label>
              <Input value={(active.subjects || []).join(", ")} onBlur={() => save()}
                     onChange={(ev) => patch({
                       subjects: ev.target.value.split(",").map((x) => x.trim()).filter(Boolean),
                     })} className="h-8 text-sm" />
            </div>
          </Card>

          <Card className="p-4 space-y-3">
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <div className="text-sm font-semibold">Contents</div>
              <div className="flex items-center gap-1.5 flex-wrap">
                <Button size="sm" variant="secondary" onClick={addPart}>
                  <Plus className="w-3.5 h-3.5 mr-1.5" />Part
                </Button>
                <select onChange={(ev) => { if (ev.target.value) { addMatter(ev.target.value); ev.target.value = ""; } }}
                        className="h-8 rounded-md border border-border bg-background px-2 text-xs">
                  <option value="">Add a page…</option>
                  {kinds.map((m) => <option key={m.id} value={m.id}>{m.label}</option>)}
                </select>
                <Button size="sm" variant="secondary" onClick={() => save()} disabled={busy === "save"}>
                  Save order
                </Button>
              </div>
            </div>

            {!contents.length && (
              <p className="text-[11px] text-muted-foreground">Nothing in it yet.</p>
            )}

            <div className="space-y-1.5">
              {contents.map((entry, i) => (
                <div key={i} className="rounded-lg border border-border p-2 space-y-1.5">
                  <div className="flex items-center gap-2">
                    <Badge variant={entry.kind === "edition" ? "secondary" : "outline"}
                           className="text-[9px] shrink-0">{entry.kind}</Badge>
                    <div className="text-sm flex-1 min-w-0 truncate">{label(entry)}</div>
                    <div className="flex items-center gap-0.5 shrink-0">
                      <button onClick={() => move(i, -1)} className="p-1 rounded hover:bg-muted/40 text-muted-foreground">
                        <ChevronUp className="w-3.5 h-3.5" />
                      </button>
                      <button onClick={() => move(i, 1)} className="p-1 rounded hover:bg-muted/40 text-muted-foreground">
                        <ChevronDown className="w-3.5 h-3.5" />
                      </button>
                      <button onClick={() => drop(i)} className="p-1 rounded hover:bg-muted/40 text-destructive">
                        <X className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </div>

                  {entry.kind === "part" && (
                    <Input value={entry.title || ""} placeholder="Part title"
                           onChange={(ev) => editEntry(i, { title: ev.target.value })}
                           className="h-8 text-sm" />
                  )}

                  {entry.kind === "matter" && (
                    <>
                      <Textarea rows={entry.body ? 4 : 2} value={entry.body || ""}
                                placeholder={kinds.find((m) => m.id === entry.role)?.hint || ""}
                                onChange={(ev) => editEntry(i, { body: ev.target.value })}
                                className="text-sm" />
                      <Button size="sm" variant="ghost" onClick={() => writeMatter(entry.role)}
                              disabled={busy === `matter:${entry.role}`} className="text-xs">
                        {busy === `matter:${entry.role}`
                          ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                          : <Wand2 className="w-3.5 h-3.5 mr-1.5" />}
                        Draft it for me
                      </Button>
                    </>
                  )}
                </div>
              ))}
            </div>
          </Card>

          <Card className="p-4 space-y-3">
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <div className="text-sm font-semibold">Before it goes anywhere</div>
              <div className="flex items-center gap-1.5">
                <Button size="sm" variant="secondary" onClick={preflight} disabled={busy === "check"}>
                  {busy === "check" ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                                    : <CheckCircle2 className="w-3.5 h-3.5 mr-1.5" />}
                  Check it
                </Button>
                <Button size="sm" onClick={bind} disabled={busy === "bind"}>
                  {busy === "bind" ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                                   : <Download className="w-3.5 h-3.5 mr-1.5" />}
                  Bind the EPUB
                </Button>
              </div>
            </div>

            {check && (
              <div className="space-y-2">
                <div className="text-[11px] text-muted-foreground">
                  <span className="text-mono">{check.chapters}</span>
                  <span> chapters, </span>
                  <span className="text-mono">{check.pages}</span>
                  <span> pages.</span>
                </div>
                {!check.findings?.length && (
                  <p className="text-xs text-emerald-600 dark:text-emerald-400">
                    Nothing standing in the way.
                  </p>
                )}
                {(check.findings || []).map((f, i) => (
                  <div key={i} className={`rounded-lg border p-2 text-xs flex items-start gap-2
                    ${f.severity === "blocking" ? "border-destructive/40 bg-destructive/5" : "border-amber-500/40 bg-amber-500/5"}`}>
                    <AlertTriangle className={`w-3.5 h-3.5 mt-0.5 shrink-0
                      ${f.severity === "blocking" ? "text-destructive" : "text-amber-500"}`} />
                    <div className="min-w-0">
                      <div>{f.what}</div>
                      <div className="text-muted-foreground">{f.fix}</div>
                    </div>
                  </div>
                ))}
                <p className="text-[11px] text-muted-foreground">
                  None of this stops you binding it. A draft you can hold is worth more than a
                  checklist you cannot get past.
                </p>
              </div>
            )}

            {active.epub_path && (
              <p className="text-[11px] text-muted-foreground break-all">{active.epub_path}</p>
            )}
          </Card>
        </>
      )}
    </div>
  );
}
