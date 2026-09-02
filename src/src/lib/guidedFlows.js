// The explicit .js extension keeps this importable by plain node (tests/guided-flows.test.mjs) as
// well as by vite.
import { musicEngine, imageEngine, supports, lyricTagHint, engineContext, visibleMusicEngines } from "./engineCapabilities.js";

// ─────────────────────────────────────────────────────────────────────────────
// Flow definitions: a page expressed as a short sequence of decisions.
//
// Each step is `{ id, title, question, help, reveals, when, options, primary }`. Options are built
// from the live page context, so what is offered adapts to the project and — importantly — to the
// selected engine: a step whose `when` fails, or an option whose `when` fails, is simply not shown.
// That is how "Midjourney stylize" disappears when FLUX is the image engine, instead of being offered
// and silently ignored.
//
// `apply(ctx)` writes into the page's own state through the setters the page passes in. No flow knows
// how the page stores anything beyond those setters.
// ─────────────────────────────────────────────────────────────────────────────

/** Shared AI context builder: what the proposal backend needs to personalise picks. */
const baseAiContext = (ctx) => ({
  project_id: ctx.projectId || "",
  project_name: ctx.project?.name || "",
  daily_topic: ctx.project?.daily_topic || "",
  brief: ctx.project?.brief || {},
  channels: (ctx.channels || []).map((c) => ({ name: c.name, language: c.language, region: c.region })),
  engines: engineContext(ctx.settings),
});

// ── AI Composer ──────────────────────────────────────────────────────────────
// The page the user described as bloated: nine sections at once, most of them irrelevant to any one
// run. As a flow it is six decisions, and each one opens exactly the section it is about.

