// Which engines a picker is allowed to offer.
//
// Suno and Midjourney are reached by driving a session the user is logged into, which their terms
// reserve for their own interface — so the account that gets suspended is the user's. They stay in
// the code (an official Suno API is reportedly coming) but must not be something anybody arrives at
// by accident. These tests hold that line in both directions: hidden by default, and never hidden so
// thoroughly that somebody already using one cannot see it to switch away.
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  MUSIC_ENGINES, IMAGE_ENGINES, visibleMusicEngines, visibleImageEngines,
  fallbackMusicEngines, musicEngine, imageEngine, isRisky, isPaid, priceLine,
} from "../src/src/lib/engineCapabilities.js";

const ids = (pairs) => pairs.map(([id]) => id);

test("nothing that automates the user's own account is offered by default", () => {
  assert.deepEqual(ids(visibleMusicEngines({}, "heartmula")),
    ["acestep", "heartmula", "riffusion", "elevenlabs"]);
  assert.deepEqual(ids(visibleImageEngines({}, "flux")),
    ["flux", "comfyui", "leonardo", "fal", "ideogram", "recraft", "gemini"]);
  // Including for a settings object that has never heard of the flag.
  assert.ok(!ids(visibleMusicEngines(undefined, "")).includes("suno"));
  assert.ok(!ids(visibleImageEngines(null, "")).includes("midjourney"));
});

test("switching the flag on offers them, and each one says whose account is at risk", () => {
  const on = { show_risky_engines: true };
  assert.ok(ids(visibleMusicEngines(on, "heartmula")).includes("suno"));
  assert.ok(ids(visibleImageEngines(on, "flux")).includes("midjourney"));

  for (const engine of [MUSIC_ENGINES.suno, IMAGE_ENGINES.midjourney]) {
    assert.ok(isRisky(engine), `${engine.label} must be marked risky`);
    assert.match(engine.riskNote, /your(s)? own|yours/i,
      `${engine.label}'s warning must be about the user's own account, not an abstraction`);
  }
});

test("the engine already in use is always offered, hidden or not", () => {
  // A picker whose value is missing from its own list renders blank, and somebody on an engine they
  // cannot see has no way to choose to leave it.
  assert.ok(ids(visibleMusicEngines({}, "suno")).includes("suno"));
  assert.ok(ids(visibleImageEngines({}, "midjourney")).includes("midjourney"));
  // But only that one — selecting Suno must not also reveal Midjourney.
  assert.ok(!ids(visibleImageEngines({}, "flux")).includes("midjourney"));
});

test("no free engine is marked risky, so the warning keeps its meaning", () => {
  for (const id of ["acestep", "heartmula", "riffusion", "elevenlabs"]) {
    assert.ok(!isRisky(MUSIC_ENGINES[id]), id);
  }
  for (const id of ["flux", "comfyui", "gemini"]) assert.ok(!isRisky(IMAGE_ENGINES[id]), id);
  // Paid is not the same as risky, and conflating them would either hide four legitimate engines or
  // dilute a warning that is about somebody's account being suspended.
  for (const id of ["leonardo", "fal", "ideogram", "recraft"]) {
    assert.ok(!isRisky(IMAGE_ENGINES[id]), `${id} costs money but risks no account`);
  }
});

test("a paid engine says so, and says how much, before it is ever selected", () => {
  // Finding out what an engine costs from an invoice is the failure this prevents.
  const paid = Object.entries(IMAGE_ENGINES).filter(([, e]) => isPaid(e)).map(([id]) => id);
  assert.deepEqual(paid, ["leonardo", "fal", "ideogram", "recraft"]);
  for (const id of paid) {
    const e = IMAGE_ENGINES[id];
    assert.ok(e.priceHint > 0, `${id} has no price`);
    assert.match(e.note, /paid/, `${id}'s picker row must say it is paid`);
    assert.match(priceLine(e), /^about \$\d+\.\d{3} an image$/, priceLine(e));
  }
  for (const id of ["flux", "comfyui", "gemini", "midjourney"]) {
    assert.ok(!isPaid(IMAGE_ENGINES[id]), `${id} must not be marked paid`);
    assert.equal(priceLine(IMAGE_ENGINES[id]), "free");
  }
});

