// The guided layer's promise is that it never offers a control the selected engine ignores. These
// tests hold that line: they walk the real flow definitions with different engines selected and
// assert what appears and what does not.
//
// Run with: node --test tests/
import { test } from "node:test";
import assert from "node:assert/strict";
import { composerFlow, musicFlow, imagesFlow, videoFlow, FLOWS } from "../src/src/lib/guidedFlows.js";
import { musicEngine, imageEngine, supports, lyricTagHint } from "../src/src/lib/engineCapabilities.js";

/** Steps a flow would actually show for this context — mirrors GuidedFlow's own filtering. */
const steps = (flow, ctx) => (flow.steps || []).filter((s) => (typeof s.when === "function" ? s.when(ctx) : true));
const optionIds = (step, ctx) => {
  const raw = typeof step.options === "function" ? step.options(ctx) : step.options || [];
  return raw.filter((o) => (typeof o.when === "function" ? o.when(ctx) : true)).map((o) => o.id);
};
const stepById = (flow, ctx, id) => steps(flow, ctx).find((s) => s.id === id);

const baseCtx = (over = {}) => ({
  projectId: "p1",
  project: { name: "Test", brief: {}, daily_topic: "" },
  channels: [],
  settings: { music_engine: "suno", image_engine: "midjourney" },
  cfg: { targets: [{ language: "English", styles: "dnb" }], themes: { global: "sacred" }, style_keywords: ["a"], generate: {} },
  ...over,
});

const ALL_FLOWS = Object.values(FLOWS);

test("every registered flow is exercised by these invariants", () => {
  assert.ok(ALL_FLOWS.length >= 10, `expected the full registry, got ${ALL_FLOWS.length}`);
  for (const [key, flow] of Object.entries(FLOWS)) {
    assert.equal(flow.id, key, "a flow's id must match its registry key — state is stored under it");
    assert.ok(flow.title, `${key}: no title`);
  }
});

test("every flow step has an id, a question and at least one option", () => {
  for (const flow of ALL_FLOWS) {
    const ctx = baseCtx();
    for (const s of steps(flow, ctx)) {
      assert.ok(s.id, `${flow.id}: step without id`);
      assert.ok(s.question, `${flow.id}/${s.id}: step without a question`);
      assert.ok(optionIds(s, ctx).length >= 1, `${flow.id}/${s.id}: no options`);
    }
  }
});

test("option ids are unique within a step — the backend picks by id", () => {
  for (const flow of ALL_FLOWS) {
    const ctx = baseCtx();
    for (const s of steps(flow, ctx)) {
      const ids = optionIds(s, ctx);
      assert.equal(new Set(ids).size, ids.length, `${flow.id}/${s.id}: duplicate option ids`);
    }
  }
});

test("stylisation is only applied for engines with flag syntax", () => {
  // Both engines can frame 16:9 — the difference is that Midjourney takes `--stylize` and FLUX does
  // not, so the same choice must write a different config.
  assert.equal(supports(imageEngine("midjourney"), "stylize"), true);
  assert.equal(supports(imageEngine("flux"), "stylize"), false);
  assert.equal(supports(imageEngine("flux"), "negativePrompt"), false, "schnell ignores negatives");

  const applyWide = (engineId) => {
    let cfg = {};
    const ctx = baseCtx({
      settings: { music_engine: "suno", image_engine: engineId },
      setCfg: (fn) => { cfg = typeof fn === "function" ? fn(cfg) : fn; },
    });
    const step = stepById(composerFlow, ctx, "visuals");
    step.options(ctx).find((o) => o.id === "cinematic_wide").apply(ctx);
    return cfg;
  };

  const mjCfg = applyWide("midjourney");
  assert.equal(mjCfg.mj_ar, "16:9");
  assert.equal(mjCfg.mj_stylize, 250, "Midjourney gets a stylize value");

  const fluxCfg = applyWide("flux");
  assert.equal(fluxCfg.mj_ar, "16:9");
  assert.equal(fluxCfg.mj_stylize, undefined, "FLUX must not be handed a flag it ignores");
});

