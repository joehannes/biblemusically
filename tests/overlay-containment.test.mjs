// Full-screen overlays must be anchored to the viewport, not to whatever happens to contain them.
//
// The bug this exists to prevent: "Add another account" in Settings blurred the screen and showed
// nothing. `position: fixed` is only relative to the viewport while no ancestor establishes a
// containing block, and a transform — even the identity matrix — establishes one. Every page wrapper
// carries `.fade-in`, whose keyframes touch `transform`; with `animation-fill-mode: both` the
// finished animation kept applying `transform: none`, which computes to `matrix(1,0,0,1,0,0)` rather
// than to `none`. So the modal's `fixed inset-0` box became the 8,000px-tall Settings page, and
// `items-center` centred the card ~3,000px below the fold. All that was left on screen was the
// backdrop — which covers the page, and so looks exactly like a modal that failed to open.
//
// Two things hold it shut, and both are checked here, because either alone leaves a gap: the fill
// mode (the steady state) and portalling (the 0.4s while the animation is genuinely running, plus
// any transformed ancestor someone adds later).
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

const root = new URL("../src/src/", import.meta.url).pathname;
const read = (rel) => readFileSync(path.join(root, rel), "utf8");

test(".fade-in does not survive its own animation as a containing block", () => {
  const css = read("index.css");
  const rule = css.match(/\.fade-in\s*\{[^}]*\}/)?.[0];
  assert.ok(rule, ".fade-in must still exist — pages rely on it");

  const shorthand = rule.match(/animation:\s*([^;]+);/)?.[1] ?? "";
  assert.ok(
    /\bbackwards\b/.test(shorthand),
    "`.fade-in` must use a backwards fill. `both`/`forwards` keeps the last keyframe applied after " +
      "the animation ends, and a kept `transform` makes every page a containing block for the " +
      `position:fixed modals rendered inside it. Got: animation: ${shorthand.trim()}`,
  );
  assert.ok(
    !/\b(forwards|both)\b/.test(shorthand),
    `\`.fade-in\` must not hold a forwards fill. Got: animation: ${shorthand.trim()}`,
  );
});

// Everything under components/ui/ is Radix, which portals on its own.
const HAND_ROLLED = readdirSync(path.join(root, "components"))
  .filter((f) => f.endsWith(".jsx"))
  .filter((f) => /className="fixed inset-0/.test(read(path.join("components", f))));

test("every hand-rolled full-screen overlay is portalled to <body>", () => {
  assert.ok(HAND_ROLLED.length > 0, "the scan found no overlays at all — it has stopped working");

  // Shell mounts outside page content and is the app frame itself; its own chrome is not the
  // hazard. What matters is anything a *page* renders, since pages are the transformed ancestors.
  const mountedInsidePages = HAND_ROLLED.filter((f) => f !== "Shell.jsx" && f !== "Tours.jsx" && f !== "WelcomeTour.jsx");
  assert.ok(mountedInsidePages.length > 0, "expected GuideStepDialog and ImageLightbox at least");

  for (const file of mountedInsidePages) {
    const src = read(path.join("components", file));
    assert.match(
      src,
      /createPortal\(/,
      `${file} renders a fixed full-screen overlay from inside a page, so it must createPortal into ` +
        "document.body — otherwise a transformed page wrapper silently resizes it to the page.",
    );
    assert.match(
      src,
      /document\.body/,
      `${file} calls createPortal but not into document.body`,
    );
  }
});

test("the guided-step dialog can tell its caller it finished", () => {
  // The other half of the same report: the dialog opened (once visible), an account was connected,
  // and the list behind it still read "No Kaggle account connected yet" because nothing told the
  // page to reload. The hook has to forward a completion callback for that to be fixable at all.
  const src = read("components/GuideStepDialog.jsx");
  const hook = src.slice(src.indexOf("export function useRequireGuideStep"));
  assert.match(hook, /useRequireGuideStep\(stepId,\s*onDone\)/,
    "useRequireGuideStep must accept an onDone callback");
  assert.match(hook, /<GuideStepDialog[\s\S]*?onDone=\{onDone\}/,
    "useRequireGuideStep must pass onDone through to the dialog");

  const settings = read("pages/Settings.jsx");
  assert.match(settings, /useRequireGuideStep\("kaggle",\s*loadKaggleAccounts\)/,
    "Settings must refresh its Kaggle account list when the guided step finishes");
});
