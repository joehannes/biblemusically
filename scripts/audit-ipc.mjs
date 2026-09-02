#!/usr/bin/env node
// The IPC surface, checked statically: nothing invented, nothing built and left unreachable.
//
// Three classes of defect live in the seam between `src-tauri` and `src/src/lib/api.js`, and none of
// them is caught by `cargo check`, by `tsc`, or by any test — because the seam is a pair of strings:
//
//   BROKEN     api.js invokes a command name that no `#[tauri::command]` defines. The button throws
//              "command not found" the first time somebody presses it, which may be months later.
//   UNROUTED   a command is defined but missing from `generate_handler!` in lib.rs. Same symptom,
//              different cause; ARCHITECTURE.md §8 calls manual registration out as a known hazard
//              and it has already happened once (`probe_node`).
//   ORPHAN     a command is defined and registered and wrapped in api.js — and no page or component
//              ever calls the wrapper. The feature exists and cannot be reached. Thirty of these
//              were found by hand in 2026-08 (WISHLIST.md Part 1); this is so it never needs doing
//              by hand again.
//
// BROKEN and UNROUTED are always failures. ORPHANs are not always defects — some wrappers are
// alternates to a path the app deliberately prefers, and some belong to an engine that is off on
// purpose — so they are checked against a committed allowlist that has to say *why*. Adding to that
// list is a deliberate act, and shows up in review as one.
//
// Usage: node scripts/audit-ipc.mjs [--json]
//   exit 0 = clean, 1 = something to answer for.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const API = join(ROOT, "src/src/lib/api.js");
const LIB = join(ROOT, "src-tauri/src/lib.rs");
const ALLOWLIST = join(ROOT, "scripts/ipc-orphans.json");

const json = process.argv.includes("--json");