export const composerFlow = {
  id: "composer",
  title: "Guided composing",
  aiContext: (ctx) => ({
    ...baseAiContext(ctx),
    current: {
      targets: (ctx.cfg?.targets || []).map((t) => `${t.language}: ${t.styles}`),
      theme: ctx.cfg?.themes?.global || "",
      style_keywords: ctx.cfg?.style_keywords || [],
      chapter: ctx.chapterRef || "",
      craft: ctx.cfg?.craft || {},
      generate_fields: ctx.cfg?.generate || {},
    },
  }),
  steps: [
    {
      id: "source",
      title: "Source",
      question: "What is today's song built on?",
      help: "The composer can start from a Bible chapter, from material you paste in, or from the brief alone.",
      reveals: "chapter",
      options: (ctx) => [
        {
          id: "chapter",
          label: ctx.chapterRef ? `Continue with ${ctx.chapterRef}` : "A Bible chapter",
          hint: ctx.chapterRef ? "Already loaded and ready." : "Pick book and chapter below, then fetch it.",
          recommended: true,
          apply: () => {},
        },
        {
          id: "next_chapter",
          label: "The next chapter in the book",
          hint: "Advances the chapter number so a series keeps moving.",
          when: (c) => Boolean(c.chapterRef),
          apply: (c) => c.setChapterNum?.((n) => Number(n || 0) + 1),
        },
        {
          id: "pasted",
          label: "Text I bring myself",
          hint: "A devotional, a testimony, your own draft — anything in the chapter box.",
          apply: (c) => c.setChapterText?.(c.chapterText || ""),
        },
        {
          id: "brief_only",
          label: "Just the brief and today's topic",
          hint: "No source text; the lyrics come from the project's own voice.",
          apply: (c) => { c.setChapterText?.(""); c.setChapterRef?.(""); },
        },
      ],
    },
    {
      id: "reach",
      title: "Reach",
      question: "How many channels should this run feed?",
      help: "Each target is one channel × language × style. More targets means more songs from one generation.",
      reveals: "targets",
      options: (ctx) => {
        const langs = [...new Set((ctx.channels || []).map((c) => c.language).filter(Boolean))];
        const cur = ctx.cfg?.targets?.length || 0;
        return [
          {
            id: "keep",
            label: `Keep my ${cur} target${cur === 1 ? "" : "s"}`,
            hint: (ctx.cfg?.targets || []).map((t) => t.language).join(", ") || "None configured yet.",
            recommended: cur > 0,
            apply: () => {},
          },
          {
            id: "one",
            label: "Just one — my main language",
            hint: "Fastest path to a finished video; least review.",
            apply: (c) => c.setCfg?.((p) => ({ ...p, targets: (p.targets || []).slice(0, 1) })),
          },
          {
            id: "all_channels",
            label: `One per channel language (${langs.length})`,
            hint: langs.join(", ") || "No channel languages found yet.",
            when: () => langs.length > 1,
            apply: (c) => c.setCfg?.((p) => ({
              ...p,
              targets: langs.map((language) => ({
                language,
                styles: p.targets?.find((t) => t.language === language)?.styles || p.targets?.[0]?.styles || "",
              })),
            })),
          },
        ];
      },
    },
    {
      id: "mood",
      title: "Mood",
      question: "What should today's song feel like?",
      help: "This becomes the global theme every language and channel variation is built on.",
      reveals: "themes",
      options: (ctx) => {
        const brief = ctx.project?.brief || {};
        const opts = [];
        if (brief.mood) {
          opts.push({
            id: "brief_mood",
            label: `The project's own mood`,
            hint: String(brief.mood).slice(0, 90),
            recommended: true,
            apply: (c) => c.setCfg?.((p) => ({ ...p, themes: { ...p.themes, global: String(brief.mood) } })),
          });
        }
        if (ctx.project?.daily_topic) {
          opts.push({
            id: "daily_topic",
            label: "Today's topic",
            hint: String(ctx.project.daily_topic).slice(0, 90),
            apply: (c) => c.setCfg?.((p) => ({
              ...p,
              themes: { ...p.themes, global: `${p.themes?.global || ""}, ${ctx.project.daily_topic}`.replace(/^,\s*/, "") },
            })),
          });
        }
        opts.push(
          {
            id: "comforting",
            label: "Comforting and quiet",
            hint: "sacred, gentle dawn, restful, soft chamber reverb",
            apply: (c) => c.setCfg?.((p) => ({ ...p, themes: { ...p.themes, global: "sacred, gentle dawn, restful, soft chamber reverb" } })),
          },
          {
            id: "triumphant",
            label: "Triumphant and bright",
            hint: "sovereign majesty, celebratory, radiant divine light",
            apply: (c) => c.setCfg?.((p) => ({ ...p, themes: { ...p.themes, global: "sovereign majesty, celebratory, radiant divine light" } })),
          },
        );
        return opts;
      },
    },
    {
      id: "faithfulness",
      title: "Closeness",
      question: (ctx) => ctx.chapterRef
        ? `How close should the words stay to ${ctx.chapterRef}?`
        : "How close should the words stay to your source?",
      help: "The one decision that changes the result most, and the only one nobody can make for you.",
      reveals: "craft",
      // Only worth asking when there is a source to be faithful to.
      when: (ctx) => Boolean(ctx.chapterText?.trim() || ctx.chapterRef),
      options: () => [
        {
          id: "quote", label: "Use its own words",
          hint: "Selected and arranged, not rewritten. Nothing invented.",
          apply: (c) => c.setCfg?.((p) => ({ ...p, craft: { ...(p.craft || {}), faithfulness: "quote" } })),
        },
        {
          id: "close", label: "Say the same thing, singably",
          hint: "Every image and claim kept; the wording is the song's.",
          recommended: true,
          apply: (c) => c.setCfg?.((p) => ({ ...p, craft: { ...(p.craft || {}), faithfulness: "close" } })),
        },
        {
          id: "inspired", label: "Take it somewhere",
          hint: "A starting point rather than a boundary.",
          apply: (c) => c.setCfg?.((p) => ({ ...p, craft: { ...(p.craft || {}), faithfulness: "inspired" } })),
        },
      ],
    },
    {
      id: "shape",
      title: "Shape",
      question: "What shape is the song?",
      help: (ctx) => lyricTagHint(ctx.settings?.music_engine),
      reveals: "craft",
      options: () => [
        {
          id: "verse_chorus", label: "Verses and a chorus",
          hint: "The chorus is the line people leave with.",
          recommended: true,
          apply: (c) => c.setCfg?.((p) => ({ ...p, craft: { ...(p.craft || {}), form: "verse_chorus" } })),
        },
        {
          id: "verse_refrain", label: "One line that comes back",
          hint: "Quieter than a chorus — it ends every verse.",
          apply: (c) => c.setCfg?.((p) => ({ ...p, craft: { ...(p.craft || {}), form: "verse_refrain" } })),
        },
        {
          id: "litany", label: "A list that builds",
          hint: "The same frame, filled differently, until it lands.",
          apply: (c) => c.setCfg?.((p) => ({ ...p, craft: { ...(p.craft || {}), form: "litany" } })),
        },
        {
          id: "through_composed", label: "It never repeats",
          hint: "A story that only moves forward.",
          apply: (c) => c.setCfg?.((p) => ({ ...p, craft: { ...(p.craft || {}), form: "through_composed" } })),
        },
      ],
    },
    {
      id: "style_depth",
      title: "Sound",
      question: (ctx) => `How much should the AI shape the ${musicEngine(ctx.settings?.music_engine).label} style?`,
      help: (ctx) => lyricTagHint(ctx.settings?.music_engine),
      reveals: "targets",
      options: (ctx) => {
        const eng = musicEngine(ctx.settings?.music_engine);
        return [
          {
            id: "keep_styles",
            label: "Keep the styles I already set",
            hint: (ctx.cfg?.targets || []).map((t) => t.styles).filter(Boolean).join(" · ").slice(0, 90) || "Nothing set yet.",
            recommended: true,
            apply: (c) => c.setCfg?.((p) => ({ ...p, generate: { ...p.generate, styles: false } })),
          },
          {
            id: "ai_styles",
            label: "Let the AI write the style descriptor",
            hint: supports(eng, "styleCsv")
              ? `A comma-separated descriptor tuned for ${eng.label} (≤${eng.caps.styleCsvLimit} chars).`
              : `Adapted to ${eng.label}.`,
            apply: (c) => c.setCfg?.((p) => ({ ...p, generate: { ...p.generate, styles: true } })),
          },
          {
            id: "regional_flavor",
            label: "Style per channel region",
            hint: "Each channel gets culturally-fitting instrumentation for its own audience.",
            when: (c) => (c.channels || []).some((ch) => ch.region),
            apply: (c) => c.setCfg?.((p) => ({ ...p, generate: { ...p.generate, styles: true, language: true } })),
          },
        ];
      },
    },
    {
      id: "visuals",
      title: "Visuals",
      question: "How should the images be prompted?",
      // Points at the ComfyUI style control while this step is on screen, so "how should the images
      // be prompted" is answered next to the thing that decides it rather than three pages away.
      spotlight: { testid: "settings-comfy-style" },
      help: (ctx) => {
        const eng = imageEngine(ctx.settings?.image_engine);
        return `${eng.label}: ${eng.strengths}`;
      },
      reveals: "images",
      options: (ctx) => {
        const eng = imageEngine(ctx.settings?.image_engine);
        const packs = Object.keys(ctx.stylePacks || {});
        return [
          {
            id: "keep_keywords",
            label: "Keep my style keywords",
            hint: (ctx.cfg?.style_keywords || []).slice(0, 3).join(", ") || "None yet.",
            recommended: (ctx.cfg?.style_keywords || []).length > 0,
            apply: () => {},
          },
          {
            id: "pack",
            label: "Use a style pack",
            hint: packs.slice(0, 4).join(" · "),
            when: () => packs.length > 0,
            apply: (c) => {
              const pack = packs[Math.floor(Math.random() * packs.length)];
              c.setCfg?.((p) => ({ ...p, style_keywords: c.stylePacks[pack] }));
            },
          },
          {
            id: "cinematic_wide",
            label: supports(eng, "flagSyntax") ? "Cinematic 16:9 with gentle stylisation" : "Cinematic 16:9",
            hint: supports(eng, "flagSyntax") ? "Sets --ar 16:9 and a moderate --stylize." : "Widescreen framing for video.",
            apply: (c) => c.setCfg?.((p) => ({
              ...p,
              mj_ar: "16:9",
              ...(supports(eng, "stylize") ? { mj_stylize: 250 } : {}),
            })),
          },
          {
            id: "vertical",
            label: "Vertical 9:16 for shorts",
            hint: "Use when this run is aimed at Shorts rather than long-form.",
            when: () => (eng.caps.aspectRatios || []).includes("9:16"),
            apply: (c) => c.setCfg?.((p) => ({ ...p, mj_ar: "9:16" })),
          },
        ];
      },
    },
    {
      id: "run",
      title: "Run",
      question: "Ready to generate?",
      help: "Everything below stays editable — this only fills in what you just decided.",
      reveals: "results",
      options: (ctx) => [
        {
          id: "generate",
          label: "Generate now",
          hint: `${(ctx.cfg?.targets || []).length} target(s) · ${ctx.chapterRef || "no chapter"}`,
          recommended: true,
          apply: (c) => c.generate?.(),
        },
        {
          id: "review_first",
          label: "Show me the full config first",
          hint: "Opens every section so you can check the details before running.",
          apply: (c) => c.showAll?.(),
        },
      ],
    },
  ],
};

