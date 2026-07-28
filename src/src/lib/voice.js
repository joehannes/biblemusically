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

const defaults = { engine: "gemini", voice: "Kore", systemVoice: "", speak: false, listen: false };

// ── system voices ───────────────────────────────────────────────────────────
// The voices the platform itself ships: free, offline, no key, and no request budget. Android's WebView
// has a full set; WebKitGTK on Linux usually has none at all, which is why Gemini is the default rather
// than the fallback.
//
// `getVoices()` is famously empty on the first call — the list arrives asynchronously — so this waits
// for `voiceschanged` once rather than reporting "no voices" to someone who has plenty.
let systemVoicesCache = null;

export async function systemVoices() {
  if (systemVoicesCache) return systemVoicesCache;
  const synth = typeof window !== "undefined" ? window.speechSynthesis : null;
  if (!synth?.getVoices) return (systemVoicesCache = []);

  let list = synth.getVoices();
  if (!list.length) {
    list = await new Promise((resolve) => {
      const done = () => resolve(synth.getVoices() || []);
      // Both paths, because some engines fire the event and some only populate late.
      synth.addEventListener?.("voiceschanged", done, { once: true });
      setTimeout(done, 1200);
    });
  }
  systemVoicesCache = (list || []).map((v) => ({
    id: v.voiceURI || v.name,
    name: v.name,
    lang: v.lang,
    local: v.localService !== false,
    default: !!v.default,
  }));
  return systemVoicesCache;
}

/** Resolve a stored system-voice id back to the live SpeechSynthesisVoice object. */
function pickSystemVoice(id) {
  const synth = window.speechSynthesis;
  const all = synth?.getVoices?.() || [];
  if (!all.length) return null;
  return all.find((v) => (v.voiceURI || v.name) === id) || all.find((v) => v.default) || all[0];
}

export function voicePrefs() {
  try { return { ...defaults, ...JSON.parse(localStorage.getItem(PREF_KEY) || "{}") }; }
  catch { return { ...defaults }; }
}
/**
 * Has the user actually chosen, or are they still on the defaults?
 *
 * `voicePrefs()` merges defaults in, so it can never answer this — and the difference matters: a
 * first-run guide may quietly preselect the free offline voice, but must not overwrite a Gemini
 * voice somebody deliberately picked.
 */
export function voicePrefsChosen() {
  try { return localStorage.getItem(PREF_KEY) != null; } catch { return false; }
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
    // Make sure the list is populated before choosing, or the first line of a session always uses the
    // default voice regardless of what was picked.
    await systemVoices();
    return await new Promise((resolve) => {
      const u = new SpeechSynthesisUtterance(text);
      const chosen = prefs.systemVoice ? pickSystemVoice(prefs.systemVoice) : null;
      if (chosen) { u.voice = chosen; u.lang = chosen.lang; }
      u.rate = 1.0;
      u.pitch = 1.0;
      u.onend = () => resolve({ spoken: true, engine: "browser", voice: chosen?.name });
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

/**
 * Ask for the microphone, and say what happened.
 *
 * Separate from `listen()` because the permission prompt should appear during setup, when the user is
 * expecting it and can be told why — not in the middle of a guided question, where a browser dialog
 * appearing over the question is how people end up denying it by reflex. A denial is remembered by the
 * platform, so the first ask is the one that matters.
 */
export async function requestMicPermission() {
  if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
    return { granted: false, reason: "This device exposes no microphone to the app." };
  }
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    // Release it immediately: holding the mic open would show a recording indicator for the whole
    // session and, on a phone, keep the hardware awake.
    stream.getTracks().forEach((t) => t.stop());
    return { granted: true };
  } catch (err) {
    const name = err?.name || "";
    return {
      granted: false,
      reason: name === "NotAllowedError"
        ? "Microphone access was denied. Your platform remembers that, so it has to be re-enabled in system or app settings."
        : name === "NotFoundError"
        ? "No microphone was found."
        : `Microphone unavailable: ${err}`,
    };
  }
}

/** Has the microphone already been granted, without prompting? Undefined where the API is missing. */
export async function micPermissionState() {
  try {
    const status = await navigator.permissions?.query?.({ name: "microphone" });
    return status?.state;            // "granted" | "denied" | "prompt"
  } catch { return undefined; }
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