// ── what Rust defines ────────────────────────────────────────────────────────
// `#[tauri::command]` and the `fn` it decorates can be separated by doc comments, other attributes
// and `#[cfg(...)]` gates, so this scans forward to the next `fn` rather than assuming adjacency.
function definedCommands() {
  const out = new Set();
  const walk = (dir) => {
    for (const name of readdirSync(dir)) {
      if (name === "target" || name === "node_modules" || name === "gen") continue;
      const p = join(dir, name);
      const st = statSync(p);
      if (st.isDirectory()) walk(p);
      else if (name.endsWith(".rs")) {
        const lines = readFileSync(p, "utf8").split("\n");
        for (let i = 0; i < lines.length; i++) {
          // Both spellings: `#[tauri::command]` and the `use tauri::command;` short form.
          if (!/^\s*#\[(?:tauri::)?command[\](]/.test(lines[i])) continue;
          for (let j = i + 1; j < Math.min(i + 12, lines.length); j++) {
            const m = lines[j].match(/^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([a-z0-9_]+)/);
            if (m) { out.add(m[1]); break; }
          }
        }
      }
    }
  };
  walk(join(ROOT, "src-tauri"));
  return out;
}

// ── what lib.rs registers ────────────────────────────────────────────────────
// Only the `commands::x` / bare-identifier entries inside `generate_handler![…]`. Comments inside
// that block are prose and must not be mistaken for entries, which is why this matches a path shape
// rather than splitting on commas.
function registeredCommands() {
  const src = readFileSync(LIB, "utf8");
  const start = src.indexOf("generate_handler![");
  if (start === -1) throw new Error("no generate_handler! in lib.rs");
  let depth = 0, end = start;
  for (let i = src.indexOf("[", start); i < src.length; i++) {
    if (src[i] === "[") depth++;
    else if (src[i] === "]") { depth--; if (depth === 0) { end = i; break; } }
  }
  const block = src
    .slice(start, end)
    .split("\n")
    .map((l) => l.replace(/\/\/.*$/, ""))          // line comments are prose
    .join("\n")
    .replace(/\/\*[\s\S]*?\*\//g, "");             // and so are block comments
  const out = new Set();
  for (const m of block.matchAll(/(?:^|[,\[\s])(?:[a-z_][a-z0-9_]*::)*([a-z_][a-z0-9_]*)\s*(?=[,\]])/gm)) {
    out.add(m[1]);
  }
  out.delete("generate_handler");
  return out;
}

// ── what api.js invokes, and what the interface calls ────────────────────────
function apiWrappers() {
  const src = readFileSync(API, "utf8");
  const out = [];
  // `name: (…) => invokeCommand("command_name"…)`, including wrappers whose body spans lines.
  const re = /^\s{2}([a-zA-Z0-9_]+)\s*:\s*(?:async\s*)?\([^)]*\)\s*=>\s*(?:\{[\s\S]*?\n\s{2}\}|[\s\S]*?)(?=\n\s{2}[a-zA-Z0-9_]+\s*:|\n\};)/gm;
  for (const m of src.matchAll(re)) {
    const commands = [...m[0].matchAll(/invokeCommand\(\s*"([a-z0-9_]+)"/g)].map((c) => c[1]);
    out.push({ name: m[1], commands });
  }
  return out;
}

/** Every `api.x` reference anywhere in the interface except api.js itself. */
function calledWrappers() {
  const out = new Set();
  const walk = (dir) => {
    for (const name of readdirSync(dir)) {
      if (name === "node_modules" || name === "i18n") continue;
      const p = join(dir, name);
      const st = statSync(p);
      if (st.isDirectory()) walk(p);
      else if (/\.(jsx?|tsx?)$/.test(name) && p !== API) {
        const src = readFileSync(p, "utf8");
        for (const m of src.matchAll(/\bapi\.([a-zA-Z0-9_]+)/g)) out.add(m[1]);
      }
    }
  };
  walk(join(ROOT, "src/src"));
  return out;
}

// ── the report ───────────────────────────────────────────────────────────────
const defined = definedCommands();
const registered = registeredCommands();
const wrappers = apiWrappers();
const called = calledWrappers();
const allow = JSON.parse(readFileSync(ALLOWLIST, "utf8"));
const allowed = new Set(Object.keys(allow.orphans || {}));

const broken = [];      // api.js invokes a command nothing defines
const unrouted = [];    // defined but never registered
const orphans = [];     // wrapped and registered, never called

for (const w of wrappers) {
  for (const cmd of w.commands) {
    if (!defined.has(cmd)) broken.push({ wrapper: w.name, command: cmd });
  }
  if (w.commands.length && !called.has(w.name) && !allowed.has(w.name)) {
    orphans.push({ wrapper: w.name, commands: w.commands });
  }
}
for (const cmd of defined) {
  if (!registered.has(cmd)) unrouted.push(cmd);
}
// An allowlist entry that is no longer an orphan is stale: the feature got its button, and the
// exemption should go with it rather than quietly covering the next one.
const staleAllow = [...allowed].filter((name) => {
  const w = wrappers.find((x) => x.name === name);
  return !w || called.has(name);
});

if (json) {
  console.log(JSON.stringify({ broken, unrouted, orphans, staleAllow }, null, 2));
} else {
  const rel = (p) => relative(ROOT, p);
  console.log(`${defined.size} commands defined · ${registered.size} registered · ${wrappers.length} wrappers in ${rel(API)}`);
  if (broken.length) {
    console.log(`\n✗ ${broken.length} wrapper(s) invoke a command that does not exist:`);
    for (const b of broken) console.log(`    api.${b.wrapper} → "${b.command}"`);
  }
  if (unrouted.length) {
    console.log(`\n✗ ${unrouted.length} command(s) defined but not in generate_handler!:`);
    for (const c of unrouted) console.log(`    ${c}`);
  }
  if (orphans.length) {
    console.log(`\n✗ ${orphans.length} feature(s) reachable from nothing — built, wrapped, and never called:`);
    for (const o of orphans) console.log(`    api.${o.wrapper} → ${o.commands.join(", ")}`);
    console.log(`\n  Either give each one a button, or add it to ${rel(ALLOWLIST)} with the reason.`);
  }
  if (staleAllow.length) {
    console.log(`\n✗ ${staleAllow.length} allowlist entr(y/ies) no longer needed — remove them:`);
    for (const n of staleAllow) console.log(`    ${n}`);
  }
  if (!broken.length && !unrouted.length && !orphans.length && !staleAllow.length) {
    console.log(`\nok — every wrapper reaches a registered command, and every command reaches the interface.`);
  }
}

process.exit(broken.length || unrouted.length || orphans.length || staleAllow.length ? 1 : 0);