// ── Music generation ─────────────────────────────────────────────────────────

export const musicFlow = {
  id: "music",
  title: "Guided music generation",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), pending_songs: ctx.pendingCount || 0 }),
  steps: [
    {
      id: "engine",
      title: "Engine",
      question: "Which engine should render the audio?",
      help: (ctx) => `Currently ${musicEngine(ctx.settings?.music_engine).label}. ${musicEngine(ctx.settings?.music_engine).strengths}`,
      // Whatever the picker offers — hardcoding the list here meant the guided flow kept offering
      // Suno after every other surface had stopped, and never learned about Riffusion or the paid
      // engine at all.
      options: (ctx) => visibleMusicEngines(ctx.settings, ctx.settings?.music_engine).map(([id, eng]) => {
        return {
          id,
          label: eng.label,
          hint: eng.strengths,
          recommended: ctx.settings?.music_engine === id,
          apply: (c) => c.setEngine?.(id),
        };
      }),
    },
    {
      id: "scope",
      title: "Scope",
      question: "How many songs this run?",
      options: (ctx) => [
        { id: "one", label: "One song", hint: "Check the result before committing GPU time to the rest.", recommended: true, apply: (c) => c.setScope?.("one") },
        { id: "all_ready", label: `All ready songs (${ctx.pendingCount || 0})`, hint: "Queue everything that has lyrics but no audio.", apply: (c) => c.setScope?.("all") },
      ],
    },
    {
      id: "length",
      title: "Length",
      question: "How long should the track be?",
      when: (ctx) => supports(musicEngine(ctx.settings?.music_engine), "durationControl"),
      options: () => [
        { id: "short", label: "~2 minutes", hint: "Quick to render, good for testing a style.", apply: (c) => c.setDuration?.(120) },
        { id: "standard", label: "~4 minutes", hint: "Normal song length.", recommended: true, apply: (c) => c.setDuration?.(240) },
        { id: "long", label: "~6 minutes", hint: "More room for instrumental passages.", apply: (c) => c.setDuration?.(360) },
      ],
    },
    {
      id: "start",
      title: "Start",
      question: "Queue it?",
      options: () => [
        { id: "go", label: "Queue the generation", recommended: true, apply: (c) => c.run?.() },
        { id: "later", label: "Not yet", hint: "Leaves your selection in place.", apply: () => {} },
      ],
    },
  ],
};

// ── Images ───────────────────────────────────────────────────────────────────

export const imagesFlow = {
  id: "images",
  title: "Guided image generation",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), sections: ctx.sectionCount || 0 }),
  steps: [
    {
      id: "coverage",
      title: "Coverage",
      question: "How many images does this song need?",
      options: (ctx) => [
        { id: "per_section", label: `One per section (${ctx.sectionCount || 0})`, hint: "Every analysed section gets its own scene.", recommended: true, apply: (c) => c.setCoverage?.("per_section") },
        { id: "key_moments", label: "Only the key moments", hint: "Chorus and bridge — cheaper, still carries the video.", apply: (c) => c.setCoverage?.("key") },
        { id: "single", label: "One still for the whole song", hint: "Fastest; the overlay carries the motion.", apply: (c) => c.setCoverage?.("single") },
      ],
    },
    {
      id: "consistency",
      title: "Consistency",
      question: "Should a character carry through the images?",
      when: (ctx) => (ctx.characters || []).length > 0,
      options: (ctx) => [
        { id: "none", label: "No recurring character", hint: "Scenes only.", apply: (c) => c.setCharacter?.(null) },
        ...(ctx.characters || []).slice(0, 3).map((ch) => ({
          id: `char:${ch.id}`,
          label: ch.name,
          hint: (ch.appearance || ch.prompt || "").slice(0, 80),
          apply: (c) => c.setCharacter?.(ch.id),
        })),
      ],
    },
    {
      id: "quality",
      title: "Quality",
      question: "Speed or fidelity?",
      when: (ctx) => supports(imageEngine(ctx.settings?.image_engine), "steps"),
      options: () => [
        { id: "fast", label: "Fast", hint: "Few steps — good enough for a first pass.", recommended: true, apply: (c) => c.setSteps?.(4) },
        { id: "fine", label: "Fine", hint: "More steps, slower, cleaner detail.", apply: (c) => c.setSteps?.(20) },
      ],
    },
    {
      id: "go",
      title: "Run",
      question: "Generate the images?",
      options: () => [
        { id: "run", label: "Generate", recommended: true, apply: (c) => c.run?.() },
        { id: "wait", label: "Not yet", apply: () => {} },
      ],
    },
  ],
};