test("only the engines that honour a negative prompt advertise one", () => {
  // A "things to avoid" box that does nothing is worse than no box: the user believes the
  // restraint was applied. FLUX runs at CFG 1 and cannot use one at all.
  assert.ok(!IMAGE_ENGINES.flux.caps.negativePrompt);
  assert.ok(!IMAGE_ENGINES.fal.caps.negativePrompt);
  assert.ok(!IMAGE_ENGINES.recraft.caps.negativePrompt);
  assert.ok(IMAGE_ENGINES.leonardo.caps.negativePrompt);
  assert.ok(IMAGE_ENGINES.ideogram.caps.negativePrompt);
  assert.ok(IMAGE_ENGINES.comfyui.caps.negativePrompt);
});

test("exactly one engine returns vector, and it is the one sold for logos", () => {
  const vector = Object.entries(IMAGE_ENGINES).filter(([, e]) => e.caps.vectorOutput).map(([id]) => id);
  assert.deepEqual(vector, ["recraft"]);
});

test("ComfyUI is found by the id the rest of the app actually stores", () => {
  // It was keyed `comfy` here and `comfyui` everywhere else, so every capability lookup returned the
  // empty engine and the guided flow silently offered none of ComfyUI's controls.
  assert.equal(imageEngine("comfyui").label, "ComfyUI");
  assert.ok(imageEngine("comfyui").caps.negativePrompt);
  assert.equal(imageEngine("comfy").label, "ComfyUI", "the old id must still resolve");
  assert.equal(imageEngine("nonsense").label, "Unknown engine");
  assert.equal(musicEngine("HEARTMULA").label, "HeartMuLa", "ids are matched case-insensitively");
});

test("every engine carries the one line a picker needs", () => {
  for (const [id, engine] of [...Object.entries(MUSIC_ENGINES), ...Object.entries(IMAGE_ENGINES)]) {
    assert.ok(engine.label, `${id} has no label`);
    assert.ok(engine.note, `${id} has no picker note — the label alone tells an artist nothing`);
    assert.ok(engine.strengths, `${id} has no strengths line`);
  }
});

// ── The fallback rule ───────────────────────────────────────────────────────
//
// A fallback exists so one engine's outage costs a retry rather than the night's output. Being
// billed for that retry instead is a worse surprise, and one nobody notices until the invoice.

test("a free engine is never offered a paid fallback", () => {
  const offered = ids(fallbackMusicEngines({}, "heartmula", "none"));
  assert.ok(offered.includes("acestep"));
  assert.ok(offered.includes("riffusion"));
  assert.ok(!offered.includes("elevenlabs"), "a free engine must not fall back to a paid one");
  assert.ok(!offered.includes("heartmula"), "falling back to yourself is not a fallback");
});

test("somebody already paying may fall back to another paid engine", () => {
  // The rule is about an unexpected charge, not about paid engines being untouchable.
  const offered = ids(fallbackMusicEngines({}, "elevenlabs", "none"));
  assert.ok(offered.includes("heartmula"));
  assert.ok(!offered.includes("elevenlabs"));
});

test("a paid fallback already configured stays visible so it can be changed", () => {
  // Silently dropping it from the list would leave a saved setting nobody can see or undo.
  const offered = ids(fallbackMusicEngines({}, "heartmula", "elevenlabs"));
  assert.ok(offered.includes("elevenlabs"));
});

test("the music engines are in the decided order, free first and paid last", () => {
  // HeartMuLa is the default and ACE-Step follows it; the paid one is last so nobody meets it
  // before they have seen every free option.
  const visible = ids(visibleMusicEngines({}, "heartmula"));
  assert.ok(visible.indexOf("elevenlabs") === visible.length - 1, visible.join(","));
  assert.ok(MUSIC_ENGINES.elevenlabs.paid);
  assert.equal(priceLine(MUSIC_ENGINES.elevenlabs), "about $0.100 a track");
  for (const id of ["heartmula", "acestep", "riffusion"]) {
    assert.equal(priceLine(MUSIC_ENGINES[id]), "free", id);
  }
});
