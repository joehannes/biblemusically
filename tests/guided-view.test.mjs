// The rules a guided page is built on: what is visible, what stays expanded, how a section's status
// can change, and which questions a template removes.
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  visibility, mergeStatus, remainingSteps, resumeIndex, progressSummary, STATUS_ORDER,
} from "../src/src/lib/guidedView.js";

const ORDER = ["a", "b", "c", "d"];

test("nothing is visible before anything is settled", () => {
  const v = visibility({ order: ORDER, manifested: [] });
  assert.deepEqual(Object.values(v).map((x) => x.visible), [false, false, false, false]);
});

test("a settled section manifests, and is expanded", () => {
  const v = visibility({ order: ORDER, manifested: ["a"] });
  assert.equal(v.a.visible, true);
  assert.equal(v.a.open, true);
  assert.equal(v.b.visible, false);
});

test("the two most recent stay expanded; older ones collapse but remain visible", () => {
  // This is the rule the user asked for: once the second-next section appears, everything but the
  // previous one is collapsed — still there, still openable, just not in the way.
  const v = visibility({ order: ORDER, manifested: ["a", "b", "c"] });
  assert.deepEqual(
    ORDER.map((id) => [id, v[id].visible, v[id].open]),
    [["a", true, false], ["b", true, true], ["c", true, true], ["d", false, false]],
  );
});

test("a user can pin an older section open, or force a recent one closed", () => {
  const v = visibility({ order: ORDER, manifested: ["a", "b", "c"], pinned: { a: true, c: false } });
  assert.equal(v.a.open, true, "pinned open");
  assert.equal(v.c.open, false, "pinned closed");
});

test("show-all reveals every section regardless of manifestation", () => {
  const v = visibility({ order: ORDER, manifested: [], showAll: true });
  assert.deepEqual(Object.values(v).map((x) => x.visible), [true, true, true, true]);
});

test("manifestation order, not page order, decides what is expanded", () => {
  // The user watched 'd' get settled last, so 'd' is what should be open.
  const v = visibility({ order: ORDER, manifested: ["d", "a"] });
  assert.equal(v.d.open, true);
  assert.equal(v.a.open, true);
  const v2 = visibility({ order: ORDER, manifested: ["d", "a", "b"] });
  assert.equal(v2.d.open, false, "oldest of three collapses");
  assert.equal(v2.b.open, true);
});

test("status never downgrades", () => {
  assert.equal(mergeStatus("untouched", "common"), "common");
  assert.equal(mergeStatus("tuned", "common"), "tuned", "a template must not undo a user's answer");
  assert.equal(mergeStatus("fine", "tuned"), "fine", "a hand edit outranks everything");
  assert.equal(mergeStatus(undefined, "crude"), "crude");
  assert.deepEqual(STATUS_ORDER, ["untouched", "crude", "common", "tuned", "fine"]);
});

test("a template removes the questions it covers and ranks what is left", () => {
  const steps = [
    { id: "s1", reveals: "themes" },
    { id: "s2", reveals: "targets" },
    { id: "s3", reveals: "images" },
    { id: "s4" },
  ];
  const left = remainingSteps({
    steps,
    answers: {},
    coveredSections: ["themes", "images"],
    openQuestions: ["s4"],
  });
  // s4 was ranked first by the template; s1/s3 are covered and drop out; s2 keeps its place after.
  assert.deepEqual(left.map((s) => s.id), ["s4", "s2"]);
});

test("a covered step reappears once it has actually been answered", () => {
  const steps = [{ id: "s1", reveals: "themes" }, { id: "s2" }];
  const left = remainingSteps({ steps, answers: { s1: "x" }, coveredSections: ["themes"], openQuestions: [] });
  assert.deepEqual(left.map((s) => s.id), ["s1", "s2"], "an answered step stays in the rail");
});

test("resume lands on the first unanswered step", () => {
  const steps = [{ id: "a" }, { id: "b" }, { id: "c" }];
  assert.equal(resumeIndex(steps, {}), 0);
  assert.equal(resumeIndex(steps, { a: "1" }), 1);
  assert.equal(resumeIndex(steps, { a: "1", b: "2", c: "3" }), 3, "finished sessions point past the end");
});

test("the summary counts answers and flags rough sections", () => {
  const summary = progressSummary({
    steps: [{ id: "a" }, { id: "b" }],
    answers: { a: "1" },
    sections: { x: { status: "crude" }, y: { status: "tuned" } },
  });
  assert.match(summary, /1\/2 answered/);
  assert.match(summary, /1 rough/);
});