// ── Video assembly + publishing ──────────────────────────────────────────────

export const videoFlow = {
  id: "video",
  title: "Guided video assembly",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), has_overlay: Boolean(ctx.hasOverlay), remote_provider: ctx.settings?.remote_render_provider || "local" }),
  steps: [
    {
      id: "where",
      title: "Where",
      question: "Where should the render run?",
      help: "Rendering remotely keeps your machine and your connection free; see docs/REMOTE_RENDER.md for the cost of each option.",
      options: (ctx) => [
        {
          id: "local",
          label: "On this machine",
          hint: "Uses your CPU and uploads over your connection.",
          recommended: (ctx.settings?.remote_render_provider || "local") === "local",
          apply: (c) => c.setProvider?.("local"),
        },
        {
          id: "kaggle",
          label: "On Kaggle (free)",
          hint: "12 h CPU sessions, 5 at a time, and it uploads to YouTube itself.",
          recommended: ctx.settings?.remote_render_provider === "kaggle",
          apply: (c) => c.setProvider?.("kaggle"),
        },
        {
          id: "actions",
          label: "On GitHub Actions",
          hint: "Free and unmetered on a public repo; 2,000 min/month on a private one.",
          when: (c) => Boolean(c.settings?.git_remote_url || c.settings?.github_token),
          apply: (c) => c.setProvider?.("actions"),
        },
        {
          id: "modal",
          label: "On Modal",
          hint: "$30/month of free credits — this workload costs a few dollars.",
          when: (c) => Boolean(c.settings?.modal_endpoint),
          apply: (c) => c.setProvider?.("modal"),
        },
      ],
    },
    {
      id: "motion",
      title: "Motion",
      question: "How should the stills move?",
      options: (ctx) => [
        { id: "crossfade", label: "Crossfade between sections", hint: "Calm, matches most worship material.", recommended: true, apply: (c) => c.setTransition?.("fade") },
        { id: "kenburns", label: "Slow push in", hint: "Ken-Burns drift; adds life to a single still.", apply: (c) => c.setTransition?.("kenburns") },
        { id: "overlay", label: "Audio-reactive overlay", hint: ctx.hasOverlay ? "Overlay already rendered for this song." : "Renders a visualiser layer over the stills.", apply: (c) => c.setTransition?.("overlay") },
      ],
    },
    {
      id: "publish",
      title: "Publish",
      question: "What happens when the render finishes?",
      options: () => [
        { id: "hold", label: "Hold it for review", hint: "Nothing reaches YouTube until you say so.", recommended: true, apply: (c) => c.setPublish?.("hold") },
        { id: "private", label: "Upload as private", hint: "Lands on the channel, visible only to you.", apply: (c) => c.setPublish?.("private") },
        { id: "public", label: "Upload and publish", hint: "Goes live on the channel immediately.", apply: (c) => c.setPublish?.("public") },
      ],
    },
    {
      id: "start",
      title: "Start",
      question: "Start the render?",
      options: () => [
        { id: "go", label: "Start", recommended: true, apply: (c) => c.run?.() },
        { id: "no", label: "Not yet", apply: () => {} },
      ],
    },
  ],
};

// ── Bible Sources ────────────────────────────────────────────────────────────

export const bibleFlow = {
  id: "bible",
  title: "Guided source picking",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), current_book: ctx.book || "", current_chapter: ctx.chapter || 0 }),
  steps: [
    {
      id: "kind",
      title: "Source",
      question: "What should today's song come from?",
      reveals: "source",
      options: (ctx) => [
        {
          id: "continue",
          label: ctx.book ? `Continue ${ctx.book} ${Number(ctx.chapter || 0) + 1}` : "Continue where I left off",
          hint: "Keeps a series moving through a book, chapter by chapter.",
          recommended: Boolean(ctx.book),
          when: (c) => Boolean(c.book),
          apply: (c) => c.setChapter?.(Number(c.chapter || 0) + 1),
        },
        { id: "pick", label: "A specific chapter", hint: "Choose the book and chapter yourself.", recommended: !ctx.book, apply: () => {} },
        { id: "paste", label: "Text I bring myself", hint: "A devotional, testimony or your own draft.", apply: (c) => c.setMode?.("paste") },
      ],
    },
    {
      id: "translation",
      title: "Translation",
      question: "Which translation?",
      help: "Public-domain translations only — anything else could not be published safely.",
      reveals: "translation",
      options: (ctx) => {
        const langs = [...new Set((ctx.channels || []).map((c) => c.language).filter(Boolean))];
        return [
          { id: "keep", label: "Keep the current one", hint: ctx.translation || "none chosen yet", recommended: Boolean(ctx.translation), apply: () => {} },
          {
            id: "match_channel",
            label: "Match my channel's language",
            hint: langs.slice(0, 3).join(", ") || "no channel languages found",
            when: () => langs.length > 0,
            apply: (c) => c.setTranslationForLanguage?.(langs[0]),
          },
          { id: "english", label: "English (KJV/WEB)", hint: "Safest default; the AI translates the lyrics per channel anyway.", apply: (c) => c.setTranslation?.("kjv") },
        ];
      },
    },
    {
      id: "fetch",
      title: "Fetch",
      question: "Load it?",
      reveals: "text",
      options: () => [
        { id: "fetch", label: "Load this chapter", recommended: true, apply: (c) => c.fetch?.() },
        { id: "later", label: "Not yet", apply: () => {} },
      ],
    },
  ],
};

