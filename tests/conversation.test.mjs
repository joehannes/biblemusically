// The hands-free loop's decisions, without a microphone.
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import { createBargeInGate, decide, speakableStep, confirmationOf, MAX_MISSES, SPOKEN }
  from "../src/src/lib/conversation.js";

// ── barge-in ────────────────────────────────────────────────────────────────

/** Feed a gate a constant level and return the first ms at which it says stop, or null. */
function run(gate, level, { until = 4000, step = 50, from = 0 } = {}) {
  for (let t = from; t < until; t += step) {
    if (gate.push(t, typeof level === "function" ? level(t) : level) === "stop") return t;
  }
  return null;
}

test("the assistant's own first syllable is not somebody interrupting", () => {
  // The speaker feeds the microphone. Without the grace window the loop talks over itself forever.
  const gate = createBargeInGate({ graceMs: 400 });
  for (let t = 0; t < 400; t += 50) {
    assert.equal(gate.push(t, 0.9), "speak", `loud at ${t}ms is still the assistant`);
  }
});

test("somebody talking over it stops it", () => {
  const gate = createBargeInGate();
  const at = run(gate, (t) => (t < 1000 ? 0.005 : 0.4));
  assert.ok(at !== null, "it never stopped");
  assert.ok(at >= 1000 && at < 1600, `stopped at ${at}ms — should be shortly after the speech began`);
});

test("a door closing is not an interruption", () => {
  // One loud sample, then quiet. A gate that stopped on that would cut the question in half every
  // time somebody put a mug down.
  const gate = createBargeInGate();
  assert.equal(run(gate, (t) => (t > 1000 && t < 1100 ? 0.9 : 0.004)), null);
});

test("a noisy room does not read as an interruption", () => {
  // Calibrated: the bar is relative to what the room was already doing.
  const gate = createBargeInGate();
  assert.equal(run(gate, 0.03), null, "steady room noise must not stop the assistant");
});

test("the bar is higher than the one for hearing an answer end", () => {
  // A false barge-in cuts the question off and leaves the user not knowing what was asked, so this
  // gate must be harder to trip than the silence gate is.
  const quiet = createBargeInGate();
  assert.equal(run(quiet, 0.02), null, "0.02 is speech to the silence gate and not to this one");
});

// ── what to do with what was heard ──────────────────────────────────────────

test("a confident match is acted on and clears the misses", () => {
  const d = decide({ heard: "the second one", match: { option: "b", confidence: 0.9 }, misses: 1 });
  assert.equal(d.action, "apply");
  assert.equal(d.option, "b");
  assert.equal(d.misses, 0, "a success wipes the slate");
});

test("a shaky match is repeated back rather than acted on", () => {
  const d = decide({ heard: "erm the quiet one maybe", match: { option: "b", confidence: 0.55 } });
  assert.equal(d.action, "confirm");
  assert.equal(d.option, "b");
});

test("a misunderstanding ends instead of asking a third time", () => {
  // A voice loop that re-asks forever is the most unpleasant thing an interface can do, and it is
  // the natural behaviour if nobody writes this rule down.
  let misses = 0;
  const first = decide({ heard: "mmm", match: { option: null, confidence: 0 }, misses });
  assert.equal(first.action, "reask");
  misses = first.misses;
  const second = decide({ heard: "mmm", match: { option: null, confidence: 0 }, misses });
  assert.equal(second.action, "hand_back");
  assert.equal(MAX_MISSES, 2);
});

test("silence is a miss and runs out at the same point", () => {
  const first = decide({ heard: null, misses: 0 });
  assert.equal(first.action, "reask");
  assert.equal(decide({ heard: null, misses: first.misses }).action, "hand_back");
  assert.equal(decide({ heard: "   ", misses: 1 }).action, "hand_back", "whitespace is silence");
});

test("saying no is an answer and never counts as a failure", () => {
  // Somebody declining twice must not be told the assistant has given up on them.
  const d = decide({ heard: "skip that", match: { option: null, confidence: 0.8, reason: "declined" }, misses: 1 });
  assert.equal(d.action, "skip");
  assert.equal(d.misses, 1, "the miss budget is untouched");
});

// ── what it says ────────────────────────────────────────────────────────────

test("options are spoken numbered, because that is how people answer", () => {
  const line = speakableStep({
    question: "How should it feel?",
    options: [{ label: "Comforting" }, { label: "Bright" }, { label: "Serious" }],
  });
  assert.ok(line.startsWith("How should it feel?"));
  assert.ok(line.includes("1. Comforting"));
  assert.ok(line.includes("2. Bright"));
  assert.ok(line.includes("3. Serious"));
});

test("a spoken list stops at four however long the list on screen is", () => {
  const line = speakableStep({
    question: "Pick one",
    options: Array.from({ length: 9 }, (_, i) => ({ label: `Option ${i + 1}` })),
  });
  assert.ok(line.includes("4. Option 4"));
  assert.ok(!line.includes("5. Option 5"), "nobody holds a spoken list of nine");
});

test("a step with no options is just its question", () => {
  assert.equal(speakableStep({ question: "What is this project for?" }), "What is this project for?");
  assert.equal(speakableStep({ label: "Write the words", options: [] }), "Write the words");
  assert.equal(speakableStep(null), "");
});

test("a confirmation comes back in two pieces so both are translatable", () => {
  // One interpolated sentence would match nothing in the catalogue and be spoken in English under a
  // German interface — the exact bug the voice layer was fixed for.
  const step = { options: [{ id: "a", label: "Comforting and quiet" }, { id: "b", label: "Bright" }] };
  const c = confirmationOf(step, "a");
  assert.equal(c.ask, SPOKEN.did_you_mean.text);
  assert.equal(c.label, "Comforting and quiet");
  // An id that is no longer on the step still produces something answerable rather than "undefined".
  assert.equal(confirmationOf(step, "gone").label, "");
  assert.ok(confirmationOf(step, "gone").ask.length > 0);
});

test("everything the assistant says outside a question is in the prose catalogue", () => {
  // A bare string constant is invisible to the extractor, and then it is spoken in English however
  // the interface is set. The `text` key is what makes these ship translated.
  for (const [key, entry] of Object.entries(SPOKEN)) {
    assert.equal(typeof entry.text, "string", `${key} is not a prose entry`);
    assert.ok(entry.text.length > 3, key);
  }
  // Every line `decide` can return is one of them.
  const said = [
    decide({ heard: null, misses: 0 }).say,
    decide({ heard: null, misses: 1 }).say,
    decide({ heard: "mm", match: null, misses: 0 }).say,
    decide({ heard: "mm", match: null, misses: 1 }).say,
    decide({ heard: "no", match: { reason: "declined" } }).say,
  ];
  const known = Object.values(SPOKEN).map((e) => e.text);
  for (const line of said) assert.ok(known.includes(line), `"${line}" is not in SPOKEN`);
});