test("an engine's aspect list gates the shorts option", () => {
  // gemini's image model has no 21:9 etc.; every current engine does offer 9:16, so the gate is
  // asserted through the capability data rather than a hardcoded expectation per engine.
  for (const id of ["midjourney", "flux", "comfy", "gemini"]) {
    const ctx = baseCtx({ settings: { image_engine: id } });
    const offered = optionIds(stepById(composerFlow, ctx, "visuals"), ctx).includes("vertical");
    assert.equal(offered, (imageEngine(id).caps.aspectRatios || []).includes("9:16"), id);
  }
});

test("the track-length step only exists for engines that take a duration", () => {
  const suno = baseCtx({ settings: { music_engine: "suno" } });
  const ace = baseCtx({ settings: { music_engine: "acestep" } });
  assert.equal(supports(musicEngine("suno"), "durationControl"), false);
  assert.equal(supports(musicEngine("acestep"), "durationControl"), true);
  assert.ok(!steps(musicFlow, suno).some((s) => s.id === "length"), "Suno has no duration control");
  assert.ok(steps(musicFlow, ace).some((s) => s.id === "length"), "ACE-Step does");
});

test("image quality step follows the engine's step support", () => {
  const withSteps = baseCtx({ settings: { image_engine: "flux" } });
  const without = baseCtx({ settings: { image_engine: "midjourney" } });
  assert.ok(steps(imagesFlow, withSteps).some((s) => s.id === "quality"));
  assert.ok(!steps(imagesFlow, without).some((s) => s.id === "quality"));
});

test("the character step appears only when the project has characters", () => {
  assert.ok(!steps(imagesFlow, baseCtx({ characters: [] })).some((s) => s.id === "consistency"));
  const ctx = baseCtx({ characters: [{ id: "c1", name: "Elijah" }] });
  const step = stepById(imagesFlow, ctx, "consistency");
  assert.ok(step, "step should appear");
  assert.deepEqual(optionIds(step, ctx), ["none", "char:c1"]);
});

test("remote render targets are gated on being configured", () => {
  const bare = baseCtx({ settings: {} });
  const ids = optionIds(stepById(videoFlow, bare, "where"), bare);
  assert.deepEqual(ids, ["local", "kaggle"], "Actions and Modal need configuration first");

  const configured = baseCtx({ settings: { github_token: "x", modal_endpoint: "https://x.modal.run" } });
  const ids2 = optionIds(stepById(videoFlow, configured, "where"), configured);
  assert.ok(ids2.includes("actions") && ids2.includes("modal"));
});

test("applying an option mutates only through the setters it was given", () => {
  let cfg = { targets: [{ language: "English", styles: "dnb" }, { language: "German", styles: "folk" }], generate: {}, themes: {} };
  const ctx = baseCtx({ cfg, setCfg: (fn) => { cfg = typeof fn === "function" ? fn(cfg) : fn; } });
  const step = stepById(composerFlow, ctx, "reach");
  const one = (typeof step.options === "function" ? step.options(ctx) : []).find((o) => o.id === "one");
  one.apply(ctx);
  assert.equal(cfg.targets.length, 1, "'just one' trims the target list");
});

test("lyric tag hints match the engine's dialect", () => {
  assert.match(lyricTagHint("suno"), /performance hints/i);
  assert.match(lyricTagHint("acestep"), /lowercase/i);
  assert.match(lyricTagHint("heartmula"), /section header/i);
});

test("every option applies without throwing, even with nothing wired up", () => {
  // A page that forgets to pass a setter must degrade to a no-op, not crash mid-flow. Options use
  // optional calls (`c.setThing?.()`) precisely so this holds.
  for (const flow of ALL_FLOWS) {
    const ctx = baseCtx({ characters: [{ id: "c1", name: "X" }], accounts: [{ platform: "x" }], channels: [{ id: "ch", name: "C", language: "English", region: "US" }] });
    for (const step of steps(flow, ctx)) {
      const raw = typeof step.options === "function" ? step.options(ctx) : step.options || [];
      for (const o of raw.filter((x) => (typeof x.when === "function" ? x.when(ctx) : true))) {
        assert.doesNotThrow(() => o.apply?.(ctx), `${flow.id}/${step.id}/${o.id}`);
      }
    }
  }
});

test("step titles are short enough for the progress rail", () => {
  for (const flow of ALL_FLOWS) {
    for (const step of steps(flow, baseCtx())) {
      if (!step.title) continue;
      assert.ok(step.title.length <= 14, `${flow.id}/${step.id}: "${step.title}" is too long for the rail`);
    }
  }
});
