// AI generation pipeline orchestrator — drives Suno's create page end-to-end for one or many
// songs: stage fields → open the browser tab → sign in if needed → advanced mode + latest
// engine → fill → submit → poll → download to the project/channel folder → hand back to the
// Music Studio tab for preview, then (in bulk) repeat for the next song.
//
// Runs as a plain module-level singleton (not React state) so the promise chain keeps executing
// across route changes — Suno click-automation needs the embedded Browser tab actually mounted
// (native child webviews are positioned over its placeholder stage), so this pipeline can't run
// fully "headless" while the user is on another page. Instead it publishes `needsView` and a
// small always-mounted watcher (see Shell.jsx) navigates the app to the Browser tab while
// automation needs to be visible/interactive (loading the page, signing in) and back to the
// Music Studio tab once a clip is downloaded, so the user always lands where the actual state
// change happened instead of having to go look for it.
//
// Step 0 (stage fields) writes the song's title/styles/lyrics into the named field bucket
// (see macroRecorder.js) for transparency and so a *recorded* macro on some future, not-yet-
// scripted engine site can paste from it — Suno itself is filled directly via sunoComposeScript
// (React-native setters / execCommand insertText), which is far more reliable than simulating a
// clipboard paste for a heavily-scripted SPA.

import { api } from "./api";
import {
  sunoComposeScript, sunoSubmitScript, sunoAudioSnapshotScript, sunoScrapeOnceScript,
  sunoReadyProbeScript, loginProbeScript,
} from "./browserSites";
import { setSongFieldBucket } from "./macroRecorder";

// Suno's own cap is ~80-100 chars depending on model version, but titles that land right at
// that edge have been seen leaving the Create button stuck disabled (Suno's async title
// validation apparently doesn't always clear once it's flagged a too-long value) — keep this
// well under the real limit so a long AI-generated title never silently blocks submission.
const MAX_TITLE_CHARS = 50;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SUNO_PAGE = "suno";
const POLL_INTERVAL_MS = 10000; // "poll every interval (10s)" per spec
const POLL_MAX_MS = 4 * 60 * 1000; // give Suno ~4 minutes to render both clips
const READY_RETRY_ATTEMPTS = 3; // reload-and-retry budget for a page that never hydrates

// ── Pub/sub state ────────────────────────────────────────────────────────────
let state = {
  status: "idle", // idle | running | awaiting-continue | awaiting-login | done | error | cancelled
  needsView: null, // 'browser' | 'preview' | null — which app tab the run currently needs visible
  projectId: null,
  autoAdvance: true,
  queue: [],
  index: -1,
  currentSongId: null,
  currentStep: null,
  log: [],
  results: {}, // songId -> { ok, clips: [{audioUrl, localPath}], reason }
};
const listeners = new Set();

function emit() { listeners.forEach((fn) => { try { fn(state); } catch { /* ignore */ } }); }
function patch(p) { state = { ...state, ...p }; emit(); }
function pushLog(message, level = "info") {
  state = { ...state, log: [...state.log.slice(-299), { t: Date.now(), level, message }] };
  emit();
}

export function subscribePipeline(fn) {
  listeners.add(fn);
  fn(state);
  return () => listeners.delete(fn);
}
export function getPipelineState() { return state; }

let cancelRequested = false;
let continueResolver = null;

export function cancelPipeline() {
  cancelRequested = true;
  if (continueResolver) { continueResolver(); continueResolver = null; }
  patch({ status: "cancelled", needsView: null, currentStep: null });
}

// Bulk runs pause here between songs unless autoAdvance is set — gives the user a moment to
// actually look at what just got downloaded (per spec step 12) before the next song bumps them
// back to the Browser tab.
export function continuePipeline() {
  if (continueResolver) { continueResolver(); continueResolver = null; }
}

function throwIfCancelled() {
  if (cancelRequested) throw new Error("Pipeline cancelled");
}

// ── Low-level page helpers (work whether or not Browser.jsx is currently mounted, once the
//    Suno page has been opened at least once — see needsView/'browser' below) ──

async function evalAndTake(js, key, { settleMs = 800, tries = 6 } = {}) {
  await api.webviewEval(js, SUNO_PAGE);
  for (let i = 0; i < tries; i++) {
    await sleep(settleMs);
    throwIfCancelled();
    const res = await api.webviewTakeScrape(key);
    if (res !== null && res !== undefined) return res;
  }
  return null;
}

