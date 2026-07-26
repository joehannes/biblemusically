import { api } from "./api";

// ─────────────────────────────────────────────────────────────────────────────
// What the app is allowed to do, held in one place.
//
// The backend is the actual gate — every restricted command checks the signed entitlement before doing
// any work, so deleting a CSS class reveals a page whose buttons still refuse. This module exists so the
// interface can *explain* that instead of letting somebody click into a wall.
//
// It is a module-level singleton with subscribers rather than context, because the answer is the same for
// the whole app and half the things that need it are not React components.
// ─────────────────────────────────────────────────────────────────────────────

let state = { signed_in: false, status: "none", features: {}, days_left: null, loading: true };
const listeners = new Set();

const emit = () => listeners.forEach((fn) => { try { fn(state); } catch { /* ignore */ } });

export function subscribeEntitlement(fn) {
  listeners.add(fn);
  fn(state);
  return () => listeners.delete(fn);
}

export const entitlement = () => state;

/** Can this be done? Absent means no — a missing flag must never read as permission. */
export const can = (feature) => Boolean(state.features?.[feature]);

/** True while everything except reading is off: the state that earns the paywall. */
export const isLocked = () => !can("generate");

export async function refreshEntitlement({ remote = false } = {}) {
  try {
    if (remote) await api.subsRefresh().catch(() => {});
    const next = await api.subsStatus();
    state = { ...next, loading: false };
  } catch {
    // Offline or a broken server must not lock somebody out: keep whatever was last known, and say so.
    state = { ...state, loading: false, offline: true };
  }
  emit();
  return state;
}

/**
 * Start the entitlement lifecycle.
 *
 * Refreshed against the server hourly rather than per action: the signed token lasts about a day and
 * carries a week of grace, so more often would spend requests to learn nothing.
 */
export function installEntitlement() {
  refreshEntitlement({ remote: true });
  setInterval(() => refreshEntitlement({ remote: true }), 60 * 60 * 1000);
  // Coming back to a laptop that was shut for a week is exactly when the answer has changed.
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") refreshEntitlement();
  });
}
