import test from "node:test";
import assert from "node:assert/strict";
import { parseMarkdown, parseInline } from "../src/src/lib/markdown.js";

// The parser exists so rendered text is DATA, never an HTML string — the content comes from AI output
// and a `dangerouslySetInnerHTML` path would make a stray tag executable.

test("html in the source is text, not structure", () => {
  const blocks = parseMarkdown("<script>alert(1)</script> and <b>bold</b>");
  assert.equal(blocks.length, 1);
  assert.equal(blocks[0].type, "paragraph");
  // It survives as literal text — the renderer prints it, so it can never execute.
  const text = blocks[0].spans.map((s) => s.text).join("");
  assert.ok(text.includes("<script>alert(1)</script>"), text);
});

test("bold is never read as two italics", () => {
  const spans = parseInline("**really** and *just* one");
  assert.deepEqual(spans.filter((s) => s.type === "strong").map((s) => s.text), ["really"]);
  assert.deepEqual(spans.filter((s) => s.type === "em").map((s) => s.text), ["just"]);
});

test("inline code keeps its contents verbatim", () => {
  const spans = parseInline("use `instagram_business_content_publish` here");
  const code = spans.find((s) => s.type === "code");
  assert.equal(code.text, "instagram_business_content_publish");
});

test("a link carries its href separately from its text", () => {
  const spans = parseInline("see [the docs](https://example.com/a?b=c) now");
  const link = spans.find((s) => s.type === "link");
  assert.equal(link.text, "the docs");
  assert.equal(link.href, "https://example.com/a?b=c");
});

test("a table needs its divider row, or it is just a paragraph", () => {
  const real = parseMarkdown("| a | b |\n|---|---|\n| 1 | 2 |");
  assert.equal(real[0].type, "table");
  assert.equal(real[0].head.length, 2);
  assert.equal(real[0].rows.length, 1);

  // Pipes in prose must not silently become a table.
  const prose = parseMarkdown("this | that | the other");
  assert.equal(prose[0].type, "paragraph");
});

test("fenced code is taken verbatim, markdown inside included", () => {
  const blocks = parseMarkdown("```json\n{ \"a\": **1** }\n```");
  assert.equal(blocks[0].type, "code");
  assert.equal(blocks[0].lang, "json");
  assert.ok(blocks[0].text.includes("**1**"), "no inline parsing inside a fence");
});

test("lists, both kinds, and headings do not swallow the next block", () => {
  const blocks = parseMarkdown("## Title\n\n- one\n- two\n\n1. first\n2. second\n\nafter");
  assert.deepEqual(blocks.map((b) => b.type), ["heading", "list", "list", "paragraph"]);
  assert.equal(blocks[0].level, 2);
  assert.equal(blocks[1].ordered, false);
  assert.equal(blocks[2].ordered, true);
  assert.equal(blocks[2].items.length, 2);
});

test("empty and undefined input produce nothing rather than throwing", () => {
  assert.deepEqual(parseMarkdown(""), []);
  assert.deepEqual(parseMarkdown(undefined), []);
  assert.deepEqual(parseMarkdown("   \n\n  "), []);
});
