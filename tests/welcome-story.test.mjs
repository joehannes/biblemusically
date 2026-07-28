// The welcome tour's script.
//
// This is the first thing a new user reads, and it is prose rather than code, so the thing worth
// testing is not behaviour but completeness: every stop written at every level, pointed at a route
// that exists, at a length that fits the card and the pause it schedules for itself. A stop that
// silently falls back to another level's wording is the failure mode this catches — it still
// renders, so nothing looks broken, and a professional gets read a children's sentence.
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { WELCOME_STOPS, LEVELS, DEFAULT_LEVEL, storyFor, dwellMs } from "../src/src/lib/welcomeStory.js";

// The routes the app actually mounts, read from the router rather than duplicated here — a stop
// pointing at a route that was renamed would otherwise navigate to a blank page mid-tour.
const APP_ROUTES = new Set(
  [...readFileSync(new URL("../src/src/App.jsx", import.meta.url), "utf8")
    .matchAll(/<Route\s+path="([^"]+)"/g)].map((m) => m[1]),
);

test("every stop is written at every level", () => {
  for (const stop of WELCOME_STOPS) {
    for (const level of LEVELS) {
      const written = stop.levels?.[level];
      assert.ok(written, `stop "${stop.id}" has no ${level} wording`);
      assert.ok(written.title?.trim(), `stop "${stop.id}" (${level}) has no title`);
      assert.ok(written.body?.trim(), `stop "${stop.id}" (${level}) has no body`);
    }
  }
});

test("no two levels of a stop share the same words", () => {
  // Copy-pasting one level into another is the cheap way to make the test above pass, and it
  // defeats the entire point of asking the user what level they wanted.
  for (const stop of WELCOME_STOPS) {
    const bodies = LEVELS.map((l) => stop.levels[l].body);
    assert.equal(new Set(bodies).size, LEVELS.length, `stop "${stop.id}" repeats a body across levels`);
    const titles = LEVELS.map((l) => stop.levels[l].title);
    assert.equal(new Set(titles).size, LEVELS.length, `stop "${stop.id}" repeats a title across levels`);
  }
});

// Simplicity is sentence length, not word count. Professional wording is *denser* — it often says
// more in fewer words — so "the kid version must be shorter overall" is the wrong test and fails on
// prose that is perfectly fine. What actually makes a sentence easy to follow is how much has to be
// held in mind before it ends.
const meanSentenceWords = (text) => {
  const sentences = String(text).split(/[.!?]+/).map((s) => s.trim()).filter(Boolean);
  const words = sentences.reduce((n, s) => n + s.split(/\s+/).length, 0);
  return words / sentences.length;
};

test("simple wording uses shorter sentences than professional wording", () => {
  for (const stop of WELCOME_STOPS) {
    const kid = meanSentenceWords(stop.levels.kid.body);
    const pro = meanSentenceWords(stop.levels.pro.body);
    assert.ok(kid < pro, `stop "${stop.id}": kid sentences average ${kid.toFixed(1)} words, pro ${pro.toFixed(1)}`);
    assert.ok(kid <= 22, `stop "${stop.id}": kid sentences average ${kid.toFixed(1)} words — too long to be simple`);
  }
});

test("no level runs long enough to overflow the card", () => {
  const words = (s) => s.trim().split(/\s+/).length;
  for (const stop of WELCOME_STOPS) {
    for (const level of LEVELS) {
      const n = words(stop.levels[level].body);
      assert.ok(n <= 70, `stop "${stop.id}" (${level}) is ${n} words — too much for one card`);
    }
    assert.ok(words(stop.levels.kid.body) <= 50, `stop "${stop.id}": kid wording is too long to be simple`);
  }
});

test("every stop lands on a route the app actually mounts", () => {
  for (const stop of WELCOME_STOPS) {
    assert.ok(APP_ROUTES.has(stop.to), `stop "${stop.id}" navigates to ${stop.to}, which is not a route`);
  }
});

test("stop ids are unique", () => {
  const ids = WELCOME_STOPS.map((s) => s.id);
  assert.equal(new Set(ids).size, ids.length, "two stops share an id");
});

test("the tour opens and closes on the dashboard", () => {
  // Arriving somewhere deep in the app and being left there is a poor first and last impression.
  assert.equal(WELCOME_STOPS[0].to, "/");
  assert.equal(WELCOME_STOPS[WELCOME_STOPS.length - 1].to, "/");
});

test("storyFor resolves the asked-for level", () => {
  const stop = WELCOME_STOPS[0];
  for (const level of LEVELS) {
    assert.equal(storyFor(stop, level).body, stop.levels[level].body);
  }
});

test("storyFor never returns nothing, whatever it is asked for", () => {
  for (const level of ["", null, undefined, "expert", "toddler"]) {
    const s = storyFor(WELCOME_STOPS[0], level);
    assert.ok(s.title && s.body, `level ${JSON.stringify(level)} produced an empty card`);
  }
  assert.equal(storyFor(WELCOME_STOPS[0], "nonsense").body, WELCOME_STOPS[0].levels[DEFAULT_LEVEL].body);
});

test("a stop-level tip is inherited when a level does not write its own", () => {
  const withTip = WELCOME_STOPS.find((s) => s.tip);
  assert.ok(withTip, "no stop carries a shared tip");
  assert.equal(storyFor(withTip, "pro").tip, withTip.tip);
  // ...and a level that writes its own keeps it.
  const overridden = WELCOME_STOPS.find((s) => LEVELS.some((l) => s.levels[l].tip));
  if (overridden) {
    const level = LEVELS.find((l) => overridden.levels[l].tip);
    assert.equal(storyFor(overridden, level).tip, overridden.levels[level].tip);
  }
});

test("the pause is always long enough to look and short enough to bear", () => {
  for (const stop of WELCOME_STOPS) {
    for (const level of LEVELS) {
      const ms = dwellMs(stop.levels[level].body);
      assert.ok(ms >= 2000, `stop "${stop.id}" (${level}) barely pauses at all`);
      assert.ok(ms <= 9000, `stop "${stop.id}" (${level}) holds the user for ${ms}ms`);
    }
  }
  // Degenerate input still yields a usable pause rather than zero or NaN.
  for (const junk of ["", null, undefined, "   "]) {
    assert.equal(dwellMs(junk), 2000);
  }
});

test("more text always gets at least as much time, never less", () => {
  // The pause is derived from length, so this is a property of dwellMs itself rather than of any
  // particular stop — a regression here would shorten the pause on exactly the densest passages.
  let previous = 0;
  for (let words = 0; words < 120; words += 3) {
    const ms = dwellMs("word ".repeat(words));
    assert.ok(ms >= previous, `${words} words got less time than ${words - 3}`);
    previous = ms;
  }
});