async function waitForSunoPageOpen(ms = 30000) {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    throwIfCancelled();
    try { if (await api.webviewCurrentUrl(SUNO_PAGE)) return true; } catch { /* not open yet */ }
    await sleep(400);
  }
  return false;
}

// ── Step implementations ──────────────────────────────────────────────────────

function lyricsAsString(song) {
  if (Array.isArray(song.lyrics)) return song.lyrics.map((l) => `[${l.section}]\n${l.lines}`).join("\n\n");
  return String(song.lyrics || "");
}

async function stageFieldBucket(song) {
  patch({ currentStep: "stage" });
  pushLog(`Staging "${song.title}" into the field bucket.`);
  setSongFieldBucket({ songId: song.id, title: song.title || "", styles: song.styles || "", lyrics: lyricsAsString(song) });
}

// Ensures the Browser tab is visible and the Suno "create" page is open, retrying/reloading a
// few times if the SPA never hydrates the create form — the old flow just gave up after one
// 20s timeout, with no way to tell "still logged out" from "Suno was just slow this once".
//
// This used to reload on essentially every run and never actually finish: Browser.jsx's own
// mount effect already navigates a freshly-opened page to suno.com/create, and this function
// used to *immediately* fire its own redundant navigate right after — cancelling that in-flight
// load before it ever got anywhere. On top of that, sunoReadyProbeScript has its own internal
// timeout (independent of how long we poll for its result) after which it self-reports
// 'not-rendered' even if the page is still genuinely loading — a cold webview (fresh TLS + SPA
// bundle parse) routinely needs longer than the old 6s default, while the user's already-warm
// manual browser tab loads fine. Each "not-rendered" then triggered another reload, restarting
// the clock on a page that just needed more time — a self-inflicted starvation loop.
const SUNO_PROBE_TIMEOUT_MS = 15000; // give a cold webview real time to hydrate before giving up
async function ensureCreatePageReady() {
  patch({ currentStep: "open-browser", needsView: "browser" });
  pushLog("Switching to the Browser tab and loading suno.com/create…");
  if (!(await waitForSunoPageOpen())) throw new Error("The Browser tab never opened the Suno page.");

  patch({ currentStep: "create-page" });
  // Only navigate ourselves if the page isn't already pointed at Suno — avoids cancelling the
  // load Browser.jsx's own mount effect just kicked off for a freshly-opened page.
  const currentUrl = await api.webviewCurrentUrl(SUNO_PAGE).catch(() => null);
  if (!currentUrl || !currentUrl.includes("suno.com")) {
    await api.webviewNavigate("https://suno.com/create", SUNO_PAGE);
  }
  await sleep(3000);

  for (let attempt = 1; attempt <= READY_RETRY_ATTEMPTS; attempt++) {
    throwIfCancelled();
    const probe = await evalAndTake(
      sunoReadyProbeScript({ timeoutMs: SUNO_PROBE_TIMEOUT_MS }),
      "suno:ready",
      { settleMs: 600, tries: Math.ceil(SUNO_PROBE_TIMEOUT_MS / 600) + 5 }
    );
    if (probe?.state === "ready") { pushLog("Suno's create page is ready."); return; }
    if (probe?.state === "auth-wall") {
      pushLog("Suno looks signed out — waiting for you to sign in…", "warn");
      await waitForManualLogin();
      await api.webviewNavigate("https://suno.com/create", SUNO_PAGE);
      await sleep(3000);
      continue; // re-probe after login without burning a reload attempt
    }
    if (attempt === READY_RETRY_ATTEMPTS) break;
    pushLog(`Suno's page hasn't rendered yet (attempt ${attempt}/${READY_RETRY_ATTEMPTS}) — reloading…`, "warn");
    await api.webviewNavigate("https://suno.com/create", SUNO_PAGE);
    await sleep(3000);
  }
  throw new Error("Suno's create page never finished loading after several reloads.");
}

// Blocks until loginProbeScript stops seeing a logged-out hint, or 6 minutes pass. The Browser
// tab stays showing (needsView stays 'browser') the whole time so the user can actually sign in.
async function waitForManualLogin() {
  patch({ status: "awaiting-login" });
  for (let i = 0; i < 90; i++) {
    throwIfCancelled();
    await sleep(4000);
    const res = await evalAndTake(loginProbeScript("suno"), "suno:login", { settleMs: 300, tries: 3 });
    if (res && !res.loggedOutHint) { patch({ status: "running" }); return; }
  }
  throw new Error("Timed out waiting for Suno sign-in.");
}