// ── Audio analysis ───────────────────────────────────────────────────────────

export const analysisFlow = {
  id: "analysis",
  title: "Guided analysis",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), sections: ctx.sectionCount || 0 }),
  steps: [
    {
      id: "when",
      title: "Sections",
      question: "How should this track be cut into scenes?",
      help: "Sections drive one image each, so this decides how the video is paced.",
      reveals: "sections",
      options: (ctx) => [
        {
          id: "annotations",
          label: "From the lyric structure",
          hint: "Uses the [Verse]/[Chorus] tags already in the lyrics — fastest and usually right.",
          recommended: true,
          apply: (c) => c.analyze?.(),
        },
        {
          id: "reanalyse",
          label: "Re-cut it",
          hint: ctx.sectionCount ? `Replaces the ${ctx.sectionCount} sections that exist.` : "Nothing to replace yet.",
          when: (c) => Boolean(c.sectionCount),
          apply: (c) => c.analyze?.(),
        },
        { id: "manual", label: "I'll cut it myself", hint: "Opens the Section Editor instead.", apply: (c) => c.goManual?.() },
      ],
    },
    {
      id: "review",
      title: "Review",
      question: "Anything to fix before images?",
      reveals: "review",
      options: (ctx) => [
        { id: "ok", label: `Looks right (${ctx.sectionCount || 0} sections)`, recommended: true, apply: () => {} },
        { id: "edit", label: "Let me adjust the timings", apply: (c) => c.goManual?.() },
      ],
    },
  ],
};

// ── Characters ───────────────────────────────────────────────────────────────

export const charactersFlow = {
  id: "characters",
  title: "Guided characters",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), characters: (ctx.characters || []).map((c) => c.name) }),
  steps: [
    {
      id: "need",
      title: "Faces",
      question: "Should this project have recurring characters?",
      help: "A character carries appearance tags into every image prompt, so the same figure keeps showing up.",
      reveals: "list",
      options: (ctx) => [
        {
          id: "propose",
          label: "Suggest them from the lyrics",
          hint: "Reads the song and proposes the figures it actually mentions.",
          recommended: (ctx.characters || []).length === 0,
          apply: (c) => c.propose?.(),
        },
        {
          id: "keep",
          label: `Use the ${(ctx.characters || []).length} I have`,
          hint: (ctx.characters || []).map((c) => c.name).slice(0, 3).join(", "),
          when: (c) => (c.characters || []).length > 0,
          recommended: (ctx.characters || []).length > 0,
          apply: () => {},
        },
        { id: "none", label: "No characters — scenes only", hint: "Landscapes and symbolism instead of people.", apply: () => {} },
      ],
    },
    {
      id: "reach",
      title: "Per channel",
      question: "Should characters adapt per channel?",
      help: "A regional variant keeps the same person recognisable while fitting each audience.",
      reveals: "variants",
      when: (ctx) => (ctx.characters || []).length > 0 && (ctx.channels || []).length > 1,
      options: () => [
        { id: "global", label: "One look everywhere", hint: "Simplest, and consistent across channels.", recommended: true, apply: () => {} },
        { id: "per_channel", label: "A variant per channel", hint: "Costs one image generation per character per channel.", apply: (c) => c.adaptAll?.() },
      ],
    },
  ],
};

// ── Publishing ───────────────────────────────────────────────────────────────

export const uploadFlow = {
  id: "upload",
  title: "Guided publishing",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), pending: ctx.pendingCount || 0 }),
  steps: [
    {
      id: "target",
      title: "Channel",
      question: "Where is this going?",
      reveals: "target",
      options: (ctx) => {
        const chans = (ctx.channels || []).filter((c) => c.refresh_token || c.youtube_channel_id);
        return [
          {
            id: "auto",
            label: "Match the song's language",
            hint: "Picks the channel whose language the song was written for.",
            recommended: chans.length > 1,
            apply: (c) => c.setChannel?.("auto"),
          },
          ...chans.slice(0, 3).map((ch) => ({
            id: `chan:${ch.id}`,
            label: ch.name || ch.youtube_channel_id,
            hint: [ch.language, ch.region].filter(Boolean).join(" · "),
            apply: (c) => c.setChannel?.(ch.id),
          })),
        ];
      },
    },
    {
      id: "metadata",
      title: "Title & text",
      question: "Who writes the title and description?",
      reveals: "metadata",
      options: () => [
        { id: "ai", label: "Let the AI write them", hint: "From the song, the brief and the channel's language.", recommended: true, apply: (c) => c.generateMetadata?.() },
        { id: "mine", label: "I'll write them", hint: "The fields below stay as they are.", apply: () => {} },
      ],
    },
    {
      id: "visibility",
      title: "Visibility",
      question: "How should it go up?",
      help: "Nothing is published without this answer — 'private' is a safe first run on a new channel.",
      reveals: "visibility",
      options: () => [
        { id: "private", label: "Private", hint: "On the channel, visible only to you.", recommended: true, apply: (c) => c.setPrivacy?.("private") },
        { id: "unlisted", label: "Unlisted", hint: "Anyone with the link can watch.", apply: (c) => c.setPrivacy?.("unlisted") },
        { id: "public", label: "Public", hint: "Live immediately.", apply: (c) => c.setPrivacy?.("public") },
      ],
    },
    {
      id: "go",
      title: "Publish",
      question: "Queue the upload?",
      reveals: "queue",
      options: (ctx) => [
        { id: "one", label: "Queue this one", recommended: true, apply: (c) => c.create?.() },
        { id: "all", label: `Queue everything ready (${ctx.pendingCount || 0})`, when: (c) => (c.pendingCount || 0) > 1, apply: (c) => c.publishAll?.() },
        { id: "hold", label: "Not yet", apply: () => {} },
      ],
    },
  ],
};

