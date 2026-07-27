// What each generation engine can actually do.
//
// The studio speaks to three music engines and four image engines, and they differ in ways that are
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
  return Object.entries(catalogue).filter(([id, engine]) =>
    // The one already selected is always offered, even when it is hidden. A picker whose value is
    // missing from its own list renders blank, and somebody on an engine they cannot see cannot
    // choose to leave it.
    !engine.risky || show || id === canonical(current));
}

export const visibleMusicEngines = (settings, current) => offer(MUSIC_ENGINES, settings, current);
export const visibleImageEngines = (settings, current) => offer(IMAGE_ENGINES, settings, current);

/** Is this engine one the user has to switch on before it appears? */
export const isRisky = (engine) => Boolean(engine?.risky);

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
