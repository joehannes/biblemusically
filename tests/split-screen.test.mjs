// Opening a link beside the instructions, and admitting when that is impossible.
//
// The failure this guards against is a blank half-screen: an iframe pointed at a site that refuses
// framing renders nothing, says nothing, and leaves somebody staring at grey. Every site this app
// actually sends people to for a key or a login is on that list — Google, Kaggle, GitHub, Meta — so
// "try it and see" is not a strategy, it is the common case.
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  knownToRefuse, splitFor, planFor, watchFrame, refusalMessage, FRAME_TIMEOUT_MS,
} from "../src/src/lib/splitScreen.js";

test("the sites this app sends people to are the ones that refuse framing", () => {
  // Not a hypothetical list: these are the pages the guides link to for keys and sign-ins.
  for (const url of [
    "https://accounts.google.com/signin",
    "https://www.kaggle.com/settings",
    "https://github.com/settings/tokens",
    "https://aistudio.google.com/apikey",
    "https://www.facebook.com/x", "https://x.com/i/flow/login",
    "https://printify.com/app/account/api",
  ]) {
    assert.ok(knownToRefuse(url), `${url} should be known to refuse`);
  }
  // A subdomain of a listed host counts; an unrelated host that merely contains the name does not.
  assert.ok(knownToRefuse("https://docs.github.com/en"));
  assert.ok(!knownToRefuse("https://github.example.com/"));
  assert.ok(!knownToRefuse("https://huggingface.co/settings/tokens"));
  assert.ok(!knownToRefuse("not a url at all"));
});

test("a portrait phone splits top and bottom, a landscape one left and right", () => {
  assert.equal(splitFor({ width: 400, height: 900 }), "vertical");
  assert.equal(splitFor({ width: 900, height: 400 }), "horizontal");
  // Half of a small screen is not a usable page, so below a floor there is no split at all.
  assert.equal(splitFor({ width: 320, height: 640 }), "none");
  assert.equal(splitFor({ width: 0, height: 0 }), "none");
});

test("a link that cannot be framed goes to the browser instead of proving a point", () => {
  const viewport = { width: 400, height: 900 };
  assert.equal(planFor("https://www.kaggle.com/settings", { mobile: true, viewport }), "browser");
  assert.equal(planFor("https://huggingface.co/settings", { mobile: true, viewport }), "split");
  // A screen too small to split usefully falls back rather than showing a sliver.
  assert.equal(planFor("https://huggingface.co/settings",
    { mobile: true, viewport: { width: 320, height: 500 } }), "browser");
});

test("the desktop keeps its own browser tab, which is better than either", () => {
  assert.equal(planFor("https://huggingface.co/x", { mobile: false }), "tab");
  assert.equal(planFor("https://www.kaggle.com/x", { mobile: false }), "tab");
});

test("a preference wins, except where it cannot", () => {
  const viewport = { width: 400, height: 900 };
  assert.equal(planFor("https://huggingface.co/x", { mobile: true, preference: "browser", viewport }), "browser");
  assert.equal(planFor("https://huggingface.co/x", { mobile: false, preference: "browser" }), "browser");
  assert.equal(planFor("https://huggingface.co/x", { mobile: true, preference: "split", viewport }), "split");
  // Asking for split on a site that refuses still does not produce a blank half-screen.
  assert.equal(planFor("https://accounts.google.com/x", { mobile: true, preference: "split", viewport }), "browser");
});

test("a frame that never loads is treated as a refusal, and one that loads is not", async () => {
  // The only signal available: a refused frame never fires `load`, and its contents are unreadable
  // from outside by design.
  const listeners = {};
  const fakeFrame = {
    addEventListener: (name, fn) => { listeners[name] = fn; },
    removeEventListener: (name) => { delete listeners[name]; },
  };

  const loaded = await new Promise((resolve) => {
    watchFrame(fakeFrame, resolve, 50);
    listeners.load?.();
  });
  assert.equal(loaded, true);

  const refused = await new Promise((resolve) => {
    watchFrame({ addEventListener() {}, removeEventListener() {} }, resolve, 30);
  });
  assert.equal(refused, false);
});

test("watching settles exactly once, whatever happens", async () => {
  const listeners = {};
  const frame = {
    addEventListener: (n, fn) => { listeners[n] = fn; },
    removeEventListener: (n) => { delete listeners[n]; },
  };
  let calls = 0;
  watchFrame(frame, () => { calls += 1; }, 20);
  listeners.load?.();
  listeners.load?.();
  await new Promise((r) => setTimeout(r, 40));
  assert.equal(calls, 1, "a load followed by the timeout must not report twice");
});

test("the refusal is explained in words, not left as an empty panel", () => {
  const msg = refusalMessage("https://www.kaggle.com/settings");
  assert.ok(msg.includes("kaggle.com"), msg);
  assert.ok(msg.includes("browser"), msg);
  assert.ok(msg.includes("nothing is lost"), "somebody mid-guide needs to hear that");
  // A malformed URL still produces a sentence rather than a crash.
  assert.ok(refusalMessage("nonsense").length > 0);
});

test("the timeout is short enough to not feel broken", () => {
  assert.ok(FRAME_TIMEOUT_MS <= 5000, "waiting longer than this reads as a hang");
  assert.ok(FRAME_TIMEOUT_MS >= 2000, "and shorter than this would false-positive on a slow network");
});
