// One-click Kaggle engine provisioning: chains the three separate manual steps (Start server →
// watch it boot → Test connection) into a single automatic flow that shows real progress the whole
// way, so "Start & connect" is the only button a user needs to press before generating.
//
// Built as a client-side orchestrator (mirrors genPipeline.js's pub/sub pattern) rather than one
// long-running Rust command: every step it calls (startKaggleServer, kaggleStartMonitor,
// kaggleProgress, testX) already exists and runs headlessly via the local `kaggle` CLI + token —
// none of it needs a browser session. The heavy lifting of *watching* the boot is done by the Rust
// monitor (kaggle_monitor.rs), which streams `kaggle kernels logs -f`, parses the notebook's phase
// milestones (install → download ~21 GB → load → tunnel up), extracts + liveness-probes the tunnel
// URL, and captures the fatal error (e.g. flux's missing HF_TOKEN, GPU-quota exhaustion). Here we
// just poll that monitor and gate "ready" on an actual API test.

import { api } from "./api";
import { markStarted } from "./serverLifecycle";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const PROGRESS_POLL_MS = 2500;
const POLL_MAX_MS = 14 * 60 * 1000; // Kaggle notebooks need ~8-10 min; give headroom for slow downloads

// engine key -> the matching test_* call (the readiness gate: a live tunnel isn't enough, the API must answer)
const TEST_FN = {
  acestep: () => api.testAcestep(),
  heartmula: () => api.testHeartmula(),
  comfyui: () => api.testComfy(),
  flux: () => api.testFlux(),
};

// An actionable next step for each hint the Rust monitor / start command can return.
const HINT_NEXT_STEP = {
  hf_token: "FLUX needs a Hugging Face token. Open the notebook (button above), add a secret named HF_TOKEN (Add-ons → Secrets) with a read token from huggingface.co/settings/tokens, accept the FLUX.1-schnell license, then Start & connect again.",
  oom: "The GPU ran out of memory. Lower the model/resolution in the notebook, or retry — a fresh T4 sometimes has more free VRAM.",
  cli: "The kaggle CLI couldn't run. Install it (pipx install kaggle) and save your token at ~/.kaggle/kaggle.json.",
  tunnel_dead: "The notebook is running and printed a tunnel URL, but Cloudflare never routed it — usually a flaky quick tunnel. Press Start & connect again; the run itself is fine.",
  gpu_denied:
    "Kaggle ran the notebook on CPU instead of a GPU, so it could not serve. Your weekly Kaggle GPU quota (30 h) is most likely used up — it resets Saturdays UTC. Check kaggle.com/settings → Accelerator usage. Until it resets, either wait, or run the engine on another free GPU host (Colab / Lightning.ai) and paste its URL into this engine's server-URL field.",
};

let state = {}; // engine -> { status, phase, url, detail, next_step, hint, log: [], monitor }
const listeners = new Set();

function emit() { listeners.forEach((fn) => { try { fn({ ...state }); } catch { /* ignore */ } }); }

function patch(engine, p) {
  state = { ...state, [engine]: { ...(state[engine] || { log: [] }), ...p } };
  emit();
}

function pushLog(engine, message, level = "info") {
  const prev = state[engine]?.log || [];
  state = { ...state, [engine]: { ...(state[engine] || {}), log: [...prev.slice(-49), { t: Date.now(), level, message }] } };
  emit();
}

export function subscribeKaggle(fn) {
  listeners.add(fn);
  fn({ ...state });
  return () => listeners.delete(fn);
}
export function getKaggleState(engine) { return state[engine] || null; }

let runSeq = 0;

