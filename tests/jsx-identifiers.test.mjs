// Every component a .jsx file renders must be defined in that file.
//
// v0.143.0 shipped an AI Composer that could not open: it passed `titleIcon={PenLine}` without
// importing PenLine from lucide-react, and the view died with "Can't find variable: PenLine". Vite
// does not check that (an undefined identifier is legal JavaScript until the line runs), and no test
// rendered that section, so the first place it failed was somebody's screen.
//
// This is deliberately a text check, not a parser: the repo carries no ESLint, and the mistake it
// guards is always the same shape — a capitalised name used as a tag (`<PenLine`) or handed over as
// a component (`icon={PenLine}`) with no import, function, class or const that introduces it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = new URL("../src/src/", import.meta.url).pathname;

function jsxFiles(dir) {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return jsxFiles(p);
    return p.endsWith(".jsx") ? [p] : [];
  });
}

// Built-ins a file can use without declaring them.
const GLOBALS = new Set(["React", "Fragment", "Math", "Date", "JSON", "Object", "Array", "Number",
  "String", "Boolean", "Promise", "Error", "Set", "Map", "URL", "Intl", "Symbol", "RegExp", "Infinity",
  "NaN", "Blob", "File", "FileReader", "Image", "Audio", "Notification", "AbortController", "Event",
  "CustomEvent", "TextEncoder", "TextDecoder", "Uint8Array", "WeakMap", "WeakSet", "Proxy", "Reflect",
  "BigInt", "FormData", "Headers", "Request", "Response", "IntersectionObserver", "ResizeObserver",
  "MutationObserver", "HTMLElement", "Node", "Element", "DOMParser", "Worker", "MediaRecorder",
  "AudioContext", "OfflineAudioContext", "SpeechSynthesisUtterance", "WebSocket", "EventSource"]);

function stripComments(src) {
  return src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'`])\/\/.*$/gm, "$1");
}

function declared(src) {
  const names = new Set();
  // import X, { A, B as C } from "…"   /   import * as X from "…"
  for (const m of src.matchAll(/import\s+([^;]*?)\s+from\s+["'][^"']+["']/g)) {
    const clause = m[1];
    const def = clause.match(/^([A-Za-z_$][\w$]*)/);
    if (def) names.add(def[1]);
    const star = clause.match(/\*\s+as\s+([A-Za-z_$][\w$]*)/);
    if (star) names.add(star[1]);
    const braces = clause.match(/\{([^}]*)\}/);
    if (braces) for (const part of braces[1].split(",")) {
      const n = part.trim().split(/\s+as\s+/).pop().trim();
      if (n) names.add(n);
    }
  }
  for (const m of src.matchAll(/\b(?:function|class)\s+([A-Za-z_$][\w$]*)/g)) names.add(m[1]);
  for (const m of src.matchAll(/\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)/g)) names.add(m[1]);
  // Destructuring that introduces names: `const { A, b: C } = …`, `const [A, B] = …`, and the same
  // shapes as parameters — `({ icon: Icon }) =>`, `.map(([id, label, Icon]) =>`. Only these
  // positions count: a bare `{PenLine}` in JSX *uses* the name, and must not be read as defining it.
  const patterns = [
    /\b(?:const|let|var)\s*\{([^}]*)\}/g, /\b(?:const|let|var)\s*\[([^\]]*)\]/g,
    /\(\s*\{([^}]*)\}\s*[,)=]/g, /\(\s*\[([^\]]*)\]\s*[,)=]/g,
  ];
  for (const re of patterns) for (const m of src.matchAll(re)) {
    for (const part of m[1].split(",")) {
      const bits = part.split("=")[0].split(":").map((x) => x.trim().replace(/^\.\.\./, ""));
      const name = bits[bits.length - 1];
      if (/^[A-Za-z_$][\w$]*$/.test(name)) names.add(name);
    }
  }
  return names;
}

function used(src) {
  const names = new Set();
  // <PenLine …   (but not <Foo.Bar, where Foo is what must exist)
  for (const m of src.matchAll(/<([A-Z][\w$]*)[\s/>.]/g)) names.add(m[1]);
  // icon={PenLine}   titleIcon={PenLine}   as={Link}
  for (const m of src.matchAll(/\b\w*(?:[Ii]con|[Cc]omponent|as)=\{([A-Z][\w$]*)\}/g)) names.add(m[1]);
  return names;
}

test("every component a .jsx file renders is imported or defined in it", () => {
  const missing = [];
  for (const file of jsxFiles(ROOT)) {
    const src = stripComments(readFileSync(file, "utf8"));
    const have = declared(src);
    for (const name of used(src)) {
      if (!have.has(name) && !GLOBALS.has(name)) missing.push(`${file.slice(ROOT.length)}: ${name}`);
    }
  }
  assert.deepEqual(missing, [], `used but never imported or defined:\n  ${missing.join("\n  ")}`);
});
