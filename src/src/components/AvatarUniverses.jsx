import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Input } from "./ui/input";
import { Textarea } from "./ui/textarea";
import {
  Users, Loader2, Mic, Check, ArrowRight, Volume2, VolumeX, Sparkles,
  Trash2, Shuffle, Languages, Plus, X,
} from "lucide-react";
import { toast } from "sonner";
import { speak, stopSpeaking, listen, voiceInputAvailable, voicePrefs, setVoicePrefs } from "../lib/voice";

// ─────────────────────────────────────────────────────────────────────────────
// Avatar universes.
//
// An edition is written for somebody whether or not anybody said who. This is where that somebody
// gets written down — a person, plus the givens their world supplies — and once it is written down,
// a second edition for a different reader costs one click instead of a second act of authorship.
//
// Three things happen here, in the order somebody would do them:
//   1. **Describe one reader**, through the same cascading interview the project uses, at whichever
//      depth they have patience for. A sketch is three axes; a deep one is nine.
//   2. **Derive neighbours** by naming the axes to move. What differs between two universes is a
//      fact you can read off the card rather than a diff you have to infer.
//   3. **Retell an edition** through any of them: written again in that reader's language and world,
//      page for page, so the art that already exists still belongs to the pages it was made for.
// ─────────────────────────────────────────────────────────────────────────────

const DEPTH_FALLBACK = [{ id: "grounded", label: "Grounded", hint: "", axes: 6, questions: 9 }];

