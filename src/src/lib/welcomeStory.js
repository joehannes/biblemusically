// ─────────────────────────────────────────────────────────────────────────────
// The welcome tour's script.
//
// This is the FIRST thing a new user sees — before keys, before accounts, before anything
// technical. It walks the real pages of the app and explains what each one is for, once, in a
// human voice. It generates nothing, saves nothing and connects nothing.
//
// Every stop is written four times over, because "explain it simply" and "explain it densely" are
// not the same text with different adjectives — they answer different questions. The kid version
// answers *what happens*; the beginner version answers *why this step exists*; the adult version
// assumes creative-app literacy and only covers what is specific to this app; the professional
// version names the model, the limit and the trade-off, and skips the reassurance.
//
// The level comes from `audience_level` in settings (asked once, up front, changeable any time).
// It is about prior knowledge, never about age — see AudienceStepBody in guideSteps.jsx.
// ─────────────────────────────────────────────────────────────────────────────

export const LEVELS = ["kid", "beginner", "adult", "pro"];
export const DEFAULT_LEVEL = "beginner";

export const WELCOME_STOPS = [
  {
    id: "hello",
    to: "/",
    tag: "welcome",
    tip: "This tour only looks. Nothing is generated, saved or sent while it runs.",
    levels: {
      kid: {
        title: "Hi! This app makes music videos",
        body: "You give it an idea. It writes a song, sings it, paints the pictures, and puts them together into a video. You can watch every step, and change anything you don't like.",
      },
      beginner: {
        title: "What this app does",
        body: "It turns an idea into a finished music video. Words first, then the singing, then the pictures, then the video itself. Each of those is its own screen, so you can stop, look, and change your mind at any point — nothing happens all at once behind your back.",
      },
      adult: {
        title: "The short version",
        body: "An end-to-end music video pipeline: source text, lyrics, audio, per-section stills, assembled video, upload. Every stage is a page you can open on its own, and every stage's output is editable before the next one reads it.",
      },
      pro: {
        title: "The pipeline",
        body: "Source → lyrics (LLM) → audio (HeartMuLa / ACE-Step / Suno) → per-section stills (ComfyUI / FLUX / MidJourney) → ffmpeg assembly → YouTube. The stages are decoupled: each writes to project state and none auto-chains, except through the Workflow runner. Engines are swappable per stage.",
      },
    },
  },
  {
    id: "project",
    to: "/",
    tag: "the brief",
    tip: "Channels each carry their own language and region — one brief, spoken differently.",
    levels: {
      kid: {
        title: "Every video lives in a project",
        body: "A project is like a folder for one idea. Inside it you write down the mood you want, who it's for, and what today's song is about. The app reads that and keeps everything in the same style.",
      },
      beginner: {
        title: "Projects, and the Brief",
        body: "Everything starts with a project. Its Brief is a short description — mood, audience, what you're aiming for, today's topic. You write it once, and every later step reads from it, so your songs and pictures stay recognisably yours instead of drifting apart.",
      },
      adult: {
        title: "The Brief is the creative DNA",
        body: "A project holds one body of work; its Brief carries mood, audience, intent and today's topic. That gets folded into everything downstream — lyrics, styles, imagery, characters — so output stays coherent across sessions rather than restarting from zero each time.",
      },
      pro: {
        title: "Brief as persistent context",
        body: "The Brief is the context object merged into every generation prompt downstream, re-weighted per channel by region, culture and language. Tighten it and cross-run variance drops sharply — it is the cheapest quality lever in the app.",
      },
    },
  },
  {
    id: "sources",
    to: "/bible",
    tag: "raw material",
    levels: {
      kid: {
        title: "Where the words come from",
        body: "A song has to be about something. You can pick a piece of the Bible, or just type your own idea. Whatever you pick becomes the starting text.",
      },
      beginner: {
        title: "Choosing your source material",
        body: "This is where you pick what a song is built from — a scripture chapter, or free text you write yourself in the Freeform composer. Whatever you choose here is the raw material the songwriting step works from.",
        tip: "Projects that aren't scripture-based hide this tab and work straight from Freeform.",
      },
      adult: {
        title: "Source material",
        body: "Two inlets: a scripture passage, or arbitrary text via Freeform. Either becomes the source the composer works from. Non-scripture projects hide this tab entirely.",
      },
      pro: {
        title: "Source inlet",
        body: "Scripture reference or free text, both landing as the same source payload. The composer treats it as reference rather than template — structure comes from the target engine's section grammar, not from the source's shape.",
      },
    },
  },
  {
    id: "composer",
    to: "/composer",
    tag: "the words",
    tip: "Lyrics stay plain text everywhere — edit them like any other writing.",
    levels: {
      kid: {
        title: "The app writes the song",
        body: "Press a button and it writes the words, line by line. It also thinks up a picture for each part of the song. If you don't like a line, you can change it.",
      },
      beginner: {
        title: "Writing the lyrics",
        body: "The composer turns your source into an actual song: verses, chorus, and a picture idea for each section. It writes one version per channel, in that channel's language. You read them, edit anything, and import the ones you want to keep.",
      },
      adult: {
        title: "AI Composer",
        body: "Generates one song per channel — lyrics in each channel's language, engine-aware section tags, and a distinct image prompt per section. Review and edit in place, then import. Nothing enters the project until you import it.",
      },
      pro: {
        title: "Composer",
        body: "One pass yields per-channel lyrics with section tags matched to the target audio engine's grammar, plus a per-section image prompt. It reads the Brief and any live channel research, so register and idiom shift per audience rather than being translated after the fact.",
      },
    },
  },
  {
    id: "music",
    to: "/music",
    tag: "the sound",
    levels: {
      kid: {
        title: "Now it sings",
        body: "The words turn into real music with a voice. That takes a lot of computer power, so a big machine on the internet does it — for free.",
      },
      beginner: {
        title: "Turning lyrics into audio",
        body: "Each imported song is rendered into actual audio. That needs a powerful graphics card, so the app borrows a free one online: it starts the machine just before it's needed and shuts it down when things go quiet, because free accounts have weekly limits worth protecting.",
        tip: "You never have to remember to turn a server off — an idle guard does it after 25 quiet minutes.",
      },
      adult: {
        title: "Music generation",
        body: "Songs render on whichever engine you pick. The free GPU backends start automatically just before a job and stop after about 25 idle minutes, which is what keeps a weekly quota from evaporating on a session you walked away from.",
      },
      pro: {
        title: "Audio engines",
        body: "HeartMuLa and ACE-Step run on Kaggle GPUs; Suno goes through session capture. Style strings are re-tuned per engine and channel and biased by the Brief. The idle guard ends a session after 25 quiet minutes and never while a job is queued or running — Kaggle allows roughly 30 GPU-hours a week.",
      },
    },
  },
  {
    id: "images",
    to: "/images",
    tag: "the pictures",
    levels: {
      kid: {
        title: "A picture for every part",
        body: "Each piece of the song gets its own picture. Tap one to see it big. If you don't like it, ask for another one.",
      },
      beginner: {
        title: "The visuals",
        body: "Every section of the song gets an image, drawn from the idea the composer wrote for it. Click any image to open the explorer: compare versions side by side, generate alternatives, and mark the one you want to keep.",
        tip: "Marking a favourite quietly teaches the app your taste — later suggestions lean that way.",
      },
      adult: {
        title: "Images",
        body: "Section stills render from the per-section prompts. The explorer compares variants, generates alternates and sets a primary. Choosing a primary feeds the learning store, so later suggestions lean your way.",
      },
      pro: {
        title: "Image stack",
        body: "ComfyUI graphs — 29 registered, covering hi-res passes, LoRA stacks, two-stage refine, tiled upscale and area splits — plus FLUX and MidJourney paths. Graphs are API-format JSON with placeholder tokens filled at submit time. Primary selections are recorded as preference signal.",
      },
    },
  },
  {
    id: "characters",
    to: "/characters",
    tag: "who's in it",
    levels: {
      kid: {
        title: "The same faces every time",
        body: "If you want the same person in lots of pictures, tell the app about them once. Then they look the same in every picture, in every video.",
      },
      beginner: {
        title: "Keeping people consistent",
        body: "Image generators normally invent a new face every single time. This page fixes that: describe a figure once, and the app reuses that description everywhere, so the same person shows up across sections and across videos.",
      },
      adult: {
        title: "Characters",
        body: "A named-figure registry that pins appearance across generations, so recurring people stay recognisable between sections and between videos. Per-channel variants adapt the same character to a different visual culture without making them a different person.",
      },
      pro: {
        title: "Character consistency",
        body: "Persistent appearance descriptors merged into image prompts, with optional character LoRAs on the ComfyUI path. Per-channel variants re-skin one identity for regional visual conventions — held constant on identity terms, varied on styling.",
      },
    },
  },
  {
    id: "video",
    to: "/video",
    tag: "assembly",
    levels: {
      kid: {
        title: "Putting it all together",
        body: "The pictures and the music become one video. The pictures change along with the music, and words can appear on the screen.",
      },
      beginner: {
        title: "Building the video",
        body: "Here the images, the audio and the timings become a single video file. Sections follow the music, transitions smooth the cuts between pictures, and overlays put text on screen where you want it.",
      },
      adult: {
        title: "Video Composer",
        body: "Assembles sections, transitions and overlays against the audio's analysed structure into the final file. Transitions and overlays are their own editable stages when you want finer control than the defaults give.",
      },
      pro: {
        title: "Assembly",
        body: "ffmpeg composites section stills against beat and section analysis, with configurable transitions and overlay tracks. Runs locally, or offloads to a remote worker when the machine can't carry the encode.",
      },
    },
  },
  {
    id: "upload",
    to: "/upload",
    tag: "publishing",
    levels: {
      kid: {
        title: "Sharing it",
        body: "When the video is finished, the app can put it on YouTube for you. It always asks you first.",
      },
      beginner: {
        title: "Publishing",
        body: "Finished videos go to YouTube from here — the right video to the right channel, with a title and description already written for you. Nothing is published until you say so.",
      },
      adult: {
        title: "Upload",
        body: "Publishes to the connected YouTube channel with generated title, description and tags. Channel branding is read and merged rather than overwritten, and a dry-run shows exactly what would be sent before anything is.",
      },
      pro: {
        title: "Publish",
        body: "YouTube Data API upload with per-channel metadata. Branding updates read-then-merge, so a partial payload can't wipe existing brandingSettings. Dry-run prints the exact request body, and metadata enrichment is batched to stay inside free LLM request budgets.",
      },
    },
  },
  {
    id: "workflow",
    to: "/workflow",
    tag: "hands-off",
    levels: {
      kid: {
        title: "Or let it do everything",
        body: "You don't have to press every button yourself. This page does all the steps for you, one after another. You just watch it happen.",
      },
      beginner: {
        title: "Doing it all in one go",
        body: "Once you know the steps, this page runs them for you — source, lyrics, music, images, video — one after another, showing you where it has got to. Publishing stays switched off unless you turn it on.",
      },
      adult: {
        title: "The Workflow runner",
        body: "Chains the whole pipeline end to end with live per-stage progress. Upload and publish are excluded from 'run all' by default: going public should stay a deliberate act.",
      },
      pro: {
        title: "Orchestrator",
        body: "A stage sequencer over the same commands the individual pages call, with per-stage status, retry and abort. Publish stages are opt-in. Server start and stop is handled by the lifecycle guard around it, so a full run is genuinely unattended.",
      },
    },
  },
  {
    id: "next",
    to: "/",
    tag: "your turn",
    tip: "You can reopen this tour any time from the graduation-cap button at the top.",
    levels: {
      kid: {
        title: "That's the whole app!",
        body: "Now you can try it. If something needs a password or a key, the app will ask you at the moment it needs it — not before.",
      },
      beginner: {
        title: "That's the tour",
        body: "That's the whole shape of it. Nothing is connected yet — the setup guide walks through keys and accounts one at a time, and you can open it whenever you feel ready from the graduation-cap button up top.",
      },
      adult: {
        title: "You're set",
        body: "That's the pipeline. Connecting providers — an AI key, free GPUs, YouTube — lives under 'Set up & configure' in the graduation-cap menu, and can be done piecemeal as you need each one.",
      },
      pro: {
        title: "Done",
        body: "Providers, GPU backends and OAuth live under 'Set up & configure'. Minimum viable path is one LLM key plus one audio engine; everything else is additive and can wait until a run actually needs it.",
      },
    },
  },
];

/**
 * Resolve one stop to the wording for a level.
 *
 * Falls back level-by-level rather than to a generic string, so a stop that is only written at three
 * levels still reads like prose instead of showing a gap.
 */
export function storyFor(stop, level) {
  const chain = [level, DEFAULT_LEVEL, "adult", "kid"].filter(Boolean);
  const chosen = chain.map((l) => stop.levels?.[l]).find(Boolean) || {};
  return {
    title: chosen.title || "",
    body: chosen.body || "",
    tip: chosen.tip || stop.tip || "",
    tag: stop.tag || "",
  };
}

/**
 * How long to let someone look at a page before "Next" lights up.
 *
 * Scaled to the length of what is being said rather than fixed, because a two-line stop that makes
 * you wait as long as a dense one feels broken. Clamped at both ends: never so short that the page
 * has not visibly arrived, never so long that a returning user is held hostage by a pause.
 */
export function dwellMs(text) {
  const words = String(text || "").trim().split(/\s+/).filter(Boolean).length;
  return Math.min(9000, Math.max(2000, 1400 + words * 110));
}
