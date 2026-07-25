import { imageEngine, musicEngine, supports } from "./engineCapabilities.js";

// ─────────────────────────────────────────────────────────────────────────────
// Control catalogues: the settings a template is allowed to decide, per view.
//
// A template is one pick that fills most of a page. For the AI to propose one it has to know what the
// page actually has — so each view declares its controls: an id, what kind of value it takes, the
// values it accepts, and which section it belongs to. The backend validates every proposed value
// against this (`sanitise_patch`), so a template can only ever set real controls to legal values.
//
// `apply(ctx, value)` is how a value reaches the page — the same contract the flow options use, so a
// template and a hand answer end up in exactly the same place.
//
// Keep `section` ids identical to the `reveals` ids in guidedFlows.js: that is what lets a template
// say "I have settled the visuals section" and have the page manifest it.
// ─────────────────────────────────────────────────────────────────────────────

const setCfg = (ctx, fn) => ctx.setCfg?.((p) => fn({ ...p }));

/** AI Composer. The biggest page, and the reason this exists. */
export const composerControls = (ctx = {}) => {
  const img = imageEngine(ctx.settings?.image_engine);
  const list = [
    {
      id: "theme_global", label: "Global mood/theme", kind: "text", section: "themes",
      hint: "The sacred/emotional register every language and channel variation is built on.",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, themes: { ...p.themes, global: String(v) } })),
    },
    {
      id: "style_keywords", label: "Image style keywords", kind: "list", section: "keywords",
      hint: "3–6 short visual descriptors, e.g. 'renaissance oil painting', 'dramatic chiaroscuro'.",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, style_keywords: (Array.isArray(v) ? v : [v]).map(String).slice(0, 8) })),
    },
    {
      id: "title_pattern", label: "Title pattern", kind: "text", section: "profiles",
      hint: "Placeholders: {artist} {book} {chapter} {channel} {language}.",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, title_pattern: String(v) })),
    },
    {
      id: "generate_lyrics", label: "Generate lyrics", kind: "bool", section: "fields",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, generate: { ...p.generate, lyrics: !!v } })),
    },
    {
      id: "generate_title", label: "Generate titles", kind: "bool", section: "fields",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, generate: { ...p.generate, title: !!v } })),
    },
    {
      id: "generate_styles", label: "Let the AI write the music style", kind: "bool", section: "fields",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, generate: { ...p.generate, styles: !!v } })),
    },
    {
      id: "generate_image_styles", label: "Generate per-section image prompts", kind: "bool", section: "fields",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, generate: { ...p.generate, image_styles: !!v } })),
    },
    {
      id: "generate_annotations", label: "Generate section annotations", kind: "bool", section: "fields",
      hint: "The [Verse]/[Chorus] structure the music engine reads.",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, generate: { ...p.generate, annotations: !!v } })),
    },
    {
      id: "aspect_ratio", label: "Image aspect ratio", kind: "enum", section: "images",
      options: img.caps?.aspectRatios || ["16:9", "9:16", "1:1"],
      apply: (c, v) => setCfg(c, (p) => ({ ...p, mj_ar: String(v) })),
    },
  ];

  // Engine-conditional controls: offering a Midjourney flag while FLUX is selected would produce a
  // template that writes settings the engine silently ignores.
  if (supports(img, "stylize")) {
    list.push({
      id: "stylize", label: "Stylisation strength", kind: "number", min: 0, max: 1000, section: "images",
      hint: "Midjourney --stylize. 100 is literal, 250–500 is painterly, 750+ is loose.",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, mj_stylize: Number(v) })),
    });
  }
  if (supports(img, "chaos")) {
    list.push({
      id: "chaos", label: "Variety between the four images", kind: "number", min: 0, max: 100, section: "images",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, mj_chaos: Number(v) })),
    });
  }
  if (supports(img, "negativePrompt")) {
    list.push({
      id: "negative_prompt", label: "Exclude from images", kind: "text", section: "images",
      hint: "Midjourney --no. e.g. 'text, watermark, distorted faces'.",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, mj_no: String(v) })),
    });
  }
  if (supports(img, "videoClip")) {
    list.push({
      id: "image_video", label: "Also make a moving clip per image", kind: "bool", section: "images",
      apply: (c, v) => setCfg(c, (p) => ({ ...p, mj_video: !!v })),
    });
  }
  return list;
};

