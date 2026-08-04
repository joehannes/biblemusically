// What each generation engine can actually do.
//
// The studio speaks to three music engines and eight image engines, and they differ in ways that are
// invisible in the interface until something silently degrades: Suno reads rich bracketed
// performance tags, ACE-Step only plain lowercase structure tags, HeartMuLa sings anything that is
// not a section header. Midjourney takes `--ar/--stylize/--chaos` flags; FLUX takes steps and
// guidance and no negative prompt; ComfyUI takes whatever the workflow exposes.
//
// Declaring that here lets the guided flow offer only controls the selected engine honours, and lets
// prompts be written in the dialect that engine parses. Anything absent from a capability set is
// simply not offered — the alternative (offering it and hoping) produces prompts the engine ignores.
//
// ── Hidden engines ──────────────────────────────────────────────────────────
//
// Two engines are marked `risky` and are not offered unless the user turns them on. Suno and
// Midjourney have no public API, so the only way to reach them is to drive *the user's own* logged-in
// account — which their terms restrict to their own interface. The account that dies is the user's,
// not ours, and that is exactly why it must not be a default somebody stumbles into.
//
// They are hidden rather than deleted, on purpose: Suno has said an official API is coming, and
// Midjourney takes applications for one. The job branches, the cookie capture and the settings all
// stay exactly where they are, so switching them back on is a flag and not a rewrite. When they are
// shown, each says in one line whose account is at risk.
//
// ── Paid engines ────────────────────────────────────────────────────────────
//
// Four image engines are `paid: true` and carry a `priceHint` in US dollars per image. The picker
// shows both, because somebody who picked a paid engine by accident and found out from an invoice
// has been treated badly. The same figure is logged by the backend before it spends anything.

export const MUSIC_ENGINES = {
  suno: {
    label: "Suno",
    note: "unofficial, needs a cookie",
    tier: "account",              // needs a logged-in Suno session
    risky: true,
    riskNote: "Reaches Suno by driving your own logged-in session, which Suno's terms restrict to \
their own interface. The subscription at risk is yours. An official API is reportedly in progress.",
    lyricDialect: "suno",         // [Verse] + performance hints
    caps: {
      structureTags: "rich",      // [Intro], [Soft female vocal], …
      performanceHints: true,
      styleCsv: true,             // comma-separated genre/instrument descriptor
      styleCsvLimit: 200,
      negativeStyle: true,        // "exclude styles" field
      lyricsLimit: 5000,
      durationControl: false,
      seed: false,
      instrumental: true,
      referenceAudio: true,
      vocalGender: true,
    },
    strengths: "Best vocals and mixing of the free engines; understands genre names and performance direction.",
  },
  acestep: {
    label: "ACE-Step",
    note: "free, open weights, your own GPU",
    tier: "gpu",                  // self-served on Kaggle/Colab GPU
    lyricDialect: "acestep",      // [verse] lowercase, no directions
    caps: {
      structureTags: "plain",
      performanceHints: false,
      styleCsv: true,
      styleCsvLimit: 120,
      negativeStyle: false,
      lyricsLimit: 2500,
      durationControl: true,      // explicit seconds
      seed: true,
      instrumental: true,
      referenceAudio: false,
      vocalGender: false,
      guidance: true,             // cfg-style knob
    },
    strengths: "Free on your own GPU, fast, deterministic with a seed. Ignores performance direction.",
  },
  heartmula: {
    label: "HeartMuLa",
    note: "free, Apache-2.0, your own GPU",
    tier: "gpu",
    lyricDialect: "heartmula",    // plain [Verse] headers only — everything else is sung
    caps: {
      structureTags: "headers",
      performanceHints: false,
      styleCsv: true,
      styleCsvLimit: 150,
      negativeStyle: false,
      lyricsLimit: 3000,
      durationControl: true,
      seed: true,
      instrumental: false,
      referenceAudio: false,
      vocalGender: false,
      guidance: true,
    },
    strengths: "Free on your own GPU with strong melodic phrasing. Sings anything that is not a section header, so no inline notes.",
  },
  riffusion: {
    label: "Riffusion",
    note: "free, open weights, your own GPU",
    tier: "gpu",
    lyricDialect: "acestep",      // section tags drive the arrangement; no performance direction
    caps: {
      structureTags: "plain",
      performanceHints: false,
      styleCsv: true,
      styleCsvLimit: 120,
      negativeStyle: false,
      lyricsLimit: 4000,
      durationControl: true,      // 5–10 minutes, and it refuses anything outside that
      seed: true,
      instrumental: true,
      referenceAudio: false,
      vocalGender: false,
      guidance: false,
    },
    strengths: "Reads the section tags and arranges around them, so it holds a five to ten minute structure better than the others. Slow on a free T4 — minutes per track, not seconds.",
  },
  // The paid one, last and clearly marked. A genuine public API with commercial licensing, so
  // unlike Suno there is nobody's account at risk — only a bill.
  elevenlabs: {
    label: "ElevenLabs Music",
    note: "paid — a real API, commercially licensed",
    tier: "api",
    paid: true,
    priceHint: 0.10,
    priceUnit: "a track",
    lyricDialect: "heartmula",    // a prompt, not a tag language
    caps: {
      structureTags: "headers",
      performanceHints: true,
      styleCsv: true,
      styleCsvLimit: 300,
      negativeStyle: false,
      lyricsLimit: 5000,
      durationControl: true,
      seed: false,
      instrumental: true,
      referenceAudio: false,
      vocalGender: false,
      guidance: false,
    },
    strengths: "The best-sounding option that does not touch anybody's account, with licensing that permits commercial release. It charges per track.",
  },
};

