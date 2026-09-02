// Taking a spoken answer: which language it is in, and when the speaker has finished.
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import { resolveSpeechLanguage, createSilenceGate } from "../src/src/lib/speech.js";

const LANGUAGES = [
  { code: "en", label: "English", native: "English" },
  { code: "de", label: "German", native: "Deutsch" },
  { code: "he", label: "Hebrew", native: "עברית" },
];

// ── language ────────────────────────────────────────────────────────────────

test("with nothing explicit, the interface language is what is being spoken", () => {
  const r = resolveSpeechLanguage("", "de", LANGUAGES);
  assert.deepEqual(r, { code: "de", name: "German" });
});

test("both shapes come back, because the two listening paths need different ones", () => {
  // SpeechRecognition.lang wants the tag; the transcription prompt wants the name.
  const r = resolveSpeechLanguage("", "he", LANGUAGES);
  assert.equal(r.code, "he");
  assert.equal(r.name, "Hebrew");
});

test("an explicit language wins over the interface, by code, English name or native name", () => {
  assert.equal(resolveSpeechLanguage("de", "en", LANGUAGES).name, "German");
  assert.equal(resolveSpeechLanguage("German", "en", LANGUAGES).code, "de");
  assert.equal(resolveSpeechLanguage("Deutsch", "en", LANGUAGES).code, "de");
});

test("a language with no code is still a usable hint, not an error", () => {
  const r = resolveSpeechLanguage("Brazilian Portuguese", "en", LANGUAGES);
  assert.equal(r.name, "Brazilian Portuguese");
});

test("an unknown interface language falls back rather than returning nothing", () => {
  assert.equal(resolveSpeechLanguage("", "xx", LANGUAGES).code, "en");
  assert.equal(resolveSpeechLanguage("", "de", []).code, "en");
});

// ── silence gate ────────────────────────────────────────────────────────────

/** Feed a gate a list of `[elapsedMs, rms]` samples and report when it first says stop. */
function runGate(gate, samples) {
  for (const [t, rms] of samples) {
    if (gate.push(t, rms) === "stop") return t;
  }
  return null;
}

test("speech then silence stops the recording, long before the ceiling", () => {
  const gate = createSilenceGate({ maxMs: 8000, silenceMs: 1000, minMs: 700 });
  const samples = [];
  for (let t = 0; t < 300; t += 50) samples.push([t, 0.002]);   // calibrate on a quiet room
  for (let t = 300; t < 1500; t += 50) samples.push([t, 0.2]);  // talking
  for (let t = 1500; t < 8000; t += 50) samples.push([t, 0.002]); // finished
  const stoppedAt = runGate(gate, samples);
  assert.ok(stoppedAt !== null, "never stopped");
  assert.ok(stoppedAt >= 2500 && stoppedAt <= 2600, `stopped at ${stoppedAt}`);
});

test("a pause mid-sentence does not end the answer", () => {
  const gate = createSilenceGate({ maxMs: 8000, silenceMs: 1200, minMs: 700 });
  const samples = [];
  for (let t = 0; t < 300; t += 50) samples.push([t, 0.002]);
  for (let t = 300; t < 900; t += 50) samples.push([t, 0.2]);    // "let's do…"
  for (let t = 900; t < 1700; t += 50) samples.push([t, 0.002]); // …thinking, 800ms
  for (let t = 1700; t < 2600; t += 50) samples.push([t, 0.2]);  // "…the quiet one"
  for (let t = 2600; t < 3000; t += 50) samples.push([t, 0.002]);
  assert.equal(runGate(gate, samples), null, "cut the speaker off mid-sentence");
});

test("silence alone never stops it — somebody pausing before they start still gets to speak", () => {
  const gate = createSilenceGate({ maxMs: 5000, silenceMs: 300, minMs: 700 });
  const samples = [];
  for (let t = 0; t <= 5000; t += 50) samples.push([t, 0.001]);
  // Only the ceiling ends it, and only at the ceiling.
  assert.equal(runGate(gate, samples), 5000);
});

test("the ceiling always ends it, even while somebody is still talking", () => {
  const gate = createSilenceGate({ maxMs: 2000, silenceMs: 1000, minMs: 700 });
  const samples = [];
  for (let t = 0; t < 3000; t += 50) samples.push([t, t < 300 ? 0.002 : 0.3]);
  assert.equal(runGate(gate, samples), 2000);
});

test("a noisy room raises the bar instead of reading as endless speech", () => {
  const gate = createSilenceGate({ maxMs: 8000, silenceMs: 800, minMs: 700 });
  const samples = [];
  // A loud fan during calibration…
  for (let t = 0; t < 300; t += 50) samples.push([t, 0.06]);
  for (let t = 300; t < 1200; t += 50) samples.push([t, 0.4]);   // …a voice well above it…
  for (let t = 1200; t < 8000; t += 50) samples.push([t, 0.06]); // …and the fan again.
  const stoppedAt = runGate(gate, samples);
  assert.ok(stoppedAt !== null && stoppedAt < 8000, `room tone kept it open to ${stoppedAt}`);
});

test("nothing above the floor means nothing was heard, and the caller can skip transcribing", () => {
  const gate = createSilenceGate({ maxMs: 1000 });
  for (let t = 0; t < 1000; t += 50) gate.push(t, 0.001);
  assert.equal(gate.heardAnything(), false);
});

test("a clip is never cut shorter than minMs", () => {
  // silenceMs is tiny, so only minMs can hold the recording open.
  const gate = createSilenceGate({ maxMs: 8000, silenceMs: 50, minMs: 2000, calibrateMs: 100 });
  const samples = [];
  for (let t = 0; t < 100; t += 25) samples.push([t, 0.002]);
  for (let t = 100; t < 300; t += 25) samples.push([t, 0.3]);    // one short word
  for (let t = 300; t < 8000; t += 25) samples.push([t, 0.002]);
  const stoppedAt = runGate(gate, samples);
  assert.ok(stoppedAt >= 2000, `stopped at ${stoppedAt}, before minMs`);
});
