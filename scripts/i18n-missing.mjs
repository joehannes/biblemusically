#!/usr/bin/env node
// What is still untranslated, and whether what *is* translated is trustworthy.
//
//   node scripts/i18n-missing.mjs              # a count per language
//   node scripts/i18n-missing.mjs de           # the missing strings for one language, as JSON
//   node scripts/i18n-missing.mjs --audit      # what is present but wrong
//   node scripts/i18n-missing.mjs --gate       # fail if the untranslated backlog grew
//   node scripts/i18n-missing.mjs --gate --update   # accept the current backlog as the new ceiling
//
// The gate exists because of how coverage was lost. The catalogues reached 100% in v0.106.0 and were
// at 86.6% by v0.109.0, and nobody did anything wrong: `i18n:check` guards the *inventory*, so it
// stays green as long as the inventory is refreshed, and no check at all guarded the *translations*.
// Every release added English strings, each one individually reasonable, and coverage fell a little
// each time with every check passing. A ratchet is the smallest thing that stops that: the backlog
// may shrink freely, and growing it takes a deliberate, reviewable edit to the committed ceiling.
//
// The audit half matters more than the count. A catalogue can be "100% covered" and still be broken
// in three ways that a coverage number hides completely:
//
//   * **An untranslated copy.** The English string echoed back is worse than a missing one, because
//     the runtime translator skips anything the catalogue claims to cover — so an echo is a string
//     that will never be translated by anything, ever.
//   * **A dropped placeholder.** These strings are interpolated at runtime. A translation that loses
//     `{0}` renders a sentence with a hole where the number was.
//   * **Runaway length.** A German string is legitimately longer than its English source, but three
//     times longer is a model that started explaining rather than translating, and it will overflow
//     the control it sits in.

import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const I18N = join(dirname(fileURLToPath(import.meta.url)), "..", "src/src/i18n");
const inventory = JSON.parse(readFileSync(join(I18N, "ui-strings.json"), "utf8"));
const KEYS = Array.isArray(inventory.strings) ? inventory.strings : Object.keys(inventory.strings);

const argv = process.argv.slice(2);
const audit = argv.includes("--audit");
const gate = argv.includes("--gate");
const update = argv.includes("--update");
const only = argv.find((a) => !a.startsWith("--"));
const FLOOR = join(I18N, "coverage-floor.json");

// Everything in here is a language catalogue except the two bookkeeping files.
const NOT_A_LANGUAGE = new Set(["ui-strings.json", "coverage-floor.json"]);
const codes = readdirSync(I18N)
  .filter((f) => f.endsWith(".json") && !NOT_A_LANGUAGE.has(f))
  .map((f) => f.replace(/\.json$/, ""))
  .filter((c) => !only || c === only);

/** `{0}`, `{name}`, `%s` — anything the runtime substitutes into. */
const placeholders = (s) => (String(s).match(/\{[^}]*\}|%[sd]/g) || []).sort().join(",");

let worstFirst = [];
for (const code of codes) {
  const path = join(I18N, `${code}.json`);
  const cat = JSON.parse(readFileSync(path, "utf8"));
  const strings = cat.strings || {};

  const missing = KEYS.filter((k) => !String(strings[k] ?? "").trim());
  // An echo is normally a bug: the runtime skips anything the catalogue claims to cover, so an
  // English string echoed back will never be translated by anything. But some terms genuinely are
  // the English ones — German writes "Graphic Novels" — and a catalogue can say so explicitly in
  // `same_as_english`. Deliberate identity is fine; silent identity is not.
  const deliberate = new Set(cat.same_as_english || []);
  const echoed = KEYS.filter(
    (k) => strings[k] && strings[k] === k && k.split(/\s+/).length > 1 && !deliberate.has(k));
  const lostPlaceholder = KEYS.filter(
    (k) => strings[k] && placeholders(k) && placeholders(strings[k]) !== placeholders(k));
  const runaway = KEYS.filter(
    (k) => strings[k] && k.length > 12 && strings[k].length > k.length * 3);

  worstFirst.push({ code, missing, echoed, lostPlaceholder, runaway, total: KEYS.length });
}
worstFirst.sort((a, b) => b.missing.length - a.missing.length);