/** Music Studio. */
export const musicControls = (ctx = {}) => {
  const eng = musicEngine(ctx.settings?.music_engine);
  const list = [
    {
      id: "music_engine", label: "Music engine", kind: "enum", section: "engine",
      options: ["suno", "acestep", "heartmula"],
      apply: (c, v) => c.setEngine?.(String(v)),
    },
    {
      id: "scope", label: "How many songs this run", kind: "enum", section: "scope",
      options: ["one", "all"],
      apply: (c, v) => c.setScope?.(String(v)),
    },
  ];
  if (supports(eng, "durationControl")) {
    list.push({
      id: "duration_s", label: "Track length (seconds)", kind: "number", min: 60, max: 600, section: "length",
      apply: (c, v) => c.setDuration?.(Number(v)),
    });
  }
  return list;
};

/** Image Generation. */
export const imagesControls = (ctx = {}) => {
  const eng = imageEngine(ctx.settings?.image_engine);
  const list = [
    {
      id: "coverage", label: "How many images", kind: "enum", section: "coverage",
      options: ["per_section", "key", "single"],
      apply: (c, v) => c.setCoverage?.(String(v)),
    },
    {
      id: "aspect_ratio", label: "Aspect ratio", kind: "enum", section: "coverage",
      options: eng.caps?.aspectRatios || ["16:9"],
      apply: (c, v) => c.setAspect?.(String(v)),
    },
  ];
  if (supports(eng, "steps")) {
    list.push({
      id: "steps", label: "Sampling steps", kind: "number", min: 1, max: 50, section: "quality",
      hint: "FLUX schnell is designed for 4; more steps mostly costs time.",
      apply: (c, v) => c.setSteps?.(Number(v)),
    });
  }
  return list;
};

/** Video Composer. */
export const videoControls = () => ([
  {
    id: "render_where", label: "Where the render runs", kind: "enum", section: "where",
    options: ["local", "kaggle", "actions", "modal", "http"],
    apply: (c, v) => c.setProvider?.(String(v)),
  },
  {
    id: "motion", label: "How the stills move", kind: "enum", section: "motion",
    options: ["fade", "kenburns", "overlay", "none"],
    apply: (c, v) => c.setTransition?.(String(v)),
  },
  {
    id: "publish", label: "What happens after the render", kind: "enum", section: "publish",
    options: ["hold", "private", "public"],
    hint: "'hold' never touches YouTube.",
    apply: (c, v) => c.setPublish?.(String(v)),
  },
]);

export const CATALOGS = {
  composer: composerControls,
  music: musicControls,
  images: imagesControls,
  video: videoControls,
};

/** The catalogue for a view, resolved against the current context. */
export const catalogFor = (view, ctx) => (CATALOGS[view] ? CATALOGS[view](ctx) : []);

/** What the backend needs: the descriptors without the `apply` closures. */
export const describeControls = (controls) =>
  controls.map(({ id, label, kind, options, min, max, section, hint }) => ({
    id, label, kind, options, min, max, section, hint: hint || "",
  }));

/**
 * Apply a validated template patch. Returns the ids that were applied, so the caller can mark the
 * sections those controls belong to as "set by a template" rather than "answered by the user".
 */
export function applyPatch(controls, patch, ctx) {
  const applied = [];
  for (const [id, value] of Object.entries(patch || {})) {
    const ctrl = controls.find((c) => c.id === id);
    if (!ctrl) continue;
    try {
      ctrl.apply(ctx, value);
      applied.push(id);
    } catch (err) {
      console.warn(`[guide] control ${id} could not be applied:`, err);
    }
  }
  return applied;
}
