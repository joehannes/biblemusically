#!/usr/bin/env node
// Extract the interface's literal strings into src/src/i18n/ui-strings.json.
//
// The runtime translator (src/src/lib/uiTranslate.js) keys translations by the EXACT trimmed text of
// a DOM text node, so this extractor has to mirror how JSX becomes text nodes:
//   • a `{expr}` inside JSX text splits the surrounding literal into separate text nodes;
//   • entities (&amp; &nbsp; …) are decoded, because the DOM holds the decoded character;
//   • the same eligibility rules apply (2…140 chars, must contain a letter, no code/urls).
//
// It also pulls the string literals out of the tour/guide/step catalogs (lib/pageSteps.js,
// lib/guideSteps.jsx, components/Tours.jsx …), whose text only reaches the DOM once the user opens
// that panel — those are exactly the strings that used to stay English.
//
// Usage: node scripts/extract-ui-strings.mjs [--check]
//   --check exits non-zero when the committed inventory is out of date (for CI/pre-release).

import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = join(ROOT, "src/src");
const OUT = join(SRC, "i18n/ui-strings.json");

// 140 kept the inventory small, and made every explanatory paragraph in the app permanently
// untranslatable: a string absent from the inventory is refused at runtime by `mayTranslate` once it
// passes 80 characters, so anything between the two ceilings could never be translated into any
// language, bundled ones included. That is the "descriptions stayed English" bug.
//
// 400 covers the app's real prose — the sentences under a setting that explain what it costs — while
// still excluding the things this cap exists to exclude, which are runtime values, not long text.
const MAX_LEN = Number(process.env.I18N_MAX_LEN) || 400;
// Attributes whose value is interface chrome the user reads.
const TEXT_ATTRS = ["placeholder", "title", "aria-label", "alt", "label"];
// Object keys in the tour/guide/step catalogs that hold prose.
const PROSE_KEYS = ["title", "text", "body", "label", "desc", "description", "hint", "note", "why", "tip", "question", "answer", "placeholder", "cta", "heading", "subtitle", "summary", "detail", "caption", "message"];

const files = [];
(function walk(dir) {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "i18n") continue;
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p);
    else if (/\.(jsx?|tsx?)$/.test(name)) files.push(p);
  }
})(SRC);
files.sort();