// ── Social presence ──────────────────────────────────────────────────────────

export const socialFlow = {
  id: "social",
  title: "Guided social posts",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), platforms: (ctx.accounts || []).map((a) => a.platform) }),
  steps: [
    {
      id: "purpose",
      title: "Purpose",
      question: "What are these posts for?",
      reveals: "purpose",
      options: () => [
        { id: "announce", label: "Announce the new video", hint: "A short post per platform pointing at the upload.", recommended: true, apply: (c) => c.setKind?.("announce") },
        { id: "clip", label: "Share a clip", hint: "A vertical cut from the song as its own post.", apply: (c) => c.setKind?.("clip") },
        { id: "verse", label: "Post the verse alone", hint: "Scripture card, no link — reach without promotion.", apply: (c) => c.setKind?.("verse") },
      ],
    },
    {
      id: "where",
      title: "Platforms",
      question: "Which platforms?",
      reveals: "platforms",
      options: (ctx) => {
        const linked = (ctx.accounts || []).map((a) => a.platform).filter(Boolean);
        return [
          { id: "all_linked", label: `All connected (${linked.length})`, hint: linked.join(", ") || "none connected yet", recommended: linked.length > 0, when: () => linked.length > 0, apply: (c) => c.setPlatforms?.(linked) },
          ...linked.slice(0, 3).map((p) => ({ id: `p:${p}`, label: p, apply: (c) => c.setPlatforms?.([p]) })),
          { id: "connect", label: "Connect an account first", when: () => linked.length === 0, apply: (c) => c.openConnect?.() },
        ];
      },
    },
    {
      id: "write",
      title: "Write",
      question: "Draft them now?",
      reveals: "drafts",
      options: () => [
        { id: "draft", label: "Draft the posts", hint: "You review every one before anything is sent.", recommended: true, apply: (c) => c.draft?.() },
        { id: "later", label: "Not yet", apply: () => {} },
      ],
    },
  ],
};

// ── Whole-pipeline run ───────────────────────────────────────────────────────

export const workflowFlow = {
  id: "workflow",
  title: "Guided run",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), stages: ctx.stages || [] }),
  steps: [
    {
      id: "scope",
      title: "How far",
      question: "How far should this run go?",
      help: "Nothing reaches YouTube unless the last option is chosen explicitly.",
      reveals: "scope",
      options: () => [
        { id: "to_music", label: "Lyrics and music", hint: "Stop once there is audio to listen to.", apply: (c) => c.setStopAfter?.("music") },
        { id: "to_video", label: "All the way to a video file", hint: "Lyrics, music, analysis, images, video — held for review.", recommended: true, apply: (c) => c.setStopAfter?.("video") },
        { id: "to_publish", label: "Including publishing", hint: "Uploads to the matching channel when the video is done.", apply: (c) => c.setStopAfter?.("upload") },
      ],
    },
    {
      id: "where",
      title: "Where",
      question: "Where should the heavy steps run?",
      reveals: "where",
      options: (ctx) => [
        { id: "local", label: "On this machine", hint: "Uses your CPU and your connection.", recommended: (ctx.settings?.remote_render_provider || "local") === "local", apply: (c) => c.setProvider?.("local") },
        { id: "kaggle", label: "On Kaggle (free)", hint: "Renders and uploads without touching your bandwidth.", recommended: ctx.settings?.remote_render_provider === "kaggle", apply: (c) => c.setProvider?.("kaggle") },
      ],
    },
    {
      id: "start",
      title: "Start",
      question: "Start the run?",
      reveals: "run",
      options: () => [
        { id: "go", label: "Run it", recommended: true, apply: (c) => c.run?.() },
        { id: "no", label: "Not yet", apply: () => {} },
      ],
    },
  ],
};


// ── Publicity ────────────────────────────────────────────────────────────────

