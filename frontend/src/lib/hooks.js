import { useEffect, useRef, useState, useCallback } from "react";
import { api } from "./api";

/** useAutoSave — debounced auto-save of a value to backend drafts + localStorage mirror.
 *  Returns {status: 'idle'|'saving'|'saved'|'error', lastSaved, restore} */
export function useAutoSave(key, value, { delay = 700, enabled = true } = {}) {
  const [status, setStatus] = useState("idle");
  const [lastSaved, setLastSaved] = useState(null);
  const timerRef = useRef();
  const firstRef = useRef(true);

  useEffect(() => {
    if (!enabled) return;
    if (firstRef.current) { firstRef.current = false; return; }
    if (timerRef.current) clearTimeout(timerRef.current);
    setStatus("saving");
    timerRef.current = setTimeout(async () => {
      try {
        // local mirror first (instant recovery)
        localStorage.setItem(`draft:${key}`, JSON.stringify({ value, t: Date.now() }));
        await api.putDraft(key, { value });
        setStatus("saved"); setLastSaved(Date.now());
      } catch (e) {
        setStatus("error");
      }
    }, delay);
    return () => timerRef.current && clearTimeout(timerRef.current);
  }, [key, value, delay, enabled]);

  const restore = useCallback(async () => {
    // try backend first, then local
    try {
      const d = await api.getDraft(key);
      if (d && Object.keys(d).length > 0 && d.value !== undefined) return d.value;
    } catch {}
    try {
      const local = localStorage.getItem(`draft:${key}`);
      if (local) { const p = JSON.parse(local); return p.value; }
    } catch {}
    return null;
  }, [key]);

  return { status, lastSaved, restore };
}

/** Tiny indicator chip used in headers. */
export function AutoSaveChip({ status, lastSaved }) {
  const txt = status === "saving" ? "saving…"
    : status === "saved" ? `saved${lastSaved ? " " + new Date(lastSaved).toLocaleTimeString() : ""}`
    : status === "error" ? "save failed"
    : "auto-save";
  const color = status === "error" ? "text-destructive" : status === "saved" ? "text-primary" : "text-muted-foreground";
  return <span className={`text-mono text-[10px] uppercase tracking-widest ${color}`}>{txt}</span>;
}
