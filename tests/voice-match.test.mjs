// Mapping spoken answers onto the choices on screen, without spending an AI request on "yes".
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import { matchOption } from "../src/src/lib/voiceMatch.js";

const OPTIONS = [
  { id: "comforting", label: "Comforting and quiet" },
  { id: "triumphant", label: "Triumphant and bright" },
  { id: "brief_mood", label: "The project's own mood" },
];

test("naming an option picks it", () => {
  const r = matchOption("let's go triumphant today", OPTIONS);
  assert.equal(r.option, "triumphant");
  assert.ok(r.confidence >= 0.5, `confidence was ${r.confidence}`);
});

test("a whole label said back is a strong match", () => {
  const r = matchOption("comforting and quiet please", OPTIONS);
  assert.equal(r.option, "comforting");
  assert.ok(r.confidence >= 0.7);
});

test("ordinals work, because people say 'the second one'", () => {
  assert.equal(matchOption("the second one", OPTIONS).option, "triumphant");
  assert.equal(matchOption("first", OPTIONS).option, "comforting");
  assert.equal(matchOption("number 3", OPTIONS).option, "brief_mood");
});

test("bare agreement takes the suggestion, not the first option", () => {
  const r = matchOption("yes please", OPTIONS, { recommended: "brief_mood" });
  assert.equal(r.option, "brief_mood");
  assert.equal(r.reason, "agreement with the suggestion");
});

test("agreement with nothing suggested falls back to the first option", () => {
  assert.equal(matchOption("okay", OPTIONS).option, "comforting");
});

test("declining is recognised and does not pick anything", () => {
  const r = matchOption("no, skip this", OPTIONS);
  assert.equal(r.option, null);
  assert.equal(r.reason, "declined");
});

test("an ambiguous sentence refuses to guess", () => {
  // Both labels share "and"; nothing distinctive was said. This is the case that escalates to the AI
  // rather than picking something the user did not mean.
  const r = matchOption("hmm I'm not sure what I want", OPTIONS);
  assert.equal(r.option, null);
  assert.equal(r.confidence, 0);
});

test("empty input and empty option lists are handled", () => {
  assert.equal(matchOption("", OPTIONS).option, null);
  assert.equal(matchOption("anything", []).option, null);
});

test("punctuation and case do not matter", () => {
  assert.equal(matchOption("TRIUMPHANT!", OPTIONS).option, "triumphant");
});

test("a word shared by two labels does not decide anything on its own", () => {
  const opts = [
    { id: "a", label: "Cinematic wide shot" },
    { id: "b", label: "Cinematic close up" },
  ];
  assert.equal(matchOption("cinematic", opts).option, null, "shared word is not a distinguishing match");
  assert.equal(matchOption("close up", opts).option, "b");
});
