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
  tunnel_dead: "The notebook printed a tunnel URL that never answered, and the run has since ended. Press Start & connect to launch a fresh one.",
  tunnel_slow: "The server is up and its tunnel is open — this computer just cannot route the brand-new trycloudflare address yet, which can take several minutes. The address is already saved, so press Test connection in a few minutes. Do not press Start & connect: that would throw away a working GPU run and the replacement address would be just as new.",
  gpu_denied:
    "Kaggle ran the notebook on CPU instead of a GPU, so it could not serve. Your weekly Kaggle GPU quota (30 h) is most likely used up — it resets Saturdays UTC. Check kaggle.com/settings → Accelerator usage. Until it resets, either wait, or run the engine on another free GPU host (Colab / Lightning.ai) and paste its URL into this engine's server-URL field.",
  gpu_unavailable:
    "Kaggle had no free GPU for this run, so the notebook ran on CPU and could not serve. This is not your quota — that still has hours on it — and connecting another account would not help, because the shortage is Kaggle's. Free T4s come and go through the day; press Start & connect again in a few minutes.",
};

// How many times to re-push by ourselves when Kaggle has no free GPU but the quota is fine, and
// how long to wait before the first retry (it backs off from there). Two is deliberate: it covers
// the usual case of one busy moment without quietly burning an afternoon of pushes if the free
// tier is genuinely saturated.
const GPU_UNAVAILABLE_RETRIES = 2;
const GPU_RETRY_WAIT_MS = 60_000;

// How patiently to keep testing a server whose tunnel address this machine cannot route yet.
// Ten minutes on top of the monitor's own twelve: the one measured case became reachable about ten
// minutes after the address was printed, and none of this costs anything — the run is already up.
const TUNNEL_SLOW_CHECKS = 10;
const TUNNEL_SLOW_CHECK_MS = 60_000;

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
export async function autoStartKaggleServer(engine, { gpuRetries = 0 } = {}) {
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

      // Kaggle declined a GPU while this account still has hours left, so the quota is not the
      // problem and neither rotating accounts nor asking the user to connect one would help — the
      // free T4s were simply all taken for that minute. Waiting a little and pushing again is the
      // only thing that actually works, so do it here instead of ending the run with advice the
      // user cannot act on. Bounded, because if Kaggle stays full this has to stop and say so.
      if (m.hint === "gpu_unavailable") {
        if (gpuRetries < GPU_UNAVAILABLE_RETRIES) {
          const wait = GPU_RETRY_WAIT_MS * (gpuRetries + 1); // back off: 60s, then 120s
          pushLog(engine, `${m.error || "Kaggle had no free GPU for this run."} Waiting ${Math.round(wait / 1000)}s and trying again (${gpuRetries + 1}/${GPU_UNAVAILABLE_RETRIES})…`, "error");
          patch(engine, { status: "waiting", phase: "queued", detail: "Kaggle had no free GPU — waiting to try again…" });
          await sleep(wait);
          if (!stillCurrent()) return;
          return autoStartKaggleServer(engine, { gpuRetries: gpuRetries + 1 });
        }
        patch(engine, { status: "error", phase: "error", hint: "gpu_unavailable",
          detail: m.error || "Kaggle had no free GPU for this run.",
          next_step: HINT_NEXT_STEP.gpu_unavailable });
        pushLog(engine, `Kaggle had no free GPU on ${GPU_UNAVAILABLE_RETRIES + 1} attempts — the quota is fine, the machines are busy. Try again in a while.`, "error");
        return;
      }

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

      // A server that is up behind an address this machine cannot route yet is not a failed start,
      // and must not be dressed as one — the red panel is what talked people into pressing Start &
      // connect again, discarding a healthy GPU run for a replacement address that would be exactly
      // as new. The URL is already saved by the monitor, so the only thing left to do is wait.
      if (m.hint === "tunnel_slow") {
        patch(engine, { status: "waiting", phase: "tunneling", url: m.url || null, hint: "tunnel_slow",
          detail: m.error, next_step: HINT_NEXT_STEP.tunnel_slow });
        pushLog(engine, m.error || "The tunnel is open but not routable from here yet.", "info");
        // And keep checking, instead of leaving somebody to remember to come back and press a
        // button. The run is already paid for and the address usually starts routing within a few
        // minutes, so the honest end to this is "ready", reached on its own.
        for (let i = 0; i < TUNNEL_SLOW_CHECKS; i++) {
          await sleep(TUNNEL_SLOW_CHECK_MS);
          if (!stillCurrent()) return;
          // Re-discover rather than only re-testing the address we already have. The notebook keeps
          // its own tunnel attempt chain running, so the run may by now be answering on a different
          // hostname entirely; fetchKaggleUrl reads the current one out of the log and only reports
          // ok once it has actually answered, which covers both "the old address started routing"
          // and "the notebook moved to a new one".
          let found = null;
          try { found = await api.fetchKaggleUrl(engine); } catch { /* keep waiting */ }
          if (!stillCurrent()) return;
          if (found?.ok && found.url) {
            pushLog(engine, `Reachable now at ${found.url} — testing…`);
            await finishWithTest(engine, found.url, stillCurrent);
            return;
          }
          pushLog(engine, `Still not routable from here (check ${i + 1}/${TUNNEL_SLOW_CHECKS})…`);
        }
        pushLog(engine, "Still not routable after waiting. The run is up regardless — press Test connection later.", "error");
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