export const publicityFlow = {
  id: "publicity",
  title: "Guided publicity",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), platforms: (ctx.accounts || []).map((a) => a.platform), pieces: ctx.pieceCount || 0 }),
  steps: [
    {
      id: "where",
      title: "Where",
      question: "Who is this piece for?",
      help: "Each platform gets its own register — a Reddit post that reads like a press release is removed, a blog post that reads like a tweet is ignored.",
      reveals: "target",
      options: (ctx) => {
        const linked = (ctx.accounts || []).map((a) => a.platform);
        const named = (id) => (ctx.platforms || []).find((p) => p.id === id)?.label || id;
        return [
          {
            id: "all",
            label: `Every connected platform (${linked.length})`,
            hint: linked.map(named).join(", ") || "none connected yet",
            when: () => linked.length > 1,
            recommended: linked.length > 1,
            apply: (c) => c.setPlatform?.(""),
          },
          ...linked.slice(0, 3).map((id) => ({
            id: `p:${id}`, label: named(id), hint: "One piece, written for this platform only.",
            apply: (c) => c.setPlatform?.(id),
          })),
          {
            id: "article",
            label: "A long-form article",
            hint: "Blog/DEV length — the song illustrates the idea rather than being the subject.",
            apply: (c) => c.setPlatform?.("devto"),
          },
        ];
      },
    },
    {
      id: "angle",
      title: "Angle",
      question: "What should it lead with?",
      help: "Leave it open and the writer draws the angle from the brief, the scripture and today's topic.",
      reveals: "angle",
      options: (ctx) => [
        { id: "auto", label: "Let it choose", hint: "From the brief, the song and today's topic.", recommended: true, apply: (c) => c.setAngle?.("") },
        { id: "scripture", label: "The scripture itself", hint: "The passage first, the song as its setting.", apply: (c) => c.setAngle?.("open with the scripture passage and what it is actually saying") },
        { id: "craft", label: "How it was made", hint: "The production story — style choices, imagery, language.", apply: (c) => c.setAngle?.("tell the making-of: the style choices, the imagery and why") },
        { id: "topic", label: "Today's topic", hint: ctx.project?.daily_topic || "No topic set for today.", when: (c) => Boolean(c.project?.daily_topic), apply: (c) => c.setAngle?.(`tie it to today's topic: ${ctx.project.daily_topic}`) },
      ],
    },
    {
      id: "write",
      title: "Write",
      question: "Write it now?",
      reveals: "pieces",
      options: () => [
        { id: "go", label: "Write the draft", hint: "Nothing is posted — it lands here for you to read.", recommended: true, apply: (c) => c.write?.() },
        { id: "later", label: "Not yet", apply: () => {} },
      ],
    },
  ],
};

export const FLOWS = {
  composer: composerFlow,
  music: musicFlow,
  images: imagesFlow,
  video: videoFlow,
  bible: bibleFlow,
  analysis: analysisFlow,
  characters: charactersFlow,
  upload: uploadFlow,
  social: socialFlow,
  workflow: workflowFlow,
  publicity: publicityFlow,
};

// ── Distribution ────────────────────────────────────────────────────────────
// The one guided flow whose first question is a policy question, because it has to be: two of the
// distributors in the list refuse fully AI-generated music, and finding that out at delivery costs an
// account rather than an upload.
export const distributionFlow = {
  id: "distribution",
  title: "Guided distribution",
  aiContext: (ctx) => ({
    ...baseAiContext(ctx),
    releases: ctx.releases?.length || 0,
    ready_albums: ctx.plan?.albums?.length || 0,
    ready_singles: ctx.plan?.singles?.length || 0,
  }),
  steps: [
    {
      id: "what",
      title: "What",
      question: "What are you releasing?",
      help: "A chapter is a single; a finished book is an album. Albums do better on streaming — a listener finds one thing, not fifty.",
      reveals: "releases",
      options: (ctx) => [
        {
          id: "album",
          label: `A whole book as an album (${ctx.plan?.albums?.length || 0} ready)`,
          hint: "Every chapter, in order, under one release.",
          when: (c) => (c.plan?.albums?.length || 0) > 0,
          recommended: true,
          apply: (c) => c.setTab?.("releases"),
        },
        {
          id: "single",
          label: "One chapter as a single",
          hint: "Good for testing the whole path end to end before committing a book.",
          when: (c) => (c.plan?.singles?.length || 0) > 0,
          apply: (c) => c.setTab?.("releases"),
        },
        {
          id: "compilation",
          label: "A book as one long video",
          hint: "Not a streaming release — a single YouTube upload with chapter markers.",
          apply: (c) => c.setTab?.("compilations"),
        },
      ],
    },
    {
      id: "who",
      title: "Artist",
      question: "Who is it by?",
      help: "A release needs an artist, and the artist is the channel — that is what keeps the royalties attached to the thing that earns them.",
      reveals: "artist",
      options: (ctx) => [
        {
          id: "existing",
          label: `Release as ${ctx.artists?.[0]?.name || "the saved artist"}`,
          hint: "Already set up.",
          when: (c) => (c.artists?.length || 0) > 0,
          recommended: (ctx.artists?.length || 0) > 0,
          apply: () => {},
        },
        {
          id: "new",
          label: "Set up an artist now",
          hint: "Name, bio, and which channel it releases as.",
          apply: (c) => c.setTab?.("artists"),
        },
      ],
    },
    {
      id: "distributor",
      title: "Distributor",
      question: "Who delivers it to the stores?",
      help: "None of them offer an upload API, so this decides the terms, not the automation. What matters most is their AI policy: two on the list refuse generated music outright.",
      reveals: "distributor",
      options: (ctx) => {
        const ok = (ctx.distributors || []).filter((d) => d.ai === "accepts");
        const free = ok.find((d) => (d.price || "").toLowerCase().includes("free"));
        return [
          {
            id: "free",
            label: `Start free — ${free?.name || "RouteNote"}`,
            hint: free ? `${free.price}. ${free.royalty}.` : "A free tier takes a cut instead of a fee.",
            recommended: true,
            when: () => Boolean(free),
            apply: (c) => c.setTab?.("distributors"),
          },
          {
            id: "uncapped",
            label: "Pay, and keep everything",
            hint: "A yearly fee, full royalties, no per-release cut.",
            apply: (c) => c.setTab?.("distributors"),
          },
          {
            id: "compare",
            label: "Show me the comparison",
            hint: "Price, royalty, AI policy and rate caps side by side.",
            apply: (c) => c.setTab?.("distributors"),
          },
        ];
      },
    },
    {
      id: "package",
      title: "Package",
      question: "Build the upload package?",
      help: "Numbered audio, the cover, metadata as CSV, and the AI-credits disclosure Spotify's DDEX standard requires. Then a macro or you does the upload itself.",
      reveals: "package",
      options: () => [
        { id: "go", label: "Export the package", hint: "Writes a folder; nothing is delivered yet.", recommended: true, apply: (c) => c.setTab?.("releases") },
        { id: "cover", label: "Make the cover first", hint: "Stores reject a release without square artwork.", apply: (c) => c.setTab?.("releases") },
      ],
    },
  ],
};

