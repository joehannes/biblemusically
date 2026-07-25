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

export const MUSIC_ENGINES = {
  suno: {
    label: "Suno",
    tier: "account",              // needs a logged-in Suno session
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
    tier: "account",
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
  comfy: {
    label: "ComfyUI",
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

export const musicEngine = (id) => MUSIC_ENGINES[String(id || "").toLowerCase()] || EMPTY;
export const imageEngine = (id) => IMAGE_ENGINES[String(id || "").toLowerCase()] || EMPTY;

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
