#!/usr/bin/env node
// Build the bundled GUI translation catalogs in src/src/i18n/<code>.json.
//
// The runtime translator only ever calls the AI for strings no catalog covers, so the point of this
// script is to make that set empty for the shipped languages: it translates the whole extracted
// inventory (src/src/i18n/ui-strings.json) once, at build time, and commits the result. Switching
// to German then costs zero AI calls and paints instantly — including panels the user has not
// opened yet, which is what used to leave tours and dialogs in English.
//
// Usage:
//   node scripts/extract-ui-strings.mjs                 # refresh the inventory first
//   node scripts/build-i18n-catalogs.mjs de es pt ru     # fill/extend the catalogs
//   node scripts/build-i18n-catalogs.mjs de --provider openrouter --model nvidia/nemotron-3-ultra-550b-a55b:free
//
// Keys come from the app's own settings store (~/.config/studio-lightkid/data/settings.json), so
// there is nothing extra to configure. Work is written after every batch and existing translations
// are kept, which makes the script resumable: if a free tier cuts you off mid-run, run it again
// later and it picks up exactly where it stopped.

import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { homedir } from "node:os";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const I18N = join(ROOT, "src/src/i18n");
const SETTINGS = join(homedir(), ".config/studio-lightkid/data/settings.json");

const LANGS = {
  de: "German", es: "Spanish", pt: "Portuguese", ru: "Russian", fr: "French", it: "Italian",
  nl: "Dutch", pl: "Polish", he: "Hebrew", ar: "Arabic", hi: "Hindi", id: "Indonesian",
  ja: "Japanese", ko: "Korean", zh: "Chinese (Simplified)",
};

const argv = process.argv.slice(2);
const flag = (name, dflt) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : dflt;
};
const codes = argv.filter((a) => !a.startsWith("--") && LANGS[a]);
const batchSize = Number(flag("batch", 80));
const limit = Number(flag("limit", 0));           // 0 = no cap
if (!codes.length) {
  console.error(`usage: build-i18n-catalogs.mjs <lang…>  (known: ${Object.keys(LANGS).join(" ")})`);
  process.exit(2);
}

const settings = (() => {
  try {
    const doc = JSON.parse(readFileSync(SETTINGS, "utf8"));
    return (doc.documents || []).find((d) => d._id === "singleton") || {};
  } catch { return {}; }
})();

const provider = flag("provider", settings.ai_provider === "gemini" ? "gemini" : "openrouter");
// A free tier answers "high demand" more often than it answers. Rotating models turns that into a
// pause of seconds instead of the end of the run — the same idea as the app's runtime fallback.
const MODEL_POOL = {
  gemini: ["gemini-3.6-flash", "gemini-3.5-flash", "gemini-3.5-flash-lite", "gemini-3.1-flash-lite", "gemini-2.5-flash", "gemini-2.5-flash-lite"],
  openrouter: ["nvidia/nemotron-3-ultra-550b-a55b:free", "nvidia/nemotron-3-super-120b-a12b:free", "qwen/qwen3-next-80b-a3b-instruct:free"],
};
const models = flag("model", "").split(",").filter(Boolean);
const pool = models.length ? models : MODEL_POOL[provider];
const key = provider === "gemini" ? settings.gemini_api_key : settings.openrouter_api_key;
if (!key) {
  console.error(`No ${provider} API key in ${SETTINGS} — set one in the app's Settings page first.`);
  process.exit(2);
}

const inventory = JSON.parse(readFileSync(join(I18N, "ui-strings.json"), "utf8")).strings;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function systemPrompt(languageName) {
  // Mirrors the wording of the runtime prompt in src-tauri/commands/ai.rs so a string translated
  // here and one translated live read the same.
  return `You localise software interface text into ${languageName}.
You receive a JSON array of UI strings. Return ONLY a JSON object mapping EACH original string
EXACTLY as given to its ${languageName} translation — no extra keys, no commentary, no fences.
Rules: keep it short enough for buttons and labels; preserve capitalisation style, surrounding
punctuation and any leading/trailing spaces; keep placeholder-ish examples ("e.g. …") in that shape;
do NOT translate proper nouns, brand or product names (Kaggle, YouTube, Suno, ComfyUI, HeartMuLa,
ACE-Step, FLUX, Midjourney, OpenRouter, Gemini, Google, Hugging Face, GPU, API, JSON, OAuth, URL,
LoRA, ControlNet), Bible book names in citations, file names, code, numbers or units.
Write the way a native ${languageName} interface is written: prefer the impersonal/infinitive label
style used on buttons and headings ("Save", "Choose a folder") over addressing the user with a
pronoun, and stay consistent across the whole batch.
This is a Bible music-video production studio: "song", "track", "verse", "chorus", "bridge",
"channel", "render", "upload", "prompt" are studio/production terms — translate them the way a
${languageName} creative-software UI would, not literally. If a string should stay as-is, map it to
itself.`;
}

