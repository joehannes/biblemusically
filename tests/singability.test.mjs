// Whether a lyric will scan, before a GPU is asked to sing it.
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import { syllablesInWord, syllablesInLine, isSectionHeader, checkSingability }
  from "../src/src/lib/singability.js";

test("the corrections that matter for sung English", () => {
  // Silent final e.
  assert.equal(syllablesInWord("make"), 1);
  assert.equal(syllablesInWord("hope"), 1);
  // …but not where the e is the vowel.
  assert.equal(syllablesInWord("the"), 1);
  assert.equal(syllablesInWord("be"), 1);
  // -le after a consonant keeps its syllable.
  assert.equal(syllablesInWord("gentle"), 2);
  assert.equal(syllablesInWord("temple"), 2);
  // -ed is swallowed except after t or d.
  assert.equal(syllablesInWord("walked"), 1);
  assert.equal(syllablesInWord("wanted"), 2);
  assert.equal(syllablesInWord("guided"), 2);
});

test("ordinary words come out right", () => {
  for (const [word, n] of [
    ["light", 1], ["water", 2], ["mercy", 2], ["forever", 3],
    // ha-lle-lu-jah. Four, not the five a hymnbook's "-ia" ending would give.
    ["hallelujah", 4], ["everlasting", 4], ["I", 1], ["a", 1],
  ]) {
    assert.equal(syllablesInWord(word), n, word);
  }
});

test("nothing ever counts as zero syllables, because nothing is unsingable", () => {
  // A one-syllable floor: a word the heuristic cannot read is still sung.
  assert.equal(syllablesInWord("rhythm"), 1);
  assert.equal(syllablesInWord("'tis"), 1);
  // Digits are spoken.
  assert.ok(syllablesInWord("2024") >= 4);
});

test("a section header is an instruction to the engine, not a line to sing", () => {
  assert.ok(isSectionHeader("[Chorus]"));
  assert.ok(isSectionHeader("  [Soft female vocal]  "));
  assert.ok(!isSectionHeader("The light [that] came"));
  assert.equal(syllablesInLine("[Verse]"), 0);
});

test("a line is the sum of its words", () => {
  assert.equal(syllablesInLine("the light of the world"), 5);
  assert.equal(syllablesInLine(""), 0);
  assert.equal(syllablesInLine("   "), 0);
});

// ── the check ───────────────────────────────────────────────────────────────

const EVEN = [
  "[Verse]",
  "the light has come to stay",
  "the night has passed away",
  "we walk into the day",
  "and find the words to pray",
].join("\n");

test("a lyric that scans passes, and says what it measured", () => {
  const r = checkSingability(EVEN);
  assert.ok(r.ok, JSON.stringify(r.outliers));
  assert.equal(r.checked, 4, "the header is not counted as a line");
  assert.ok(r.median >= 5 && r.median <= 7, `median ${r.median}`);
});

test("one long line among short ones is found, and named by its position", () => {
  const lyric = EVEN + "\nand everything that ever was or will be sung by anyone at all forever";
  const r = checkSingability(lyric);
  assert.ok(!r.ok);
  assert.equal(r.outliers.length, 1);
  assert.ok(r.outliers[0].over, "it is over, not under");
  assert.ok(r.outliers[0].text.includes("everything"));
  // The index is into the original text, so an editor can point at the line.
  assert.equal(r.outliers[0].index, 5);
});

test("a song of uniformly long lines is a style, not a fault", () => {
  const long = Array.from({ length: 6 }, () =>
    "and everything that ever was or will be sung by anyone").join("\n");
  assert.ok(checkSingability(long).ok);
});

test("too short to judge says so rather than guessing", () => {
  const r = checkSingability("[Verse]\nthe light\ncame down");
  assert.ok(r.ok);
  assert.equal(r.reason, "too short to judge");
});

test("an explicit range overrides the song's own median", () => {
  // These lines are consistent with each other, so without a range they pass…
  assert.ok(checkSingability(EVEN).ok);
  // …and against a range the user asked for, they do not.
  const r = checkSingability(EVEN, { range: [10, 14] });
  assert.ok(!r.ok);
  assert.equal(r.low, 10);
  assert.equal(r.high, 14);
  assert.ok(r.outliers.every((o) => !o.over), "these are under, not over");
});

test("a lyric with no metre at all is reported as uneven rather than as a list of lines", () => {
  const chaos = ["one", "and everything that ever was or will be sung by anyone at all",
                 "two", "another line that goes on considerably longer than the last short one",
                 "three", "yet another which is also very much longer than its neighbours here"].join("\n");
  const r = checkSingability(chaos);
  assert.ok(!r.ok);
  assert.ok(r.uneven, "half the lines being outliers is a fact about the song");
});

test("the tolerance is wide enough not to cry wolf on ordinary variation", () => {
  // Six, seven and eight syllables in one verse is normal writing, not a defect. A checker that
  // flagged this would be turned off, and then it would catch nothing at all.
  const natural = [
    "the light has come to stay",          // 6
    "the night has passed away from us",   // 8
    "we walk into the day",                // 6
    "and find the words we need to pray",  // 8
  ].join("\n");
  assert.ok(checkSingability(natural).ok, JSON.stringify(checkSingability(natural)));
});

test("blank lines and headers never become outliers", () => {
  const spaced = "[Verse]\n\nthe light has come to stay\n\n[Chorus]\n\nthe night has passed away\n\nwe walk into the day\n\nand find the words to pray";
  const r = checkSingability(spaced);
  assert.equal(r.checked, 4);
  assert.ok(r.ok);
});