const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " ", mdash: "—", ndash: "–", hellip: "…", rarr: "→", larr: "←", times: "×", copy: "©", deg: "°", check: "✓" };
const decode = (s) => s
  .replace(/&#(\d+);/g, (_, d) => String.fromCodePoint(Number(d)))
  .replace(/&#x([0-9a-f]+);/gi, (_, h) => String.fromCodePoint(parseInt(h, 16)))
  .replace(/&([a-z]+);/gi, (m, name) => ENTITIES[name.toLowerCase()] ?? m);

/** Same gate as the runtime translator, plus source-only noise filters. */
function eligible(raw) {
  const s = raw.trim();
  if (NEVER_TRANSLATED.has(s)) return false;
  if (s.length < 2 || s.length > MAX_LEN) return false;
  if (!/[a-zA-Z]/.test(s)) return false;
  if (/^[a-z0-9_]+$/.test(s) && !/\s/.test(s)) {          // identifier / enum value
    return false;
  }
  if (/^(https?:|\/|\.\/|#|data:|mailto:)/.test(s)) return false;
  if (/^[\w.-]+\.(jsx?|tsx?|json|png|jpe?g|svg|mp3|mp4|wav|py|rs|md)$/i.test(s)) return false;
  if (/^[A-Za-z-]+\/[A-Za-z0-9.:-]+$/.test(s)) return false;  // model ids like nvidia/nemotron-…
  if (/[{}<>]/.test(s) && !/[a-z] [a-z]/i.test(s)) return false;
  // Code that slipped through the JSX text match: statement punctuation, or a bare call/return.
  if (/;\s*$|;\s*\n|=>|\breturn\s*\(|^\w+\($/.test(s)) return false;
  // A template literal or an unbalanced expression fragment. `(prefix ? `$` reached the inventory
  // from a redaction helper, and a string nobody can read is a string nobody can translate.
  if (/[`$]\{|\?\s*`|`\s*\$|^\(/.test(s)) return false;
  // Optional chaining, a bare comparison, a lone operator: JSX text matching splits on braces, so a
  // fragment of an expression can look like a sentence. Any of these means it is not one.
  if (/\?\.|&&|\|\||[!=<>]=|\breturn\b|=>/.test(s)) return false;
  // A masked example ("sk-...", "AIza...", "UC...") is a placeholder the user reads but nobody
  // translates, and __TOKEN__ is a substitution marker in a workflow template.
  if (/^__[A-Z_]+__$/.test(s) || /^[A-Za-z-]{2,6}\.\.\.$/.test(s)) return false;
  // A fragment left over from a sentence split across JSX elements: it starts mid-clause and has no
  // verb of its own, so a translator has nothing to work with.
  if (/^(?:s |· |you (?:are|have)|use it$)/.test(s)) return false;
  // A ternary or a call left half-open by the JSX text split: `) : ready ? (`, `status[k] ? (`.
  if (/[?:]\s*\($|^\)|\[\w+\]/.test(s)) return false;
  // An unbalanced bracket at either end is a fragment, not a sentence.
  if (/\($/.test(s) || /^\S+\)/.test(s)) return false;
  // A hostname, an env var, a package name, a branch name: identifiers a user reads but nobody
  // translates.
  if (/^[a-z0-9-]+\.(?:com|org|io|net|ai|co)$/i.test(s)) return false;
  if (/^[A-Z][A-Z0-9_]{4,}$/.test(s)) return false;
  if (/^[a-z0-9]+(?:-[a-z0-9]+)+$/.test(s)) return false;
  // Starts or ends mid-clause: a leading connective with no subject, or a dangling unit.
  if (/^(?:in the |and the |of the |or the )/.test(s) && s.split(/\s+/).length <= 4) return false;
  if (/^-\w/.test(s)) return false;
  // A path, a sample credential, a placeholder address, a model file: things a user reads literally
  // and nobody translates. Every one of these reached a translator's list before this filter.
  if (/^(?:scripts|src|\.github)\//.test(s) || /\/$/.test(s)) return false;
  if (/\.(?:safetensors|ya?ml|ttf|otf)$/i.test(s)) return false;
  if (/^[\w.+-]+@[\w.-]+\.\w+$/.test(s)) return false;              // yourname@gmail.com
  if (/^[a-z][\w-]*(?:\.[a-z][\w-]*){1,}(?:\/\S*)?$/i.test(s) && !/\s/.test(s)) return false; // hosts
  if (/^[A-Z]+(?:-[A-Z]+)+$/.test(s)) return false;                  // GIFT-CODE-HERE
  if (/^\w+:\s*\w+$/.test(s) && s.split(/\s+/).length <= 2) return false; // positional: item
  if (/^(?:studio-api|sk-|bmstudio-|AIza|hf_|ghp_)/.test(s)) return false;
  if (/\)\s*;?\s*$/.test(s) && !/[a-z]\s[a-z]/i.test(s)) return false;
  if (/^(?:[\w-]+\s)*(?:flex|grid|px|py|mt|mb|ml|mr|gap|text|bg|border|rounded|w|h|min|max)-[\w./[\]-]+/.test(s)) return false; // tailwind
  // Tailwind that starts with a bare utility (`flex justify-center …`) or a variant selector
  // (`group-[.toast]:text-…`). The rule above only caught the hyphenated forms.
  if (/^(?:flex|grid|block|inline|hidden|absolute|relative|fixed|sticky)(\s|$)/.test(s)) return false;
  if (/\w+-\[[^\]]+\]:/.test(s) || /:[a-z-]+\]/.test(s)) return false;
  // A statement, not a sentence: a call with arguments, a loop header, an escaped newline.
  if (/\w+\([^)]*\)\s*;|\bfor\s*\(|\bawait\b|\\n/.test(s)) return false;
  if (/(^|\s)(?:className|font-mono|text-xs|text-sm|uppercase tracking)/.test(s)) return false;
  return true;
}

/// Names that are the same in every language: brands, engines, file formats, protocols. Listing
/// them keeps them out of the inventory entirely, so a coverage figure is not diluted by strings
/// nobody would ever translate — and so no catalogue can "translate" a product name by accident.
const NEVER_TRANSLATED = new Set([
  "Suno", "Midjourney", "ComfyUI", "FLUX", "FLUX.1", "ACE-Step", "HeartMuLa", "Riffusion",
  "ElevenLabs", "ElevenLabs Music", "Leonardo", "Ideogram", "Recraft", "fal.ai", "Gemini",
  "OpenRouter", "Anthropic", "OpenAI", "Kaggle", "Colab", "Printify", "Google", "Gmail", "YouTube",
  "Instagram", "TikTok", "Facebook", "GitHub", "GitLab", "Codeberg", "Hugging Face", "Modal",
  "Hotjar", "Obsidian", "Qwen-Image", "Juggernaut XL", "Krea 2 RAW", "Krea 2 Turbo", "SD 3.5",
  "API", "CFG", "DPI", "SVG", "PNG", "JPEG", "EPUB", "MP3", "MP4", "URL", "JSON", "LoRA", "GPU",
  "IP-Adapter", "ControlNet", "SAF", "OAuth", "LFS", "SIL OFL", "Apache 2.0",
  "Inter", "Playfair Display", "Cormorant Garamond", "Bebas Neue", "Comic Neue", "Lora", "Montserrat",

  // Music genres. Kept in English the world over — a German channel tags "Gospel", not
  // "Evangelium", and translating a genre would break the style string sent to the engine anyway.
  "Pop", "Gospel", "Klezmer", "Reggaeton", "Future Bass", "Lo-Fi Hip-Hop", "Worship Pop", "Anime",
  "Global Choir Series",

  // Theme names. Proper nouns for a palette, like a paint colour: "Slate" is the name of the theme,
  // not a description of it.
  "Aurora", "Cosmos", "Ember", "Mint", "Rosewood", "Sandstone", "Slate", "Vellum", "Obsidian",

  // Keyboard shortcuts, field labels that are literal keys, and shapes quoted as ratios.
  "Ctrl+S", "Ctrl+Enter", "YouTube 16:9", "Shorts 9:16", "TikTok 9:16", "YT Shorts 9:16",
  "Suno Style CSV", "Suno AI Style CSV", "Printify Personal Access Token", "YouTube Studio",
  "Google Gemini", "Lightkid AI", "Midjourney imagine", "FLUX.1 schnell", "Open API",
  "Google OAuth (YouTube Data API v3)", "Client ID (xxxxx.apps.googleusercontent.com)",
  "@channelname", "Vol.", "Worker URL",

  // Prompts and commands shown verbatim. Translating a prompt changes what the engine receives;
  // translating a command makes it stop working.
  "cinematic lighting, --ar 16:9", "sacred, gentle dawn, restful, soft chamber reverb",
  "modal deploy scripts/remote/modal_app.py",
  "python3 scripts/remote/cli_worker.py job.json",
  "python3 scripts/remote/render_worker.py job.json",
  "src-tauri/comfy_workflows/animate_sdxl.json.example",
  "…apps.googleusercontent.com",
  "…apps.googleusercontent.com — type Android, package com.studio.lightkid",

  // Abbreviated units and field prefixes rendered next to a value. "img ·" and "med ·" are column
  // separators, not words; "id:", "kernel:", "format:" label a literal that follows.
  "img ·", "med ·", "Media ·", "id:", "kernel:", "format:", "API:", "Chaos", "Chaos:",
]);

const strings = new Map();  // string -> Set(source file)
const add = (s, file) => {
  // Collapse whitespace exactly as JSX does before the browser sees it: a text block written across
  // several indented source lines reaches the DOM as one line with single spaces. Without this the
  // inventory holds the *source* spelling — newlines and all — which can never match a text node at
  // runtime, so those strings are translated by somebody and then never used by anything.
  const t = decode(s).replace(/\s*\n\s*/g, " ").replace(/[ \t]{2,}/g, " ").trim();
  if (!eligible(t)) return;
  if (!strings.has(t)) strings.set(t, new Set());
  strings.get(t).add(relative(ROOT, file));
};

for (const file of files) {
  const code = readFileSync(file, "utf8");
  // Strip comments so commentary prose never lands in the inventory.
  const stripped = code
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "))
    .replace(/(^|[^:])\/\/[^\n]*/g, (m, p) => p + m.slice(p.length).replace(/[^\n]/g, " "));

  // 1. JSX text between tags — split on {expressions}, which break text nodes apart.
  for (const m of stripped.matchAll(/>([^<>{}]*(?:\{[^{}]*\}[^<>{}]*)*)</g)) {
    for (const piece of m[1].split(/\{[^{}]*\}/)) {
      if (piece.trim()) add(piece, file);
    }
  }
  // 2. Chrome attributes with literal values.
  for (const attr of TEXT_ATTRS) {
    const re = new RegExp(`\\b${attr}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|\\{\\s*["'\`]([^"'\`]*)["'\`]\\s*\\})`, "g");
    for (const m of stripped.matchAll(re)) add(m[1] ?? m[2] ?? m[3] ?? "", file);
  }
  // 3. Prose-valued object properties (tour steps, guide steps, page actions, toasts).
  for (const key of PROSE_KEYS) {
    const re = new RegExp(`\\b${key}\\s*:\\s*(?:"((?:[^"\\\\]|\\\\.)*)"|'((?:[^'\\\\]|\\\\.)*)')`, "g");
    for (const m of stripped.matchAll(re)) {
      const v = (m[1] ?? m[2] ?? "").replace(/\\"/g, '"').replace(/\\'/g, "'").replace(/\\n/g, " ");
      add(v, file);
    }
  }
  // 4. toast.success("…") / toast.error("…") — user-facing feedback.
  for (const m of stripped.matchAll(/toast\.(?:success|error|info|warning|message)\(\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)')/g)) {
    add((m[1] ?? m[2] ?? "").replace(/\\"/g, '"'), file);
  }
}

const sorted = [...strings.keys()].sort((a, b) => a.localeCompare(b, "en"));
const payload = {
  _comment: "Generated by scripts/extract-ui-strings.mjs — the full inventory of interface strings, including panels that only render on demand (tours, guides, dialogs). Bundled catalogs in this folder are keyed by these exact strings.",
  generated_from: `${files.length} source files`,
  count: sorted.length,
  strings: sorted,
};

if (process.argv.includes("--check")) {
  let existing = null;
  try { existing = JSON.parse(readFileSync(OUT, "utf8")); } catch { /* missing */ }
  const same = existing && JSON.stringify(existing.strings) === JSON.stringify(sorted);
  if (!same) {
    const before = new Set(existing?.strings || []);
    const added = sorted.filter((s) => !before.has(s));
    const removed = [...before].filter((s) => !strings.has(s));
    console.error(`ui-strings.json is stale: +${added.length} / -${removed.length}`);
    for (const s of added.slice(0, 20)) console.error(`  + ${s}`);
    process.exit(1);
  }
  console.log(`ui-strings.json up to date (${sorted.length} strings)`);
  process.exit(0);
}

writeFileSync(OUT, JSON.stringify(payload, null, 2) + "\n");
console.log(`${sorted.length} strings from ${files.length} files → ${relative(ROOT, OUT)}`);
