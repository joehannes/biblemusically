import { parseMarkdown } from "../lib/markdown";

// Renders Markdown as React elements — never as an HTML string.
//
// The text here comes from guides and from AI output, and a library that hands back HTML invites
// `dangerouslySetInnerHTML`. Going through elements means a stray `<script>` in a model's answer is
// text on the page rather than a script on the page. See lib/markdown.js for the parser.

function Spans({ spans }) {
  return spans.map((s, i) => {
    switch (s.type) {
      case "strong": return <strong key={i} className="font-semibold text-foreground">{s.text}</strong>;
      case "em": return <em key={i}>{s.text}</em>;
      case "code":
        return (
          <code key={i} className="px-1 py-0.5 rounded bg-muted/60 text-[0.9em] font-mono" data-no-i18n>
            {s.text}
          </code>
        );
      case "link":
        // Opens in the app's own browser rather than leaving for an external one — the guides depend on
        // the page being *beside* the steps.
        return (
          <a key={i} href={`#/browser?url=${encodeURIComponent(s.href)}`}
             className="text-primary underline underline-offset-2" title={s.href}>
            {s.text}
          </a>
        );
      default: return <span key={i}>{s.text}</span>;
    }
  });
}

export default function Markdown({ text, className = "" }) {
  const blocks = parseMarkdown(text);
  return (
    <div className={`space-y-2 text-sm leading-relaxed ${className}`}>
      {blocks.map((b, i) => {
        switch (b.type) {
          case "heading": {
            const size = b.level <= 2 ? "text-base font-semibold" : "text-sm font-semibold";
            return <div key={i} className={`${size} text-foreground pt-1`}><Spans spans={b.spans} /></div>;
          }
          case "code":
            return (
              <pre key={i} className="rounded-lg bg-muted/50 border border-border p-2.5 overflow-x-auto text-xs"
                   data-no-i18n>
                <code>{b.text}</code>
              </pre>
            );
          case "list":
            return b.ordered ? (
              <ol key={i} className="list-decimal ml-5 space-y-1">
                {b.items.map((it, j) => <li key={j}><Spans spans={it} /></li>)}
              </ol>
            ) : (
              <ul key={i} className="list-disc ml-5 space-y-1">
                {b.items.map((it, j) => <li key={j}><Spans spans={it} /></li>)}
              </ul>
            );
          case "table":
            return (
              // Wide tables scroll inside their own box rather than pushing the page sideways.
              <div key={i} className="overflow-x-auto">
                <table className="text-xs border-collapse">
                  <thead>
                    <tr>
                      {b.head.map((h, j) => (
                        <th key={j} className="text-left font-semibold border-b border-border px-2 py-1">
                          <Spans spans={h} />
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {b.rows.map((r, j) => (
                      <tr key={j}>
                        {r.map((c, k) => (
                          <td key={k} className="border-b border-border/40 px-2 py-1 align-top">
                            <Spans spans={c} />
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            );
          case "quote":
            return (
              <blockquote key={i} className="border-l-2 border-primary/50 pl-3 text-muted-foreground">
                <Spans spans={b.spans} />
              </blockquote>
            );
          case "rule":
            return <hr key={i} className="border-border/60" />;
          default:
            return <p key={i} className="text-muted-foreground"><Spans spans={b.spans} /></p>;
        }
      })}
    </div>
  );
}
