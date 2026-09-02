// The two decisions behind taking a spoken answer, kept free of the Web Audio API, of `localStorage`
// and of any Tauri import so they can be tested directly (see tests/speech.test.mjs).
//
// Both existed only implicitly before, and both were wrong in a way a user would feel:
//
//   • the recorder ran for a fixed eight seconds every time, so the guide's fastest input was its
//     slowest — seven of those seconds were usually silence;
//   • no language was ever passed to either listening path, because the caller read
//     `voicePrefs().language`, which nothing has ever written.

// ── Which language is being spoken ───────────────────────────────────────────

/**
 * Resolve a language for speech, in the two shapes the two paths need.
 *
 * `SpeechRecognition.lang` wants a BCP-47 tag. `stt_transcribe` drops the value into a sentence for
 * the model ("it is spoken in {lang}"), where a name reads and a bare code does not. Handing either
 * one the other's shape is worse than saying nothing at all, so both come out of here.
 *
 * `explicit` wins when given — a page that knows the answer is in the *song's* language rather than
 * the interface's should say so — and may be a code, an English name, or a native name. Anything
 * unrecognised passes through as a name: "Brazilian Portuguese" is a perfectly good hint for the
 * model even though no code exists for it, and refusing it would lose real information.
 */
export function resolveSpeechLanguage(explicit, uiCode, languages = []) {
  const given = String(explicit || "").trim();
  const find = (pred) => languages.find(pred);
  if (given) {
    const hit = find(
      (l) => l.code === given
        || String(l.label || "").toLowerCase() === given.toLowerCase()
        || l.native === given,
    );
    return hit ? { code: hit.code, name: hit.label } : { code: given, name: given };
  }
  const ui = find((l) => l.code === uiCode) || languages[0];
  return { code: ui?.code || "en", name: ui?.label || "English" };
}

// ── When the speaker has finished ────────────────────────────────────────────

/**
 * A silence gate: fed loudness samples, it says when to stop recording.
 *
 * The rule, and why each part of it is there:
 *
 *   • **Calibrate first.** The opening `calibrateMs` sets the noise floor, so a fan or a room tone
 *     raises the bar instead of being heard as continuous speech and never letting the clip end.
 *   • **Only stop after hearing something.** A gate that stopped on silence alone would cut the clip
 *     off before a speaker who paused to think ever started.
 *   • **Never before `minMs`.** Pressing the button and drawing breath must not return an empty clip.
 *   • **Never after `maxMs`.** Somebody who walks away still gets their microphone released.
 *
 * Pure and synchronous: the caller owns the analyser, the clock and the timer.
 */
export function createSilenceGate({
  maxMs = 8000,
  silenceMs = 1200,
  minMs = 700,
  calibrateMs = 300,
  floorFactor = 2.5,
  minBar = 0.015,
} = {}) {
  let floor = null;
  let heardSpeech = false;
  let quietSince = null;

  return {
    /** @returns {"listen"|"stop"} what to do after this sample. */
    push(elapsedMs, rms) {
      if (elapsedMs >= maxMs) return "stop";
      if (elapsedMs < calibrateMs) {
        floor = floor === null ? rms : Math.max(floor, rms);
        return "listen";
      }
      const bar = Math.max((floor ?? 0) * floorFactor, minBar);
      if (rms > bar) {
        heardSpeech = true;
        quietSince = null;
        return "listen";
      }
      if (!heardSpeech) return "listen";
      if (quietSince === null) { quietSince = elapsedMs; return "listen"; }
      return elapsedMs - quietSince >= silenceMs && elapsedMs >= minMs ? "stop" : "listen";
    },
    /** Whether anything above the noise floor was ever heard — a caller may skip transcribing if not. */
    heardAnything: () => heardSpeech,
  };
}
