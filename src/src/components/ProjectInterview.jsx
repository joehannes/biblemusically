import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Textarea } from "./ui/textarea";
import { Compass, Loader2, Mic, Check, ArrowRight, Volume2, VolumeX } from "lucide-react";
import { toast } from "sonner";
import { speak, stopSpeaking, listen, voiceInputAvailable, voicePrefs, setVoicePrefs } from "../lib/voice";

// ─────────────────────────────────────────────────────────────────────────────
// The conversation that starts a project.
//
// The Brief is eight empty boxes, and everything downstream reads it — lyrics, per-channel style,
// imagery, characters. Eight empty boxes is the same wall as thirty-five nav entries: a person who
// does not yet know what the app makes cannot say what "storyline" means for their project.
//
// So it is asked as a conversation, one question at a time, and the questions **cascade** — the
// backend sees every answer so far and picks the next thing worth knowing, so a children's-story
// project and a grief-poetry project stop sharing a path after the first answer. Options are a way
// in rather than a cage: every question also takes your own words, and the whole thing can be ended
// at any point with what has been said kept.
//
// It reads aloud and takes a spoken answer through the same layer the guided flows use, so the
// answer to "what is this project for?" can be given the way somebody would actually say it.
// ─────────────────────────────────────────────────────────────────────────────

export default function ProjectInterview({ project, onDone, onSaved }) {
  const [question, setQuestion] = useState(null);
  const [answers, setAnswers] = useState({});
  const [free, setFree] = useState("");
  const [busy, setBusy] = useState(false);
  const [listening, setListening] = useState(false);
  const [prefs, setPrefs] = useState(() => voicePrefs());
  const spokenFor = useRef(null);

  const ask = useCallback(async (soFar, finish = false) => {
    setBusy(true);
    try {
      const r = await api.projectInterviewNext({ project_id: project.id, answers: soFar, finish });
      if (r?.done || !r?.question) { setQuestion(null); onDone?.(soFar); return; }
      setQuestion(r.question);
      setFree("");
    } catch (err) {
      toast.error(`The guide could not think of a question: ${err}`);
      setQuestion(null);
    } finally { setBusy(false); }
  }, [project?.id, onDone]);

  useEffect(() => { if (project?.id) ask({}); /* eslint-disable-next-line */ }, [project?.id]);

  // Read the question and its options, once per question. Failure is silent by design: a missing key
  // or a webview without speech must never stop somebody answering.
  useEffect(() => {
    if (!question || !prefs.speak) return;
    const key = `${question.field}:${question.question}`;
    if (spokenFor.current === key) return;
    spokenFor.current = key;
    const opts = (question.options || []).slice(0, 4).map((o, i) => `${i + 1}. ${o.label}`).join(". ");
    speak([question.question, opts].filter(Boolean).join(" ")).catch(() => {});
    return () => stopSpeaking();
  }, [question, prefs.speak]);

  const answer = async (text) => {
    const value = String(text || "").trim();
    if (!value) return;
    stopSpeaking();
    const next = { ...answers, [question.field]: value };
    setAnswers(next);
    // Saved as we go rather than at the end: somebody who closes this half-way has still told the
    // project four true things about itself, and losing them would teach them not to start again.
    api.projectInterviewSave(project.id, next).then(() => onSaved?.()).catch(() => {});
    await ask(next);
  };

  const answerAloud = async () => {
    if (listening) return;
    stopSpeaking();
    setListening(true);
    try {
      const heard = await listen({ maxMs: 20000 });
      if (!heard) return void toast.message("I didn't catch that — try again, or type it.");
      // Straight into the box rather than straight into the answer: this is somebody describing
      // their own project in their own words, and a transcription they cannot correct first is a
      // worse offer than no microphone at all.
      setFree((f) => (f ? `${f.trim()} ${heard}` : heard));
    } finally { setListening(false); }
  };

  const finishNow = async () => {
    stopSpeaking();
    await ask(answers, true);
  };

  const toggleSpeech = () => {
    const next = setVoicePrefs({ speak: !prefs.speak });
    setPrefs(next);
    if (!next.speak) stopSpeaking();
    else spokenFor.current = null;
  };

  if (!project) return null;
  if (!question && !busy) return null;

  const answeredCount = Object.keys(answers).length;

  return (
    <Card className="p-4 mb-5 border-primary/30 space-y-3" data-testid="project-interview">
      <div className="flex items-start gap-2">
        <div className="p-1.5 rounded-md bg-primary/10 shrink-0"><Compass className="w-4 h-4 text-primary" /></div>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-semibold leading-tight">Getting to know this project</div>
          {/* Both branches as JSX text, not as strings inside an expression: a string in an
              expression is invisible to the extractor, so it would be paid for at runtime by the
              translation budget instead of shipping in the catalogues for free. */}
          <div className="text-[11px] text-muted-foreground">
            {answeredCount > 0
              ? <><span className="text-mono">{answeredCount}</span> <span>answered — every one shapes what gets written from here on.</span></>
              : <span>A few questions, in your own words. Everything you say goes into the brief.</span>}
          </div>
        </div>
        <button onClick={toggleSpeech} title={prefs.speak ? "Stop reading these aloud" : "Read these aloud"}
                className={`p-1.5 rounded shrink-0 hover:bg-muted/40 ${prefs.speak ? "text-primary" : "text-muted-foreground"}`}>
          {prefs.speak ? <Volume2 className="w-4 h-4" /> : <VolumeX className="w-4 h-4" />}
        </button>
      </div>

      {busy && !question && (
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
              <button key={o.label} onClick={() => answer(o.label)} disabled={busy}
                      className="text-left rounded-lg border border-border p-2.5 text-sm
                                 hover:border-primary/60 hover:bg-primary/5 transition-all disabled:opacity-50">
                {o.label}
              </button>
            ))}
          </div>

          <Textarea
            rows={2}
            value={free}
            onChange={(e) => setFree(e.target.value)}
            data-testid="interview-free-text"
            placeholder="Or say it your own way — this is the answer that gets kept."
            className="text-sm"
          />

          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-1">
              {voiceInputAvailable() && (
                <Button size="sm" variant={listening ? "default" : "ghost"} onClick={answerAloud} disabled={busy}>
                  {listening
                    ? <><Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />Listening…</>
                    : <><Mic className="w-3.5 h-3.5 mr-1.5" />Say it</>}
                </Button>
              )}
              <Button size="sm" variant="ghost" onClick={finishNow} disabled={busy}
                      className="text-xs text-muted-foreground">
                <Check className="w-3.5 h-3.5 mr-1.5" />That's enough for now
              </Button>
            </div>
            <Button size="sm" onClick={() => answer(free)} disabled={busy || !free.trim()}>
              {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    : <>Next<ArrowRight className="w-3.5 h-3.5 ml-1.5" /></>}
            </Button>
          </div>
        </div>
      )}
    </Card>
  );
}
