// Regression tests for the two rules that stopped runtime GUI translation from leaking requests.
// Run with: node --test tests/
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mayTranslate, baselineOriginal } from "../src/src/lib/uiTranslateRules.js";

const INVENTORY = new Set(JSON.parse(readFileSync(new URL("../src/src/i18n/ui-strings.json", import.meta.url), "utf8")).strings);

test("inventory strings are translatable", () => {
  assert.ok(INVENTORY.size > 500, "the extracted inventory should be substantial");
  for (const s of ["Save", "Dashboard", "AI Composer", "Settings"]) {
    assert.ok(INVENTORY.has(s), `${s} should be in the inventory`);
    assert.equal(mayTranslate(s, INVENTORY), true);
  }
});

test("runtime data is never sent, however label-like it looks", () => {
  // Each of these is a real shape that used to cost one request per distinct value.
  const volatile = [
    "Started: 3m ago",
    "12 jobs queued",
    "nvidia/nemotron-3-ultra-550b-a55b:free",
    "chapter-04-verse-12.mp3",
    "https://openrouter.ai/api/v1",
    "user@example.com",
    "OpenRouter HTTP 429: rate limit exceeded, add credits to unlock 1000 free model requests",
  ];
  for (const s of volatile) assert.equal(mayTranslate(s, INVENTORY, 0, 300), false, s);
});

test("unknown label-like strings are allowed only within budget", () => {
  const s = "Pick a folder";
  assert.equal(INVENTORY.has(s), false);
  assert.equal(mayTranslate(s, INVENTORY, 0, 300), true, "inside budget");
  assert.equal(mayTranslate(s, INVENTORY, 300, 300), false, "budget exhausted");
  assert.equal(mayTranslate(s, INVENTORY, 0, 0), false, "no budget at all");
});

test("a node keeps its baseline while only we write to it", () => {
  const node = { nodeValue: "Save" };
  assert.equal(baselineOriginal(node), "Save");
  // We translate it, recording what we wrote.
  node.nodeValue = "Speichern";
  node.__i18nApplied = "Speichern";
  assert.equal(baselineOriginal(node), "Save", "our own write must not become the new baseline");
});

test("a node React re-rendered with the same text keeps its baseline", () => {
  const node = { nodeValue: "Save" };
  baselineOriginal(node);
  node.nodeValue = "Speichern";
  node.__i18nApplied = "Speichern";
  node.nodeValue = "Save";                       // React repaints the original text
  assert.equal(baselineOriginal(node), "Save");
  // The applied marker is deliberately left alone here: the caller compares it against the node's
  // live value, so a node showing English is simply painted again on the next pass.
  assert.notEqual(node.nodeValue, node.__i18nApplied, "the node is repainted, not treated as done");
});

test("a node React reused for DIFFERENT text is re-baselined — this is the flicker fix", () => {
  const node = { nodeValue: "Save" };
  baselineOriginal(node);
  node.nodeValue = "Speichern";
  node.__i18nApplied = "Speichern";
  node.nodeValue = "Saved";                      // React hands the same node new content
  assert.equal(baselineOriginal(node), "Saved", "the new text is the baseline now");
  assert.equal(node.__i18nApplied, undefined, "the stale translation must never be re-applied");
});
