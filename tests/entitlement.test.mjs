// The licence rules, tested against the worker's own source.
//
// worker.js is a Cloudflare module worker: it has no exports to import and its handlers expect a
// live KV binding. But the two rules worth pinning here — who holds a licence, and what the promo
// code is — are pure decisions expressed as a function and a constant. So the source is read and the
// relevant pieces evaluated in isolation, which keeps the test honest (it fails when worker.js
// changes) without standing up a Workers runtime.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const source = readFileSync(join(root, "server", "worker.js"), "utf8");

/** Pull a top-level `function name(...) { ... }` out of the source by brace-matching. */
function extractFunction(name) {
  const start = source.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `worker.js no longer defines ${name}()`);
  let depth = 0, i = source.indexOf("{", start);
  for (let j = i; j < source.length; j++) {
    if (source[j] === "{") depth++;
    else if (source[j] === "}" && --depth === 0) return source.slice(start, j + 1);
  }
  throw new Error(`unbalanced braces in ${name}()`);
}

// `nowIso` is the only helper entitlementState leans on.
const entitlementState = new Function(
  `${extractFunction("entitlementState")}
   function nowIso() { return new Date().toISOString(); }
   return entitlementState;`,
)();

const future = new Date(Date.now() + 86400_000).toISOString();
const past = new Date(Date.now() - 86400_000).toISOString();
const cfg = { lifetime_major: 1, permanent_free: ["johannes.neugschwentner@gmail.com"] };

test("a named account keeps a full licence through an expired trial and a lapsed card", () => {
  // The point of the list: no dated state can lock this account out. Both fields are in the past,
  // which for anybody else is the "expired" path with generate switched off.
  const user = { email: "johannes.neugschwentner@gmail.com", trial_ends: past, period_ends: past };
  const state = entitlementState(user, cfg);
  assert.equal(state.status, "lifetime");
  assert.equal(state.features.generate, true);
  assert.equal(state.features.export, true);
  assert.equal(state.features.remote_sync, true);

  const other = entitlementState({ email: "someone@example.com", trial_ends: past, period_ends: past }, cfg);
  assert.equal(other.status, "expired", "the same dates must still expire for everybody else");
  assert.equal(other.features.generate, false);
});

test("the address is matched the way people actually type it", () => {
  for (const email of ["Johannes.Neugschwentner@Gmail.com", "  johannes.neugschwentner@gmail.com  "]) {
    assert.equal(entitlementState({ email }, cfg).status, "lifetime", `${email} should match`);
  }
});

test("being on the list does not unblock a blocked account", () => {
  // Ordering that matters: `blocked` is a safety control, and an account on the free list is not a
  // reason to keep serving one that has been shut off.
  const state = entitlementState(
    { email: "johannes.neugschwentner@gmail.com", blocked: true }, cfg);
  assert.equal(state.status, "blocked");
  assert.equal(state.features.read, false);
  assert.equal(state.features.generate, false);
});

test("an empty list changes nothing for anybody", () => {
  const bare = { lifetime_major: 1 };
  assert.equal(entitlementState({ email: "johannes.neugschwentner@gmail.com" }, bare).status, "expired");
  assert.equal(entitlementState({ email: "x@y.z", period_ends: future }, bare).status, "active");
});

test("a trial can do everything except take the work out of the app", () => {
  const state = entitlementState({ email: "x@y.z", trial_ends: future }, cfg);
  assert.equal(state.status, "trial");
  assert.equal(state.features.generate, true, "a trial that cannot generate is not a trial");
  for (const locked of ["export", "save_copies", "remote_sync"]) {
    assert.equal(state.features[locked], false, `${locked} is what stops "new trial, reimport"`);
  }
});

test("the promo code is typeable and redeemed once per account", () => {
  const code = source.match(/promo_code:\s*"([^"]+)"/)?.[1];
  assert.ok(code, "worker.js no longer carries a default promo code");

  // No I/L/O/U/0/1: this gets read aloud and typed by hand, and those are the characters people
  // get wrong. Guarding it here because a regenerated code could quietly reintroduce them.
  assert.match(code, /^BM1(-[23456789ABCDEFGHJKMNPQRSTVWXYZ]{5}){3}$/,
               `${code} is not in the unambiguous alphabet`);

  // The reuse guard must be keyed by code *and* email, or one leak becomes unlimited licences.
  assert.match(source, /promo:\$\{promo\}:\$\{String\(user\.email\)\.toLowerCase\(\)\}/,
               "the promo marker must be per-account, not per-code");
  assert.match(source, /already used that code on this account/);
});

test("the promo grants the current major version, not a fixed number", () => {
  // A lifetime licence covers the major version it was bought in; hardcoding 1 here would silently
  // hand out the next major version too.
  assert.match(source, /user\.lifetime_major = cfgEarly\.lifetime_major \|\| 1;/);
});
