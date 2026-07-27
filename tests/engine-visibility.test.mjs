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
  musicEngine, imageEngine, isRisky,
} from "../src/src/lib/engineCapabilities.js";

const ids = (pairs) => pairs.map(([id]) => id);

test("nothing that automates the user's own account is offered by default", () => {
  assert.deepEqual(ids(visibleMusicEngines({}, "heartmula")), ["acestep", "heartmula"]);
  assert.deepEqual(ids(visibleImageEngines({}, "flux")), ["flux", "comfyui", "gemini"]);
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
  for (const id of ["acestep", "heartmula"]) assert.ok(!isRisky(MUSIC_ENGINES[id]), id);
  for (const id of ["flux", "comfyui", "gemini"]) assert.ok(!isRisky(IMAGE_ENGINES[id]), id);
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
