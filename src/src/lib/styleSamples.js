import { useState, useEffect, useCallback } from "react";
import { api } from "./api";
import { convertFileSrc } from "@tauri-apps/api/core";

// Shared access to the on-demand style/genre samples. Samples are generated with the user's own
// engine and saved in the global app folder; here we just read the manifest and turn each entry's
// absolute path into a webview-loadable src (Tauri asset protocol). Keyed as "<kind>:<key>".

const listeners = new Set();
let cache = null;

async function refresh() {
  try { cache = (await api.listStyleSamples()) || {}; }
  catch { cache = {}; }
  listeners.forEach((fn) => { try { fn(cache); } catch { /* ignore */ } });
  return cache;
}

/** Live map of samples; re-fetches on mount and whenever notifyStyleSamplesChanged() fires. */
export function useStyleSamples() {
  const [map, setMap] = useState(cache || {});
  useEffect(() => {
    listeners.add(setMap);
    if (cache) setMap(cache); else refresh();
    return () => listeners.delete(setMap);
  }, []);
  return map;
}

export function notifyStyleSamplesChanged() { refresh(); }

/** The loadable src for a sample, or null. `kind` = "image" | "music". */
export function sampleSrc(map, kind, key) {
  const entry = map?.[`${kind}:${key}`];
  if (!entry?.path) return null;
  try { return convertFileSrc(entry.path); } catch { return null; }
}

/** Generate a sample and refresh the shared cache. Returns the entry (or throws). */
export async function generateSample(kind, key, label, spec) {
  const entry = await api.generateStyleSample(kind, key, label, spec);
  await refresh();
  return entry;
}
