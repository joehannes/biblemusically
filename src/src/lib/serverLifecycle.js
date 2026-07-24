import { api } from "./api";

// ─────────────────────────────────────────────────────────────────────────────
// Kaggle GPU-hour conservation.
//
// A Kaggle GPU batch session burns the free weekly quota (30 h) for as long as it RUNS — whether
// or not anything is using it. Leaving one up after a workflow finishes is what silently drains
// the allowance and later makes Kaggle hand back CPU-only containers.
//
// So: servers are started on demand (never at app startup), tracked here, and stopped when they
// are no longer needed — after a workflow, after 15 minutes of app inactivity, or on shutdown.
// The notebook's own 15-minute idle watchdog is the backstop for anything that escapes this.
// ─────────────────────────────────────────────────────────────────────────────

const IDLE_MS = 15 * 60 * 1000;

/** Engines this app session started, so we only ever stop what we brought up. */
const started = new Set();
let idleTimer = null;
let inited = false;

const listeners = new Set();
const emit = () => listeners.forEach((fn) => { try { fn(new Set(started)); } catch { /* ignore */ } });
export const subscribeServers = (fn) => { listeners.add(fn); fn(new Set(started)); return () => listeners.delete(fn); };

export const startedEngines = () => new Set(started);

export function markStarted(engine) {
  if (!engine) return;
  started.add(engine);
  emit();
  resetIdle();
}

export function markStopped(engine) {
  started.delete(engine);
  emit();
  resetIdle();
}

/** Stop one engine's Kaggle session (frees its GPU slot immediately). */
export async function stopEngine(engine, reason = "") {
  try {
    await api.stopKaggleServer(engine);
    markStopped(engine);
    console.info(`[servers] stopped ${engine}${reason ? ` (${reason})` : ""}`);
    return true;
  } catch (e) {
    console.warn(`[servers] could not stop ${engine}:`, e);
    return false;
  }
}

/** Stop every engine this session started. Used after a workflow, on idle, and on shutdown. */
export async function stopAllStarted(reason = "") {
  const list = [...started];
  if (!list.length) return 0;
  const results = await Promise.all(list.map((e) => stopEngine(e, reason)));
  return results.filter(Boolean).length;
}

function resetIdle() {
  if (idleTimer) clearTimeout(idleTimer);
  idleTimer = null;
  if (!started.size) return; // nothing running → nothing to time out
  idleTimer = setTimeout(() => {
    stopAllStarted("app idle for 15 minutes");
  }, IDLE_MS);
}

/**
 * Arm the idle watchdog and the shutdown hook. Safe to call more than once.
 * Any real user interaction counts as activity and postpones the shutdown.
 */
export function initServerLifecycle() {
  if (inited || typeof window === "undefined") return;
  inited = true;

  const activity = () => { if (started.size) resetIdle(); };
  ["mousedown", "keydown", "wheel", "touchstart", "focus"].forEach((ev) =>
    window.addEventListener(ev, activity, { passive: true, capture: true })
  );

  // Best-effort stop when the window goes away. The Rust side also stops tracked engines on exit,
  // which is the reliable path — this just gets the request in earlier when it can.
  window.addEventListener("beforeunload", () => {
    for (const e of started) { api.stopKaggleServer(e).catch(() => {}); }
  });
}