async function fillAndSubmit(song) {
  patch({ currentStep: "fill-lyrics" });
  const lyrics = lyricsAsString(song);
  const style = song.styles || "";
  const title = (song.title || "").slice(0, MAX_TITLE_CHARS);
  if (!lyrics.trim()) throw new Error("This song has no lyrics yet.");

  pushLog("Filling lyrics, style and title…");
  const filled = await evalAndTake(sunoComposeScript({ lyrics, style, title }), "suno:compose", { settleMs: 1000, tries: 30 });
  if (!filled?.filledLyrics) {
    throw new Error(filled?.reason ? `Couldn't fill Suno: ${filled.reason}` : "Couldn't write into Suno's lyrics field.");
  }
  patch({ currentStep: "fill-styles" });
  if (!filled.filledStyle && style) pushLog("Filled lyrics, but not the style field — continuing anyway.", "warn");
  patch({ currentStep: "fill-title" });
  // Suno's own field can silently reject/truncate a title it doesn't like (its async validity
  // check doesn't always recover once it's flagged one), which then shows up downstream as the
  // Create button staying stuck disabled — surface it here instead of only at the submit step.
  if (title && (!filled.titleField || filled.titleField.chars < title.length)) {
    pushLog(`Title field only kept ${filled.titleField?.chars ?? 0}/${title.length} chars — if Create won't submit, this is likely why.`, "warn");
  }

  const known = (await evalAndTake(sunoAudioSnapshotScript(), "suno:snapshot", { settleMs: 200, tries: 3 })) || [];

  patch({ currentStep: "submit" });
  await sleep(600);
  // The script itself now retries internally (disabled-button re-checks + a post-click
  // verification pass) before it ever reports back — budget enough settle time to outlast that,
  // not just one quick read.
  const submitted = await evalAndTake(sunoSubmitScript(), "suno:submit", { settleMs: 500, tries: 16 });
  if (!submitted?.clicked) {
    throw new Error(`Filled the fields but didn't submit: ${submitted?.reason || "no answer from the page"}.`);
  }
  if (submitted.verified === false) {
    pushLog(`Clicked "${submitted.label}" but Suno gave no visible sign it started — waiting for a clip anyway, but check the Browser tab if this times out.`, "warn");
  } else {
    pushLog("Submitted to Suno — waiting for clips to finish (usually 30-90s)…");
  }
  return known;
}

// Polls at a 10s cadence, re-injecting a fresh single-shot scan script every tick (see
// sunoScrapeOnceScript) instead of firing one long-lived in-page loop and waiting on it — a
// script's own setTimeout loop dies silently the moment the page it's running in navigates,
// which used to make a run that actually succeeded on Suno's side read as "timed out" here.
const WANT_CLIPS = 2; // Suno renders two variants per run
async function pollForClips(known) {
  patch({ currentStep: "poll" });
  const started = Date.now();
  let best = [];
  while (Date.now() - started < POLL_MAX_MS) {
    throwIfCancelled();
    await sleep(POLL_INTERVAL_MS);
    const res = await evalAndTake(sunoScrapeOnceScript(known), "suno:scan", { settleMs: 400, tries: 5 });
    if (res?.failed) throw new Error("Suno reported the generation failed (out of credits, or the run errored out).");
    if (res?.clips?.length > best.length) best = res.clips;

    const elapsed = Date.now() - started;
    if (best.length >= WANT_CLIPS) {
      pushLog(`${best.length} clips finished.`);
      return best;
    }
    if (best.length > 0 && elapsed > POLL_MAX_MS * 0.6) {
      pushLog(`${best.length} clip finished — the other didn't render in time, continuing with what's there.`);
      return best;
    }
    pushLog(`Still waiting on Suno… (${Math.round(elapsed / 1000)}s)`);
  }
  if (best.length) return best;
  throw new Error("Timed out waiting for Suno to finish rendering.");
}

function slugFilename(title, suffix) {
  const safe = (title || "song").replace(/[^a-zA-Z0-9\s-_]/g, "_").trim() || "song";
  return `${safe}${suffix}.mp3`;
}

