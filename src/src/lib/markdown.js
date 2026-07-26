// A small Markdown parser, producing a block list the renderer walks.
//
// Written rather than pulled in for one reason that matters: the text it renders comes from AI output
// and from guides, and a library that turns Markdown into an HTML string invites
// `dangerouslySetInnerHTML`. This produces *data*, which React renders as elements, so a stray
// `<script>` in a model's answer is text on the page rather than a script on the page.
//
// It supports what the app's own content actually uses: headings, paragraphs, bold, italic, inline
// code, links, fenced code, bullet and numbered lists, tables, blockquotes and rules. Anything it does
// not understand stays visible as plain text, which is the right failure: unreadable beats invisible.

/** Inline spans: bold, italic, code, links. Returns a flat list of `{type, text, href}`. */
export function parseInline(text) {
  const out = [];
  // One pass, longest markers first so `**` is never read as two `*`.
  const re = /(`[^`]+`)|(\*\*[^*]+\*\*)|(__[^_]+__)|(\*[^*\n]+\*)|(\[[^\]]+\]\([^)\s]+\))/g;
  let last = 0;
  let m;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) out.push({ type: "text", text: text.slice(last, m.index) });
    const token = m[0];
    if (token.startsWith("`")) {
      out.push({ type: "code", text: token.slice(1, -1) });
    } else if (token.startsWith("**") || token.startsWith("__")) {
      out.push({ type: "strong", text: token.slice(2, -2) });
    } else if (token.startsWith("*")) {
      out.push({ type: "em", text: token.slice(1, -1) });
    } else {
      const split = token.indexOf("](");
      out.push({
        type: "link",
        text: token.slice(1, split),
        href: token.slice(split + 2, -1),
      });
    }
    last = m.index + token.length;
  }
  if (last < text.length) out.push({ type: "text", text: text.slice(last) });
  return out.length ? out : [{ type: "text", text }];
}

/** Split a table row into cells, tolerating the optional leading and trailing pipes. */
function cells(line) {
  return line.replace(/^\||\|$/g, "").split("|").map((c) => c.trim());
}

const isDivider = (line) => /^\s*\|?[\s:-]*-[\s|:-]*\|?\s*$/.test(line) && line.includes("-");

/** Parse a document into blocks. */
export function parseMarkdown(src) {
  const lines = String(src || "").replace(/\r\n?/g, "\n").split("\n");
  const blocks = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Fenced code — taken verbatim, including any Markdown inside it.
    if (/^```/.test(line)) {
      const lang = line.slice(3).trim();
      const body = [];
      i++;
      while (i < lines.length && !/^```/.test(lines[i])) { body.push(lines[i]); i++; }
      i++;                                     // the closing fence
      blocks.push({ type: "code", lang, text: body.join("\n") });
      continue;
    }

    if (!line.trim()) { i++; continue; }

    if (/^#{1,6}\s/.test(line)) {
      const level = line.match(/^#+/)[0].length;
      blocks.push({ type: "heading", level, spans: parseInline(line.replace(/^#+\s*/, "")) });
      i++;
      continue;
    }

    if (/^\s*([-*_])\s*\1\s*\1[\s\1]*$/.test(line)) {
      blocks.push({ type: "rule" });
      i++;
      continue;
    }

    // A table needs a divider on the second line; without it these are just paragraphs with pipes.
    if (line.includes("|") && i + 1 < lines.length && isDivider(lines[i + 1])) {
      const head = cells(line);
      i += 2;
      const rows = [];
      while (i < lines.length && lines[i].includes("|") && lines[i].trim()) {
        rows.push(cells(lines[i]));
        i++;
      }
      blocks.push({
        type: "table",
        head: head.map(parseInline),
        rows: rows.map((r) => r.map(parseInline)),
      });
      continue;
    }

    if (/^\s*>\s?/.test(line)) {
      const body = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) {
        body.push(lines[i].replace(/^\s*>\s?/, ""));
        i++;
      }
      blocks.push({ type: "quote", spans: parseInline(body.join(" ")) });
      continue;
    }

    if (/^\s*([-*•]|\d+[.)])\s+/.test(line)) {
      const ordered = /^\s*\d/.test(line);
      const items = [];
      while (i < lines.length && /^\s*([-*•]|\d+[.)])\s+/.test(lines[i])) {
        items.push(parseInline(lines[i].replace(/^\s*([-*•]|\d+[.)])\s+/, "")));
        i++;
      }
      blocks.push({ type: "list", ordered, items });
      continue;
    }

    // A paragraph runs until a blank line or the start of another block.
    const para = [];
    while (
      i < lines.length && lines[i].trim() &&
      !/^#{1,6}\s/.test(lines[i]) && !/^```/.test(lines[i]) &&
      !/^\s*([-*•]|\d+[.)])\s+/.test(lines[i]) && !/^\s*>\s?/.test(lines[i])
    ) {
      para.push(lines[i].trim());
      i++;
    }
    blocks.push({ type: "paragraph", spans: parseInline(para.join(" ")) });
  }
  return blocks;
}
