import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Textarea } from "../components/ui/textarea";
import {
  KeyRound, CheckCircle2, Circle, Clock, ExternalLink, Copy, Camera, Loader2,
  ChevronRight, ChevronLeft, ListOrdered,
} from "lucide-react";
import { toast } from "sonner";
import { useNavigate } from "react-router-dom";
import Markdown from "../components/Markdown";

// ─────────────────────────────────────────────────────────────────────────────
// Getting permission out of other people's dashboards.
//
// Every token and API permission has to be created by the user, in a flow that changes without notice.
// No app can do that part. What it can do is remove the twelve tabs and the half-remembered blog post:
// the exact page opens in the app's own browser **beside** the step, the value to paste is already on
// the clipboard, and the screenshot the user takes of their own dashboard is kept against the step.
//
// The guides ship no screenshots of their own, on purpose. A stock image of somebody else's Meta
// dashboard from a year ago looks authoritative and is wrong; your own capture is accurate for your
// account, and still accurate when you come back after a two-week review.
//
// Progress is only ever what you marked. The app cannot see inside a review queue, and pretending to
// know would be worse than asking.
// ─────────────────────────────────────────────────────────────────────────────

export default function AccessGuides() {
  const nav = useNavigate();
  const [guides, setGuides] = useState([]);
  const [open, setOpen] = useState(null);      // the loaded guide
  const [at, setAt] = useState(0);             // which step
  const [busy, setBusy] = useState("");
  const [note, setNote] = useState("");

  const load = async () => {
    try { setGuides((await api.listAccessGuides())?.guides || []); }
    catch (err) { toast.error(`${err}`); }
  };
  useEffect(() => { load(); }, []);

  const openGuide = async (id, step = 0) => {
    setBusy(`open-${id}`);
    try {
      const g = await api.accessGuide(id);
      setOpen(g);
      setAt(step);
      setNote(g.notes?.[g.steps[step]?.id] || "");
      primeStep(g.steps[step]);
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  // Opening a step does two things for you: puts its values on the clipboard in the order the form
  // wants them, and shows the page itself next door.
  const primeStep = async (step) => {
    if (!step) return;
    const lines = step.copy_lines || [];
    if (lines.length > 1) {
      try {
        await api.clipboardQueueSet({ items: lines, remember: true });
        toast.message(`${lines.length} values queued — paste, then "Advance" for the next.`);
      } catch { /* the copy button still works */ }
    } else if (lines.length === 1) {
      try { await api.clipboardAdd({ text: lines[0], kind: "system" }); } catch { /* ignore */ }
    }
  };

  const go = (delta) => {
    const next = Math.max(0, Math.min((open.steps.length - 1), at + delta));
    setAt(next);
    setNote(open.notes?.[open.steps[next].id] || "");
    primeStep(open.steps[next]);
  };

  const step = open?.steps?.[at];
  const done = new Set(open?.done || []);

  const toggle = async () => {
    setBusy("toggle");
    try {
      const r = await api.setAccessStep({ guide: open.id, step: step.id, done: !done.has(step.id) });
      setOpen({ ...open, done: r.done });
      load();
      // Moving on is what you want next nine times out of ten.
      if (!done.has(step.id) && at < open.steps.length - 1) go(1);
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const saveNote = async () => {
    setBusy("note");
    try {
      await api.saveAccessCapture({ guide: open.id, step: step.id, note });
      toast.success("Noted.");
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  /**
   * Keep a screenshot of your own dashboard against this step.
   *
   * A paste target rather than a clipboard read: the platform clipboard API hands back raw RGBA and
   * warns about deadlocking the whole app on Linux, while a paste event hands over a real image file
   * that a canvas turns into a PNG — and works identically on a phone, where "read the clipboard
   * whenever you like" is not a thing an app gets to do.
   */
  const onPaste = async (ev) => {
    const items = Array.from(ev.clipboardData?.items || []);
    const imageItem = items.find((i) => i.type?.startsWith("image/"));
    if (!imageItem) return;                     // a text paste is not a capture
    ev.preventDefault();
    const file = imageItem.getAsFile();
    if (!file) return;
    setBusy("capture");
    try {
      const dataUrl = await new Promise((resolve, reject) => {
        const img = new Image();
        const url = URL.createObjectURL(file);
        img.onload = () => {
          // Cap the long edge: a 4K screenshot per step would make the project repo unpleasant.
          const scale = Math.min(1, 1600 / Math.max(img.width, img.height));
          const canvas = document.createElement("canvas");
          canvas.width = Math.round(img.width * scale);
          canvas.height = Math.round(img.height * scale);
          canvas.getContext("2d").drawImage(img, 0, 0, canvas.width, canvas.height);
          URL.revokeObjectURL(url);
          resolve(canvas.toDataURL("image/png"));
        };
        img.onerror = () => { URL.revokeObjectURL(url); reject(new Error("that image could not be read")); };
        img.src = url;
      });
      await api.saveAccessCapture({ guide: open.id, step: step.id, image: dataUrl });
      toast.success("Saved against this step.");
      openGuide(open.id, at);
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  if (!open) {
    return (
      <div className="p-4 sm:p-6 lg:p-8 max-w-5xl mx-auto fade-in space-y-4">
        <div>
          <h1 className="text-4xl sm:text-5xl font-bold flex items-center gap-3">
            <KeyRound className="w-5 h-5 text-primary" /> Access &amp; permissions
          </h1>
          <p className="text-sm text-muted-foreground">
            Tokens and API permissions have to be created in someone else's dashboard. These walk you
            through it with the page open next door — and two of them save weeks.
          </p>
        </div>
        {guides.map((g) => (
          <Card key={g.id} className="p-4 flex items-start justify-between gap-3 flex-wrap">
            <div className="min-w-0">
              <div className="font-medium flex items-center gap-2">
                {g.label}
                {g.complete && <Badge variant="outline" className="text-[10px] text-emerald-400 border-emerald-500/40">done</Badge>}
                {!g.complete && g.done > 0 && (
                  <Badge variant="outline" className="text-[10px]">{g.done} of {g.steps}</Badge>
                )}
              </div>
              <div className="text-xs text-muted-foreground">{g.summary}</div>
              <div className="text-[11px] text-muted-foreground mt-1">→ {g.outcome}</div>
            </div>
            <Button size="sm" onClick={() => openGuide(g.id)} disabled={busy === `open-${g.id}`}>
              {busy === `open-${g.id}` ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : null}
              {g.done > 0 && !g.complete ? "Continue" : "Start"}
            </Button>
          </Card>
        ))}
        {!guides.length && (
          <Card className="p-4 text-sm text-muted-foreground">No guides available.</Card>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div className="flex items-center gap-2">
          <Button size="sm" variant="ghost" onClick={() => setOpen(null)}>
            <ChevronLeft className="w-3.5 h-3.5 mr-1" />All guides
          </Button>
          <span className="font-medium">{open.label}</span>
          <Badge variant="outline" className="text-[10px]">step {at + 1} of {open.steps.length}</Badge>
          {step.waiting && (
            <Badge variant="outline" className="text-[10px] text-amber-400 border-amber-500/40">
              <Clock className="w-3 h-3 mr-1" />waits on them, not you
            </Badge>
          )}
        </div>
        <div className="flex gap-1.5">
          <Button size="sm" variant="secondary" onClick={() => go(-1)} disabled={at === 0}>
            <ChevronLeft className="w-3.5 h-3.5" />
          </Button>
          <Button size="sm" variant="secondary" onClick={() => go(1)} disabled={at >= open.steps.length - 1}>
            <ChevronRight className="w-3.5 h-3.5" />
          </Button>
        </div>
      </div>

      {/* The step's own progress rail, so a two-week wait does not mean starting over. */}
      <div className="flex gap-1 flex-wrap">
        {open.steps.map((s, i) => (
          <button key={s.id} onClick={() => { setAt(i); setNote(open.notes?.[s.id] || ""); primeStep(s); }}
                  title={s.title}
                  className={`h-1.5 flex-1 min-w-[18px] rounded-full transition-colors ${
                    open.done?.includes(s.id) ? "bg-emerald-500"
                      : i === at ? "bg-primary" : "bg-muted"
                  }`} />
        ))}
      </div>

      {/* The whole card is the paste target, so Ctrl+V anywhere in the step keeps the screenshot. */}
      <Card className="p-4 space-y-3" onPaste={onPaste} tabIndex={-1}>
        <div className="flex items-start justify-between gap-2 flex-wrap">
          <h2 className="font-medium flex items-center gap-2">
            {open.done?.includes(step.id)
              ? <CheckCircle2 className="w-4 h-4 text-emerald-400" />
              : <Circle className="w-4 h-4 text-muted-foreground" />}
            {step.title}
          </h2>
          <div className="flex gap-1.5 flex-wrap">
            {step.url && (
              <Button size="sm" variant="secondary"
                      onClick={() => nav(`/browser?url=${encodeURIComponent(step.url)}&label=${encodeURIComponent(open.label)}`)}>
                <ExternalLink className="w-3.5 h-3.5 mr-1.5" />Open the page here
              </Button>
            )}
            {(step.copy_lines || []).length > 0 && (
              <Button size="sm" variant="secondary" onClick={() => {
                navigator.clipboard.writeText(step.copy);
                toast.success("Copied.");
              }}>
                <Copy className="w-3.5 h-3.5 mr-1.5" />Copy the values
              </Button>
            )}
            <span className="text-[11px] text-muted-foreground flex items-center gap-1"
                  title="Screenshot your own dashboard, then paste it here with Ctrl+V">
              {busy === "capture" ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Camera className="w-3.5 h-3.5" />}
              paste a screenshot
            </span>
            <Button size="sm" onClick={toggle} disabled={busy === "toggle"}>
              {open.done?.includes(step.id) ? "Not done after all" : "Done"}
            </Button>
          </div>
        </div>

        <Markdown text={step.body} />

        {(step.copy_lines || []).length > 1 && (
          <div className="text-xs text-muted-foreground flex items-start gap-1.5">
            <ListOrdered className="w-3.5 h-3.5 mt-0.5 shrink-0" />
            <span>
              {step.copy_lines.length} values are queued in order. Paste the first, then use the
              clipboard's <b>Advance</b> for the next — or a macro's "paste the next queued value" step.
            </span>
          </div>
        )}

        {open.captures?.[step.id.replace(/[^a-zA-Z0-9_-]/g, "-")] && (
          <div className="text-xs text-muted-foreground">
            A capture of your own screen is saved for this step.
          </div>
        )}

        <div className="space-y-1.5">
          <div className="text-[10px] uppercase tracking-widest text-muted-foreground">
            Notes — what you named things, which account, what the reviewer said
          </div>
          <Textarea value={note} onChange={(e) => setNote(e.target.value)} className="h-20 text-sm"
                    placeholder="App name, which Instagram account, ticket number…" data-no-i18n />
          <Button size="sm" variant="secondary" onClick={saveNote} disabled={busy === "note"}>Save note</Button>
        </div>
      </Card>

      <p className="text-xs text-muted-foreground">
        Progress here is what you marked. The app cannot see inside a review queue, and pretending to
        know would be worse than asking.
      </p>
    </div>
  );
}