export const IMAGE_ENGINES = {
  midjourney: {
    label: "Midjourney",
    note: "browser automation of your own account",
    tier: "account",
    risky: true,
    riskNote: "Driven through your own Midjourney account in a browser. Every route to it automates a \
session Midjourney's terms reserve for their own interface, and a termination takes the subscription \
with it. FLUX, Leonardo, Ideogram and Recraft cover the same ground without that.",
    caps: {
      flagSyntax: true,           // --ar --stylize --chaos --weird --no
      aspectRatios: ["16:9", "9:16", "1:1", "4:3", "3:2", "21:9"],
      stylize: true,
      chaos: true,
      weird: true,
      tile: true,
      negativePrompt: true,       // via --no
      seed: true,
      steps: false,
      guidance: false,
      referenceImage: true,       // character/style reference
      videoClip: true,            // v8 image-to-video
      batchOf4: true,
    },
    strengths: "Strongest aesthetics; the flag syntax gives fine control over framing and stylisation.",
  },
  flux: {
    label: "FLUX.1 schnell",
    note: "free, single-model, your own GPU",
    tier: "gpu",
    caps: {
      flagSyntax: false,
      aspectRatios: ["16:9", "9:16", "1:1"],
      stylize: false,
      chaos: false,
      weird: false,
      tile: false,
      negativePrompt: false,      // schnell ignores negatives
      seed: true,
      steps: true,
      guidance: true,
      referenceImage: false,
      videoClip: false,
      batchOf4: false,
    },
    strengths: "Free on your own GPU, excellent prompt adherence, very fast at 4 steps. No negative prompt.",
  },
  // Keyed `comfyui` because that is the id the settings and the job runner use. It was `comfy` here
  // and nowhere else, so every capability lookup for ComfyUI quietly returned the empty engine and
  // the guided flow offered none of its controls.
  comfyui: {
    label: "ComfyUI",
    note: "free, multi-model: styles, character consistency, animation",
    tier: "gpu",
    caps: {
      flagSyntax: false,
      aspectRatios: ["16:9", "9:16", "1:1", "4:3"],
      stylize: false,
      chaos: false,
      weird: false,
      tile: false,
      negativePrompt: true,
      seed: true,
      steps: true,
      guidance: true,
      referenceImage: true,       // depends on the loaded workflow
      videoClip: false,
      batchOf4: true,
      workflowDriven: true,       // controls depend on the chosen workflow/checkpoint
    },
    strengths: "Anything the loaded workflow supports, including LoRAs and reference images.",
  },
  // ── The paid APIs ─────────────────────────────────────────────────────────
  //
  // `paid: true` is not decoration. Somebody who picked one of these by accident and found out from
  // an invoice has been treated badly, so the picker shows the badge and the per-image price, and
  // `priceHint` is what the backend logs before it spends anything (see image_apis.rs).
  leonardo: {
    label: "Leonardo",
    note: "paid — one key for Flux, Ideogram, Recraft, Phoenix",
    tier: "api",
    paid: true,
    priceHint: 0.007,
    caps: {
      flagSyntax: false,
      aspectRatios: ["16:9", "9:16", "1:1", "4:3", "2:3"],
      stylize: false, chaos: false, weird: false, tile: false,
      negativePrompt: true,
      seed: true, steps: false, guidance: true,
      referenceImage: true, videoClip: false, batchOf4: true,
    },
    strengths: "A meta-platform: several of the best models behind one key and one bill. The cheapest per image of the four, and the one to start with if you do not want six accounts.",
  },
  fal: {
    label: "fal.ai",
    note: "paid — pay per image, no subscription",
    tier: "api",
    paid: true,
    priceHint: 0.05,
    caps: {
      flagSyntax: false,
      aspectRatios: ["16:9", "9:16", "1:1", "4:3", "2:3"],
      stylize: false, chaos: false, weird: false, tile: false,
      // No negative prompt on the FLUX-family models it fronts. The GUI must not offer one here:
      // a "things to avoid" box that does nothing is worse than no box at all.
      negativePrompt: false,
      seed: true, steps: true, guidance: true,
      referenceImage: true, videoClip: false, batchOf4: true,
    },
    strengths: "The widest model list and no subscription — you pay for what you generate. Good when you want one specific model rather than a platform.",
  },
  ideogram: {
    label: "Ideogram",
    note: "paid — best at text inside the image",
    tier: "api",
    paid: true,
    priceHint: 0.06,
    caps: {
      flagSyntax: false,
      aspectRatios: ["16:9", "9:16", "1:1", "4:3", "3:4", "2:3"],
      stylize: false, chaos: false, weird: false, tile: false,
      negativePrompt: true,
      seed: true, steps: false, guidance: false,
      referenceImage: true, videoClip: false, batchOf4: true,
      textInImage: true,
    },
    strengths: "Renders words inside a picture better than anything else here — verse cards and study cards. For words on a product, the typography layer is still more reliable.",
  },
  recraft: {
    label: "Recraft",
    note: "paid — logos, and real SVG output",
    tier: "api",
    paid: true,
    priceHint: 0.04,
    caps: {
      flagSyntax: false,
      aspectRatios: ["16:9", "9:16", "1:1", "4:3", "2:3"],
      stylize: false, chaos: false, weird: false, tile: false,
      negativePrompt: false,
      seed: false, steps: false, guidance: false,
      referenceImage: true, videoClip: false, batchOf4: false,
      vectorOutput: true,
      textInImage: true,
    },
    strengths: "The only one here that returns real vector. Logos, badges and ornaments that have to scale to a print area without going soft.",
  },
  gemini: {
    label: "Gemini image",
    note: "the API key you already use for text",
    tier: "api",
    caps: {
      flagSyntax: false,
      aspectRatios: ["16:9", "9:16", "1:1"],
      stylize: false,
      chaos: false,
      weird: false,
      tile: false,
      negativePrompt: false,
      seed: false,
      steps: false,
      guidance: false,
      referenceImage: true,
      videoClip: false,
      batchOf4: false,
    },
    strengths: "No GPU or account needed — just the API key you already use for text.",
  },
};

