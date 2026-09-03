// Hands-free: the loop that chains speaking to listening.
//
// The guide already speaks a question and can take a spoken answer, but only push-to-talk: it says
// the question and then waits for a click on "Say it" before it will listen. Every piece of the
// chain exists — `speak()` resolves when playback ends, `listen()` ends on silence rather than on a
// stopwatch, `interpretAnswer` maps the answer and escalates only when genuinely ambiguous — and
// what was missing is the thing that joins them: speak → listen → interpret → apply → speak the
// next question, without a hand on the mouse.
//
// Two rules that are not obvious, and are the difference between this being usable and being an
// assistant people switch off in the first minute:
//
//   * **Barge-in.** If somebody starts talking while it is still speaking, it stops. An assistant
//     that finishes its sentence over you is worse than one that never starts, and it is worse
//     precisely when you are trying to correct it.
//   * **A misunderstanding must end.** Two failures on one question and it hands the question back
//     rather than asking a third time. A voice loop that re-asks forever is the single most
//     unpleasant thing an interface can do, and it is the natural behaviour if nobody writes this
//     rule down.
//
// The decisions live here as pure functions so they can be tested without a microphone
// (see tests/conversation.test.mjs); the async plumbing is in useConversation.js.

/**
 * Watch the microphone while the assistant is speaking, and say when to stop it.
 *
 * The mirror image of `createSilenceGate`: that one waits for speech to *end*, this one waits for it
 * to *begin*. Same calibration idea, because the same room noise would otherwise trip it — but the
 * bar is deliberately higher and the confirmation longer, since a false barge-in cuts the assistant
 * off mid-question and the user then has no idea what was asked.
 *
 * `graceMs` exists because the speaker feeds the microphone. Without it the assistant's own first
 * syllable reads as somebody interrupting, and the loop talks over itself forever.
 */
export function createBargeInGate({
  graceMs = 400,
  calibrateMs = 300,
  speechMs = 350,
  floorFactor = 4,
  minBar = 0.05,
} = {}) {
  let floor = null;
  let loudSince = null;

  return {
    /** @returns {"speak"|"stop"} whether the assistant should keep talking. */
    push(elapsedMs, rms) {
      if (elapsedMs < graceMs) return "speak";
      if (elapsedMs < graceMs + calibrateMs) {
        floor = floor === null ? rms : Math.max(floor, rms);
        return "speak";
      }
      const bar = Math.max((floor ?? 0) * floorFactor, minBar);
      if (rms <= bar) { loudSince = null; return "speak"; }
      if (loudSince === null) { loudSince = elapsedMs; return "speak"; }
      // Sustained, not a door closing.
      return elapsedMs - loudSince >= speechMs ? "stop" : "speak";
    },
  };
}

/** How many times one question may be re-asked before it is handed back. */
export const MAX_MISSES = 2;

/**
 * What the assistant says when it is not asking a question.
 *
 * Objects with a `text` key on purpose: that is what `scripts/extract-ui-strings.mjs` picks up from a
 * prose catalogue, so these ship in the fifteen bundled languages like every other line. A bare
 * string constant here would be invisible to the extractor, and the assistant would ask its question
 * in German and then apologise in English — which is the exact bug the voice layer was fixed for
 * earlier in this pass.
 */
export const SPOKEN = {
  not_caught: { text: "I didn't catch that." },
  gave_up: { text: "I'll leave this one to you." },
  not_following: { text: "I'm not following — have a look and pick one." },
  which_one: { text: "Sorry — which one?" },
  skipping: { text: "Alright, skipping that." },
  did_you_mean: { text: "Did you mean this?" },
};

/**
 * What the loop should do with what it just heard.
 *
 * Pure, and the whole policy in one place: the alternative is this logic living inside an async
 * function where "what happens on the second failure" cannot be checked without a microphone and a
 * stopwatch.
 *
 * `heard` is the transcript (`null` when nothing was audible), `match` is what `interpretAnswer`
 * returned, and `misses` is how many times this same question has already failed.
 */
export function decide({ heard, match, misses = 0 } = {}) {
  const said = String(heard || "").trim();

  if (!said) {
    return misses + 1 >= MAX_MISSES
      ? { action: "hand_back", say: SPOKEN.gave_up.text, misses: misses + 1 }
      : { action: "reask", say: SPOKEN.not_caught.text, misses: misses + 1 };
  }

  // An explicit no is an answer, not a failure — it must not count toward the miss budget or a
  // person declining twice would be told the assistant gave up on them.
  if (match?.reason === "declined") {
    return { action: "skip", say: SPOKEN.skipping.text, misses };
  }

  // The bar for acting without confirmation. Below it the answer is repeated back as a question,
  // which is cheaper for everybody than an assistant that confidently picks the wrong thing.
  if (match?.option && (match.confidence ?? 0) >= 0.75) {
    return { action: "apply", option: match.option, misses: 0 };
  }
  if (match?.option) {
    return { action: "confirm", option: match.option, misses, say: null };
  }

  return misses + 1 >= MAX_MISSES
    ? { action: "hand_back", say: SPOKEN.not_following.text, misses: misses + 1 }
    : { action: "reask", say: SPOKEN.which_one.text, misses: misses + 1 };
}

/**
 * The sentence the assistant says for a step: the question, then the options, numbered.
 *
 * Numbered because "the second one" is the commonest spoken answer and `matchOption` reads ordinals
 * and numerals — an unnumbered list makes the easiest possible answer unavailable. Capped at four,
 * since nobody holds a spoken list longer than that and the rest are on screen anyway.
 */
export function speakableStep(step, { max = 4 } = {}) {
  const question = String(step?.question || step?.label || "").trim();
  const options = (step?.options || []).slice(0, max)
    .map((o, i) => `${i + 1}. ${o.label}`)
    .filter((s) => s.trim().length > 3);
  return [question, options.join(". ")].filter(Boolean).join(" ");
}

/**
 * The confirmation for a shaky match: a fixed question, then the option's own words.
 *
 * Two pieces rather than one sentence, because both halves are then catalogue lookups — the phrase
 * ships translated, and an option label is already in the inventory from the page it appears on. A
 * single interpolated sentence would match nothing and be spoken in English.
 */
export function confirmationOf(step, optionId) {
  const option = (step?.options || []).find((o) => o.id === optionId);
  return { ask: SPOKEN.did_you_mean.text, label: option?.label || "" };
}