async function downloadClips(song, clips, { projectId, channelId }) {
  patch({ currentStep: "download" });
  const saved = [];
  for (let i = 0; i < clips.length; i++) {
    throwIfCancelled();
    const clip = clips[i];
    const filename = slugFilename(song.title, i === 0 ? "" : `-alt${i > 1 ? i : ""}`);
    pushLog(`Downloading clip ${i + 1}/${clips.length}…`);
    try {
      const localPath = await api.saveGeneratedAsset(clip.audioUrl, projectId, channelId, filename, "mp3");
      saved.push({ ...clip, localPath });
    } catch (e) {
      pushLog(`Couldn't save clip ${i + 1}: ${e}`, "warn");
      saved.push({ ...clip, localPath: null });
    }
  }

  const patchBody = { channel_id: channelId || null };
  if (saved[0]) { patchBody.audio_url = saved[0].audioUrl; if (saved[0].duration) patchBody.duration = saved[0].duration; patchBody.audio_url_primary = saved[0].audioUrl; if (saved[0].duration) patchBody.duration_primary = saved[0].duration; if (saved[0].localPath) patchBody.local_audio_path = saved[0].localPath; }
  if (saved[1]) { patchBody.audio_url_alt = saved[1].audioUrl; if (saved[1].duration) patchBody.duration_alt = saved[1].duration; if (saved[1].localPath) patchBody.local_audio_path_alt = saved[1].localPath; }
  await api.updateSong(song.id, patchBody);
  return saved;
}

// ── Single-song run ────────────────────────────────────────────────────────────

async function runOneSong(song, { projectId, channelId }) {
  patch({ currentSongId: song.id, status: "running" });
  try {
    await stageFieldBucket(song);
    await ensureCreatePageReady();
    const known = await fillAndSubmit(song);
    const clips = await pollForClips(known);
    const saved = await downloadClips(song, clips, { projectId, channelId });
    state = { ...state, results: { ...state.results, [song.id]: { ok: true, clips: saved } } };
    pushLog(`"${song.title}" done — ${saved.filter((s) => s.localPath).length}/${saved.length} clip(s) saved to disk.`, "success");
    return true;
  } catch (e) {
    const reason = e?.message || String(e);
    state = { ...state, results: { ...state.results, [song.id]: { ok: false, reason } } };
    pushLog(`"${song.title}" failed: ${reason}`, "error");
    return false;
  }
}

// ── Queue driver ────────────────────────────────────────────────────────────────

/**
 * Run the AI generation pipeline for one or more songs, in order. Suno only ever runs one song
 * at a time in the browser tab, so bulk is a sequential loop, not parallel jobs.
 *
 * @param songs        array of song objects (already loaded, with lyrics/styles/title)
 * @param opts.projectId    project the songs belong to (for the download folder)
 * @param opts.channelId    explicit destination channel, or a per-song resolver (song) => channelId | null
 * @param opts.autoAdvance  when false, pauses after each song's preview until continuePipeline() is called
 */
export async function runPipeline(songs, { projectId, channelId, autoAdvance = true } = {}) {
  if (!songs?.length) return;
  cancelRequested = false;
  patch({
    status: "running", projectId, autoAdvance,
    queue: songs.map((s) => s.id), index: -1, currentSongId: null, currentStep: null,
    log: [], results: {}, needsView: "browser",
  });

  for (let i = 0; i < songs.length; i++) {
    if (cancelRequested) break;
    const song = songs[i];
    patch({ index: i });
    const resolvedChannelId = typeof channelId === "function" ? channelId(song) : channelId;
    await runOneSong(song, { projectId, channelId: resolvedChannelId });
    if (cancelRequested) break;

    // Step 12: hand back to the AI generation tab so the just-downloaded clip is right there
    // for preview / subsequent stages (analysis, video generation).
    patch({ needsView: "preview", currentStep: "preview" });

    const isLast = i === songs.length - 1;
    if (!isLast && !autoAdvance) {
      patch({ status: "awaiting-continue" });
      await new Promise((resolve) => { continueResolver = resolve; });
      if (cancelRequested) break;
      patch({ status: "running", needsView: "browser" });
    } else if (!isLast) {
      await sleep(1500); // brief look before bouncing back for the next song
      patch({ needsView: "browser" });
    }
  }

  if (!cancelRequested) patch({ status: "done", currentStep: null, needsView: "preview" });
}

// Channel auto-suggestion: mirrors the server's own case-insensitive language match
// (uploads.rs::bulk_uploads_from_videos) so the pipeline's pre-picked channel agrees with what
// Upload's bulk flow would later match anyway.
export function suggestChannelForSong(song, channels) {
  if (!song || !channels?.length) return null;
  const lang = (song.language || "").toLowerCase();
  return channels.find((c) => (c.language || "").toLowerCase() === lang) || null;
}