const EMPTY = { label: "Unknown engine", tier: "unknown", caps: {}, strengths: "" };

/** Ids that older settings files may still carry. */
const ALIASES = { comfy: "comfyui" };
const canonical = (id) => {
  const key = String(id || "").toLowerCase();
  return ALIASES[key] || key;
};

export const musicEngine = (id) => MUSIC_ENGINES[canonical(id)] || EMPTY;
export const imageEngine = (id) => IMAGE_ENGINES[canonical(id)] || EMPTY;

/** The engines a picker should offer, as `[id, engine]` pairs. */
function offer(catalogue, settings, current) {
  const show = settings?.show_risky_engines === true;
  return Object.entries(catalogue).filter(([id, engine]) => {
    // A switched-off engine is not offered even when it is the current selection — unlike the risky
    // ones below, there is no "turn it on" setting that would bring it back, so keeping it in the
    // list would only offer a choice that cannot be acted on. Somebody already on it therefore sees
    // the picker fall to its first entry, which is the honest outcome: that engine is gone for now.
    if (engineHidden(id)) return false;
    // The one already selected is always offered, even when it is hidden behind the risky switch. A
    // picker whose value is missing from its own list renders blank, and somebody on an engine they
    // cannot see cannot choose to leave it.
    return !engine.risky || show || id === canonical(current);
  });
}

