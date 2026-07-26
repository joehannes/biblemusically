import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Badge } from "../components/ui/badge";
import {
  Wand2, Play, Trash2, ClipboardPaste, Plug, Globe, MousePointerClick, MousePointer2, Keyboard, Type, CornerDownLeft, Save, Plus, ChevronUp, ChevronDown, Copy, Zap, FilePlus2, Check, X, Hourglass, DownloadCloud, FileDown, FileUp, EyeOff, Eye, Package, Loader2,
} from "lucide-react";
import { toast } from "sonner";
import { api } from "../lib/api";
import { useStudio } from "../lib/store";
import { authorMacroForOpenPage } from "../lib/macroAuthor";
import {
  loadMacros, saveMacro, deleteMacro, updateMacro, describeStep,
  getVirtualClipboard, setVirtualClipboard, CONNECTORS, MACRO_ACTIONS,
  getClipboardQueue, pushClipboardEntries, clearClipboardQueue,
  getSongFieldBucket, DEFAULT_MACRO_PRESETS, addPresetMacro,
  WAIT_CONDITIONS, exportMacros, importMacros,
  MACRO_PACKAGES, addPresetPackage, findMacroPackage,
} from "../lib/macroRecorder";

// Icon per recorded step type, for the sequence display.
const STEP_ICONS = {
  nav: Globe,
  click: MousePointerClick,
  contextmenu: MousePointer2,
  paste: Keyboard,
  "paste-clipboard": ClipboardPaste,
  connector: Plug,
  fill: Type,
  enter: CornerDownLeft,
  action: Zap,
  wait: Hourglass,
  download: DownloadCloud,
};

// What a freshly-inserted step of each type looks like, and what fields it needs edited.
// Recorded steps arrive with a positional `target` (selector/byText/list context) already set
// by the page; a manually-added step has none, so its target has to be hand-specified — the
// editors below write plain `{selector}` or `{byText:{tag,text}}` targets, which stepScript
// resolves the exact same way it resolves a recorded one.
const STEP_TYPES = [
  { type: "nav", label: "Go to URL", needsTarget: false },
  { type: "click", label: "Click", needsTarget: true },
  { type: "contextmenu", label: "Right-click", needsTarget: true },
  { type: "fill", label: "Type text", needsTarget: true, needsValue: true },
  { type: "paste", label: "Paste text", needsTarget: true, needsValue: true },
  { type: "paste-clipboard", label: "Paste app clipboard", needsTarget: true },
  { type: "connector", label: "Connector (app-wide value)", needsTarget: true, needsConnector: true },
  { type: "enter", label: "Press Enter", needsTarget: true },
  { type: "action", label: "Run action script", needsAction: true },
  { type: "wait", label: "Wait for…", needsTarget: false },
  { type: "download", label: "Click → save download to folder", needsTarget: true },
];

function blankStep(type) {
  switch (type) {
    case "nav": return { type: "nav", href: "https://" };
    case "fill": return { type: "fill", value: "", target: { selector: "" } };
    case "paste": return { type: "paste", value: "", target: { selector: "" } };
    case "paste-clipboard": return { type: "paste-clipboard", target: { selector: "" } };
    case "connector": return { type: "connector", connector: CONNECTORS[0].id, binding: null, target: { selector: "" } };
    case "action": return { type: "action", action: MACRO_ACTIONS[0]?.id || "" };
    case "wait": return { type: "wait", condition: WAIT_CONDITIONS[0].id, target: { selector: "" }, containerSelector: "", text: "", timeoutMs: 20000 };
    case "download": return { type: "download", target: { selector: "" }, folderName: "macro-download", filename: "", timeoutMs: 30000 };
    case "click": case "contextmenu": case "enter":
    default: return { type, target: { selector: "" } };
  }
}

// Compact editor for a step's `target` — the two shapes stepScript actually understands besides
// recorded list/positional context: a CSS selector, or an element found by its visible text.
function TargetEditor({ target, onChange }) {
  const kind = target?.byText ? "text" : "selector";
  const setKind = (k) => {
    if (k === "text") onChange({ byText: { tag: target?.byText?.tag || "button", text: target?.byText?.text || "" } });
    else onChange({ selector: target?.selector || "" });
  };
  return (
    <div className="flex items-center gap-1.5 flex-wrap">
      <select
        value={kind}
        onChange={(e) => setKind(e.target.value)}
        className="h-6 text-[10px] rounded border border-border bg-background px-1"
      >
        <option value="selector">CSS selector</option>
        <option value="text">Element text</option>
      </select>
      {kind === "selector" ? (
        <Input
          value={target?.selector || ""}
          onChange={(e) => onChange({ selector: e.target.value })}
          placeholder="e.g. textarea[name=lyrics]"
          className="h-6 text-[10px] font-mono flex-1 min-w-[140px]"
        />
      ) : (
        <>
          <Input
            value={target?.byText?.tag || "button"}
            onChange={(e) => onChange({ byText: { ...(target?.byText || {}), tag: e.target.value } })}
            placeholder="tag"
            className="h-6 text-[10px] font-mono w-16"
          />
          <Input
            value={target?.byText?.text || ""}
            onChange={(e) => onChange({ byText: { ...(target?.byText || {}), text: e.target.value } })}
            placeholder="visible text starts with…"
            className="h-6 text-[10px] flex-1 min-w-[120px]"
          />
        </>
      )}
    </div>
  );
}

