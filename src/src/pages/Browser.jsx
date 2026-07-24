import { useState, useEffect, useLayoutEffect, useRef, useCallback } from "react";
import { useSearchParams, useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Badge } from "../components/ui/badge";
import {
  Globe, RotateCw, ExternalLink, Play, LogIn, ListTree, HeartPulse,
  Image as ImageIcon, X, Plus, Wand2, CircleDot, Square, Trash2, ChevronDown,
  ClipboardPaste, Plug, Cookie,
} from "lucide-react";
import { toast } from "sonner";
import {
  buildSites, sunoComposeScript, sunoSubmitScript, sunoAudioSnapshotScript, sunoScrapeResultScript,
  loginProbeScript, midjourneyImagineScript, midjourneyScrapeJobsScript,
  youtubeListChannelsScript, youtubeSwitchChannelScript, engineHealthScript,
  GOOGLE_LOGIN_URL, KAGGLE_NOTEBOOK,
} from "../lib/browserSites";
import {
  loadMacros, saveMacro, deleteMacro, tidySteps, describeStep, playMacro,
  getVirtualClipboard, setVirtualClipboard, CONNECTORS, recordSpecialStepScript,
  getClipboardQueue, pushClipboardEntries, clearClipboardQueue,
} from "../lib/macroRecorder";

const PAGES_KEY = "studio:browser-pages";
const GOOGLE_PAGE_ID = "google-auth";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function loadStoredPages() {
  try {
    const raw = localStorage.getItem(PAGES_KEY);
    const list = raw ? JSON.parse(raw) : [];
    return Array.isArray(list) && list.length ? list : null;
  } catch {
    return null;
  }
}

function labelForUrl(url) {
  try { return new URL(url).hostname.replace(/^www\./, ""); } catch { return url; }
}

// An engine site whose URL is still the Kaggle notebook has no live tunnel saved in Settings —
// opening it lands on the notebook page rather than the engine, which is worth knowing *before*
// you click.
const KAGGLE_URLS = new Set(Object.values(KAGGLE_NOTEBOOK));
const isNotebookFallback = (site) => KAGGLE_URLS.has(site.url);

// Run a script in a page (the active one unless `pageId` says otherwise), then poll the scrape
// bucket for its result key. Scripts post their result whenever they're done, so polling —
// rather than a fixed delay — is what keeps a slow page from reading as a failure.
async function evalAndTake(js, key, { settleMs = 800, tries = 6, pageId } = {}) {
  await api.webviewEval(js, pageId);
  for (let i = 0; i < tries; i++) {
    await sleep(settleMs);
    const res = await api.webviewTakeScrape(key);
    if (res !== null && res !== undefined) return res;
  }
  return null;
}

// Wait until a page's webview exists and has loaded a URL. Deep links (?site=…&auto=…) start
// the automation and the page open at the same time; evaluating into a page that isn't open
// yet just errors, which the old code hid behind a fixed sleep.
async function waitForPage(pageId, ms = 20000) {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    try { if (await api.webviewCurrentUrl(pageId)) return true; } catch { /* not open yet */ }
    await sleep(300);
  }
  return false;
}