if (gate) {
  const worst = Math.max(...worstFirst.map((r) => r.missing.length));
  const ceiling = JSON.parse(readFileSync(FLOOR, "utf8"));
  if (update) {
    writeFileSync(FLOOR, `${JSON.stringify({ ...ceiling, max_missing: worst, total: KEYS.length }, null, 2)}\n`);
    console.log(`ceiling set to ${worst} untranslated of ${KEYS.length}`);
    process.exit(0);
  }
  if (worst > ceiling.max_missing) {
    const over = worstFirst.filter((r) => r.missing.length > ceiling.max_missing);
    console.error(
      `${worst} strings are untranslated; the committed ceiling is ${ceiling.max_missing}.\n` +
      `Worst: ${over.map((r) => `${r.code} (${r.missing.length})`).join(", ")}\n\n` +
      "New interface copy needs a translation in every bundled language, or the language nobody is\n" +
      "watching quietly turns back into English. Translate it — `node scripts/i18n-missing.mjs <code>`\n" +
      "lists what each catalogue is missing — then run this with --update to lower the ceiling.\n" +
      "Raising the ceiling is a deliberate act; it should show up in review as one.");
    process.exit(1);
  }
  console.log(`${worst} untranslated of ${KEYS.length}, ceiling ${ceiling.max_missing} — ok`);
  process.exit(0);
}

if (only && !audit) {
  // One language's missing strings, ready to be translated and pasted back.
  const one = worstFirst[0];
  process.stdout.write(JSON.stringify(one.missing, null, 2));
} else {
  const rows = worstFirst.map((r) => {
    const covered = r.total - r.missing.length;
    const pct = ((covered / r.total) * 100).toFixed(1);
    let line = `${r.code.padEnd(4)} ${String(covered).padStart(5)}/${r.total}  ${pct.padStart(5)}%`;
    if (audit) {
      line += `   echoed ${String(r.echoed.length).padStart(4)}` +
              `   placeholders lost ${String(r.lostPlaceholder.length).padStart(3)}` +
              `   runaway ${String(r.runaway.length).padStart(4)}`;
    }
    return line;
  });
  console.log(rows.join("\n"));
  if (audit) {
    const worst = worstFirst.find((r) => r.lostPlaceholder.length);
    if (worst) {
      console.log(`\nexample of a lost placeholder (${worst.code}):`);
      const k = worst.lostPlaceholder[0];
      const cat = JSON.parse(readFileSync(join(I18N, `${worst.code}.json`), "utf8"));
      console.log(`  en: ${k}\n  ${worst.code}: ${cat.strings[k]}`);
    }
  }
}

/** Merge hand-written translations into a catalogue, refusing anything that fails the audit. */
export function merge(code, additions) {
  const path = join(I18N, `${code}.json`);
  const cat = JSON.parse(readFileSync(path, "utf8"));
  cat.strings = cat.strings || {};
  let added = 0;
  for (const [en, translated] of Object.entries(additions)) {
    if (!translated?.trim()) continue;
    if (placeholders(en) !== placeholders(translated)) {
      throw new Error(`${code}: "${en}" lost a placeholder in "${translated}"`);
    }
    if (!cat.strings[en]) added += 1;
    cat.strings[en] = translated;
  }
  const covered = KEYS.filter((k) => String(cat.strings[k] ?? "").trim()).length;
  cat.coverage = `${covered}/${KEYS.length}`;
  writeFileSync(path, `${JSON.stringify(cat, null, 2)}\n`);
  return { added, coverage: cat.coverage };
}