export default function AvatarUniverses({ projectId, editions = [], onRetold }) {
  const [cat, setCat] = useState({ axes: [], depths: DEPTH_FALLBACK, avatar_fields: [] });
  const [universes, setUniverses] = useState([]);
  const [active, setActive] = useState(null);
  const [mode, setMode] = useState("list");   // list | interview | derive
  const [busy, setBusy] = useState("");

  // ── interview state ──────────────────────────────────────────────────────
  const [depth, setDepth] = useState("grounded");
  const [question, setQuestion] = useState(null);
  const [answers, setAnswers] = useState({});
  const [free, setFree] = useState("");
  const [listening, setListening] = useState(false);
  const [prefs, setPrefs] = useState(() => voicePrefs());
  const spokenFor = useRef(null);

  // ── derivation state ─────────────────────────────────────────────────────
  const [vary, setVary] = useState(["language", "region"]);
  const [count, setCount] = useState(3);
  const [proposed, setProposed] = useState(null);

  const load = useCallback(async () => {
    try {
      const r = await api.universeList(projectId || null);
      const list = r?.universes || [];
      setUniverses(list);
      setActive((a) => list.find((u) => u.id === a?.id) || list[0] || null);
    } catch { setUniverses([]); }
  }, [projectId]);

  useEffect(() => { api.universeAxes().then((r) => r && setCat(r)).catch(() => {}); }, []);
  useEffect(() => { load(); }, [load]);

  // ── the interview ────────────────────────────────────────────────────────

  const ask = useCallback(async (soFar, finish = false) => {
    setBusy("ask");
    try {
      const r = await api.universeInterviewNext({
        project_id: projectId || "", answers: soFar, depth, finish,
      });
      if (r?.done || !r?.question) { setQuestion(null); return r; }
      setQuestion(r.question);
      setFree("");
      return r;
    } catch (err) {
      toast.error(`The guide could not think of a question: ${err}`);
      setQuestion(null);
      return null;
    } finally { setBusy(""); }
  }, [projectId, depth]);

  useEffect(() => {
    if (!question || !prefs.speak) return;
    const key = `${question.field}:${question.question}`;
    if (spokenFor.current === key) return;
    spokenFor.current = key;
    const opts = (question.options || []).slice(0, 4).map((o, i) => `${i + 1}. ${o.label}`).join(". ");
    speak([question.question, opts].filter(Boolean).join(" ")).catch(() => {});
    return () => stopSpeaking();
  }, [question, prefs.speak]);

  const startInterview = async () => {
    setAnswers({});
    setProposed(null);
    setMode("interview");
    await ask({});
  };

  const answer = async (text) => {
    const value = String(text || "").trim();
    if (!value || !question) return;
    stopSpeaking();
    const next = { ...answers, [question.field]: value };
    setAnswers(next);
    const r = await ask(next);
    if (r?.done) await saveFromAnswers(next);
  };

  const answerAloud = async () => {
    if (listening) return;
    stopSpeaking();
    setListening(true);
    try {
      const heard = await listen({ maxMs: 20000 });
      if (!heard) return void toast.message("I didn't catch that — try again, or type it.");
      setFree((f) => (f ? `${f.trim()} ${heard}` : heard));
    } finally { setListening(false); }
  };

  // Saved when the conversation ends rather than as it goes: a half-described person in the picker
  // is worse than none, because the retelling would quietly use the blanks.
  const saveFromAnswers = async (final) => {
    if (!Object.keys(final || {}).length) { setMode("list"); return; }
    setBusy("save");
    try {
      const u = await api.universeSave({ project_id: projectId || "", answers: final, depth });
      toast.success("Saved.");
      setMode("list");
      setActive(u);
      await load();
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const finishNow = async () => {
    stopSpeaking();
    await ask(answers, true);
    await saveFromAnswers(answers);
  };

  const toggleSpeech = () => {
    const next = setVoicePrefs({ speak: !prefs.speak });
    setPrefs(next);
    if (!next.speak) stopSpeaking();
    else spokenFor.current = null;
  };

  // ── deriving ─────────────────────────────────────────────────────────────

  const derive = async (save) => {
    if (!active) return;
    setBusy("derive");
    try {
      const r = await api.universeDerive({
        id: active.id, vary, count: Number(count) || 3, save,
      });
      if (save) {
        toast.success(`${r.universes.length} added.`);
        setProposed(null);
        setMode("list");
        await load();
      } else {
        setProposed(r);
      }
    } catch (err) { toast.error(`${err}`, { duration: 8000 }); }
    finally { setBusy(""); }
  };

  const remove = async (u) => {
    if (!window.confirm(`Delete “${u.name}”? Editions already retold through it are kept.`)) return;
    try {
      await api.universeDelete(u.id);
      await load();
    } catch (err) { toast.error(`${err}`); }
  };

  // ── retelling ────────────────────────────────────────────────────────────

  const [retellFrom, setRetellFrom] = useState("");
  const retell = async () => {
    if (!active || !retellFrom) return;
    setBusy("retell");
    try {
      const ed = await api.universeRetell({ edition_id: retellFrom, universe_id: active.id });
      toast.success(`Retold for ${active.name}: ${ed.pages?.length || 0} pages.`);
      onRetold?.(ed);
    } catch (err) { toast.error(`${err}`, { duration: 8000 }); }
    finally { setBusy(""); }
  };

  const axisLabel = (id) => cat.axes.find((a) => a.id === id)?.label || id;
  const toggleVary = (id) =>
    setVary((v) => (v.includes(id) ? v.filter((x) => x !== id) : [...v, id]));

  // ── render ───────────────────────────────────────────────────────────────

  if (mode === "interview") {
    const total = cat.depths.find((d) => d.id === depth)?.questions || 9;
    const answered = Object.keys(answers).length;
    return (
      <Card className="p-4 space-y-3 border-primary/30">
        <div className="flex items-start gap-2">
          <div className="p-1.5 rounded-md bg-primary/10 shrink-0"><Users className="w-4 h-4 text-primary" /></div>
          <div className="min-w-0 flex-1">
            <div className="text-sm font-semibold leading-tight">Describing one reader</div>
            <div className="text-[11px] text-muted-foreground">
              <span className="text-mono">{answered}</span>
              <span> of about </span>
              <span className="text-mono">{total}</span>
              <span> — every answer changes what a retelling can say.</span>
            </div>
          </div>
          <button onClick={toggleSpeech} title={prefs.speak ? "Stop reading these aloud" : "Read these aloud"}
                  className={`p-1.5 rounded shrink-0 hover:bg-muted/40 ${prefs.speak ? "text-primary" : "text-muted-foreground"}`}>
            {prefs.speak ? <Volume2 className="w-4 h-4" /> : <VolumeX className="w-4 h-4" />}
          </button>
          <button onClick={() => { stopSpeaking(); setMode("list"); }} className="p-1.5 rounded shrink-0 hover:bg-muted/40 text-muted-foreground">
            <X className="w-4 h-4" />
          </button>
        </div>

        {busy === "ask" && !question && (
          <div className="flex items-center gap-2 text-[11px] text-muted-foreground py-3">
            <Loader2 className="w-3.5 h-3.5 animate-spin" />Thinking about what to ask…
          </div>
        )}

        {question && (
          <div className="space-y-2.5">
            <div>
              <div className="text-sm font-medium">{question.question}</div>
              {question.why && <div className="text-[11px] text-muted-foreground mt-0.5">{question.why}</div>}
            </div>
            <div className={`grid gap-2 ${(question.options || []).length > 2 ? "sm:grid-cols-2" : ""}`}>
              {(question.options || []).map((o) => (
                <button key={o.label} onClick={() => answer(o.label)} disabled={!!busy}
                        className="text-left rounded-lg border border-border p-2.5 text-sm
                                   hover:border-primary/60 hover:bg-primary/5 transition-all disabled:opacity-50">
                  {o.label}
                </button>
              ))}
            </div>
            <Textarea rows={2} value={free} onChange={(e) => setFree(e.target.value)}
                      data-testid="universe-free-text"
                      placeholder="Or say it your own way — this is the answer that gets kept."
                      className="text-sm" />
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <div className="flex items-center gap-1">
                {voiceInputAvailable() && (
                  <Button size="sm" variant={listening ? "default" : "ghost"} onClick={answerAloud} disabled={!!busy}>
                    {listening
                      ? <><Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />Listening…</>
                      : <><Mic className="w-3.5 h-3.5 mr-1.5" />Say it</>}
                  </Button>
                )}
                <Button size="sm" variant="ghost" onClick={finishNow} disabled={!!busy}
                        className="text-xs text-muted-foreground">
                  <Check className="w-3.5 h-3.5 mr-1.5" />That's enough
                </Button>
              </div>
              <Button size="sm" onClick={() => answer(free)} disabled={!!busy || !free.trim()}>
                {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      : <>Next<ArrowRight className="w-3.5 h-3.5 ml-1.5" /></>}
              </Button>
            </div>
          </div>
        )}
      </Card>
    );
  }

  if (mode === "derive" && active) {
    return (
      <Card className="p-4 space-y-4">
        <div className="flex items-start justify-between gap-2">
          <div>
            <div className="text-sm font-semibold">Neighbours of {active.name}</div>
            <p className="text-[11px] text-muted-foreground max-w-xl">
              Pick what moves. Everything you do not pick is held exactly as it is, so what differs
              between two of these is something you decided rather than something that drifted.
            </p>
          </div>
          <button onClick={() => { setProposed(null); setMode("list"); }}
                  className="p-1.5 rounded hover:bg-muted/40 text-muted-foreground"><X className="w-4 h-4" /></button>
        </div>

        <div className="flex flex-wrap gap-1.5">
          {cat.axes.map((a) => (
            <button key={a.id} onClick={() => toggleVary(a.id)}
                    className={`text-xs rounded-full border px-2.5 py-1 transition-all
                                ${vary.includes(a.id) ? "border-primary/60 bg-primary/10 text-primary" : "border-border text-muted-foreground hover:border-primary/40"}`}>
              {a.label}
            </button>
          ))}
        </div>

        <div className="flex items-end gap-2 flex-wrap">
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">How many</label>
            <Input type="number" min={1} max={8} value={count} onChange={(e) => setCount(e.target.value)}
                   className="w-24 h-9" />
          </div>
          <Button onClick={() => derive(false)} disabled={busy === "derive" || !vary.length}>
            {busy === "derive" ? <Loader2 className="w-4 h-4 animate-spin mr-1.5" /> : <Shuffle className="w-4 h-4 mr-1.5" />}
            Show me
          </Button>
        </div>

        {proposed && (
          <div className="space-y-2">
            {proposed.source === "offline" && (
              <p className="text-[11px] text-amber-600 dark:text-amber-400">
                No AI answered, so these came from a table: real coordinates, but nobody's life. Give
                each of them a name and a sentence before you retell anything through them.
              </p>
            )}
            <div className="grid sm:grid-cols-2 gap-2">
              {proposed.universes.map((u, i) => (
                <div key={i} className="rounded-lg border border-border p-2.5 space-y-1.5">
                  <div className="text-sm font-medium">{u.name}</div>
                  {u.avatar?.who && <div className="text-[11px] text-muted-foreground">{u.avatar.who}</div>}
                  <div className="flex flex-wrap gap-1">
                    {(u.varied || []).map((id) => (
                      <Badge key={id} variant="outline" className="text-[9px]">
                        {axisLabel(id)}: {u.axes?.[id] || "—"}
                      </Badge>
                    ))}
                  </div>
                </div>
              ))}
            </div>
            <Button size="sm" onClick={() => derive(true)} disabled={busy === "derive"}>
              <Plus className="w-3.5 h-3.5 mr-1.5" />Keep these
            </Button>
          </div>
        )}
      </Card>
    );
  }

  return (
    <div className="space-y-3">
      <Card className="p-4 space-y-3">
        <div className="flex items-start justify-between gap-3 flex-wrap">
          <div>
            <div className="text-sm font-semibold flex items-center gap-2">
              <Users className="w-4 h-4 text-primary" />Who this is for
            </div>
            <p className="text-[11px] text-muted-foreground max-w-2xl">
              Every edition is written for somebody. Say who, and the same book can be written again
              for a different reader — in their language, from where they stand — without being
              written again from nothing.
            </p>
          </div>
          <div className="flex items-center gap-1.5">
            <div className="flex gap-1">
              {cat.depths.map((d) => (
                <button key={d.id} onClick={() => setDepth(d.id)} title={d.hint}
                        className={`text-xs rounded-md border px-2 py-1 transition-all
                                    ${depth === d.id ? "border-primary/60 bg-primary/10 text-primary" : "border-border text-muted-foreground hover:border-primary/40"}`}>
                  {d.label}
                </button>
              ))}
            </div>
            <Button size="sm" onClick={startInterview} disabled={!!busy}>
              <Sparkles className="w-3.5 h-3.5 mr-1.5" />Describe one
            </Button>
          </div>
        </div>
        <p className="text-[11px] text-muted-foreground">
          {cat.depths.find((d) => d.id === depth)?.hint || ""}
        </p>
      </Card>

      {!universes.length && (
        <Card className="p-6 text-center text-sm text-muted-foreground">
          Nobody described yet. A reader takes about a minute at the sketch depth.
        </Card>
      )}

      {universes.length > 0 && (
        <div className="grid sm:grid-cols-2 gap-2">
          {universes.map((u) => (
            <button key={u.id} onClick={() => setActive(u)}
                    className={`text-left rounded-lg border p-3 space-y-1.5 transition-all
                                ${active?.id === u.id ? "border-primary/60 bg-primary/5" : "border-border hover:border-primary/40"}`}>
              <div className="flex items-start justify-between gap-2">
                <div className="text-sm font-medium">{u.name}</div>
                <div className="flex items-center gap-1 shrink-0">
                  {u.derived_from && <Badge variant="outline" className="text-[9px]">derived</Badge>}
                  <Badge variant="outline" className="text-[9px]">{u.depth}</Badge>
                </div>
              </div>
              {u.avatar?.who && <div className="text-[11px] text-muted-foreground">{u.avatar.who}</div>}
              <div className="flex flex-wrap gap-1">
                {Object.entries(u.axes || {}).slice(0, 4).map(([k, v]) => (
                  <Badge key={k} variant="secondary" className="text-[9px] font-normal">{v}</Badge>
                ))}
              </div>
            </button>
          ))}
        </div>
      )}

      {active && (
        <Card className="p-4 space-y-3">
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="text-sm font-semibold">{active.name}</div>
            <div className="flex items-center gap-1.5">
              <Button size="sm" variant="secondary" onClick={() => { setProposed(null); setMode("derive"); }}>
                <Shuffle className="w-3.5 h-3.5 mr-1.5" />Neighbours
              </Button>
              <Button size="sm" variant="ghost" onClick={() => remove(active)} className="text-destructive">
                <Trash2 className="w-3.5 h-3.5" />
              </Button>
            </div>
          </div>

          <div className="grid sm:grid-cols-2 gap-x-4 gap-y-1.5">
            {cat.axes.filter((a) => active.axes?.[a.id]).map((a) => (
              <div key={a.id} className="text-xs">
                <span className="text-muted-foreground">{a.label}: </span>
                <span>{active.axes[a.id]}</span>
              </div>
            ))}
          </div>

          <div className="pt-2 border-t border-border/60 space-y-2">
            <div className="text-xs font-medium flex items-center gap-1.5">
              <Languages className="w-3.5 h-3.5 text-primary" />Retell an edition for them
            </div>
            <p className="text-[11px] text-muted-foreground">
              Written again rather than translated: same beat on every page, so the art already made
              still belongs where it is — but the words, the images and how much is explained are
              chosen for this reader.
            </p>
            <div className="flex items-end gap-2 flex-wrap">
              <select value={retellFrom} onChange={(e) => setRetellFrom(e.target.value)}
                      className="h-9 rounded-md border border-border bg-background px-2 text-sm max-w-xs">
                <option value="">Pick an edition…</option>
                {editions.filter((e) => !e.retold_from).map((e) => (
                  <option key={e.id} value={e.id}>{e.title} ({e.pages?.length || 0} pages)</option>
                ))}
              </select>
              <Button size="sm" onClick={retell} disabled={busy === "retell" || !retellFrom}>
                {busy === "retell" ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" />
                                   : <Languages className="w-3.5 h-3.5 mr-1.5" />}
                Retell it
              </Button>
            </div>
          </div>
        </Card>
      )}
    </div>
  );
}
