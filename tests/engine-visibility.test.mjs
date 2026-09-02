// Which engines a picker is allowed to offer.
//
// Suno and Midjourney are reached by driving a session the user is logged into, which their terms
// reserve for their own interface — so the account that gets suspended is the user's.
//
// They used to be hidden until a switch in Settings was found. That protected nobody: it moved the
// explanation somewhere nobody reads and left a picker silently lacking the engine somebody came
// for, while the risk was unchanged for anyone who did find the switch. They are offered now, and
// the obligation moved rather than disappearing — **an offered risky engine must say whose account
// is at risk, at the point of choosing**. That is the line these tests hold, along with the one that
// did not change: an engine switched off in this build is never offered at all.
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  MUSIC_ENGINES, IMAGE_ENGINES, visibleMusicEngines, visibleImageEngines,
  fallbackMusicEngines, musicEngine, imageEngine, isRisky, isPaid, priceLine,
} from "../src/src/lib/engineCapabilities.js";

const ids = (pairs) => pairs.map(([id]) => id);

test("every engine that is not switched off is offered, whatever the settings say", () => {
  assert.deepEqual(ids(visibleMusicEngines({}, "heartmula")),
    ["suno", "acestep", "heartmula", "riffusion", "elevenlabs"]);
  assert.deepEqual(ids(visibleImageEngines({}, "flux")),
    ["midjourney", "flux", "comfyui", "leonardo", "fal", "ideogram", "recraft", "gemini"]);
  // Including for a settings object that has never heard of any flag, and for none at all: the
  // offer no longer depends on a setting, so no setting can make an engine disappear.
  assert.ok(ids(visibleMusicEngines(undefined, "")).includes("suno"));
  assert.ok(ids(visibleImageEngines(null, "")).includes("midjourney"));
  assert.ok(ids(visibleMusicEngines({ show_risky_engines: false }, "")).includes("suno"),
    "the retired flag must not still be gating anything");
});

test("an offered engine that drives your own account says so, in the first person", () => {
  // This is what replaced hiding, so it is now the load-bearing assertion in this file: if a risky
  // engine can be picked without the warning, the trade made in v0.143 was a straight loss.
  for (const [, engine] of [...visibleMusicEngines({}, ""), ...visibleImageEngines({}, "")]) {
    if (!isRisky(engine)) continue;
    assert.ok(engine.riskNote && engine.riskNote.trim().length > 40,
      `${engine.label} is offered and risky, so it must carry a real warning`);
    assert.match(engine.riskNote, /your(s)? own|yours/i,
      `${engine.label}'s warning must be about the user's own account, not an abstraction`);
  }
});

test("Midjourney's warning does not repeat the Discord claim that was never true of this app", () => {
  // The engine was shelved on a verdict about the Discord-user-token proxies. This one drives
  // midjourney.com's own site in the user's browser, and the copy has to keep those apart.
  const note = IMAGE_ENGINES.midjourney.riskNote;
  assert.match(note, /midjourney\.com/i, "say what it actually drives");
  assert.match(note, /not.*discord|discord.*not/i, "and that it is not the Discord route");
});

test("the engine already in use is always offered", () => {
  // A picker whose value is missing from its own list renders blank, and somebody on an engine they
  // cannot see has no way to choose to leave it.
  assert.ok(ids(visibleMusicEngines({}, "suno")).includes("suno"));
  assert.ok(ids(visibleImageEngines({}, "midjourney")).includes("midjourney"));
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

// ── The surfaces that had their own copy of the list ────────────────────────
//
// Two places kept their own hardcoded engine list and both had gone stale in the worst way: the
// welcome wizard offered a brand-new user Suno and Midjourney by name, and the guided music flow
// still offered Suno while never learning that Riffusion or ElevenLabs existed. A picker that
// disagrees with the rule is a rule that does not exist.

test("the guided music flow offers exactly what the picker offers", async () => {
  const { musicFlow } = await import("../src/src/lib/guidedFlows.js");
  const step = musicFlow.steps.find((s) => s.id === "engine");
  const ctx = { settings: { music_engine: "heartmula" } };
  const offered = step.options(ctx).map((o) => o.id);
  // Derived from the same function, not a second list that can drift: that drift is what let the
  // flow go on offering Suno while never learning Riffusion or ElevenLabs existed.
  assert.deepEqual(offered, ids(visibleMusicEngines(ctx.settings, "heartmula")));
  assert.ok(offered.includes("riffusion"), "and must know about engines added since");
  assert.ok(offered.includes("suno"), "the flow offers what the picker offers, risky included");
});
