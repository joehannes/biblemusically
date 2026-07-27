// Finishing a sign-in that happened somewhere else.
//
// Google and Kaggle refuse to authenticate inside an embedded webview, so the system browser is the
// only route. That leaves the return journey, and getting it wrong is invisible: the browser
// finishes, shows a page the user cannot act on, and the app never learns consent was granted.
//
// Collected two ways on purpose, because they race. The event is the fast path. The poll is the one
// that cannot be missed — on Android the callback intent can arrive *before* this webview has
// finished loading and attached a listener, and an event fired at nobody is a sign-in that silently
// did nothing.

import { listen } from "@tauri-apps/api/event";
import { api } from "./api";

/**
 * Start watching for a returning sign-in.
 *
 * @param {(result: object) => void} onResult called with `{ grant, message }` once per callback.
 * @returns {() => void} stop watching.
 */
export function watchDeepLinks(onResult) {
  let stopped = false;
  let unlisten = () => {};

  const finish = async (payload) => {
    if (stopped || !payload) return;
    try {
      // The backend already knows how to redeem a code against the channel that asked for it; this
      // only carries the answer across. `state` is what ties the two together — a callback whose
      // state we never issued belongs to somebody else's request and the backend rejects it.
      if (payload.grant && payload.code && payload.state) {
        await api.oauthCallback(payload.code, payload.state, null);
      }
    } catch (err) {
      onResult({ grant: false, message: `That sign-in could not be completed: ${err}` });
      return;
    }
    onResult(payload);
  };

  listen("bm:deep-link", (event) => finish(event.payload)).then((fn) => {
    if (stopped) fn(); else unlisten = fn;
  }).catch(() => {});

  // One immediate poll for anything that arrived before this listener existed, then a slow one —
  // slow because a missed callback is recovered within seconds either way, and a tight loop on a
  // phone costs battery for nothing.
  const drain = async () => {
    if (stopped) return;
    try {
      const waiting = await api.takeDeepLink();
      if (waiting?.waiting) await finish(waiting);
    } catch { /* not running under Tauri, or the command is unavailable */ }
  };
  drain();
  const timer = setInterval(drain, 5000);

  return () => { stopped = true; clearInterval(timer); unlisten(); };
}

/**
 * The escape hatch: paste the address the browser ended on.
 *
 * For a desktop where the custom scheme is not registered, or a phone whose browser refuses to hand
 * back. The sign-in already happened — this finishes it rather than starting again.
 */
export async function submitPastedLink(url) {
  const parsed = await api.submitDeepLink(url);
  if (parsed.grant && parsed.code && parsed.state) {
    await api.oauthCallback(parsed.code, parsed.state, null);
  }
  return parsed;
}