// The native webviews are positioned over `stageRef` by the backend — one child webview per
// open page (tab), only the active one visible. Everything above the stage (tabs + toolbar)
// is normal React; the stage is a transparent placeholder we just measure.
export default function Browser() {
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const [sites, setSites] = useState(() => buildSites({}));

  // ── Pages (tabs) ──
  const [pages, setPages] = useState(() =>
    loadStoredPages() || [{ id: "youtube", url: "https://studio.youtube.com/", label: "YouTube Studio", siteKey: "youtube" }],
  );
  const [activePageId, setActivePageId] = useState(null);
  const [urlInput, setUrlInput] = useState("");
  const [presetsOpen, setPresetsOpen] = useState(false);
  const presetsRef = useRef(null);

  // ── Automation / results ──
  const [lastResult, setLastResult] = useState(null); // { label, data } from the latest automation
  const [actionInput, setActionInput] = useState(""); // MJ prompt / YT channel name

  // ── Macro recorder ──
  const [macroOpen, setMacroOpen] = useState(false);
  const [macros, setMacros] = useState(() => loadMacros());
  const [recording, setRecording] = useState(false);
  const [recSteps, setRecSteps] = useState(0);
  const [pendingSteps, setPendingSteps] = useState(null); // steps awaiting a name to save
  const [macroName, setMacroName] = useState("");
  const [selectedMacroId, setSelectedMacroId] = useState("");
  const [macroParam, setMacroParam] = useState(""); // per-run paste/clipboard content
  const [listMode, setListMode] = useState("recorded"); // recorded | fromEnd | first | last
  const [playing, setPlaying] = useState(false);
  const [playNote, setPlayNote] = useState("");
  const [clipText, setClipText] = useState(() => getVirtualClipboard()); // in-app pseudo clipboard bucket
  const [connectorChoice, setConnectorChoice] = useState(CONNECTORS[0].id);
  const [bucketOpen, setBucketOpen] = useState(false);
  const [bucketQueue, setBucketQueue] = useState(() => getClipboardQueue()); // multi-entry paste queue
  const [bucketDraft, setBucketDraft] = useState("");
  const [stopOnFailure, setStopOnFailure] = useState(false);
  const [repeatCount, setRepeatCount] = useState(1); // replay the selected macro this many times in a row

  const stageRef = useRef(null);
  const openedRef = useRef(false);
  const pagesRef = useRef(pages);
  pagesRef.current = pages;

  const activePage = pages.find((p) => p.id === activePageId) || null;
  const activeSiteKey = activePage?.siteKey || null;
  const activeSite = sites.find((s) => s.key === activeSiteKey) || null;

  // Native child webviews always paint above the main webview, so any React overlay that would
  // extend over the stage (dropdowns, modals) is invisible behind it — z-index doesn't cross
  // that boundary. Suspending hides the active page for as long as an overlay is open.
  const [suspended, setSuspended] = useState(false);
  const [stageReady, setStageReady] = useState(false);

  const rectOf = useCallback(() => {
    const el = stageRef.current;
    if (!el) return null;
    const r = el.getBoundingClientRect();
    // A zero-size rect means the stage isn't laid out yet (first paint / hidden tab); placing
    // the webview on it would collapse it to 1x1 in the corner.
    if (r.width < 2 || r.height < 2) return null;
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }, []);

  // Load settings so engine URLs (ACE-Step/HeartMuLa/ComfyUI tunnels) resolve.
  useEffect(() => {
    (async () => {
      try {
        const s = await api.getSettings();
        setSites(buildSites(s || {}));
      } catch { /* keep defaults */ }
    })();
  }, []);

  // Persist the page list.
  useEffect(() => {
    try { localStorage.setItem(PAGES_KEY, JSON.stringify(pages)); } catch { /* ignore */ }
  }, [pages]);

  // ── Page management ──
  // Show a tab, letting the backend create its webview at `page.url` if it doesn't exist yet
  // (cold start, tab restored from localStorage). An already-open page is only shown — never
  // navigated — so its DOM and login state survive tab switches.
  const showPage = useCallback(async (page) => {
    if (!page) return;
    const rect = rectOf();
    if (!rect) return;
    setActivePageId(page.id);
    setUrlInput(page.url);
    try {
      await api.webviewShowPage(page.id, rect, page.url);
      openedRef.current = true;
    } catch (e) {
      console.error(e);
      toast.error("Could not open the embedded browser: " + e);
    }
  }, [rectOf]);

  const addPage = useCallback(async (url, { label, siteKey, id } = {}) => {
    const full = /^https?:\/\//i.test(url) ? url : `https://${url}`;
    const pageId = id || `page-${Date.now().toString(36)}`;
    const existing = pagesRef.current.find((p) => p.id === pageId);
    const page = existing || { id: pageId, url: full, label: label || labelForUrl(full), siteKey: siteKey || null };
    if (!existing) setPages((prev) => [...prev, page]);
    setActivePageId(page.id);
    setUrlInput(page.url);
    const rect = rectOf();
    if (!rect) return page;
    try {
      await api.webviewOpen(page.url, rect, page.id);
      openedRef.current = true;
    } catch (e) {
      console.error(e);
      toast.error("Could not open the embedded browser: " + e);
    }
    return page;
  }, [rectOf]);

  const closePage = useCallback(async (pageId) => {
    try { await api.webviewClosePage(pageId); } catch { /* backend may not have it */ }
    setPages((prev) => {
      const idx = prev.findIndex((p) => p.id === pageId);
      const next = prev.filter((p) => p.id !== pageId);
      if (pageId === activePageId) {
        const fallback = next[Math.max(0, idx - 1)];
        if (fallback) setTimeout(() => showPage(fallback), 30);
        else { setActivePageId(null); setUrlInput(""); }
      }
      return next;
    });
  }, [activePageId, showPage]);

  const openPreset = useCallback(async (siteKey) => {
    const site = sites.find((s) => s.key === siteKey);
    if (!site) return;
    setParams((prev) => { const p = new URLSearchParams(prev); p.set("site", siteKey); return p; });
    const existing = pagesRef.current.find((p) => p.id === siteKey);
    if (existing) await showPage(existing);
    else await addPage(site.url, { label: site.label, siteKey, id: siteKey });
  }, [sites, addPage, showPage, setParams]);

  // Keep the active webview glued to the placeholder as the layout changes — sidebar
  // collapse, window resize, toolbar rows appearing/disappearing. `stageReady` flips once the
  // stage has a real measured size, which is the earliest we can place a webview correctly.
  useLayoutEffect(() => {
    const sync = () => {
      const rect = rectOf();
      if (!rect) return;
      setStageReady(true);
      if (openedRef.current) api.webviewSetRect(rect).catch(() => {});
    };
    sync();
    const ro = new ResizeObserver(sync);
    if (stageRef.current) ro.observe(stageRef.current);
    window.addEventListener("resize", sync);
    return () => { ro.disconnect(); window.removeEventListener("resize", sync); };
  }, [rectOf]);

  // Open the initial page once the stage is measurable (deep link ?site=… wins).
  const bootedRef = useRef(false);
  useEffect(() => {
    if (!stageReady || bootedRef.current) return;
    bootedRef.current = true;
    const siteParam = params.get("site");
    // `?url=` opens an arbitrary address in-app, so links elsewhere in the UI (e.g. "Open
    // notebook") can stay inside the app instead of launching an external browser window.
    const urlParam = params.get("url");
    if (params.get("auto") === "login") return; // the login flow opens its own pages
    if (urlParam) {
      const label = params.get("label") || (() => {
        try { return new URL(urlParam).hostname.replace(/^www\./, ""); } catch { return "Page"; }
      })();
      addPage(urlParam, { label });
    }
    else if (siteParam) openPreset(siteParam);
    else if (pagesRef.current.length) showPage(pagesRef.current[0]);
    else openPreset("youtube");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stageReady]);

  // Leaving the Browser tab hides every page (they keep their session/DOM state).
  useEffect(() => {
    return () => { api.webviewHide().catch(() => {}); };
  }, []);

  // Hide the active page while a React overlay is open, then put it back. Keyed off the
  // suspend transition only — showPage already handles tab switches, and re-showing here on
  // every activePage change would fight it.
  const activePageRef = useRef(activePage);
  activePageRef.current = activePage;
  useEffect(() => {
    if (!openedRef.current) return;
    if (suspended) return void api.webviewHide().catch(() => {});
    const page = activePageRef.current;
    const rect = rectOf();
    if (page && rect) api.webviewShowPage(page.id, rect, page.url).catch(() => {});
  }, [suspended, rectOf]);

  // Close the presets dropdown on outside click.
  useEffect(() => {
    if (!presetsOpen) return;
    const handler = (e) => { if (presetsRef.current && !presetsRef.current.contains(e.target)) setPresetsOpen(false); };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [presetsOpen]);

  // The presets list is tall enough to reach over the stage, where the native webview would
  // paint on top of it.
  useEffect(() => { setSuspended(presetsOpen); }, [presetsOpen]);

  const go = async (url) => {
    if (!url || !activePage) return;
    const full = /^https?:\/\//i.test(url) ? url : `https://${url}`;
    try {
      await api.webviewNavigate(full, activePage.id);
      setPages((prev) => prev.map((p) => (p.id === activePage.id ? { ...p, url: full } : p)));
    } catch (e) { toast.error(String(e)); }
  };

  const reload = async () => { if (activePage) await api.webviewNavigate(urlInput || activePage.url, activePage.id); };

  // ── Suno automation ───────────────────────────────────────────
  // Poll the Suno page for a freshly-finished clip and save its URL onto the song — that's the
  // one thing MusicGen's player/Download button and Analysis both actually key off
  // (`song.audio_url`), so this alone unblocks preview and Analysis immediately. Saving an mp3
  // to disk needs a native Save-file dialog (download_and_convert_audio), which we deliberately
  // don't fire automatically here — it'd pop up unprompted minutes after the user's last click,
  // possibly while they're on another tab; they trigger that explicitly via Download in MusicGen.
  // `known` (audio URLs already on the page before generation started) lets the caller tell a
  // brand-new clip apart from older ones; pass [] to just grab whatever's already there.
  const captureSunoResult = useCallback(async (song, known = []) => {
    const result = await evalAndTake(
      sunoScrapeResultScript(known),
      "suno:result",
      { settleMs: 3000, tries: 75, pageId: "suno" }, // outlives the script's own ~210s internal timeout
    );
    setLastResult({ label: "Suno result", data: result });
    if (!result?.audioUrl) {
      toast.error(result?.reason || "Didn't see a finished clip in the webview. Once it's done, click \"Capture result\" to try again.", { duration: 9000 });
      return;
    }
    try {
      await api.updateSong(song.id, { audio_url: result.audioUrl, duration: result.duration || undefined });
      toast.success("Suno clip captured — it's ready to preview and analyze; use Download in Music Generation to save the mp3.");
    } catch (e) {
      toast.error(`Captured the clip but couldn't save it to the song: ${e}`);
    }
  }, []);

  // The compose script waits for Suno's SPA to render and posts its result when it's actually
  // done, so we poll for that result rather than guessing a fixed delay — the old fixed
  // 1200ms read reported "couldn't find the lyrics field" whenever the page was merely slow.
  const runSunoAutomation = useCallback(async (song) => {
    if (!song) return;
    const lyrics = Array.isArray(song.lyrics)
      ? song.lyrics.map((l) => `[${l.section}]\n${l.lines}`).join("\n\n")
      : String(song.lyrics || "");
    const style = song.styles || "";
    const title = song.title || "";
    if (!lyrics.trim()) return toast.error("This song has no lyrics yet.");
    if (!(await waitForPage("suno"))) return toast.error("Suno's page didn't open — open the Suno tab, then Run again.");

    toast.message("Filling Suno with the song…");
    const filled = await evalAndTake(
      sunoComposeScript({ lyrics, style, title }),
      "suno:compose",
      { settleMs: 1000, tries: 30, pageId: "suno" }, // the script's own wait for the fields is ~20s
    );
    setLastResult({ label: "Suno compose", data: filled });

    if (!filled) {
      toast.error("Suno's page never answered — reload it and try Run again.");
      return;
    }
    if (!filled.filledLyrics) {
      toast.error(
        filled.reason
          ? `Couldn't fill Suno: ${filled.reason}. Open the Custom tab on the page, then Run again — the results panel lists the fields that were found.`
          : "Couldn't write into Suno's lyrics field — see the results panel.",
        { duration: 9000 },
      );
      return;
    }
    if (!filled.filledStyle && style) toast.warning("Filled the lyrics, but not the style field — check it before creating.");

    // Snapshot whatever finished clips are already on the page *before* submitting, so the
    // post-submit poll below can tell "the clip that just finished" apart from older ones
    // already sitting in Suno's history list.
    const known = (await evalAndTake(sunoAudioSnapshotScript(), "suno:snapshot", { settleMs: 200, tries: 3, pageId: "suno" })) || [];

    await sleep(600);
    const submitted = await evalAndTake(sunoSubmitScript(), "suno:submit", { settleMs: 500, tries: 16, pageId: "suno" });
    setLastResult({ label: "Suno submit", data: submitted });
    if (!submitted?.clicked) {
      toast.error(`Filled the fields but didn't submit: ${submitted?.reason || "no answer from the page"}. Click Create yourself.`, { duration: 9000 });
      return;
    }
    if (submitted.verified === false) {
      toast.warning(`Clicked "${submitted.label}" but Suno gave no visible sign it started — waiting for a clip anyway.`, { duration: 7000 });
    } else {
      toast.success("Submitted to Suno — waiting for the clip to finish (usually 30-90s)…");
    }
    await captureSunoResult(song, known);
  }, [captureSunoResult]);

  // ── Midjourney automation ─────────────────────────────────────
  const runMidjourneyImagine = useCallback(async (prompt) => {
    if (!prompt?.trim()) return toast.error("Enter a prompt first.");
    if (!(await waitForPage("midjourney"))) return toast.error("Midjourney's page didn't open — open the Midjourney tab, then try again.");
    toast.message("Submitting /imagine to Midjourney…");
    await sleep(2000); // let the SPA render its prompt bar
    const res = await evalAndTake(midjourneyImagineScript(prompt), "midjourney:imagine", { pageId: "midjourney" });
    setLastResult({ label: "Midjourney imagine", data: res });
    if (res?.ok) toast.success("Prompt submitted — watch the grid for results.");
    else toast.error(res?.reason || "Couldn't find the prompt bar — are you logged in?");
  }, []);

  const runMidjourneyScrape = useCallback(async () => {
    const res = await evalAndTake(midjourneyScrapeJobsScript(), "midjourney:jobs");
    setLastResult({ label: "Midjourney images", data: res });
    if (res?.count) toast.success(`Scraped ${res.count} image URLs (see results panel).`);
    else toast.warning("No generated images visible on the page.");
  }, []);

  // ── YouTube automation ────────────────────────────────────────
  // Scrapes every channel card's @handle from the channel switcher page (see
  // youtubeListChannelsScript), then hands them straight to the Channel Manager: it adds them
  // as discovery tags, auto-runs "Discover channels", and auto-imports whatever that finds —
  // so one click on this page ends with the channels sitting in the Channel Manager.
  const runYoutubeList = useCallback(async () => {
    const res = await evalAndTake(youtubeListChannelsScript(), "youtube:channels");
    setLastResult({ label: "YouTube channels", data: res });
    const found = Array.isArray(res?.channels) ? res.channels : [];
    const handles = [...new Set(found.map((c) => c.handle).filter(Boolean))];
    if (!handles.length) {
      toast.warning("No channels found — open youtube.com/channel_switcher and sign in first.");
      return;
    }
    toast.success(`Found ${handles.length} channel${handles.length > 1 ? "s" : ""} — sending to Channel Manager…`);
    const qs = new URLSearchParams({
      importHandles: handles.join(","),
      autoDiscover: "1",
      autoImport: "1",
    });
    navigate(`/channels?${qs.toString()}`);
  }, [navigate]);

  const runYoutubeSwitch = useCallback(async (name) => {
    if (!name?.trim()) return toast.error("Enter a channel name.");
    const res = await evalAndTake(youtubeSwitchChannelScript(name), "youtube:switch");
    setLastResult({ label: "YouTube switch", data: res });
    if (res?.ok) toast.success(`Switched to "${name}".`);
    else toast.error(res?.reason || "Channel not found on this page.");
  }, []);

  // ── Engine health (ACE-Step / HeartMuLa / ComfyUI tunnels) ────
  const runEngineHealth = useCallback(async () => {
    if (!activeSiteKey) return;
    const res = await evalAndTake(engineHealthScript(activeSiteKey), `${activeSiteKey}:health`);
    setLastResult({ label: `${activeSiteKey} health`, data: res });
    if (res?.ok) toast.success("Engine answered — server is alive.");
    else toast.error("Engine did not answer: " + (res?.error || "no response") + ". Start/fetch it in Settings.");
  }, [activeSiteKey]);

  // Auto-run when arriving from a Generate button:
  //   /browser?site=suno&songId=…&auto=compose        (MusicGen)
  //   /browser?site=midjourney&auto=imagine&prompt=…  (Images)
  //   /browser?site=youtube&auto=switch&channel=…
  // Gated on `stageReady`: the boot effect opens the deep-linked page, and it can't do that
  // until the stage is measurable, so starting earlier would just race it.
  useEffect(() => {
    const auto = params.get("auto");
    if (!auto || !stageReady) return;
    const siteParam = params.get("site");
    (async () => {
      try {
        if (auto === "login") {
          await runLoginFlow(siteParam || "youtube");
        } else if (auto === "compose" && siteParam === "suno" && params.get("songId")) {
          await runSunoAutomation(await api.getSong(params.get("songId")));
        } else if (auto === "imagine" && siteParam === "midjourney" && params.get("prompt")) {
          setActionInput(params.get("prompt"));
          await runMidjourneyImagine(params.get("prompt"));
        } else if (auto === "switch" && siteParam === "youtube" && params.get("channel")) {
          if (!(await waitForPage("youtube"))) return toast.error("YouTube's page didn't open.");
          await sleep(2500); // let the channel list render
          await runYoutubeSwitch(params.get("channel"));
        }
      } catch (e) { console.error(e); }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params, stageReady]);

  // ── Engine session login flow (Suno / Midjourney / YouTube …) ─────────────
  // Sessions live inside the embedded webview; all pages share one cookie store, so a
  // Google session established once unlocks "Continue with Google" on every engine.
  // Flow: ensure a Google session first (open accounts.google.com and, if needed, wait
  // for the user to sign in — signed-in visitors get redirected to myaccount.google.com),
  // then open the engine's own login page in its page/tab for the social login.
  const loginFlowSeq = useRef(0);

  const googleLoggedIn = useCallback(async () => {
    try {
      const url = await api.webviewCurrentUrl(GOOGLE_PAGE_ID);
      return /myaccount\.google\.com/.test(url || "");
    } catch { return false; }
  }, []);

  // After an engine login opens, watch the webview's cookie store and capture the session
  // automatically the moment auth cookies appear: Suno's cookie lands in settings
  // (`suno_cookie`), Midjourney's cookies get injected into the Playwright profile the
  // remote-chrome automation scripts drive. No manual "capture" step needed.
  // The Kaggle engine sites need no browser login at all — the Start/Fetch flow authenticates via
  // the kaggle CLI's ~/.kaggle/kaggle.json token. runLoginFlow short-circuits these to the system
  // browser (opened only to set a notebook secret like flux's HF_TOKEN, or to free a stuck GPU slot).
  const KAGGLE_SITE_KEYS = ["acestep", "heartmula", "comfyui", "flux"];

  const watchAndCapture = useCallback(async (siteKey, seq) => {
    if (!["suno", "midjourney"].includes(siteKey)) return;
    for (let i = 0; i < 60; i++) { // ~6 minutes
      await sleep(6000);
      if (loginFlowSeq.current !== seq || !stageRef.current) return;
      try {
        if (siteKey === "suno") {
          const res = await api.webviewCaptureSunoSession();
          if (res?.ok) {
            toast.success(`Suno session captured (${res.marker}) and stored — API access is live.`);
            return;
          }
        } else {
          const probe = await api.webviewCaptureMjSession(true);
          if (probe?.ok) {
            const res = await api.webviewCaptureMjSession(false);
            if (res?.ok) toast.success("Midjourney session captured into the automation profile.");
            else toast.warning(String(res?.detail || "Midjourney profile seeding failed — use Capture session in Settings."));
            return;
          }
        }
      } catch { /* backend not ready / page mid-navigation — keep polling */ }
    }
  }, []);

  const runLoginFlow = useCallback(async (siteKey) => {
    const seq = ++loginFlowSeq.current;
    const site = sites.find((s) => s.key === siteKey) || null;
    const isGoogleSite = !site || siteKey === "youtube";

    // Kaggle's "Continue with Google" reliably gets rejected by Google inside this embedded
    // WebKit webview ("This browser or app may not be secure") regardless of the spoofed UA —
    // that's Google deliberately blocking sign-in from embedded/automated webviews, which is
    // what showed up as the login page reloading in a loop and never reaching Kaggle. None of
    // the Kaggle server Start/Fetch flow needs a browser session (it authenticates via the
    // kaggle CLI's ~/.kaggle/kaggle.json token), so skip the doomed embedded flow entirely and
    // open the notebook in the real system browser instead, where Google sign-in isn't blocked.
    if (KAGGLE_SITE_KEYS.includes(siteKey)) {
      try {
        await api.openKaggleNotebook(siteKey);
        toast.message("Opened the notebook in your system browser — Google sign-in works there. The Kaggle server itself doesn't need a browser login; use Start & connect in Settings.", { duration: 9000 });
      } catch (e) {
        toast.error(String(e));
      }
      return;
    }

    // Google's WebView-Ⅱ policy rejects sign-in inside this embedded WebKit webview for *any*
    // site — so "Continue with Google" is a dead end here, not just for Kaggle. The only logins
    // that actually complete embedded are the engines' own non-Google methods (email/password on
    // Suno, email or Discord on Midjourney). So don't waste a blocking 5-minute Google-first wait
    // on the engine sites; go straight to their own sign-in and guide the user to a method that works.

    // YouTube/Google itself can't be signed into embedded at all — the app authorizes YouTube via
    // the Channels page's system-browser OAuth. Still open Google here for users who only want a
    // reusable session for other tabs, but make the limitation explicit.
    if (isGoogleSite) {
      await addPage(GOOGLE_LOGIN_URL, { label: "Google", id: GOOGLE_PAGE_ID });
      await sleep(3500);
      let ok = await googleLoggedIn();
      if (ok) {
        toast.success("Google session is active in the webview.");
        if (site?.url) await addPage(site.url, { label: site.label, siteKey, id: siteKey });
      } else {
        toast.message("Google often blocks sign-in inside embedded browsers (\"this browser may not be secure\"). If YouTube login loops here, authorize YouTube from the Channels page instead — that uses your system browser.", { duration: 10000 });
      }
      return;
    }

    // Suno / Midjourney: open the engine's own sign-in directly and capture the session cookie.
    toast.message(
      siteKey === "suno"
        ? "Sign in to Suno with email/password (\"Continue with Google\" is blocked inside embedded browsers). Your session is captured automatically once you're in."
        : "Sign in to Midjourney with email or Discord (\"Continue with Google\" is blocked inside embedded browsers). Your session is captured automatically once you're in.",
      { duration: 10000 },
    );
    const loginUrl = site.loginUrl || site.url;
    const existing = pagesRef.current.find((p) => p.id === siteKey);
    if (existing) {
      await showPage(existing);
      try { await api.webviewNavigate(loginUrl, siteKey); setUrlInput(loginUrl); } catch (e) { toast.error(String(e)); }
    } else {
      await addPage(loginUrl, { label: site.label, siteKey, id: siteKey });
    }
    // Fire-and-forget: capture the session cookies as soon as the login completes.
    watchAndCapture(siteKey, seq);
  }, [sites, addPage, showPage, googleLoggedIn, watchAndCapture]);

  // Manual variant of the auto-capture above, for re-capturing an already-live session.
  const captureSession = useCallback(async () => {
    try {
      const res = activeSiteKey === "suno"
        ? await api.webviewCaptureSunoSession()
        : await api.webviewCaptureMjSession(false);
      setLastResult({ label: `${activeSiteKey} session capture`, data: res });
      if (res?.ok) toast.success("Session captured and stored.");
      else toast.warning(String(res?.detail || "No session cookies found — sign in first."));
    } catch (e) { toast.error(String(e)); }
  }, [activeSiteKey]);

  const checkLogin = async () => {
    const key = activeSiteKey || "page";
    await api.webviewEval(loginProbeScript(key));
    await new Promise((r) => setTimeout(r, 500));
    const res = await api.webviewTakeScrape(`${key}:login`);
    if (res?.loggedOutHint) toast.warning("Looks logged out / blocked — sign in inside the panel below.");
    else toast.success("Session looks active.");
  };

  // ── Macro recorder ────────────────────────────────────────────
  // Poll the live step count while recording so the UI shows progress.
  useEffect(() => {
    if (!recording) return;
    const t = setInterval(async () => {
      try {
        const st = await api.macroStatus();
        setRecSteps(st?.steps ?? 0);
        if (st && !st.recording) setRecording(false);
      } catch { /* ignore */ }
    }, 1200);
    return () => clearInterval(t);
  }, [recording]);

  const startRecording = useCallback(async () => {
    if (!activePage) return toast.error("Open a page first.");
    try {
      await api.macroStart();
      setRecSteps(0);
      setPendingSteps(null);
      setRecording(true);
      setMacroOpen(true);
      toast.message("Recording — every click, right-click, paste and field edit is captured. Navigations are followed.");
    } catch (e) { toast.error(String(e)); }
  }, [activePage]);

  const stopRecording = useCallback(async () => {
    try {
      const raw = await api.macroStop();
      setRecording(false);
      const steps = tidySteps(raw);
      if (!steps.length) return toast.warning("Nothing recorded.");
      setPendingSteps(steps);
      setMacroName("");
      toast.success(`Recorded ${steps.length} steps — name the macro to save it.`);
    } catch (e) { toast.error(String(e)); }
  }, []);

  // Record-time special steps: paste from the in-app pseudo clipboard, or an app-wide
  // connector slot. Both anchor to the element currently focused *inside the page* — click
  // into the target field first, then press the button. (Needed because the native OS
  // context-menu "Paste" fires no scriptable event we could record.)
  const recordSpecial = useCallback(async (step, insertText) => {
    try {
      const res = await evalAndTake(recordSpecialStepScript(step, insertText), "macro:special", { settleMs: 300 });
      if (!res?.ok) toast.warning(res?.reason || "Click into a field inside the page first, then press the button again.");
      return res;
    } catch (e) {
      toast.error(String(e));
      return null;
    }
  }, []);

  const recordPasteClipboard = useCallback(async () => {
    const text = getVirtualClipboard();
    const res = await recordSpecial({ type: "paste-clipboard", param: "clipboard" }, text || null);
    if (res?.ok) {
      toast.success(text
        ? "Recorded: paste app clipboard (current bucket content inserted)."
        : "Recorded: paste app clipboard — bucket is empty, content will be inserted at playback.");
      setRecSteps((n) => n + 1);
    }
  }, [recordSpecial]);

  const recordConnector = useCallback(async () => {
    const res = await recordSpecial({ type: "connector", connector: connectorChoice, binding: null }, null);
    if (res?.ok) {
      const c = CONNECTORS.find((x) => x.id === connectorChoice);
      toast.success(`Recorded connector slot: ${c?.label || connectorChoice}. Rebind it any time in the Macro Manager.`);
      setRecSteps((n) => n + 1);
    }
  }, [recordSpecial, connectorChoice]);

  // Refresh clipboard bucket + macro list when the panel opens (the Macro Manager
  // view edits both).
  useEffect(() => {
    if (!macroOpen) return;
    setClipText(getVirtualClipboard());
    setBucketQueue(getClipboardQueue());
    setMacros(loadMacros());
  }, [macroOpen]);

  const addBucketEntries = useCallback(() => {
    const lines = bucketDraft.split("\n").map((l) => l.trim()).filter(Boolean);
    if (!lines.length) return;
    const updated = pushClipboardEntries(lines);
    setBucketQueue(updated);
    setBucketDraft("");
    toast.success(`Added ${lines.length} entr${lines.length === 1 ? "y" : "ies"} — ${updated.length} in the bucket.`);
  }, [bucketDraft]);

  const clearBucket = useCallback(() => {
    setBucketQueue(clearClipboardQueue());
    toast.success("Cleared the clipboard bucket queue.");
  }, []);

  const savePendingMacro = useCallback(() => {
    if (!pendingSteps?.length) return;
    const macro = saveMacro({
      name: macroName.trim() || undefined,
      steps: pendingSteps,
      startUrl: activePage?.url,
    });
    setMacros(loadMacros());
    setSelectedMacroId(macro.id);
    setPendingSteps(null);
    setMacroName("");
    toast.success(`Saved macro "${macro.name}".`);
  }, [pendingSteps, macroName, activePage]);

  const runMacro = useCallback(async (macro, { runLabel } = {}) => {
    if (!macro) return toast.error("Pick a macro first.");
    if (!openedRef.current) return toast.error("Open a page first.");
    setPlaying(true);
    setPlayNote("starting…");
    try {
      const { ok, results } = await playMacro(
        macro.steps,
        { pasteText: macroParam.trim() ? macroParam : undefined, listMode, stopOnFailure },
        {
          evalJs: (js) => api.webviewEval(js),
          navigate: (url) => api.webviewNavigate(url),
          currentUrl: () => api.webviewCurrentUrl(),
          takeScrape: (key) => api.webviewTakeScrape(key),
          armDownload: (folder, filename, resultKey) => api.webviewArmDownload(folder, filename, resultKey),
          onProgress: (i, total, note) => setPlayNote(`${runLabel ? runLabel + " — " : ""}${i + 1}/${total} — ${note}`),
        },
      );
      const failed = results.filter((r) => !r.ok);
      setLastResult({ label: `Macro "${macro.name}"${runLabel ? ` (${runLabel})` : ""}`, data: results });
      if (ok) toast.success(`Macro finished${runLabel ? ` (${runLabel})` : ""} — all ${results.length} steps replayed.`);
      else toast.warning(`Macro finished${runLabel ? ` (${runLabel})` : ""} with ${failed.length}/${results.length} failed steps (see results panel).`);
      return ok;
    } catch (e) {
      toast.error("Macro playback failed: " + e);
      return false;
    } finally {
      setPlaying(false);
      setPlayNote("");
    }
  }, [activePage, macroParam, listMode, stopOnFailure]);

  // Replays the selected macro `repeatCount` times in a row — pairs naturally with the
  // clipboard-queue "pop next" connector, so each pass through the loop pastes a different
  // prefilled value (a batch of titles, prompts, etc.) instead of repeating the same run.
  const runSelectedMacro = useCallback(async () => {
    const macro = macros.find((m) => m.id === selectedMacroId);
    if (!macro) return toast.error("Pick a macro first.");
    const n = Math.max(1, Math.min(50, Number(repeatCount) || 1));
    for (let r = 0; r < n; r++) {
      const ok = await runMacro(macro, { runLabel: n > 1 ? `run ${r + 1}/${n}` : undefined });
      if (!ok && stopOnFailure) { if (n > 1) toast.warning(`Stopped after run ${r + 1}/${n} (a step failed).`); break; }
    }
  }, [runMacro, macros, selectedMacroId, repeatCount, stopOnFailure]);

  // Deep link from the Macro Manager: /browser?playMacro=<id> — open the macro panel and
  // replay once the initial page has had time to open.
  const deepLinkPlayedRef = useRef(false);
  useEffect(() => {
    const playId = params.get("playMacro");
    if (!playId || deepLinkPlayedRef.current) return;
    deepLinkPlayedRef.current = true;
    const macro = loadMacros().find((m) => m.id === playId);
    setParams((prev) => { const p = new URLSearchParams(prev); p.delete("playMacro"); return p; });
    if (!macro) { toast.error("Macro not found."); return; }
    setMacroOpen(true);
    setSelectedMacroId(macro.id);
    setMacros(loadMacros());
    const t = setTimeout(() => runMacro(macro), 2000);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params]);

  const removeSelectedMacro = useCallback(() => {
    if (!selectedMacroId) return;
    setMacros(deleteMacro(selectedMacroId));
    setSelectedMacroId("");
  }, [selectedMacroId]);

  const selectedMacro = macros.find((m) => m.id === selectedMacroId) || null;
  const groups = [...new Set(sites.map((s) => s.group))];

  return (
    // Fills the Shell's view box exactly: sidebar to the left, app header above. Inside, the
    // tab strip and toolbar take their natural height and the stage claims everything left.
    <div className="h-full flex flex-col min-h-0 overflow-hidden">
      {/* ── Page tabs ── */}
      <div className="flex items-center gap-1 px-2 pt-1.5 border-b border-border shrink-0 overflow-x-auto scroll-thin">
        {pages.map((p) => (
          <div
            key={p.id}
            className={`group flex items-center gap-1.5 pl-3 pr-1.5 py-1.5 rounded-t-md border border-b-0 text-xs cursor-pointer select-none max-w-[200px] shrink-0 ${
              p.id === activePageId
                ? "border-primary/60 bg-primary/10 text-primary"
                : "border-border/60 text-muted-foreground hover:bg-muted/40"
            }`}
            onClick={() => showPage(p)}
            title={p.url}
          >
            <Globe className="w-3 h-3 shrink-0" />
            <span className="truncate">{p.label}</span>
            <button
              onClick={(e) => { e.stopPropagation(); closePage(p.id); }}
              className="p-0.5 rounded hover:bg-muted opacity-40 group-hover:opacity-100"
              title="Close page"
            >
              <X className="w-3 h-3" />
            </button>
          </div>
        ))}
        {/* Add current URL-bar text as a new page */}
        <button
          onClick={() => { if (urlInput.trim()) addPage(urlInput.trim()); else toast.message("Type a URL in the address bar, then press + to open it as a page."); }}
          className="p-1.5 rounded-md border border-dashed border-border/60 text-muted-foreground hover:text-foreground hover:bg-muted/40 shrink-0"
          title="Add the URL in the address bar as a new page"
        >
          <Plus className="w-3.5 h-3.5" />
        </button>
        {/* Presets dropdown (grouped sites) */}
        <div className="relative shrink-0" ref={presetsRef}>
          <button
            onClick={() => setPresetsOpen((o) => !o)}
            className="flex items-center gap-1 text-xs px-2 py-1.5 rounded-md border border-border/60 text-muted-foreground hover:text-foreground hover:bg-muted/40"
          >
            Presets <ChevronDown className="w-3 h-3" />
          </button>
          {presetsOpen && (
            <div className="absolute z-[9999] top-full mt-1 left-0 bg-popover border border-border/80 rounded-lg shadow-2xl p-1.5 min-w-[320px] max-h-[70vh] overflow-y-auto scroll-thin">
              <div className="px-2 pt-1 pb-2 text-[10px] leading-snug text-muted-foreground/70 border-b border-border/50 mb-1">
                Open a known site as a page, or switch to it if it's already open. Engine
                addresses come from Settings — a <span className="text-amber-500/90">notebook</span> tag
                means no live tunnel is saved, so the Kaggle page opens instead of the engine.
              </div>
              {groups.map((g) => (
                <div key={g}>
                  <div className="px-2 pt-2 pb-0.5 text-[9px] uppercase tracking-[0.2em] text-muted-foreground/60">{g}</div>
                  {sites.filter((s) => s.group === g).map((s) => {
                    const openPage = pages.find((p) => p.id === s.key);
                    const notebook = isNotebookFallback(s);
                    return (
                      <button
                        key={s.key}
                        onClick={() => { setPresetsOpen(false); openPreset(s.key); }}
                        className={`w-full flex items-center gap-2 px-2.5 py-1.5 rounded-md text-xs text-left hover:bg-accent ${
                          s.key === activeSiteKey ? "bg-primary/10 text-primary" : ""
                        }`}
                        title={s.url}
                      >
                        <Globe className="w-3 h-3 shrink-0 opacity-60" />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate">{s.label}</span>
                          {/* The engine tunnels rotate, so the host is the useful bit. */}
                          <span className="block truncate text-[9px] font-mono text-muted-foreground/60">
                            {labelForUrl(s.url)}
                          </span>
                        </span>
                        {notebook && <Badge variant="outline" className="text-[8px] border-amber-500/50 text-amber-500/90 shrink-0">notebook</Badge>}
                        {openPage && <Badge variant="secondary" className="text-[8px] shrink-0">open</Badge>}
                        {s.needsGoogleLogin && !openPage && <Badge variant="secondary" className="text-[8px] shrink-0">login</Badge>}
                      </button>
                    );
                  })}
                </div>
              ))}
            </div>
          )}
        </div>
        <div className="flex-1" />
        {/* Macro panel toggle */}
        <button
          onClick={() => setMacroOpen((o) => !o)}
          className={`flex items-center gap-1.5 text-xs px-2 py-1.5 mb-0.5 rounded-md border shrink-0 ${
            macroOpen || recording ? "border-primary/60 bg-primary/10 text-primary" : "border-border/60 text-muted-foreground hover:bg-muted/40"
          }`}
          title="AI macro recorder"
        >
          <Wand2 className="w-3.5 h-3.5" />
          Macros
          {recording && <span className="w-2 h-2 rounded-full bg-red-500 animate-pulse" />}
        </button>
      </div>

      {/* ── Toolbar ── The macro/result panels can make this tall; cap it and scroll inside
           so it can never eat the stage. */}
      <div className="px-3 py-1.5 border-b border-border space-y-1.5 shrink-0 max-h-[45%] overflow-y-auto scroll-thin">
        <div className="flex items-center gap-2">
          <Button size="sm" variant="ghost" className="h-8 px-2" onClick={reload} title="Reload"><RotateCw className="w-3.5 h-3.5" /></Button>
          <Input
            value={urlInput}
            onChange={(e) => setUrlInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") go(urlInput); }}
            className="h-8 text-xs font-mono flex-1"
            placeholder="https://…  (Enter = navigate this page · + = open as new page)"
          />
          <Button size="sm" variant="ghost" className="h-8 px-2" onClick={() => go(urlInput)} title="Go"><ExternalLink className="w-3.5 h-3.5" /></Button>
          <Button size="sm" variant="outline" className="h-8" onClick={checkLogin}><LogIn className="w-3.5 h-3.5 mr-1.5" />Check login</Button>
          {(activeSite?.loginUrl || activeSite?.needsGoogleLogin) && (
            KAGGLE_SITE_KEYS.includes(activeSiteKey) ? (
              <Button size="sm" variant="outline" className="h-8" onClick={() => runLoginFlow(activeSiteKey)}
                title="This engine needs no browser login (it uses your kaggle CLI token). Opens the notebook in your system browser — only needed to set a secret (e.g. FLUX's HF_TOKEN) or free a stuck GPU session.">
                <ExternalLink className="w-3.5 h-3.5 mr-1.5" />Open notebook
              </Button>
            ) : (
              <Button size="sm" variant="outline" className="h-8" onClick={() => runLoginFlow(activeSiteKey)}
                title={activeSiteKey === "youtube"
                  ? "Google blocks sign-in inside embedded browsers — authorize YouTube from the Channels page (system-browser OAuth) instead."
                  : "Opens this site's own sign-in. Use email/password or Discord — 'Continue with Google' is blocked inside embedded browsers. Your session is captured automatically."}>
                <LogIn className="w-3.5 h-3.5 mr-1.5" />Sign in
              </Button>
            )
          )}
          {["suno", "midjourney"].includes(activeSiteKey) && (
            <Button size="sm" variant="outline" className="h-8" onClick={captureSession}
              title="Capture this site's session cookies from the webview (stored for API/automation use)">
              <Cookie className="w-3.5 h-3.5 mr-1.5" />Capture session
            </Button>
          )}
          {activeSiteKey === "suno" && (
            <Button size="sm" className="h-8" onClick={async () => {
              const songId = params.get("songId");
              if (!songId) return toast.error("Open this from a song's Generate button to auto-fill.");
              try { await runSunoAutomation(await api.getSong(songId)); } catch (e) { toast.error(String(e)); }
            }}><Play className="w-3.5 h-3.5 mr-1.5" />Run</Button>
          )}
          {activeSiteKey === "suno" && (
            <Button size="sm" variant="outline" className="h-8" onClick={async () => {
              const songId = params.get("songId");
              if (!songId) return toast.error("Open this from a song's Generate button, then Run, first.");
              try { await captureSunoResult(await api.getSong(songId), []); } catch (e) { toast.error(String(e)); }
            }} title="Grab whatever finished clip is currently on the page and save it to the song">
              <ListTree className="w-3.5 h-3.5 mr-1.5" />Capture result
            </Button>
          )}
          {["acestep", "heartmula", "comfyui"].includes(activeSiteKey) && (
            <Button size="sm" variant="outline" className="h-8" onClick={runEngineHealth}>
              <HeartPulse className="w-3.5 h-3.5 mr-1.5" />Health check
            </Button>
          )}
        </div>

        {/* Per-site automation row */}
        {activeSiteKey === "midjourney" && (
          <div className="flex items-center gap-2">
            <Input value={actionInput} onChange={(e) => setActionInput(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") runMidjourneyImagine(actionInput); }}
              className="h-8 text-xs flex-1" placeholder="/imagine prompt…" />
            <Button size="sm" className="h-8" onClick={() => runMidjourneyImagine(actionInput)}>
              <ImageIcon className="w-3.5 h-3.5 mr-1.5" />Imagine
            </Button>
            <Button size="sm" variant="outline" className="h-8" onClick={runMidjourneyScrape}>
              <ListTree className="w-3.5 h-3.5 mr-1.5" />Scrape results
            </Button>
          </div>
        )}
        {activeSiteKey === "youtube" && (
          <div className="flex items-center gap-2">
            <Button size="sm" variant="outline" className="h-8"
              onClick={() => go("https://www.youtube.com/channel_switcher")}>Channel switcher</Button>
            <Button size="sm" variant="outline" className="h-8" onClick={runYoutubeList}>
              <ListTree className="w-3.5 h-3.5 mr-1.5" />List channels
            </Button>
            <Input value={actionInput} onChange={(e) => setActionInput(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") runYoutubeSwitch(actionInput); }}
              className="h-8 text-xs w-56" placeholder="channel name…" />
            <Button size="sm" className="h-8" onClick={() => runYoutubeSwitch(actionInput)}>Switch</Button>
          </div>
        )}

        {/* ── AI macro recorder ── */}
        {macroOpen && (
          <div className="space-y-1.5 rounded-md border border-border/60 bg-muted/20 px-2.5 py-2">
            <div className="flex items-center gap-2 flex-wrap">
              {!recording ? (
                <Button size="sm" variant="outline" className="h-8 border-red-500/50 text-red-500 hover:bg-red-500/10" onClick={startRecording}>
                  <CircleDot className="w-3.5 h-3.5 mr-1.5" />Record
                </Button>
              ) : (
                <>
                  <Button size="sm" variant="outline" className="h-8 border-red-500 text-red-500 animate-pulse" onClick={stopRecording}>
                    <Square className="w-3.5 h-3.5 mr-1.5" />Stop ({recSteps} steps)
                  </Button>
                  {/* Visible only while recording: explicit paste + connector-slot steps
                      (native OS context-menu "Paste" can't be captured). Click into the
                      target field in the page first, then press one of these. */}
                  <Button size="sm" variant="outline" className="h-8" onClick={recordPasteClipboard}
                    title="Record: paste the in-app clipboard bucket into the field focused inside the page">
                    <ClipboardPaste className="w-3.5 h-3.5 mr-1.5" />Paste clipboard
                  </Button>
                  <div className="flex items-center rounded-md border border-border overflow-hidden">
                    <select
                      value={connectorChoice}
                      onChange={(e) => setConnectorChoice(e.target.value)}
                      className="h-8 text-xs bg-background px-2 border-r border-border max-w-[180px]"
                      title="Which app-wide connector action this slot plugs into (rebindable in the Macro Manager)"
                    >
                      {CONNECTORS.map((c) => <option key={c.id} value={c.id}>{c.label}</option>)}
                    </select>
                    <Button size="sm" variant="ghost" className="h-8 rounded-none" onClick={recordConnector}
                      title="Record: an app-wide connector slot at the field focused inside the page">
                      <Plug className="w-3.5 h-3.5 mr-1.5" />Connector
                    </Button>
                  </div>
                </>
              )}
              {pendingSteps && (
                <>
                  <Input value={macroName} onChange={(e) => setMacroName(e.target.value)}
                    onKeyDown={(e) => { if (e.key === "Enter") savePendingMacro(); }}
                    className="h-8 text-xs w-52" placeholder={`name (${pendingSteps.length} steps)…`} autoFocus />
                  <Button size="sm" className="h-8" onClick={savePendingMacro}>Save macro</Button>
                  <Button size="sm" variant="ghost" className="h-8" onClick={() => setPendingSteps(null)}>Discard</Button>
                </>
              )}
              <div className="w-px h-5 bg-border mx-1" />
              <select
                value={selectedMacroId}
                onChange={(e) => setSelectedMacroId(e.target.value)}
                className="h-8 text-xs rounded-md border border-border bg-background px-2 max-w-[220px]"
              >
                <option value="">— saved macros ({macros.length}) —</option>
                {macros.map((m) => (
                  <option key={m.id} value={m.id}>{m.name} ({m.steps.length} steps)</option>
                ))}
              </select>
              <select
                value={listMode}
                onChange={(e) => setListMode(e.target.value)}
                className="h-8 text-xs rounded-md border border-border bg-background px-2"
                title="How list items are re-resolved on playback: by their recorded position, position counted from the list's end, or always the first/last item"
              >
                <option value="recorded">List items: recorded position</option>
                <option value="fromEnd">List items: position from end</option>
                <option value="first">List items: always first (newest)</option>
                <option value="last">List items: always last</option>
              </select>
              <div className="flex items-center rounded-md border border-border overflow-hidden" title="Replay the selected macro this many times in a row — pairs with the clipboard bucket queue's 'pop next' connector for one-macro-per-item bulk runs">
                <span className="px-2 text-[10px] text-muted-foreground bg-muted/40">×</span>
                <input
                  type="number" min={1} max={50}
                  value={repeatCount}
                  onChange={(e) => setRepeatCount(e.target.value)}
                  className="h-8 w-12 text-xs bg-background px-1.5 outline-none"
                />
              </div>
              <label className="flex items-center gap-1.5 text-[11px] text-muted-foreground px-1" title="Stop the run (and any remaining repeats) as soon as a step fails, instead of playing through the whole macro regardless">
                <input type="checkbox" checked={stopOnFailure} onChange={(e) => setStopOnFailure(e.target.checked)} className="accent-primary" />
                Stop on failure
              </label>
              <Button size="sm" className="h-8" disabled={!selectedMacro || playing} onClick={runSelectedMacro}>
                <Play className="w-3.5 h-3.5 mr-1.5" />{playing ? "Playing…" : "Play"}
              </Button>
              {selectedMacro && (
                <Button size="sm" variant="ghost" className="h-8 px-2 text-muted-foreground hover:text-red-500" onClick={removeSelectedMacro} title="Delete macro">
                  <Trash2 className="w-3.5 h-3.5" />
                </Button>
              )}
            </div>
            <div className="flex items-center gap-2">
              <ClipboardPaste className="w-3.5 h-3.5 text-muted-foreground shrink-0" title="In-app pseudo clipboard bucket" />
              <Input value={clipText}
                onChange={(e) => { setClipText(e.target.value); setVirtualClipboard(e.target.value); }}
                className="h-8 text-xs flex-1 font-mono"
                placeholder="app clipboard bucket — 'Paste clipboard' steps and the clipboard connector insert this…" />
              <Input value={macroParam} onChange={(e) => setMacroParam(e.target.value)}
                className="h-8 text-xs flex-1 font-mono"
                placeholder="per-run override for paste steps (empty = use bucket / recorded text)…" />
              <Button size="sm" variant="outline" className="h-8 shrink-0" onClick={() => setBucketOpen((o) => !o)}
                title="Multi-entry clipboard bucket: prefill several values, then bind a connector step to &quot;pop next&quot; — each playback consumes one, so a macro replayed several times pastes something different each run">
                Bucket ({bucketQueue.length})
              </Button>
            </div>
            {bucketOpen && (
              <div className="flex items-start gap-2 rounded-md border border-border/60 bg-background/60 p-2">
                <Textarea
                  value={bucketDraft}
                  onChange={(e) => setBucketDraft(e.target.value)}
                  rows={2}
                  className="flex-1 text-xs font-mono"
                  placeholder={'One entry per line — prefill, then bind a connector step to "Clipboard bucket — pop next" so each playback consumes the next line.'}
                />
                <div className="flex flex-col gap-1 shrink-0">
                  <Button size="sm" className="h-7 text-xs" onClick={addBucketEntries} disabled={!bucketDraft.trim()}>Add</Button>
                  <Button size="sm" variant="ghost" className="h-7 text-xs text-muted-foreground hover:text-red-500" onClick={clearBucket} disabled={!bucketQueue.length}>Clear</Button>
                </div>
                {bucketQueue.length > 0 && (
                  <ol className="text-[10px] font-mono text-muted-foreground space-y-0.5 max-h-14 overflow-y-auto shrink-0 max-w-[160px]">
                    {bucketQueue.map((entry, i) => <li key={i} className="truncate">{i + 1}. {entry}</li>)}
                  </ol>
                )}
              </div>
            )}
            {playing && playNote && (
              <div className="text-[11px] font-mono text-primary">{playNote}</div>
            )}
            {recording && (
              <p className="text-[11px] text-muted-foreground">
                Interact with the page below — clicks, right-clicks, Ctrl+V pastes, field edits and Enter are recorded.
                The OS context-menu "Paste" can't be captured: click into the field, then use the
                <strong> Paste clipboard</strong> button (inserts the in-app clipboard bucket) or record a
                <strong> Connector</strong> slot to plug an app action in. Elements inside lists are recorded by
                <em> position</em>, so on playback the macro picks whatever sits at that position now
                (e.g. the newest download), not the old item by name.
              </p>
            )}
            {selectedMacro && !playing && (
              <details className="text-[11px] text-muted-foreground">
                <summary className="cursor-pointer">Steps in "{selectedMacro.name}"</summary>
                <ol className="list-decimal ml-5 mt-1 space-y-0.5 max-h-32 overflow-y-auto font-mono">
                  {selectedMacro.steps.map((s, i) => <li key={i}>{describeStep(s)}</li>)}
                </ol>
              </details>
            )}
          </div>
        )}

        {/* Latest automation result */}
        {lastResult && (
          <div className="flex items-start gap-2 text-[11px] font-mono bg-muted/40 border border-border/60 rounded-md px-2.5 py-1.5">
            <span className="text-primary shrink-0">{lastResult.label}:</span>
            <pre className="flex-1 overflow-x-auto whitespace-pre-wrap max-h-28 overflow-y-auto">{JSON.stringify(lastResult.data, null, 1)}</pre>
            <button onClick={() => setLastResult(null)} className="text-muted-foreground hover:text-foreground shrink-0"><X className="w-3 h-3" /></button>
          </div>
        )}
        {activeSite?.needsGoogleLogin && (
          <p className="text-[11px] text-amber-500/90">
            Google may reject sign-in inside embedded webviews ("browser not secure"). A Chrome user-agent is spoofed to reduce this; if it still blocks, use the OAuth flow in Channels.
          </p>
        )}
      </div>

      {/* Stage: the active native webview is positioned exactly over this transparent placeholder. */}
      <div ref={stageRef} className="flex-1 min-h-0 bg-background/50" />
    </div>
  );
}
