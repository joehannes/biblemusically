#!/usr/bin/env node
// Nothing that is a secret may be tracked by git.
//
// This exists because it already happened. `server/.deploy-state.json` — the Ed25519 *private*
// signing key that mints entitlements, plus the admin token — was committed in v0.88.0 and removed
// again a few commits later. Removing it does nothing: the blob is still reachable in the history,
// and the key it holds is still the key the app verifies today. See docs/SECURITY-KEY-ROTATION.md.
//
// An ignore rule would not have caught it either, because a rule does nothing about a file that is
// already tracked. So this checks the index — what git is actually carrying — rather than what the
// ignore rules say it should be.
//
// Deliberately narrow. This is not a general secret scanner and does not pretend to be one; it
// refuses the shapes that have a specific, known way of ending up in this repository. A scanner that
// flags everything gets switched off, and then catches nothing at all.
//
// Usage: node scripts/audit-secrets.mjs
//   exit 0 = clean, 1 = something is tracked that must not be.

import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Files whose *name* means "this holds a credential", wherever they sit. */
const FORBIDDEN_NAMES = [
  { re: /^\.deploy-state\.json$/, why: "the Ed25519 private signing key and the admin token" },
  { re: /^\.secrets$/, why: "the Cloudflare API token" },
  { re: /^\.env(\..+)?$/, why: "environment secrets" },
  { re: /\.(key|pem|p12|pfx|keystore|jks)$/i, why: "a private key or a keystore" },
  { re: /^kaggle\.json$/, why: "a Kaggle API key" },
  { re: /^service-account.*\.json$/, why: "a Google service-account key" },
];

/** Byte sequences that are a private key however the file is named. */
const FORBIDDEN_CONTENT = [
  { needle: "MC4CAQAwBQYDK2Vw", why: "a PKCS#8 Ed25519 private key" },
  { needle: "-----BEGIN PRIVATE KEY-----", why: "a PEM private key" },
  { needle: "-----BEGIN RSA PRIVATE KEY-----", why: "a PEM RSA private key" },
  { needle: "-----BEGIN OPENSSH PRIVATE KEY-----", why: "an OpenSSH private key" },
];

/** Text files only: a 200 MB binary is not where this class of leak lives, and reading it is slow. */
const MAX_BYTES = 2 * 1024 * 1024;
const BINARY = /\.(png|jpe?g|gif|webp|ico|mp3|mp4|wav|webm|pdf|zip|gz|tar|so|dll|dylib|woff2?|ttf|otf|deb|apk|aab|jar|node|wasm)$/i;

let tracked;
try {
  tracked = execFileSync("git", ["ls-files", "-z"], { cwd: ROOT, maxBuffer: 64 * 1024 * 1024 })
    .toString("utf8").split("\0").filter(Boolean);
} catch {
  console.log("not a git checkout — nothing to check");
  process.exit(0);
}

const found = [];

for (const rel of tracked) {
  const name = basename(rel);
  for (const rule of FORBIDDEN_NAMES) {
    if (rule.re.test(name)) found.push({ rel, why: rule.why, how: "its name" });
  }
  if (BINARY.test(rel)) continue;
  let size;
  try { size = statSync(join(ROOT, rel)).size; } catch { continue; }   // deleted but still in the index
  if (size > MAX_BYTES) continue;
  let text;
  try { text = readFileSync(join(ROOT, rel), "utf8"); } catch { continue; }
  for (const rule of FORBIDDEN_CONTENT) {
    // The needles themselves appear in this file and in the ignore rules that exist to catch them;
    // a check that trips on its own description is a check somebody deletes.
    if (rel === "scripts/audit-secrets.mjs") continue;
    if (text.includes(rule.needle)) found.push({ rel, why: rule.why, how: "its contents" });
  }
}

if (!found.length) {
  console.log(`${tracked.length} tracked files — no credential is among them.`);
  process.exit(0);
}

console.log(`✗ ${found.length} tracked file(s) hold a credential:\n`);
for (const f of found) console.log(`    ${f.rel}\n        ${f.why}, detected by ${f.how}`);
console.log(`
  Untracking it is not enough — once pushed, the value is public and stays reachable in the history.
  Rotate the credential first, then \`git rm --cached\` the file and add it to .gitignore.
  docs/SECURITY-KEY-ROTATION.md has the procedure for the signing key.`);
process.exit(1);
