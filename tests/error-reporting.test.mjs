// What leaves the machine when something breaks, and what does not.
//
// The reporting path handles text the app never chose: provider error bodies, stack traces, request
// URLs. All three routinely carry credentials. These tests hold the line that a bug report never
// becomes a credential leak — and that a render loop throwing every frame cannot bury the one
// report that matters.
//
// Run with: npm run test:unit
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  redact, fingerprint, shouldSend, shouldNotify, reportError, resetErrorReporting,
  installErrorReporting, sessionErrors,
} from "../src/src/lib/errorReporting.js";

test("no credential this app holds survives redaction", () => {
  const cases = [
    ["OpenRouter refused: sk-or-v1-3da1c20b5a0d4f5c0c41b1b47e19b6d5014f783a", "sk-or-v1-3da1c20b"],
    ["Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.signature", "eyJhbGciOiJIUzI1NiJ9"],
    ["xi-api-key: 9f8e7d6c5b4a3f2e1d0c", "9f8e7d6c5b4a"],
    ["GET https://api.example.com/v1?api_key=supersecretvalue123", "supersecretvalue123"],
    ['{"gemini_api_key":"AIzaSyD-1234567890abcdefg"}', "AIzaSyD-1234567890"],
    ["clone failed for ghp_abcdefghij1234567890", "ghp_abcdefghij"],
    ["hf_QWERTYuiopASDFGHjkl123", "hf_QWERTYuiop"],
  ];
  for (const [raw, secret] of cases) {
    const cleaned = redact(raw);
    assert.ok(!cleaned.includes(secret), `leaked ${secret} from: ${cleaned}`);
    assert.ok(cleaned.includes("redacted") || cleaned.length < raw.length, cleaned);
  }
});

test("a home directory becomes a tilde, because a path carries a real name", () => {
  assert.equal(redact("at /home/johannes/Projects/app/main.rs"), "at ~/Projects/app/main.rs");
  assert.equal(redact("/Users/Anna Smith/Desktop/x"), "~/Desktop/x");
});

test("redaction never throws on the shapes an error actually arrives in", () => {
  assert.equal(redact(null), "");
  assert.equal(redact(undefined), "");
  assert.equal(redact(0), "");
  assert.ok(redact({ toString: () => "an object" }).includes("an object"));
  // A stack trace can be enormous; the report must not be.
  assert.ok(redact("x".repeat(50000)).length <= 6000);
});

test("the same crash from two builds has the same fingerprint", () => {
  // Line numbers move between builds. Without stripping them, one bug files a new report on every
  // release and the count never tells anybody it is the same thing.
  const a = fingerprint("Cannot read property 'x'", "at foo (app.js:1201:14)\nat bar (app.js:900:3)");
  const b = fingerprint("Cannot read property 'x'", "at foo (app.js:1345:9)\nat bar (app.js:912:7)");
  assert.equal(a, b);
  const other = fingerprint("Something else entirely", "at baz (app.js:1:1)");
  assert.notEqual(a, other);
  // And a fingerprint is never itself a leak.
  assert.ok(!fingerprint("failed with sk-or-v1-abcdefghijklmnop", "").includes("abcdefghijklmnop"));
});

test("a loop throwing every frame is counted once, not filed a thousand times", async () => {
  resetErrorReporting();
  const sent = [];
  installErrorReporting({ send: async (r) => { sent.push(r); }, notify: () => {} });

  for (let i = 0; i < 50; i++) await reportError(new Error("render loop"), { silent: true });
  assert.equal(sent.length, 1, "one report, whatever the frame rate");
  assert.equal(sessionErrors()[0].count, 50, "but the app knows it happened fifty times");
});

test("a whole session has a budget, so one bad build cannot flood the server", async () => {
  resetErrorReporting();
  const sent = [];
  installErrorReporting({ send: async (r) => { sent.push(r); }, notify: () => {} });
  for (let i = 0; i < 100; i++) await reportError(new Error(`distinct error ${i}`), { silent: true });
  assert.ok(sent.length <= 25, `sent ${sent.length}`);
  assert.ok(sent.length > 0, "and it does not go silent either");
});

test("the user is interrupted once per problem, not once per occurrence", async () => {
  resetErrorReporting();
  const notices = [];
  installErrorReporting({ send: async () => {}, notify: (n) => notices.push(n) });

  await reportError(new Error("the same thing"));
  await reportError(new Error("the same thing"));
  await reportError(new Error("the same thing"));
  assert.equal(notices.length, 1);
  assert.equal(notices[0].first, true);

  await reportError(new Error("a different thing"));
  assert.equal(notices.length, 2);
});

test("a report carries a fingerprint the server can dedupe on", async () => {
  resetErrorReporting();
  const sent = [];
  installErrorReporting({ send: async (r) => { sent.push(r); }, notify: () => {} });
  await reportError(new Error("boom"), { where: "images" });
  const [report] = sent;
  assert.equal(report.kind, "error");
  assert.ok(report.fingerprint, "the Worker dedupes on this");
  assert.ok(report.title.startsWith("images:"), "where it happened belongs in the title");
  assert.ok("stack" in report);
});

test("reporting never becomes the second bug", async () => {
  resetErrorReporting();
  // A sender that throws, a notifier that throws, and an error that is not an Error.
  installErrorReporting({
    send: async () => { throw new Error("network down"); },
    notify: () => { throw new Error("toast exploded"); },
  });
  await assert.doesNotReject(() => reportError("a bare string"));
  await assert.doesNotReject(() => reportError(null));
  await assert.doesNotReject(() => reportError({ message: "no stack here" }));
});

test("shouldSend and shouldNotify are usable on their own", () => {
  resetErrorReporting();
  const fp = fingerprint("fresh", "");
  assert.ok(shouldSend(fp));
  assert.ok(shouldNotify(fp));
});
