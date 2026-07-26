import test from "node:test";
import assert from "node:assert/strict";
import { sortActions, splitActions, MAX_VISIBLE } from "../src/src/lib/actionOrder.js";

// ── Ordering ────────────────────────────────────────────────────────────────
// The rule exists so a button never slides sideways when a neighbour scrolls into view. That is a
// mis-click waiting to happen, and it is exactly what sorting by registration order produces.

test("order is by priority, then id — never by when things registered", () => {
  const registered = [
    { id: "refresh", priority: 60 },
    { id: "generate", priority: 10 },
    { id: "save", priority: 60 },
    { id: "run", priority: 20 },
  ];
  assert.deepEqual(sortActions(registered).map((a) => a.id), ["generate", "run", "refresh", "save"]);
  // Shuffling the input cannot change the result.
  const shuffled = [registered[2], registered[0], registered[3], registered[1]];
  assert.deepEqual(sortActions(shuffled).map((a) => a.id), sortActions(registered).map((a) => a.id));
});

test("an action with no priority sits in the middle rather than first", () => {
  const list = [{ id: "primary", priority: 10 }, { id: "unspecified" }, { id: "last", priority: 90 }];
  assert.deepEqual(sortActions(list).map((a) => a.id), ["primary", "unspecified", "last"]);
});

test("a section action appearing does not move the buttons that were already there", () => {
  // The whole point: scrolling a section into view adds a slot, it does not reshuffle the bar.
  const before = sortActions([{ id: "save", priority: 60 }, { id: "refresh", priority: 60 }]);
  const after = sortActions([
    { id: "save", priority: 60 }, { id: "refresh", priority: 60 }, { id: "zoom", priority: 60 },
  ]);
  assert.deepEqual(before.map((a) => a.id), ["refresh", "save"]);
  assert.deepEqual(after.slice(0, 2).map((a) => a.id), ["refresh", "save"],
    "the existing two keep their positions");
});

// ── Overflow ────────────────────────────────────────────────────────────────

test("the bar never grows without limit", () => {
  const many = Array.from({ length: 9 }, (_, i) => ({ id: `a${i}`, priority: i }));
  const { visible, overflow } = splitActions(many);
  assert.equal(visible.length, MAX_VISIBLE);
  assert.equal(overflow.length, 9 - MAX_VISIBLE);
  // The highest-priority actions stay in the bar; the rest are still reachable.
  assert.deepEqual(visible.map((a) => a.id), ["a0", "a1", "a2", "a3"]);
  assert.equal(visible.length + overflow.length, many.length, "no action is ever dropped");
});

test("a short list has no overflow menu at all", () => {
  const { visible, overflow } = splitActions([{ id: "one" }, { id: "two" }]);
  assert.equal(visible.length, 2);
  assert.equal(overflow.length, 0);
});
