// Mapping a spoken sentence onto one of the choices on screen.
//
// Local first, deliberately: "yes", "the second one" and naming an option cover most answers, and a
// free tier counts requests — a guide that spent one on every "yes" would be a bad trade. Only a
// genuinely ambiguous sentence escalates to the AI (see interpretAnswer in voice.js).
//
// No imports, so it can be unit-tested directly (tests/voice-match.test.mjs).

// Ordinal *words* only. Bare cardinals ("one", "two") are deliberately excluded: "the second one"
// contains "one", and treating that as "the first" was exactly the kind of confident wrong answer a
// voice interface must not give. A spoken digit is still understood — see the numeral pass below.
const ORDINALS = [
  ["first", "1st"], ["second", "2nd"], ["third", "3rd"], ["fourth", "4th"],
];
const AFFIRM = ["yes", "yeah", "yep", "sure", "ok", "okay", "do it", "go ahead", "sounds good", "that one", "take it"];
const DENY = ["no", "nope", "not yet", "skip", "later", "cancel"];

const norm = (s) => String(s || "").toLowerCase().replace(/[^a-z0-9äöüßáéíóúñ\s]/gi, " ").replace(/\s+/g, " ").trim();

/**
 * Best local guess at which option a sentence means.
 * Returns `{ option, confidence, reason }`; a confidence below ~0.5 should escalate to the AI.
 */
export function matchOption(transcript, options = [], { recommended } = {}) {
  const said = norm(transcript);
  if (!said || !options.length) return { option: null, confidence: 0, reason: "nothing heard" };

  // "the second one"
  for (let i = 0; i < Math.min(ORDINALS.length, options.length); i++) {
    if (ORDINALS[i].some((w) => new RegExp(`\\b${w}\\b`).test(said))) {
      return { option: options[i].id, confidence: 0.9, reason: "ordinal" };
    }
  }
  // "number 3", "option 2", or a digit on its own.
  const numeral = said.match(/\b(?:number|option|choice)\s*([1-9])\b/) || said.match(/^([1-9])$/);
  if (numeral) {
    const idx = Number(numeral[1]) - 1;
    if (idx < options.length) return { option: options[idx].id, confidence: 0.9, reason: "numeral" };
  }

  // A distinctive word from exactly one label.
  const scored = options.map((o) => {
    const label = norm(o.label);
    const words = label.split(" ").filter((w) => w.length > 3);
    const hits = words.filter((w) => said.includes(w)).length;
    const whole = label && said.includes(label) ? 3 : 0;
    return { id: o.id, score: hits + whole };
  }).sort((a, b) => b.score - a.score);
  if (scored[0]?.score > 0 && scored[0].score > (scored[1]?.score || 0)) {
    return { option: scored[0].id, confidence: Math.min(0.85, 0.5 + scored[0].score * 0.15), reason: "label match" };
  }

  // Bare agreement takes the recommendation — which is what "yes" means when something is suggested.
  if (AFFIRM.some((w) => said === w || said.startsWith(`${w} `) || said.endsWith(` ${w}`))) {
    const target = recommended || options[0]?.id;
    if (target) return { option: target, confidence: 0.8, reason: "agreement with the suggestion" };
  }
  if (DENY.some((w) => said === w || said.startsWith(`${w} `))) {
    return { option: null, confidence: 0.8, reason: "declined" };
  }
  return { option: null, confidence: 0, reason: "no local match" };
}