async function callAI(languageName, batch, model) {
  const user = JSON.stringify(batch);
  if (provider === "gemini") {
    const url = `https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent`;
    const r = await fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json", "x-goog-api-key": key },
      body: JSON.stringify({
        system_instruction: { parts: [{ text: systemPrompt(languageName) }] },
        contents: [{ role: "user", parts: [{ text: user }] }],
        generationConfig: { temperature: 0.1, responseMimeType: "application/json" },
      }),
    });
    if (!r.ok) throw Object.assign(new Error(`gemini HTTP ${r.status}: ${(await r.text()).slice(0, 200)}`), { status: r.status });
    const j = await r.json();
    return (j.candidates?.[0]?.content?.parts || []).map((p) => p.text || "").join("");
  }
  const r = await fetch("https://openrouter.ai/api/v1/chat/completions", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      authorization: `Bearer ${key}`,
      "HTTP-Referer": "https://lightkid.studio",
      "X-Title": "Lightkid AI Studio",
    },
    body: JSON.stringify({
      model,
      temperature: 0.1,
      response_format: { type: "json_object" },
      messages: [
        { role: "system", content: systemPrompt(languageName) },
        { role: "user", content: user },
      ],
    }),
  });
  if (!r.ok) throw Object.assign(new Error(`openrouter HTTP ${r.status}: ${(await r.text()).slice(0, 200)}`), { status: r.status });
  const j = await r.json();
  return j.choices?.[0]?.message?.content || "";
}

const parseJson = (text) => {
  try { return JSON.parse(text); } catch { /* models sometimes wrap it */ }
  const m = text.match(/\{[\s\S]*\}/);
  if (!m) return null;
  try { return JSON.parse(m[0]); } catch { return null; }
};

/**
 * One batch, walking the model pool and then backing off. Overloaded free tiers are the normal
 * case, not an exception, so this is patient by design: a full sweep of the pool counts as one
 * attempt, and only when every model is busy does it wait.
 */
async function translateBatch(languageName, batch) {
  let delay = 15000;
  for (let round = 1; round <= 6; round++) {
    for (const model of pool) {
      try {
        const parsed = parseJson(await callAI(languageName, batch, model));
        if (!parsed) throw new Error("unparseable JSON response");
        const out = {};
        for (const s of batch) {
          const v = parsed[s];
          // Keep only real translations: an echo of the English means the model had nothing to add,
          // and storing it would make the runtime treat the string as done in every language.
          if (typeof v === "string" && v.trim() && v.trim() !== s) out[s] = v.trim();
        }
        if (Object.keys(out).length) return { out, model };
        throw new Error("response contained no usable translations");
      } catch (err) {
        const fatal = [400, 401, 403].includes(err.status);
        if (fatal) throw err;
        process.stderr.write(`    ${model}: ${err.message.slice(0, 110)}\n`);
      }
    }
    if (round === 6) break;
    process.stderr.write(`    all ${pool.length} models busy — waiting ${Math.round(delay / 1000)}s (round ${round}/5)\n`);
    await sleep(delay);
    delay = Math.min(delay * 2, 300000);
  }
  throw new Error("every model in the pool refused this batch");
}

for (const code of codes) {
  const languageName = LANGS[code];
  const file = join(I18N, `${code}.json`);
  const existing = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : {};
  const strings = existing.strings || {};
  let pending = inventory.filter((s) => !strings[s]);
  if (limit) pending = pending.slice(0, limit);

  console.log(`\n${code} (${languageName}) — ${Object.keys(strings).length}/${inventory.length} known, ${pending.length} to do via ${provider} [${pool.join(", ")}]`);
  let done = 0;
  for (let i = 0; i < pending.length; i += batchSize) {
    const batch = pending.slice(i, i + batchSize);
    try {
      const { out } = await translateBatch(languageName, batch);
      Object.assign(strings, out);
    } catch (err) {
      console.error(`  stopped: ${err.message}`);
      console.error(`  progress is saved — re-run the same command later to continue.`);
      break;
    }
    done += batch.length;
    writeFileSync(file, JSON.stringify({
      _comment: "Bundled GUI translation catalog. Generated by scripts/build-i18n-catalogs.mjs from ui-strings.json; hand edits are kept (the script only fills missing keys).",
      language: code,
      language_name: languageName,
      coverage: `${Object.keys(strings).length}/${inventory.length}`,
      strings,
    }, null, 2) + "\n");
    process.stdout.write(`  ${Math.min(done, pending.length)}/${pending.length} → ${Object.keys(strings).length} entries\r`);
    await sleep(1500);  // pace for per-minute free-tier limits
  }
  console.log(`\n  ${code}.json: ${Object.keys(strings).length}/${inventory.length} strings`);
}