// Macro Manager: inspect recorded macro sequences, edit them (reorder / insert / delete / add
// steps by hand, not just what got recorded), rebind connector slots to app-wide actions, and
// maintain the pseudo clipboard buckets. Recording itself (capturing live clicks) still only
// happens in the Web Browser view — this is where the *result* gets shaped afterwards.
/**
 * Per-platform posting recipes.
 *
 * The insight this is built on: what changes between two posts is the *values*, not the flow. So the
 * app prepares today's values in form order, the user records the pasting once, and every later run
 * swaps the queue rather than the macro.
 */
function PostingRecipes({ macros, onLinked, navigate }) {
  const { songs, activeSongId, selectSong } = useStudio();
  const [recipes, setRecipes] = useState([]);
  const [prepared, setPrepared] = useState(null);
  const [busy, setBusy] = useState("");

  const load = () => api.listPostRecipes().then((r) => setRecipes(r?.recipes || [])).catch(() => {});
  useEffect(() => { load(); }, []);

  const prepare = async (platform) => {
    if (!activeSongId) { toast.error("Pick a song — its caption and link are what get prepared."); return; }
    setBusy(platform);
    try {
      const p = await api.preparePostMacro({ platform, song_id: activeSongId });
      // Into the queue, in the form's own order, so the recorded pastes line up on every replay.
      await api.clipboardQueueSet({ items: p.values, remember: true });
      setPrepared(p);
      toast.success(`${p.values.length} values queued for ${p.label}.`);
      if (p.url) navigate(`/browser?url=${encodeURIComponent(p.url)}&label=${encodeURIComponent(p.label)}&record=1`);
    } catch (err) { toast.error(`${err}`, { duration: 9000 }); }
    finally { setBusy(""); }
  };

  const link = async (platform, macroId) => {
    const m = macros.find((x) => x.id === macroId);
    if (!m) return;
    const queueSteps = (m.steps || []).filter((s) => s.type === "paste-queue").length;
    try {
      const r = await api.linkPostMacro({
        platform, macro_id: m.id, macro_name: m.name, queue_steps: queueSteps,
      });
      if (r.warning) toast.warning(r.warning, { duration: 12000 });
      else toast.success(`${m.name} will post for ${platform}.`);
      load();
      onLinked?.();
    } catch (err) { toast.error(`${err}`); }
  };

  return (
    <div className="border border-border rounded-lg p-3 space-y-2">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div className="text-[10px] uppercase tracking-widest text-muted-foreground">
          Record a posting macro
        </div>
        <select value={activeSongId || ""} onChange={(e) => selectSong(e.target.value)}
                className="bg-muted/30 border border-border rounded px-2 py-1 text-xs max-w-[16rem]">
          <option value="">Pick a song…</option>
          {(songs || []).slice(0, 60).map((s) => <option key={s.id} value={s.id}>{s.title}</option>)}
        </select>
      </div>
      <p className="text-[11px] text-muted-foreground">
        The values go on the clipboard queue in the order the form asks for them. Record yourself
        pasting field by field — each paste becomes a <b>paste the next queued value</b> step, so the
        same macro posts a different song tomorrow with no editing.
      </p>

      <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-2">
        {recipes.map((r) => (
          <div key={r.platform} className="rounded-lg border border-border/60 p-2 space-y-1">
            <div className="text-sm font-medium flex items-center gap-1.5">
              {r.label}
              {r.macro && <Badge variant="outline" className="text-[9px] text-emerald-400 border-emerald-500/40">recorded</Badge>}
            </div>
            <div className="text-[10px] text-muted-foreground">prepares: {(r.fields || []).join(", ")}</div>
            <div className="flex gap-1 flex-wrap">
              <Button size="sm" variant="secondary" className="h-6 px-2 text-[10px]"
                      disabled={busy === r.platform} onClick={() => prepare(r.platform)}>
                {busy === r.platform ? <Loader2 className="w-3 h-3 animate-spin" /> : "Prepare & open"}
              </Button>
              {macros.length > 0 && (
                <select className="bg-muted/30 border border-border rounded px-1 py-0.5 text-[10px] max-w-[9rem]"
                        value={r.macro?.macro_id || ""}
                        onChange={(e) => (e.target.value
                          ? link(r.platform, e.target.value)
                          : api.unlinkPostMacro(r.platform).then(load))}>
                  <option value="">link a macro…</option>
                  {macros.map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
                </select>
              )}
            </div>
            <div className="text-[10px] text-muted-foreground leading-snug">{r.why}</div>
          </div>
        ))}
      </div>

      {prepared && (
        <div className="rounded-lg border border-primary/30 p-2 space-y-1">
          <div className="text-xs font-medium">While you record — {prepared.label}</div>
          <ol className="list-decimal ml-4 text-[11px] text-muted-foreground space-y-0.5">
            {(prepared.checklist || []).map((c, i) => <li key={i}>{c}</li>)}
          </ol>
          {(prepared.media || []).length > 0 && (
            <div className="text-[10px] text-muted-foreground">
              Files to attach (a file picker is the one thing a macro cannot drive):
              <div className="font-mono break-all" data-no-i18n>{prepared.media.join("  ·  ")}</div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function MacroManager() {
  const navigate = useNavigate();
  const [macros, setMacros] = useState(() => loadMacros());
  const [selectedId, setSelectedId] = useState(() => loadMacros()[0]?.id || "");
  const [clipText, setClipText] = useState(() => getVirtualClipboard());
  const [queue, setQueue] = useState(() => getClipboardQueue());
  const [queueDraft, setQueueDraft] = useState("");
  const [fieldBucket, setFieldBucket] = useState(() => getSongFieldBucket());
  const [nameDraft, setNameDraft] = useState("");
  const [stepsDraft, setStepsDraft] = useState(null); // edited copy of the selected macro's steps
  const [dirty, setDirty] = useState(false);
  const [newType, setNewType] = useState("click");
  const [creatingMacro, setCreatingMacro] = useState(false);
  const [newMacroName, setNewMacroName] = useState("");
  const importFileRef = useRef(null);
  // ── AI-authored macros ────────────────────────────────────────────────────
  // Half the platforms worth posting to have no usable posting API. Recording that flow by hand is
  // fine once and tedious at fifty channels — so the AI writes it from the page's own structure.
  const [authorGoal, setAuthorGoal] = useState("");
  const [authoring, setAuthoring] = useState(false);
  const [authored, setAuthored] = useState([]);
  useEffect(() => { api.listAuthoredMacros().then(setAuthored).catch(() => {}); }, []);

  const authorFromOpenPage = async () => {
    if (!authorGoal.trim()) return toast.error("Say what the macro should do.");
    setAuthoring(true);
    try {
      const m = await authorMacroForOpenPage({ goal: authorGoal.trim() });
      // Straight into the normal macro library, so it plays, edits and exports like a recorded one.
      const saved = saveMacro({ name: m.name, steps: m.steps, startUrl: m.url });
      setMacros(loadMacros());
      setSelectedId(saved.id);
      setAuthored(await api.listAuthoredMacros().catch(() => authored));
      toast.success(`Wrote "${m.name}" — ${m.steps.length} steps.`, {
        description: m.caution || (m.dropped_steps ? `${m.dropped_steps} unusable step(s) were dropped.` : undefined),
        duration: 12000,
      });
    } catch (err) {
      toast.error(String(err), { duration: 12000 });
    } finally { setAuthoring(false); }
  };

  const selected = useMemo(() => macros.find((m) => m.id === selectedId) || null, [macros, selectedId]);

  // (Re)load drafts whenever the selection changes.
  useEffect(() => {
    setNameDraft(selected?.name || "");
    setStepsDraft(selected ? selected.steps.map((s) => ({ ...s })) : null);
    setDirty(false);
  }, [selectedId, selected]);

  // The field bucket is written by the AI generation pipeline (and by hand); refresh it whenever
  // the panel regains focus so it reflects whatever's currently staged, not a stale snapshot.
  useEffect(() => {
    const onFocus = () => setFieldBucket(getSongFieldBucket());
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const saveClipboard = (text) => {
    setClipText(text);
    setVirtualClipboard(text);
  };

  const prefillQueue = () => {
    const lines = queueDraft.split("\n").map((l) => l.trim()).filter(Boolean);
    if (!lines.length) return;
    const updated = pushClipboardEntries(lines);
    setQueue(updated);
    setQueueDraft("");
    toast.success(`Added ${lines.length} entr${lines.length === 1 ? "y" : "ies"} — ${updated.length} in the bucket.`);
  };

  const clearQueue = () => {
    setQueue(clearClipboardQueue());
    toast.success("Cleared the clipboard bucket queue.");
  };

  const patchStep = (idx, patch) => {
    setStepsDraft((prev) => prev.map((s, i) => (i === idx ? { ...s, ...patch } : s)));
    setDirty(true);
  };
  const patchStepTarget = (idx, targetPatch) => {
    setStepsDraft((prev) => prev.map((s, i) => (i === idx ? { ...s, target: { ...targetPatch } } : s)));
    setDirty(true);
  };

  const removeStep = (idx) => {
    setStepsDraft((prev) => prev.filter((_, i) => i !== idx));
    setDirty(true);
  };

  const duplicateStep = (idx) => {
    setStepsDraft((prev) => {
      const copy = { ...prev[idx], target: prev[idx].target ? { ...prev[idx].target } : undefined };
      const next = [...prev];
      next.splice(idx + 1, 0, copy);
      return next;
    });
    setDirty(true);
  };

  const moveStep = (idx, dir) => {
    setStepsDraft((prev) => {
      const to = idx + dir;
      if (to < 0 || to >= prev.length) return prev;
      const next = [...prev];
      [next[idx], next[to]] = [next[to], next[idx]];
      return next;
    });
    setDirty(true);
  };

  const insertStepAt = (idx, type) => {
    setStepsDraft((prev) => {
      const next = [...prev];
      next.splice(idx, 0, blankStep(type));
      return next;
    });
    setDirty(true);
  };

  const addStepAtEnd = () => {
    if (!stepsDraft) return;
    insertStepAt(stepsDraft.length, newType);
  };

  const saveSelected = () => {
    if (!selected) return;
    setMacros(updateMacro(selected.id, { name: nameDraft.trim() || selected.name, steps: stepsDraft }));
    setDirty(false);
    toast.success("Macro saved.");
  };

  const removeSelected = () => {
    if (!selected) return;
    const next = deleteMacro(selected.id);
    setMacros(next);
    setSelectedId(next[0]?.id || "");
    toast.success("Macro deleted.");
  };

  const playSelected = () => {
    if (!selected) return;
    if (dirty) saveSelected();
    navigate(`/browser?playMacro=${selected.id}`);
  };

  const usePreset = (presetId) => {
    const macro = addPresetMacro(presetId);
    setMacros(loadMacros());
    setSelectedId(macro.id);
    toast.success(`Added "${macro.name}" to your macros.`);
  };

  const usePackage = (packageId) => {
    const added = addPresetPackage(packageId);
    setMacros(loadMacros());
    if (added.length) setSelectedId(added[added.length - 1].id);
    toast.success(`Added ${added.length} macro${added.length === 1 ? "" : "s"} from "${findMacroPackage(packageId)?.label}".`);
  };

  const duplicateSelected = () => {
    if (!selected) return;
    const copy = saveMacro({
      name: `${selected.name} (copy)`,
      steps: selected.steps.map((s) => ({ ...s, target: s.target ? { ...s.target } : undefined })),
      startUrl: selected.startUrl,
    });
    setMacros(loadMacros());
    setSelectedId(copy.id);
    toast.success(`Duplicated as "${copy.name}".`);
  };

  const createMacro = () => {
    const name = newMacroName.trim();
    if (!name) return;
    const macro = saveMacro({ name, steps: [], startUrl: "" });
    setMacros(loadMacros());
    setSelectedId(macro.id);
    setNewMacroName("");
    setCreatingMacro(false);
    toast.success(`Created "${macro.name}" — add steps on the right.`);
  };

  const slugForFile = (name) => (name || "macro").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "macro";

  const downloadMacrosFile = (payload, filename) => {
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = filename; document.body.appendChild(a); a.click(); a.remove();
    URL.revokeObjectURL(url);
    toast.success(`Exported ${payload.macros.length} macro${payload.macros.length === 1 ? "" : "s"}.`);
  };

  const onImportFile = async (e) => {
    const f = e.target.files?.[0];
    e.target.value = ""; // allow re-selecting the same file next time
    if (!f) return;
    try {
      const text = await f.text();
      const { macros: merged, imported, skipped } = importMacros(text);
      setMacros(merged);
      toast.success(`Imported ${imported} macro${imported === 1 ? "" : "s"}${skipped ? ` (skipped ${skipped} malformed entr${skipped === 1 ? "y" : "ies"})` : ""}.`);
      if (imported && merged.length) setSelectedId(merged[merged.length - 1].id);
    } catch (err) {
      toast.error(err?.message || "Import failed — that doesn't look like a macro export.");
    }
  };

  return (
    <div className="p-4 sm:p-6 space-y-4 max-w-6xl">
      <div className="flex items-center gap-2">
        <Wand2 className="w-5 h-5 text-primary" />
        <h1 className="text-lg font-semibold">Macro Manager</h1>
        <span className="text-xs text-muted-foreground">
          — recorded (or hand-built) browser macros, their sequences, and the app-wide connector actions they plug into.
        </span>
      </div>

      {/* ── Post to a platform ───────────────────────────────────────────
          A recorded flow contains the values from the day it was recorded — replay it and you post
          yesterday's song again. So the values are prepared into the clipboard queue FIRST, and you
          record yourself pasting them. Each paste becomes a step that takes whatever is queued at
          replay time, which is what makes one recording work for every later song. */}
      <PostingRecipes macros={macros} onLinked={() => setMacros(loadMacros())} navigate={navigate} />

      {/* ── Write one with the AI ─────────────────────────────────────────
          Open the target page in the app's browser first: the macro is written against that page's
          own elements, so it can only reference selectors that actually exist there. */}
      <div className="border border-primary/30 rounded-lg p-3 space-y-2">
        <div className="text-[10px] uppercase tracking-widest text-muted-foreground">Write a macro with the AI</div>
        <div className="flex gap-2 flex-wrap">
          <Input value={authorGoal} onChange={(e) => setAuthorGoal(e.target.value)}
            placeholder="e.g. post the text from the app clipboard to r/ChristianMusic with a title"
            className="flex-1 min-w-[18rem]" />
          <Button size="sm" onClick={authorFromOpenPage} disabled={authoring}>
            {authoring ? "Reading the page…" : "Write it"}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => navigate("/browser")}>Open the browser</Button>
        </div>
        <div className="text-[11px] text-muted-foreground">
          Open the page you want driven in the app's browser, then describe the goal. The macro is
          written from that page's real elements — anything the player could not execute is dropped
          rather than saved, and fields meant to change per run become playback parameters.
        </div>
        {authored.length > 0 && (
          <div className="text-[11px] text-muted-foreground">
            Previously written: {authored.slice(0, 4).map((a) => a.name).join(" · ")}
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-[280px_1fr] gap-4 items-start">
        {/* ── Left: macro list + clipboard buckets + connector catalog ── */}
        <div className="space-y-4">
          <div className="border border-border rounded-lg overflow-hidden">
            <div className="flex items-center justify-between px-3 py-2 text-[10px] uppercase tracking-widest text-muted-foreground border-b border-border bg-muted/30">
              <span>Macros ({macros.length})</span>
              <button
                onClick={() => setCreatingMacro((o) => !o)}
                className="flex items-center gap-1 normal-case tracking-normal text-[11px] text-primary hover:underline"
                title="Build a new macro from scratch, step by step, instead of recording one"
              >
                <FilePlus2 className="w-3 h-3" />New
              </button>
            </div>
            {creatingMacro && (
              <div className="flex items-center gap-1.5 px-3 py-2 border-b border-border/60 bg-muted/10">
                <Input
                  value={newMacroName}
                  onChange={(e) => setNewMacroName(e.target.value)}
                  onKeyDown={(e) => { if (e.key === "Enter") createMacro(); if (e.key === "Escape") setCreatingMacro(false); }}
                  placeholder="macro name…"
                  className="h-7 text-xs flex-1"
                  autoFocus
                />
                <Button size="sm" className="h-7 w-7 p-0" disabled={!newMacroName.trim()} onClick={createMacro}><Check className="w-3.5 h-3.5" /></Button>
                <Button size="sm" variant="ghost" className="h-7 w-7 p-0" onClick={() => setCreatingMacro(false)}><X className="w-3.5 h-3.5" /></Button>
              </div>
            )}
            {macros.length === 0 && !creatingMacro && (
              <p className="px-3 py-3 text-xs text-muted-foreground">
                None yet — hit <button className="underline" onClick={() => setCreatingMacro(true)}>New</button> to build
                one by hand, or open the <button className="underline" onClick={() => navigate("/browser")}>Web Browser</button>,
                open the Macros panel and hit Record.
              </p>
            )}
            {macros.map((m) => (
              <button
                key={m.id}
                onClick={() => setSelectedId(m.id)}
                className={`w-full text-left px-3 py-2 text-sm border-l-2 transition-colors ${
                  m.id === selectedId
                    ? "border-primary bg-primary/10"
                    : "border-transparent hover:bg-muted/40 text-muted-foreground"
                }`}
              >
                <div className="font-medium truncate">{m.name}</div>
                <div className="text-[10px] text-muted-foreground font-mono truncate">
                  {m.steps.length} steps · {new Date(m.createdAt).toLocaleDateString()} · {m.startUrl}
                </div>
              </button>
            ))}
            <div className="flex items-center gap-1.5 px-3 py-2 border-t border-border/60 bg-muted/10">
              {macros.length > 0 && (
                <Button size="sm" variant="ghost" className="h-7 text-[11px] flex-1" onClick={() => downloadMacrosFile(exportMacros(), "macros-export.json")} title="Export every macro to a JSON file">
                  <FileDown className="w-3 h-3 mr-1" />Export all
                </Button>
              )}
              {selected && (
                <Button size="sm" variant="ghost" className="h-7 text-[11px] flex-1" onClick={() => downloadMacrosFile(exportMacros([selected.id]), `${slugForFile(selected.name)}.json`)} title="Export just the selected macro">
                  <FileDown className="w-3 h-3 mr-1" />Export selected
                </Button>
              )}
              <input ref={importFileRef} type="file" accept=".json,application/json" onChange={onImportFile} className="hidden" />
              <Button size="sm" variant="ghost" className="h-7 text-[11px] flex-1" onClick={() => importFileRef.current?.click()} title="Import macros from a previously exported JSON file">
                <FileUp className="w-3 h-3 mr-1" />Import
              </Button>
            </div>
          </div>

          {/* Macro packages: install a whole named group of starter presets in one click,
              instead of adding them one at a time below. Each macro still lands as its own
              independent, editable entry in your list — a package is a one-shot install, not a
              standing group. */}
          <div className="border border-border rounded-lg overflow-hidden">
            <div className="px-3 py-2 text-[10px] uppercase tracking-widest text-muted-foreground border-b border-border bg-muted/30">
              Macro packages ({MACRO_PACKAGES.length})
            </div>
            {MACRO_PACKAGES.map((pkg) => (
              <div key={pkg.id} className="flex items-center justify-between gap-2 px-3 py-2 text-sm border-b border-border/40 last:border-b-0">
                <div className="min-w-0">
                  <div className="font-medium truncate flex items-center gap-1.5"><Package className="w-3 h-3 text-primary shrink-0" />{pkg.label}</div>
                  <div className="text-[10px] text-muted-foreground truncate">{pkg.description}</div>
                </div>
                <Button size="sm" variant="outline" className="h-7 text-xs shrink-0" onClick={() => usePackage(pkg.id)}>
                  Add {pkg.presetIds.length}
                </Button>
              </div>
            ))}
          </div>

          {/* Starter presets: one built-in macro per engine, ready to add on request. Not
              recorded clicks (those only ever replay reliably against the exact page they were
              recorded on) — each is a nav step + a call into the same automation script that
              backs the matching Browser-tab toolbar button, so it works out of the box. */}
          <div className="border border-border rounded-lg overflow-hidden">
            <div className="px-3 py-2 text-[10px] uppercase tracking-widest text-muted-foreground border-b border-border bg-muted/30">
              Starter presets ({DEFAULT_MACRO_PRESETS.length})
            </div>
            {DEFAULT_MACRO_PRESETS.map((p) => (
              <div key={p.id} className="flex items-center justify-between gap-2 px-3 py-2 text-sm border-b border-border/40 last:border-b-0">
                <div className="min-w-0">
                  <div className="font-medium truncate">{p.name}</div>
                  <div className="text-[10px] text-muted-foreground font-mono truncate">{p.steps.length} steps{p.startUrl ? ` · ${p.startUrl}` : ""}</div>
                </div>
                <Button size="sm" variant="outline" className="h-7 text-xs shrink-0" onClick={() => usePreset(p.id)}>
                  Add
                </Button>
              </div>
            ))}
          </div>

          {/* Pseudo clipboard bucket */}
          <div className="border border-border rounded-lg p-3 space-y-2">
            <div className="flex items-center gap-2 text-[10px] uppercase tracking-widest text-muted-foreground">
              <ClipboardPaste className="w-3.5 h-3.5" /> App clipboard bucket
            </div>
            <Textarea
              value={clipText}
              onChange={(e) => saveClipboard(e.target.value)}
              rows={5}
              className="text-xs font-mono"
              placeholder="Our own clipboard for the webview. 'Paste clipboard' steps and the clipboard connector insert whatever sits here at playback…"
            />
            <p className="text-[10px] text-muted-foreground">
              Saved as you type — shared with the Browser view's macro panel.
            </p>
          </div>

          {/* Clipboard bucket queue: multiple prefilled entries, one consumed per playback */}
          <div className="border border-border rounded-lg p-3 space-y-2">
            <div className="flex items-center gap-2 text-[10px] uppercase tracking-widest text-muted-foreground">
              <ClipboardPaste className="w-3.5 h-3.5" /> Clipboard bucket queue ({queue.length} left)
            </div>
            <Textarea
              value={queueDraft}
              onChange={(e) => setQueueDraft(e.target.value)}
              rows={4}
              className="text-xs font-mono"
              placeholder={"One entry per line. Prefill here, then bind a connector step to \"Clipboard bucket — pop next\" so each playback consumes the next line."}
            />
            <div className="flex items-center gap-2">
              <Button size="sm" variant="outline" className="h-7 text-xs" onClick={prefillQueue} disabled={!queueDraft.trim()}>
                Add to bucket
              </Button>
              <Button size="sm" variant="ghost" className="h-7 text-xs text-muted-foreground hover:text-red-500" onClick={clearQueue} disabled={!queue.length}>
                Clear
              </Button>
            </div>
            {queue.length > 0 && (
              <ol className="text-[10px] font-mono text-muted-foreground space-y-0.5 max-h-24 overflow-y-auto">
                {queue.map((entry, i) => <li key={i} className="truncate">{i + 1}. {entry}</li>)}
              </ol>
            )}
          </div>

          {/* Named song-field bucket: what the AI generation pipeline (or you, by hand) last
              staged — title/styles/lyrics as three separate named slots, so a macro on a site
              with no bespoke automation script yet can be built just by recording paste steps
              bound to the song-title/song-styles/song-lyrics connectors below. Read-only here;
              staged by genPipeline.js or via setSongFieldBucket(). */}
          <div className="border border-border rounded-lg p-3 space-y-2">
            <div className="flex items-center gap-2 text-[10px] uppercase tracking-widest text-muted-foreground">
              <Plug className="w-3.5 h-3.5" /> Song field bucket (staged)
            </div>
            {fieldBucket.songId ? (
              <div className="text-xs space-y-1">
                <div><span className="text-muted-foreground">Title:</span> {fieldBucket.title || <em className="text-muted-foreground">empty</em>}</div>
                <div><span className="text-muted-foreground">Styles:</span> {fieldBucket.styles || <em className="text-muted-foreground">empty</em>}</div>
                <div className="text-muted-foreground truncate">Lyrics: {fieldBucket.lyrics ? `${fieldBucket.lyrics.length} chars` : <em>empty</em>}</div>
              </div>
            ) : (
              <p className="text-[11px] text-muted-foreground">
                Nothing staged yet — the AI generation pipeline stages the current song here automatically.
              </p>
            )}
          </div>

          {/* Connector actions catalog */}
          <div className="border border-border rounded-lg p-3 space-y-2">
            <div className="flex items-center gap-2 text-[10px] uppercase tracking-widest text-muted-foreground">
              <Plug className="w-3.5 h-3.5" /> Connector actions
            </div>
            {CONNECTORS.map((c) => (
              <div key={c.id} className="text-xs">
                <div className="font-medium">{c.label}</div>
                <div className="text-[11px] text-muted-foreground">{c.description}</div>
              </div>
            ))}
            <p className="text-[10px] text-muted-foreground">
              Pluggable into <Badge variant="secondary" className="text-[8px] align-middle">connector</Badge> steps —
              record one live in the Browser tab, or add one by hand below.
            </p>
          </div>
        </div>

        {/* ── Right: selected macro sequence, fully editable ── */}
        <div className="border border-border rounded-lg overflow-hidden min-h-[200px]">
          {!selected ? (
            <div className="p-6 text-sm text-muted-foreground">Select a macro (or create one) to edit its sequence.</div>
          ) : (
            <>
              <div className="flex items-center gap-2 px-3 py-2 border-b border-border bg-muted/30">
                <Input
                  value={nameDraft}
                  onChange={(e) => { setNameDraft(e.target.value); setDirty(true); }}
                  className="h-8 text-sm font-medium max-w-xs"
                />
                <span className="text-[11px] text-muted-foreground font-mono truncate flex-1">{selected.startUrl}</span>
                <Button size="sm" className="h-8" disabled={!dirty} onClick={saveSelected}>
                  <Save className="w-3.5 h-3.5 mr-1.5" />Save
                </Button>
                <Button size="sm" variant="outline" className="h-8" onClick={playSelected}>
                  <Play className="w-3.5 h-3.5 mr-1.5" />Play in Browser
                </Button>
                <Button size="sm" variant="ghost" className="h-8 px-2" onClick={duplicateSelected} title="Duplicate this macro">
                  <Copy className="w-3.5 h-3.5" />
                </Button>
                <Button size="sm" variant="ghost" className="h-8 px-2 text-muted-foreground hover:text-red-500" onClick={removeSelected} title="Delete macro">
                  <Trash2 className="w-3.5 h-3.5" />
                </Button>
              </div>

              {!stepsDraft?.length && (
                <p className="px-3 py-3 text-xs text-muted-foreground">No steps yet — add one below.</p>
              )}

              <ol className="divide-y divide-border/60">
                {(stepsDraft || []).map((s, i) => {
                  const Icon = STEP_ICONS[s.type] || MousePointerClick;
                  const isConnector = s.type === "connector";
                  const meta = STEP_TYPES.find((t) => t.type === s.type);
                  // recorded positional targets stay read-only; wait/download render their own target editor inline (conditionally, for wait)
                  const hasManualTarget = meta?.needsTarget && !s.target?.list && !["wait", "download"].includes(s.type);
                  return (
                    <li key={i} className={`flex items-start gap-2 px-3 py-2 text-xs group ${s.disabled ? "opacity-50" : ""}`}>
                      <div className="flex flex-col items-center gap-0.5 shrink-0 pt-0.5">
                        <span className="w-6 text-right font-mono text-muted-foreground">{i + 1}.</span>
                        <div className="flex flex-col opacity-0 group-hover:opacity-100">
                          <button onClick={() => moveStep(i, -1)} disabled={i === 0} className="text-muted-foreground/60 hover:text-foreground disabled:opacity-20" title="Move up"><ChevronUp className="w-3 h-3" /></button>
                          <button onClick={() => moveStep(i, 1)} disabled={i === stepsDraft.length - 1} className="text-muted-foreground/60 hover:text-foreground disabled:opacity-20" title="Move down"><ChevronDown className="w-3 h-3" /></button>
                        </div>
                      </div>
                      <Icon className={`w-3.5 h-3.5 shrink-0 mt-0.5 ${isConnector || s.type === "paste-clipboard" ? "text-primary" : "text-muted-foreground"}`} />
                      <div className="flex-1 min-w-0 space-y-1">
                        <div className="truncate">{describeStep(s)}{s.disabled && <span className="ml-1.5 text-[9px] uppercase tracking-wide text-muted-foreground/70">(skipped on playback)</span>}</div>
                        {s.target?.list && (
                          <div className="text-[10px] text-muted-foreground font-mono">
                            positional: item {s.target.list.index + 1} of {s.target.list.count} (from end: {s.target.list.indexFromEnd})
                          </div>
                        )}

                        {s.type === "nav" && (
                          <Input
                            value={s.href || ""}
                            onChange={(e) => patchStep(i, { href: e.target.value })}
                            className="h-7 text-[11px] font-mono"
                            placeholder="https://…"
                          />
                        )}

                        {(s.type === "fill" || s.type === "paste") && (
                          <Textarea
                            value={s.value || ""}
                            onChange={(e) => patchStep(i, { value: e.target.value })}
                            rows={2}
                            className="text-[11px] font-mono"
                            placeholder="text to insert…"
                          />
                        )}

                        {s.type === "action" && (
                          <select
                            value={s.action || ""}
                            onChange={(e) => patchStep(i, { action: e.target.value })}
                            className="h-7 text-[11px] rounded-md border border-border bg-background px-1.5 w-full"
                          >
                            {MACRO_ACTIONS.map((a) => <option key={a.id} value={a.id}>{a.label}</option>)}
                          </select>
                        )}

                        {s.type === "wait" && (
                          <div className="space-y-1.5">
                            <select
                              value={s.condition || WAIT_CONDITIONS[0].id}
                              onChange={(e) => patchStep(i, { condition: e.target.value })}
                              className="h-7 text-[11px] rounded-md border border-border bg-background px-1.5 w-full"
                            >
                              {WAIT_CONDITIONS.map((c) => <option key={c.id} value={c.id}>{c.label}</option>)}
                            </select>
                            {(() => {
                              const cond = WAIT_CONDITIONS.find((c) => c.id === s.condition) || WAIT_CONDITIONS[0];
                              return (
                                <>
                                  <p className="text-[10px] text-muted-foreground leading-snug">{cond.hint}</p>
                                  {cond.needsTarget && (
                                    <TargetEditor target={s.target} onChange={(t) => patchStepTarget(i, t)} />
                                  )}
                                  {(cond.needsContainer || cond.needsContainerOptional) && (
                                    <Input
                                      value={s.containerSelector || ""}
                                      onChange={(e) => patchStep(i, { containerSelector: e.target.value })}
                                      className="h-7 text-[11px] font-mono"
                                      placeholder={cond.needsContainer ? "container CSS selector (required)…" : "container CSS selector (optional — default: whole page)…"}
                                    />
                                  )}
                                  {cond.needsText && (
                                    <Input
                                      value={s.text || ""}
                                      onChange={(e) => patchStep(i, { text: e.target.value })}
                                      className="h-7 text-[11px]"
                                      placeholder="text to look for…"
                                    />
                                  )}
                                  {cond.needsQuiet && (
                                    <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
                                      quiet for
                                      <Input
                                        type="number" min={200} step={100}
                                        value={s.quietMs ?? 1200}
                                        onChange={(e) => patchStep(i, { quietMs: Number(e.target.value) || 1200 })}
                                        className="h-6 w-20 text-[10px] font-mono"
                                      />
                                      ms
                                    </div>
                                  )}
                                  <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
                                    give up after
                                    <Input
                                      type="number" min={1000} step={1000}
                                      value={s.timeoutMs ?? 20000}
                                      onChange={(e) => patchStep(i, { timeoutMs: Number(e.target.value) || 20000 })}
                                      className="h-6 w-20 text-[10px] font-mono"
                                    />
                                    ms
                                  </div>
                                </>
                              );
                            })()}
                          </div>
                        )}

                        {s.type === "download" && (
                          <div className="space-y-1.5">
                            <TargetEditor target={s.target} onChange={(t) => patchStepTarget(i, t)} />
                            <div className="flex items-center gap-1.5">
                              <Input
                                value={s.folderName || ""}
                                onChange={(e) => patchStep(i, { folderName: e.target.value })}
                                className="h-7 text-[11px] font-mono flex-1"
                                placeholder="folder name (under Documents/BibleMusically/MacroDownloads/…)"
                              />
                              <Input
                                value={s.filename || ""}
                                onChange={(e) => patchStep(i, { filename: e.target.value })}
                                className="h-7 text-[11px] font-mono flex-1"
                                placeholder="filename (optional — default: site's suggestion)"
                              />
                            </div>
                            <p className="text-[10px] text-muted-foreground leading-snug">
                              Clicks the target, then redirects whatever native download that triggers into the named folder — no save dialog. If the click doesn't actually start a browser download (e.g. it just plays inline), this step reports failure.
                            </p>
                          </div>
                        )}

                        {isConnector && (
                          <div className="flex items-center gap-2">
                            <select
                              value={s.connector || "clipboard"}
                              onChange={(e) => patchStep(i, { connector: e.target.value })}
                              className="h-7 text-[11px] rounded-md border border-border bg-background px-1.5"
                            >
                              {CONNECTORS.map((c) => <option key={c.id} value={c.id}>{c.label}</option>)}
                            </select>
                            {s.connector === "custom-text" && (
                              <Input
                                value={s.binding?.text || ""}
                                onChange={(e) => patchStep(i, { binding: { ...(s.binding || {}), text: e.target.value } })}
                                className="h-7 text-[11px] flex-1 font-mono"
                                placeholder="fixed text this slot inserts…"
                              />
                            )}
                          </div>
                        )}

                        {hasManualTarget && (
                          <TargetEditor target={s.target} onChange={(t) => patchStepTarget(i, t)} />
                        )}

                        <div className="text-[10px] text-muted-foreground/70 font-mono truncate">{s.url}</div>
                      </div>
                      <div className="flex flex-col gap-1 opacity-0 group-hover:opacity-100 shrink-0">
                        <button onClick={() => patchStep(i, { disabled: !s.disabled })} className="p-1 rounded text-muted-foreground/40 hover:text-foreground" title={s.disabled ? "Enable step" : "Disable step (skip on playback, keep it around)"}>
                          {s.disabled ? <Eye className="w-3 h-3" /> : <EyeOff className="w-3 h-3" />}
                        </button>
                        <button onClick={() => duplicateStep(i)} className="p-1 rounded text-muted-foreground/40 hover:text-foreground" title="Duplicate step">
                          <Copy className="w-3 h-3" />
                        </button>
                        <button onClick={() => removeStep(i)} className="p-1 rounded text-muted-foreground/40 hover:text-red-500" title="Remove step">
                          <Trash2 className="w-3 h-3" />
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ol>

              {/* Add a new step, by hand — appends to the end; reorder into place with the
                  up/down arrows on the step it should sit next to. */}
              <div className="flex items-center gap-2 px-3 py-2.5 border-t border-border bg-muted/10">
                <select
                  value={newType}
                  onChange={(e) => setNewType(e.target.value)}
                  className="h-8 text-xs rounded-md border border-border bg-background px-2"
                >
                  {STEP_TYPES.map((t) => <option key={t.type} value={t.type}>{t.label}</option>)}
                </select>
                <Button size="sm" variant="outline" className="h-8" onClick={addStepAtEnd}>
                  <Plus className="w-3.5 h-3.5 mr-1.5" />Add step
                </Button>
                <span className="text-[10px] text-muted-foreground">Click/right-click/fill/paste/enter steps need a target — set it below the step once added.</span>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