export const visibleMusicEngines = (settings, current) => offer(MUSIC_ENGINES, settings, current);
export const visibleImageEngines = (settings, current) => offer(IMAGE_ENGINES, settings, current);

/** Is this engine one the user has to switch on before it appears? */
export const isRisky = (engine) => Boolean(engine?.risky);

/** Does using this engine cost money per generation? */
export const isPaid = (engine) => Boolean(engine?.paid);

/** "about $0.05 an image" / "about $0.100 a track" — the phrase the picker and the logs share. */
export const priceLine = (engine) =>
  isPaid(engine)
    ? `about $${Number(engine.priceHint || 0).toFixed(3)} ${engine.priceUnit || "an image"}`
    : "free";

/**
 * The engines a *fallback* picker may offer, given what the main engine is.
 *
 * A fallback exists so that one engine's outage costs a retry instead of the night's output. Being
 * quietly billed for that retry is a different and worse failure — and one nobody would notice until
 * the invoice — so a free engine may never fall back to a paid one. The backend enforces the same
 * rule (see `music_engine_is_paid` in jobs.rs); this is only so the choice is never offered.
 */
export function fallbackMusicEngines(settings, current, fallbackValue) {
  const primaryPaid = isPaid(musicEngine(current));
  return visibleMusicEngines(settings, fallbackValue).filter(([id, engine]) =>
    id !== String(current || "").toLowerCase()
    && (primaryPaid || !isPaid(engine) || id === String(fallbackValue || "").toLowerCase()));
}

/** Does the selected engine support this capability? Unknown engines answer "no". */
export const supports = (engine, cap) => Boolean(engine?.caps?.[cap]);

/**
 * The section-tag rule for a music engine, in one sentence, ready to drop into a prompt or a hint.
 * Mirrors `engine_lyric_annotation_guide` in src-tauri/commands/ai.rs — same rules, user-facing wording.
 */
export function lyricTagHint(engineId) {
  switch (musicEngine(engineId).lyricDialect) {
    case "suno":
      return "Suno reads capitalised bracketed tags like [Verse], [Chorus] and short performance hints such as [Soft female vocal].";
    case "acestep":
      return "ACE-Step wants plain lowercase tags on their own lines — [verse], [chorus]. It ignores performance directions.";
    default:
      return "HeartMuLa sings everything that is not a section header, so use only [Verse] / [Chorus] headers and no inline notes.";
  }
}

/** Capability summary for AI context and for the "why this default" line in the guided flow. */
export function engineContext(settings) {
  const m = musicEngine(settings?.music_engine);
  const i = imageEngine(settings?.image_engine);
  return {
    music_engine: settings?.music_engine || "",
    music_engine_label: m.label,
    music_lyric_dialect: m.lyricDialect,
    music_caps: m.caps,
    image_engine: settings?.image_engine || "",
    image_engine_label: i.label,
    image_caps: i.caps,
  };
}

// ── Engines switched off in this build ──────────────────────────────────────
//
// Mirrors HIDDEN_ENGINES in commands/settings.rs. Nothing is deleted: the engine keeps its settings
// card, its job path and its capability entry, and simply stops being offered or started. The
// backend refuses to push a run or start a monitor for anything listed there, so this list only
// controls what a person can *see* — it is not the safety mechanism, just the tidiness one.
//
// Empty right now. Kept because "not this one, for now" is a recurring need and the gate below is
// where it belongs — one string, rather than deletions scattered across six files.
export const HIDDEN_ENGINES = [];

/** Is this engine switched off? */
export const engineHidden = (id) =>
  HIDDEN_ENGINES.includes(String(id || "").trim().toLowerCase());

/** Drop switched-off engines from a list of ids. */
export const visibleEngines = (ids) => (ids || []).filter((e) => !engineHidden(e));