// ── Graphic novels → ebooks ─────────────────────────────────────────────────
// The first question is the voice, because it is the only one whose answer cannot be fixed later
// without rewriting the whole edition.
export const novelFlow = {
  id: "novel",
  title: "Guided edition",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), editions: ctx.editions?.length || 0 }),
  steps: [
    {
      id: "voice",
      title: "Voice",
      question: "Whose voice is this edition in?",
      help: "These are different writers, not settings on one writer. \"More poetic\" produces the same faintly purple prose every time; a scribe's margin gloss produces something with a shape.",
      reveals: "register",
      options: (ctx) => (ctx.registers || []).slice(0, 4).map((r, i) => ({
        id: r.id,
        label: r.label,
        hint: r.hint,
        recommended: i === 1,
        apply: (c) => c.setRegister?.(r.id),
      })),
    },
    {
      id: "shape",
      title: "Shape",
      question: "What shape are the pages?",
      help: "Decided before the art, not after: a 16:9 panel dropped into a portrait book is a letterboxed strip.",
      reveals: "format",
      options: (ctx) => (ctx.formats || []).map((f, i) => ({
        id: f.id,
        label: `${f.label} (${f.aspect})`,
        hint: f.hint,
        recommended: i === 0,
        apply: (c) => c.setFormat?.(f.id),
      })),
    },
    {
      id: "length",
      title: "Length",
      question: "How long?",
      help: "Length drives the price: a longer book sits higher in every store's royalty band.",
      reveals: "pages",
      options: () => [
        { id: "short", label: "A short piece — 8 pages", hint: "One sitting, one idea.", apply: (c) => c.setPages?.(8) },
        { id: "standard", label: "A full edition — 12 pages", hint: "The default, and the shape that prices well.", recommended: true, apply: (c) => c.setPages?.(12) },
        { id: "long", label: "A long edition — 24 pages", hint: "More art to generate, and a higher price band.", apply: (c) => c.setPages?.(24) },
      ],
    },
    {
      id: "go",
      title: "Write",
      question: "Write it now?",
      reveals: "edition",
      options: () => [
        { id: "write", label: "Write the edition", hint: "Text first; the art is queued after you have read it.", recommended: true, apply: (c) => c.write?.() },
        { id: "later", label: "Not yet", apply: () => {} },
      ],
    },
  ],
};

// ── Print on demand ─────────────────────────────────────────────────────────
// The only flow here that can genuinely publish by itself, which is why the last question is about
// whether it should.
export const podFlow = {
  id: "pod",
  title: "Guided print on demand",
  aiContext: (ctx) => ({ ...baseAiContext(ctx), picked: ctx.picked?.length || 0, made: ctx.made?.length || 0 }),
  steps: [
    {
      id: "shop",
      title: "Shop",
      question: "Where do the products go?",
      help: "Printify's own Pop-Up Store is free and needs no external account — the only storefront where publishing something every day costs nothing. Etsy charges $0.20 a listing, which a daily run turns into a monthly bill.",
      reveals: "shop",
      options: (ctx) => [
        {
          id: "popup",
          label: "Printify Pop-Up Store — free",
          hint: "No external account, no listing fee.",
          recommended: true,
          apply: (c) => c.setTab?.("shop"),
        },
        {
          id: "existing",
          label: `Use a shop I already connected (${ctx.shops?.length || 0})`,
          hint: "Etsy, TikTok Shop, Shopify — whatever is linked in Printify.",
          when: (c) => (c.shops?.length || 0) > 0,
          apply: (c) => c.setTab?.("shop"),
        },
        {
          id: "connect",
          label: "Connect Printify now",
          hint: "Needs a Personal Access Token from your Printify profile.",
          apply: (c) => c.connect?.(),
        },
      ],
    },
    {
      id: "products",
      title: "Products",
      question: "Which products?",
      help: "Picked once, on purpose: a daily run should apply wording and art, never choose products. Mugs and posters show every flaw in low-resolution art; fabric hides it.",
      reveals: "products",
      options: (ctx) => [
        {
          id: "have",
          label: `Keep the ${ctx.picked?.length || 0} already picked`,
          hint: "Nothing to decide.",
          when: (c) => (c.picked?.length || 0) > 0,
          recommended: (ctx.picked?.length || 0) > 0,
          apply: () => {},
        },
        {
          id: "apparel",
          label: "Start with apparel",
          hint: "Shirts and hoodies — the most forgiving surface for generated art.",
          apply: (c) => c.setTab?.("products"),
        },
        {
          id: "hard",
          label: "Mugs and posters",
          hint: "Higher margin, but they need art at full print resolution.",
          apply: (c) => c.setTab?.("products"),
        },
      ],
    },
    {
      id: "publish",
      title: "Publish",
      question: "Should it publish, or leave drafts?",
      help: "A published product is public and priced. Drafts let you look at the mockups first, which is worth doing once before trusting the run.",
      reveals: "run",
      options: () => [
        { id: "draft", label: "Create drafts", hint: "Review the mockups in Printify, publish there.", recommended: true, apply: (c) => c.setTab?.("make") },
        { id: "live", label: "Create and publish", hint: "Straight to the storefront. Capped at 200 per 30 minutes by Printify.", apply: (c) => c.setTab?.("make") },
      ],
    },
  ],
};
