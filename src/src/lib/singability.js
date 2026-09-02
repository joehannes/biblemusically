// Whether a lyric will scan, worked out before a GPU is asked to sing it.
//
// The engines sing this text verbatim. A line far outside the metre the rest of the song establishes
// is not smoothed over — it is rushed, or padded, or the singer takes a breath in the middle of a
// word — and the way you find out today is by spending a whole generation and listening to it.
//
// This is an estimate and says so. English syllable counting without a dictionary is a heuristic, and
// the heuristic is wrong often enough that it must never *block* anything: it marks lines that are
// far outside the song's own range, which is a different and much easier question than counting any
// one line correctly. A line two syllables off is noise; a line at twice the median is a real problem
// and this finds it.
//
// Kept free of React and of the app so it can be tested directly (see tests/singability.test.mjs).

/** Section headers are instructions to the engine, not sung. */
export const isSectionHeader = (line) => /^\s*\[[^\]]+\]\s*$/.test(line);

/**
 * Roughly how many syllables a line takes to sing.
 *
 * The classic vowel-group count, plus the three corrections that matter most for lyrics:
 * a silent final `e`, `-le` after a consonant keeping its syllable ("gentle", "temple"), and
 * `-ed` after anything but t/d being swallowed ("walked" is one, "wanted" is two).
 *
 * Digits are spoken, so a line of numbers is not zero syllables — but how many depends on the
 * language and the number, so each digit counts as one and the estimate says it is an estimate.
 */
export function syllablesInWord(word) {
  const w = String(word || "").toLowerCase().replace(/[^a-z0-9']/g, "");
  if (!w) return 0;
  if (/^\d+$/.test(w)) return w.length;

  let count = (w.match(/[aeiouy]+/g) || []).length;
  // Silent final e: "make" is one, not two.
  //
  // Two words keep theirs and both are exceptions to the *subtraction*, not additions on top of it:
  // "the" and "be", where the e is the only vowel, and "gentle" / "temple", where -le after a
  // consonant is its own syllable. Adding one for the second case double-counts, because the vowel
  // scan above has already seen that e.
  const silentE = w.length > 2 && w.endsWith("e")
    && !/[aeiouy]e$/.test(w)        // "-ee", "-ie": the e is part of the vowel group
    && !/[^aeiouy]le$/.test(w);     // "-tle", "-ple": syllabic l keeps it
  if (silentE) count -= 1;
  // -ed is swallowed except after t or d: "walked" is one, "wanted" is two.
  if (/[^td]ed$/.test(w) && w.length > 3) count -= 1;
  return Math.max(1, count);
}

/** Syllables in one lyric line. Zero for a header or a blank. */
export function syllablesInLine(line) {
  if (!line || isSectionHeader(line)) return 0;
  return String(line).split(/\s+/).filter(Boolean).reduce((n, w) => n + syllablesInWord(w), 0);
}

/** The middle value, which a couple of very long lines cannot drag around the way a mean can. */
function median(values) {
  if (!values.length) return 0;
  const v = [...values].sort((a, b) => a - b);
  const mid = v.length >> 1;
  return v.length % 2 ? v[mid] : Math.round((v[mid - 1] + v[mid]) / 2);
}

/**
 * Look over a whole lyric and report the lines that will not sit with the rest.
 *
 * `range` is the metre the user asked for, when they asked for one. With no range the song's own
 * median is the standard — which is the more useful question anyway: consistency is what makes a
 * verse singable, and a song of uniformly long lines is a style, while one long line among short
 * ones is a mistake.
 *
 * `tolerance` is generous on purpose. The counter is a heuristic, so a narrow band would flag
 * ordinary lines and teach people to ignore it, and a warning that is ignored is worse than none.
 */
export function checkSingability(lyrics, { range = null, tolerance = 0.45, minLines = 4 } = {}) {
  const raw = String(lyrics || "").split("\n");
  const lines = raw
    .map((text, index) => ({ index, text, syllables: syllablesInLine(text) }))
    .filter((l) => l.syllables > 0);

  if (lines.length < minLines) {
    return { ok: true, checked: lines.length, median: 0, low: 0, high: 0, outliers: [], reason: "too short to judge" };
  }

  const counts = lines.map((l) => l.syllables);
  const mid = median(counts);
  const [low, high] = range
    ? [range[0], range[1]]
    : [Math.max(1, Math.round(mid * (1 - tolerance))), Math.round(mid * (1 + tolerance))];

  const outliers = lines
    .filter((l) => l.syllables < low || l.syllables > high)
    .map((l) => ({ ...l, over: l.syllables > high }));

  return {
    ok: outliers.length === 0,
    checked: lines.length,
    median: mid,
    low, high,
    outliers,
    // A song where most lines are outliers has no metre to be outside of, so the finding is about
    // the song rather than about the lines, and saying so is more useful than listing half of them.
    uneven: outliers.length > lines.length / 2,
  };
}
