import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Textarea } from "../components/ui/textarea";
import {
  ClipboardList, Copy, Pin, PinOff, Trash2, ArrowDownToLine, Plus,
  ListOrdered, RefreshCw, Loader2, Archive,
} from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// The clipboard tool.
//
// Two clipboards, on purpose:
//
//   • System mirrors the OS clipboard and supports popping — consuming the top entry so the next paste
//     is the one underneath. Destructive, stack-like, and exactly what sequential pasting needs.
//   • Vault never loses anything. "Pop" there appends a new entry equal to what the system clipboard
//     would hold after a pop, so the top of the list still behaves like a clipboard while every earlier
//     state stays visible.
//
// The queue is the part that makes browser macros useful: load the values a macro will need, in order,
// and each `paste the next queued value` step takes one. The cursor lives in the backend, so a page
// navigation mid-macro cannot lose its place.
//
// On desktop the same entries appear in the taskbar icon's menu. On mobile there is no tray, so this
// view is the whole interface — which is why the actions here are complete rather than a subset.
// ─────────────────────────────────────────────────────────────────────────────

export default function Clipboard() {
  const [kind, setKind] = useState("system");
  const [items, setItems] = useState([]);
  const [queue, setQueue] = useState({ loaded: false });
  const [draft, setDraft] = useState("");
  const [queueDraft, setQueueDraft] = useState("");
  const [busy, setBusy] = useState("");

  const load = useCallback(async (which = kind) => {
    try {
      const [h, q] = await Promise.all([
        api.clipboardHistory(which, 60),
        api.clipboardQueueStatus(),
      ]);
      setItems(h?.items || []);
      setQueue(q || { loaded: false });
    } catch (err) { toast.error(`${err}`); }
  }, [kind]);

  useEffect(() => { load(kind); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [kind]);
  useEffect(() => {
    // The desktop watcher records changes on its own; this only keeps the view honest while it is open.
    const t = setInterval(() => { api.clipboardSync().then(() => load()).catch(() => {}); }, 3000);
    return () => clearInterval(t);
    /* eslint-disable-next-line react-hooks/exhaustive-deps */
  }, [kind]);

  const act = async (name, fn, after) => {
    setBusy(name);
    try {
      const r = await fn();
      if (after) after(r);
      await load();
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const loadQueue = () => {
    const values = queueDraft.split("\n").map((v) => v.trim()).filter(Boolean);
    if (!values.length) { toast.error("One value per line."); return; }
    act("queue", () => api.clipboardQueueSet({ items: values, remember: true }), (r) => {
      toast.success(`${r.loaded} values queued — first one is on the clipboard.`);
      setQueueDraft("");
    });
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div>
          <h1 className="text-xl font-semibold flex items-center gap-2">
            <ClipboardList className="w-5 h-5 text-primary" /> Clipboard
          </h1>
          <p className="text-sm text-muted-foreground">
            A history that pops, a vault that never forgets, and a queue macros can paste from.
          </p>
        </div>
        <div className="flex gap-1">
          <Button variant={kind === "system" ? "default" : "secondary"} size="sm" onClick={() => setKind("system")}>
            <Copy className="w-3.5 h-3.5 mr-1.5" />System
          </Button>
          <Button variant={kind === "vault" ? "default" : "secondary"} size="sm" onClick={() => setKind("vault")}>
            <Archive className="w-3.5 h-3.5 mr-1.5" />Vault
          </Button>
          <Button variant="secondary" size="sm" onClick={() => load()}>
            <RefreshCw className="w-3.5 h-3.5" />
          </Button>
        </div>
      </div>

      {/* ── The queue ─────────────────────────────────────────────────────── */}
      <Card className="p-4 space-y-3">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <div className="flex items-center gap-2">
            <ListOrdered className="w-4 h-4 text-primary" />
            <span className="font-medium">Paste queue</span>
            {queue.loaded && (
              <Badge variant="outline" className="text-[10px]">
                {queue.cursor + 1} of {queue.of} · {queue.remaining} left
              </Badge>
            )}
          </div>
          {queue.loaded && (
            <Button size="sm" variant="secondary" disabled={busy === "next"}
                    onClick={() => act("next", () => api.clipboardQueueNext(),
                      (r) => toast.message(r.done ? "The queue is finished." : `Now on the clipboard: ${r.on_clipboard}`))}>
              <ArrowDownToLine className="w-3.5 h-3.5 mr-1.5" />Advance
            </Button>
          )}
        </div>
        <p className="text-xs text-muted-foreground">
          One value per line, in the order a macro should paste them. Each
          <b> paste the next queued value</b> step in a macro takes one and advances.
        </p>
        <Textarea value={queueDraft} onChange={(e) => setQueueDraft(e.target.value)} data-no-i18n
                  placeholder={"first value\nsecond value\nthird value"} className="h-24 text-sm font-mono" />
        <Button size="sm" onClick={loadQueue} disabled={busy === "queue"}>
          {busy === "queue" ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : <ListOrdered className="w-3.5 h-3.5 mr-1.5" />}
          Load the queue
        </Button>
        {queue.loaded && (queue.previews || []).length > 0 && (
          <div className="space-y-0.5 pt-1 border-t border-border/50">
            {queue.previews.map((p, i) => (
              <div key={i} className={`text-xs flex items-center gap-2 ${i === queue.cursor ? "text-primary" : "text-muted-foreground"}`}>
                <span className="w-5 text-right">{i + 1}.</span>
                <span className="truncate">{p}</span>
                {i === queue.cursor && <span className="text-[10px] uppercase tracking-wide">on clipboard</span>}
              </div>
            ))}
          </div>
        )}
      </Card>

      {/* ── Add + bulk actions ────────────────────────────────────────────── */}
      <Card className="p-4 space-y-2">
        <Textarea value={draft} onChange={(e) => setDraft(e.target.value)} data-no-i18n
                  placeholder={kind === "system" ? "Text to copy and remember" : "Text to keep in the vault"}
                  className="h-16 text-sm" />
        <div className="flex gap-1.5 flex-wrap">
          <Button size="sm" disabled={!draft.trim() || busy === "add"}
                  onClick={() => act("add", () => api.clipboardAdd({ text: draft, kind }),
                    () => { toast.success(kind === "system" ? "Copied and remembered." : "Kept in the vault."); setDraft(""); })}>
            <Plus className="w-3.5 h-3.5 mr-1.5" />Add to {kind}
          </Button>
          <Button size="sm" variant="secondary" disabled={busy === "pop"}
                  onClick={() => act("pop", () => api.clipboardPop(kind), (r) => {
                    if (r.ok === false) { toast.message(r.reason); return; }
                    toast.success(r.simulated
                      ? "Vault popped without losing anything — the new top is what pastes next."
                      : `Popped. Now on the clipboard: ${r.now}`);
                  })}>
            <ArrowDownToLine className="w-3.5 h-3.5 mr-1.5" />
            Pop {kind === "vault" ? "(non-destructive)" : ""}
          </Button>
          <Button size="sm" variant="ghost" disabled={busy === "clear"}
                  onClick={() => act("clear", () => api.clipboardClear(kind),
                    (r) => toast.success(`${r.removed} entries cleared. Pinned ones kept.`))}>
            <Trash2 className="w-3.5 h-3.5 mr-1.5" />Clear
          </Button>
        </div>
        {kind === "vault" && (
          <p className="text-xs text-muted-foreground">
            The vault never deletes. Popping here appends what the clipboard <i>would</i> hold after a
            pop, so the newest entry is always what pastes next and nothing you copied is ever gone.
          </p>
        )}
      </Card>

      {/* ── History ───────────────────────────────────────────────────────── */}
      <div className="space-y-2">
        {!items.length && (
          <Card className="p-4 text-sm text-muted-foreground">
            Nothing here yet. {kind === "system" ? "Copy something and it appears." : "Add an entry above."}
          </Card>
        )}
        {items.map((it, i) => (
          <Card key={it.id} className={`p-3 flex items-start justify-between gap-3 ${i === 0 ? "border-primary/40" : ""}`}>
            <div className="min-w-0">
              <div className="text-sm truncate" data-no-i18n>{it.preview}</div>
              <div className="text-[10px] text-muted-foreground">
                {i === 0 && <span className="text-primary uppercase tracking-wide mr-1.5">top</span>}
                {it.chars} chars · {it.source}
                {it.pinned ? " · pinned" : ""}
              </div>
            </div>
            <div className="flex gap-1 shrink-0">
              <Button size="sm" variant="secondary" title="Copy this back to the clipboard"
                      onClick={() => act(`use-${it.id}`, () => api.clipboardUse(it.id), () => toast.success("Copied."))}>
                <Copy className="w-3.5 h-3.5" />
              </Button>
              <Button size="sm" variant="ghost" title={it.pinned ? "Unpin" : "Pin — survives a clear"}
                      onClick={() => act(`pin-${it.id}`, () => api.clipboardPin(it.id, !it.pinned))}>
                {it.pinned ? <PinOff className="w-3.5 h-3.5" /> : <Pin className="w-3.5 h-3.5" />}
              </Button>
            </div>
          </Card>
        ))}
      </div>
    </div>
  );
}
