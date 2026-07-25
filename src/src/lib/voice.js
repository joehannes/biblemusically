import { api } from "./api";
import { matchOption } from "./voiceMatch.js";

// ─────────────────────────────────────────────────────────────────────────────
// Talking and listening.
//
// The guide reads its questions aloud and can take a spoken answer. Both directions cascade, because
// this app runs in WebKitGTK where the Web Speech API is mostly absent:
//
//   speak    backend Gemini voice (cached on disk) → the webview's own speechSynthesis → silence
//   listen   the webview's SpeechRecognition → MediaRecorder + backend transcription → nothing
//
// Silence is a valid outcome everywhere: no key, no microphone, no network, or the user simply having
// turned voice off must never block a typed answer.
//
// Matching a spoken answer to a choice is done locally first ("the second one", "let's do the quiet
// version") and only escalates to the AI when that is genuinely ambiguous — free tiers count
// requests, and a guide that burns them on "yes" would be a bad trade.
// ─────────────────────────────────────────────────────────────────────────────

const PREF_KEY = "studio:voice";

const defaults = { engine: "gemini", voice: "Kore", speak: false, listen: false };

export function voicePrefs() {
  try { return { ...defaults, ...JSON.parse(localStorage.getItem(PREF_KEY) || "{}") }; }
  catch { return { ...defaults }; }
}
export function setVoicePrefs(patch) {
  const next = { ...voicePrefs(), ...patch };
  try { localStorage.setItem(PREF_KEY, JSON.stringify(next)); } catch { /* ignore */ }
  return next;
}

// ── speaking ────────────────────────────────────────────────────────────────

let current = null;   // the Audio element in flight, so a new line interrupts the old one

export function stopSpeaking() {
  try { current?.pause(); } catch { /* ignore */ }
  current = null;
  try { window.speechSynthesis?.cancel(); } catch { /* ignore */ }
}

/** Read one line aloud. Resolves when playback ends (or immediately when muted/unavailable). */
export async function speak(text, { style, force = false } = {}) {
  const prefs = voicePrefs();
  if (!text || (!prefs.speak && !force) || prefs.engine === "off") return { spoken: false };
  stopSpeaking();

  if (prefs.engine !== "browser") {
    try {
      const r = await api.ttsSpeak({ text, voice: prefs.voice, style: style || null });
      if (r?.audio) {
        const audio = new Audio(`data:${r.mime || "audio/wav"};base64,${r.audio}`);
        current = audio;
        await audio.play();
        return await new Promise((resolve) => {
          audio.onended = () => resolve({ spoken: true, cached: r.cached, voice: r.voice });
          audio.onerror = () => resolve({ spoken: false, error: "playback failed" });
        });
      }
    } catch (err) {
      // Fall through to the system voice: a missing key or an overloaded model should downgrade,
      // not silence the assistant entirely.
      console.warn("[voice] backend speech unavailable:", err);
    }
  }

  const synth = typeof window !== "undefined" ? window.speechSynthesis : null;
  if (synth && typeof SpeechSynthesisUtterance !== "undefined") {
    return await new Promise((resolve) => {
      const u = new SpeechSynthesisUtterance(text);
      u.rate = 1.0;
      u.pitch = 1.0;
      u.onend = () => resolve({ spoken: true, engine: "browser" });
      u.onerror = () => resolve({ spoken: false });
      synth.speak(u);
    });
  }
  return { spoken: false, error: "no speech engine available" };
}

// ── listening ───────────────────────────────────────────────────────────────

function recognition() {
  const SR = typeof window !== "undefined" && (window.SpeechRecognition || window.webkitSpeechRecognition);
  if (!SR) return null;
  const rec = new SR();
  rec.continuous = false;
  rec.interimResults = false;
  return rec;
}

/** Record until silence (or `maxMs`) and return the transcript. `null` when nothing could be heard. */
export async function listen({ language, maxMs = 8000 } = {}) {
  const rec = recognition();
  if (rec) {
    if (language) rec.lang = language;
    return await new Promise((resolve) => {
      const done = (v) => { try { rec.stop(); } catch { /* ignore */ } resolve(v); };
      rec.onresult = (ev) => done(Array.from(ev.results).map((r) => r[0].transcript).join(" ").trim());
      rec.onerror = () => done(null);
      rec.onend = () => resolve(null);
      setTimeout(() => done(null), maxMs);
      try { rec.start(); } catch { resolve(null); }
    });
  }

  // No SpeechRecognition (the normal case in this webview): record and let the backend transcribe.
  if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") return null;
  let stream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
  } catch (err) {
    console.warn("[voice] microphone unavailable:", err);
    return null;
  }
  const chunks = [];
  const mime = MediaRecorder.isTypeSupported?.("audio/webm") ? "audio/webm" : "";
  const recorder = new MediaRecorder(stream, mime ? { mimeType: mime } : undefined);
  recorder.ondataavailable = (ev) => { if (ev.data?.size) chunks.push(ev.data); };
  const stopped = new Promise((resolve) => { recorder.onstop = resolve; });
  recorder.start();
  await new Promise((r) => setTimeout(r, maxMs));
  try { recorder.stop(); } catch { /* ignore */ }
  await stopped;
  stream.getTracks().forEach((t) => t.stop());
  if (!chunks.length) return null;

  const blob = new Blob(chunks, { type: recorder.mimeType || "audio/webm" });
  const base64 = await new Promise((resolve) => {
    const fr = new FileReader();
    fr.onloadend = () => resolve(String(fr.result || "").split(",")[1] || "");
    fr.readAsDataURL(blob);
  });
  if (!base64) return null;
  try {
    const r = await api.sttTranscribe({ audio: base64, mime: blob.type, language: language || null });
    return r?.text || null;
  } catch (err) {
    console.warn("[voice] transcription failed:", err);
    return null;
  }
}

/** Is a spoken answer possible at all here? Used to decide whether to show the mic button. */
export const voiceInputAvailable = () =>
  Boolean(
    (typeof window !== "undefined" && (window.SpeechRecognition || window.webkitSpeechRecognition)) ||
    (typeof navigator !== "undefined" && navigator.mediaDevices?.getUserMedia && typeof MediaRecorder !== "undefined"),
  );

/**
 * Interpret a spoken answer: locally when it is clear, with the AI when it is not.
 * `question` only travels when the AI is actually needed.
 */
export async function interpretAnswer(transcript, options, { recommended, question } = {}) {
  const local = matchOption(transcript, options, { recommended });
  if (local.option && local.confidence >= 0.5) return local;
  if (local.reason === "declined") return local;
  try {
    const r = await api.guideInterpret({
      answer: transcript,
      question: question || "",
      options: options.map((o) => ({ id: o.id, label: o.label, hint: o.hint || "" })),
    });
    if (r?.option) return { option: r.option, confidence: r.confidence ?? 0.7, reason: "ai" };
  } catch (err) {
    console.warn("[voice] interpretation failed:", err);
  }
  return local;
}