// Runs the whole chain for one engine. Safe to call on an already-running server — it checks the
// live status first and skips straight to testing instead of pushing (and disrupting) a server
// that's already serving.
export async function autoStartKaggleServer(engine) {
  const seq = ++runSeq;
  const stillCurrent = () => runSeq === seq;

  patch(engine, { status: "checking", phase: "checking", url: null, detail: null, next_step: null, hint: null, log: [], monitor: null });
  pushLog(engine, "Checking whether a server is already live…");

  let r;
  try {
    r = await api.fetchKaggleUrl(engine);
  } catch (err) {
    patch(engine, { status: "error", detail: String(err) });
    pushLog(engine, `Could not reach the kaggle CLI: ${err}`, "error");
    return;
  }
  if (!stillCurrent()) return;

  // ── Already serving? Skip straight to the connection test. ─────────────
  if (r.ok && r.url) {
    pushLog(engine, `Already running at ${r.url}.`);
    await finishWithTest(engine, r.url, stillCurrent);
    return;
  }

  // ── Not live: push a fresh GPU batch run. ──────────────────────────────
  // A kernel whose tunnel died but that Kaggle still lists as RUNNING keeps holding a GPU slot
  // until the ~9-12 h batch limit; pushing a fresh run for this SAME engine won't free it. Remember
  // that so a gpu_slots_full failure right after can point at the actual culprit.
  const ownZombieSession = r.status === "stale_url";
  pushLog(engine, r.detail || "No live server found — starting a fresh run…");

  // ── Clear the blockers before pushing, not after a push fails ──────────
  // Nothing is serving, so anything still holding this engine's slot is dead weight: a run whose
  // tunnel died but that Kaggle still lists as RUNNING keeps that slot for the full ~9-12 h batch
  // limit, and a stored URL that no longer answers makes every later check "succeed" against a
  // tunnel that is gone. Waiting for a `gpu_slots_full` reply to discover that is the wrong shape —
  // by then somebody has already watched a progress bar do nothing.
  //
  // Safe to run unconditionally: the backend leaves a genuinely live server alone and says so.
  patch(engine, { status: "starting", phase: "starting", detail: "Clearing anything in the way…" });
  try {
    const reset = await api.resetKaggleEngine(engine);
    if (!stillCurrent()) return;
    if (reset?.already_live) {
      // It came up between the check and now — take it rather than tearing it down.
      pushLog(engine, "A server answered while clearing — connecting to it instead.");
      await finishWithTest(engine, reset.url, stillCurrent);
      return;
    }
    if (reset?.cleared?.length) pushLog(engine, reset.detail);
  } catch (err) {
    // A failed clean-up must not stop the start; the push may well succeed anyway.
    pushLog(engine, `Could not fully clear the previous run (${err}) — starting anyway.`, "error");
  }
  if (!stillCurrent()) return;

  patch(engine, { status: "starting", phase: "starting", detail: null });
  let start = await api.startKaggleServer(engine);
  if (!stillCurrent()) return;

  // Said before the eight-minute wait rather than after it. A GPU-less run is not something the
  // notebook can recover from — Kaggle declines the accelerator and it refuses to serve — so the
  // one useful moment to mention an exhausted quota is now.
  if (start?.quota_warning) {
    pushLog(engine, start.quota_warning, "error");
    patch(engine, { hint: "gpu_denied", next_step: HINT_NEXT_STEP.gpu_denied });
  }

  // ── Auto-recover the "own zombie" deadlock ─────────────────────────────
  // If the push failed because this engine's own run is stuck RUNNING with a dead tunnel, don't
  // dead-end asking the user to manually Stop Session (the Kaggle CLI has no stop). Instead push a
  // GPU-off version, which supersedes that one running session and frees its GPU slot, then retry
  // the real GPU start. Fully automatic — this is the roadblock the user hit repeatedly.
  if (!start.ok && start.status === "gpu_slots_full" && ownZombieSession) {
    pushLog(engine, `${engine}'s own run is stuck (dead tunnel) and holding its GPU slot — auto-recovering: ending it, then retrying…`);
    patch(engine, { status: "starting", phase: "starting", detail: "Clearing a stuck session…" });
    try {
      const sup = await api.supersedeKaggleSession(engine);
      pushLog(engine, sup.detail || (sup.ok ? "Stuck session ended." : "Could not end the stuck session."), sup.ok ? "info" : "error");
    } catch (err) {
      pushLog(engine, `Auto-recovery push failed: ${err}`, "error");
    }
    // Give Kaggle a moment to release the slot, then retry the real start (a couple of attempts,
    // since slot release isn't instant).
    for (let i = 0; i < 3 && !start.ok; i++) {
      await sleep(15000);
      if (!stillCurrent()) return;
      pushLog(engine, `Retrying start (attempt ${i + 1}/3)…`);
      start = await api.startKaggleServer(engine);
    }
  }

  if (!start.ok) {
    const stillStuck = start.status === "gpu_slots_full" && ownZombieSession;
    const detail = stillStuck
      ? `Auto-recovery couldn't free ${engine}'s stuck GPU slot in time. It usually frees within a few minutes — press Retry, or as a last resort open the notebook and Stop Session.`
      : start.detail;
    patch(engine, { status: "error", phase: "error", detail, next_step: stillStuck ? "Wait a minute and press Retry — the stuck session is being torn down." : start.next_step });
    pushLog(engine, detail || "Could not start the server.", "error");
    return;
  }
  // Track it so the lifecycle watchdog can shut it down again — a running GPU session burns the
  // free weekly quota whether or not anything is using it.
  markStarted(engine);
  pushLog(engine, start.detail || "Push accepted — booting…");
  patch(engine, { status: "waiting", phase: "queued" });

  // ── Watch it boot via the live monitor (fresh=true clears any prior run's progress). ───
  try {
    await api.kaggleStartMonitor(engine, true);
  } catch (err) {
    pushLog(engine, `Could not start the log monitor: ${err} — falling back to plain polling.`, "error");
  }

  const started = Date.now();
  while (Date.now() - started < POLL_MAX_MS) {
    await sleep(PROGRESS_POLL_MS);
    if (!stillCurrent()) { api.kaggleStopMonitor(engine).catch(() => {}); return; }

    let m;
    try {
      m = await api.kaggleProgress(engine);
    } catch { continue; }
    if (!stillCurrent()) return;

    // Surface the live phase + streaming log to the UI.
    patch(engine, { status: m.phase === "error" ? "error" : "waiting", phase: m.phase, url: m.url || null, monitor: m });

    if (m.phase === "error" || (m.done && !m.url_live)) {
      api.kaggleStopMonitor(engine).catch(() => {});

      // GPU quota is per-account: when Kaggle denies this account a GPU, automatically rotate to the
      // next connected account and retry. Only if none is left do we stop and prompt the user to
      // connect another free account (a global listener in Shell opens the guided step).
      if (m.hint === "gpu_denied") {
        pushLog(engine, "This Kaggle account got no GPU (quota spent) — trying another connected account…", "error");
        let rot = null;
        try { rot = await api.rotateKaggleAccount(); } catch { /* ignore */ }
        if (rot?.ok) {
          pushLog(engine, `Switched to Kaggle account "${rot.username}" — retrying the server start.`);
          return autoStartKaggleServer(engine); // restart the whole flow on the fresh account
        }
        patch(engine, { status: "error", phase: "error", hint: "gpu_denied", needsAccount: true,
          detail: "This Kaggle account is out of free GPU time, and no other account is connected.",
          next_step: HINT_NEXT_STEP.gpu_denied });
        pushLog(engine, "Out of GPU quota — connect another free Kaggle account to keep going.", "error");
        try { window.dispatchEvent(new CustomEvent("bm:kaggle-needs-account", { detail: { engine } })); } catch { /* non-browser */ }
        return;
      }

      const next_step = HINT_NEXT_STEP[m.hint] || "Open the notebook (button above) to see the full log, fix the cause, then Start & connect again.";
      patch(engine, { status: "error", phase: "error", detail: m.error || "The run ended before a server came up.", next_step, hint: m.hint });
      pushLog(engine, m.error || "The run ended before a server came up.", "error");
      return;
    }
    if (m.url && m.url_live) {
      pushLog(engine, "Tunnel is live — testing the connection…");
      await finishWithTest(engine, m.url, stillCurrent);
      api.kaggleStopMonitor(engine).catch(() => {});
      return;
    }
  }

  patch(engine, { status: "error", phase: "error", detail: "Timed out waiting for the Kaggle server to come up.", next_step: "Open the notebook (button above) to check the run, then retry." });
  pushLog(engine, "Timed out waiting for the Kaggle server to come up.", "error");
  api.kaggleStopMonitor(engine).catch(() => {});
}

// Final gate: a live tunnel isn't enough — the engine's own API must answer before we say "ready".
async function finishWithTest(engine, url, stillCurrent) {
  patch(engine, { status: "testing", phase: "testing", url });
  pushLog(engine, "Testing the connection…");
  const testFn = TEST_FN[engine];
  const test = testFn ? await testFn() : { ok: false, detail: `No test defined for engine "${engine}".` };
  if (!stillCurrent()) return;
  if (test.ok) {
    patch(engine, { status: "ready", phase: "ready", url });
    pushLog(engine, "Ready — you can generate now.", "success");
  } else {
    patch(engine, { status: "error", phase: "error", detail: test.detail, url, next_step: test.next_step });
    pushLog(engine, `Connection test failed: ${test.detail || "unknown error"}`, "error");
  }
}
