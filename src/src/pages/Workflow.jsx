import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { visibleEngines } from "../lib/engineCapabilities";
import { stopAllStarted } from "../lib/serverLifecycle";
import { autoStartKaggleServer, getKaggleState } from "../lib/kaggleServerPipeline";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Switch } from "../components/ui/switch";
import {
  Workflow as WorkflowIcon, BookText, Music2, BarChart3, Image as ImageIcon, Layers, Film,
  UploadCloud, Play, Loader2, CheckCircle2, XCircle, CircleDashed, RotateCw, AlertTriangle,
  Clapperboard,
} from "lucide-react";
import { toast } from "sonner";
import GuidedPanel from "../components/GuidedPanel";
import { setRunIntent, clearRunIntent } from "../lib/runHandoff";
import { workflowFlow } from "../lib/guidedFlows";

// One row per pipeline stage, in the order they actually have to run. `pending(songs)` — when
// present — returns the subset of songs this stage still needs to touch, which both drives the
// "N songs" badge and is what `run()` iterates; stages without it (lyrics generation, which
// creates a song rather than acting on existing ones; images/overlays, which already have their
// own bulk backend commands scoped to the whole project) leave it out.
//
// `requires(songs)` returns a reason string when this stage's *input* does not exist at all, or null
// when it does. That is a different question from `pending`, and conflating the two is what made a
// broken run look like a finished one: "nothing pending" is the right answer both when the work is
// already done and when the previous stage produced nothing to work on, and the old code called both
// of them done and carried on — so a pipeline whose images never rendered still finished with a
// column of green ticks over an empty result.
// The section worth spending a video clip on, by the same rule `shorts.rs::hook_start` uses: a
// chorus is the hook by definition; failing that the brightest-mood section; failing that a third of
// the way in, never the intro. Kept in step with that function deliberately — a "hero shot" and a
// short should be cut from the same moment, or the two derivatives of one song disagree about what
// the song is about.
const BRIGHT_MOODS = ["radiant", "epic", "celestial", "warm"];
function pickHookSection(sections) {
  if (!sections?.length) return null;
  const ordered = [...sections].sort((a, b) => (a.index ?? 0) - (b.index ?? 0));
  const named = (needle) => ordered.find((s) =>
    `${s.name || ""} ${s.line || ""}`.toLowerCase().includes(needle));
  return named("chorus") || named("hook") || named("refrain")
      || ordered.find((s) => BRIGHT_MOODS.includes((s.mood || "").toLowerCase()))
      || ordered[Math.floor(ordered.length / 3)]
      || ordered[0];
}

function buildStages({ activeProjectId, includeUpload, includeVideoGen }) {
  return [
    {
      id: "lyrics",
      label: "Daily lyrics",
      icon: BookText,
      hint: "Generates the next scheduled chapter's song (title + lyrics) from this project's Daily Content schedule (set on the Dashboard).",
      pending: null,
      run: async () => {
        const res = await api.generateNextChapterNow(activeProjectId);
        const count = res?.created_song_ids?.length || 0;
        if (!count) throw new Error((res?.errors && res.errors[0]) || "No song was generated — check the Daily Content schedule on the Dashboard.");
        return `generated "${res.book} ${res.chapter}" (${count} song${count > 1 ? "s" : ""})`;
      },
    },
    {
      id: "music",
      requires: (songs) => songs.length ? null
        : "this project has no songs yet — lyrics produced nothing to set to music.",
      label: "Music generation",
      icon: Music2,
      hint: "Queues server-side Suno generation for every song that has no audio yet.",
      pending: (songs) => songs.filter((s) => !s.audio_url && !s.local_audio_path),
      run: async (songs) => {
        const targets = songs.filter((s) => !s.audio_url && !s.local_audio_path);
        for (const s of targets) await api.genMusic(s.id);
        return `queued ${targets.length} song${targets.length === 1 ? "" : "s"}`;
      },
    },
    {
      id: "analysis",
      requires: (songs) => songs.some((s) => s.audio_url || s.local_audio_path) ? null
        : "no song has audio yet — music generation produced nothing to analyse.",
      label: "Analysis",
      icon: BarChart3,
      hint: "Section/beat analysis for every song that has audio but hasn't been analyzed yet.",
      pending: (songs) => songs.filter((s) => s.audio_url && s.status !== "analyzed" && s.status !== "video_ready"),
      run: async (songs) => {
        const targets = songs.filter((s) => s.audio_url && s.status !== "analyzed" && s.status !== "video_ready");
        for (const s of targets) await api.analyze(s.id);
        return `queued ${targets.length} song${targets.length === 1 ? "" : "s"}`;
      },
    },
    {
      id: "images",
      requires: (songs) => songs.some((s) => s.status === "analyzed" || s.status === "video_ready") ? null
        : "no song has been analysed yet, so there are no sections to illustrate.",
      label: "Section images",
      icon: ImageIcon,
      hint: "Bulk-generates every section's image across the whole project (skips sections that already have one).",
      pending: null,
      run: async () => {
        const res = await api.bulkGenerateAll(activeProjectId);
        return `queued ${res?.queued ?? res?.count ?? "some"} image job(s)`;
      },
    },
    {
      // After the stills, because a clip *replaces* one on the section it lands on, and before
      // assembly, which reads whatever each section ended up holding.
      id: "videogen",
      requires: (songs) => songs.some((s) => s.status === "analyzed" || s.status === "video_ready") ? null
        : "no song has been analysed yet, so there is no hook section to animate.",
      label: "Hero clips",
      icon: Clapperboard,
      hint: includeVideoGen
        ? "Generates one moving clip per song, for its hook section — the chorus if the analysis found one. Included in \"Run full pipeline\". Each clip is 10–35 GPU-minutes out of a 30-hour week, so this is the expensive stage."
        : "Generates one moving clip per song, for its hook section — the chorus if the analysis found one. Excluded from \"Run full pipeline\" by default because each clip costs 10–35 GPU-minutes out of a 30-hour week; flip the switch above to include it, or run this stage by hand.",
      pending: null,
      run: async (songs) => {
        const settings = await api.getSettings(activeProjectId);
        // Presets differ by an order of magnitude in cost, so guessing one is not a kindness. Caught
        // here as well as in the job so the whole batch fails at once rather than one job at a time.
        if (!settings?.video_preset) {
          throw new Error("No video preset chosen yet — pick one on the Video Gen page first; they differ by 10× in GPU time.");
        }
        const targets = songs.filter((s) => s.status === "analyzed" || s.status === "video_ready");
        let queued = 0;
        let skipped = 0;
        for (const song of targets) {
          const sections = await api.listSections(song.id).catch(() => []);
          const hook = pickHookSection(sections);
          // No hook, no prompt, or already moving — a re-run must not spend another half-hour of GPU
          // on the same second of the same song.
          if (!hook || hook.is_video || !(hook.image_prompt || hook.line || "").trim()) { skipped += 1; continue; }
          await api.genSectionClip(hook.id);
          queued += 1;
        }
        if (!queued && skipped) return `nothing to animate — ${skipped} song(s) already had a clip or no usable hook`;
        return `queued ${queued} clip${queued === 1 ? "" : "s"}${skipped ? `, skipped ${skipped}` : ""}`;
      },
    },
    {
      id: "overlays",
      requires: (songs) => songs.some((s) => s.audio_url || s.local_audio_path) ? null
        : "no song has audio yet — an overlay is generated from it.",
      label: "Overlays",
      icon: Layers,
      hint: "Generates overlay assets for every analyzed song that doesn't have one yet.",
      pending: (songs) => songs.filter((s) => s.audio_url && !s.overlay_local_path),
      run: async () => {
        const res = await api.generateOverlaysBulk(activeProjectId, false);
        return `queued ${res?.queued ?? res?.count ?? "some"} overlay job(s)`;
      },
    },
    {
      id: "video",
      requires: (songs) => songs.some((s) => s.overlay_local_path) ? null
        : "no song has an overlay yet — the overlay stage produced nothing to assemble over.",
      label: "Video assembly",
      icon: Film,
      hint: "Renders/concatenates the final video for every song with an overlay that isn't video_ready yet. No bulk endpoint exists server-side, so this loops one compose call per song.",
      pending: (songs) => songs.filter((s) => s.overlay_local_path && s.status !== "video_ready"),
      run: async (songs) => {
        const targets = songs.filter((s) => s.overlay_local_path && s.status !== "video_ready");
        for (const s of targets) await api.compose(s.id);
        return `queued ${targets.length} song${targets.length === 1 ? "" : "s"}`;
      },
    },
    {
      id: "upload",
      requires: (songs) => songs.some((s) => s.status === "video_ready") ? null
        : "no song is video_ready yet — video assembly produced nothing to publish.",
      label: "Bulk upload",
      icon: UploadCloud,
      hint: includeUpload
        ? "Creates upload rows for every video_ready song, AI-enriches their metadata, then publishes to YouTube. Included in \"Run full pipeline\" — turn the switch above off to leave this stage manual."
        : "Creates upload rows for every video_ready song, AI-enriches their metadata, then publishes to YouTube. Excluded from \"Run full pipeline\" by default — flip the switch above to include it, or run this stage by hand.",
      pending: (songs) => songs.filter((s) => s.status === "video_ready"),
      run: async (songs) => {
        const targets = songs.filter((s) => s.status === "video_ready");
        if (!targets.length) return "no video_ready songs to upload";
        const created = await api.bulkFromVideos({ project_id: activeProjectId, formats: ["standard"], privacy: "private", match_by: "language" });
        await api.aiEnrich({ global_description: "", regenerate: false });
        const pre = await api.uploadsPreflight();
        const needOauth = (pre?.need_oauth || []).filter((x) => x.channel_id && !x.error);
        if (needOauth.length) {
          throw new Error(`Created ${created?.created ?? 0} upload row(s), but ${needOauth.length} channel(s) need YouTube sign-in first — connect them on the Upload page, then run this stage again to publish.`);
        }
        const res = await api.publishAll();
        return `created ${created?.created ?? 0} upload row(s), published ${res?.published ?? res?.count ?? "them"}`;
      },
    },
  ];
}

// `blocked` is deliberately not green and not red: nothing failed, but nothing happened either, and
// the honest colour for "the stage before this produced nothing" is the same amber the rest of the
// app uses for a thing you need to look at.
const STATUS_ICON = { idle: CircleDashed, running: Loader2, done: CheckCircle2, error: XCircle, blocked: AlertTriangle };
const STATUS_COLOR = { idle: "text-muted-foreground/50", running: "text-primary animate-spin", done: "text-emerald-500", error: "text-red-500", blocked: "text-amber-400" };

export default function Workflow() {
  const { activeProjectId, activeProject, songs, refreshSongs, jobs } = useStudio();
  // How far the guided run should go — read by the pipeline runner below.
  const [guidedStopAfter, setGuidedStopAfter] = useState("video");
  const navigate = useNavigate();
  const [includeUpload, setIncludeUpload] = useState(false);
  // Off by default for the same reason upload is, but a different one in kind: upload is excluded
  // because it is irreversible, this because it is expensive. A full run that quietly spent
  // half the week's GPU on hero shots would be a nasty thing to discover afterwards.
  const [includeVideoGen, setIncludeVideoGen] = useState(false);
  const [stopOnError, setStopOnError] = useState(true);
  const [runningStageId, setRunningStageId] = useState(null);
  const [runningAll, setRunningAll] = useState(false);
  const [stageState, setStageState] = useState({}); // id -> { status, note }
  const [log, setLog] = useState([]); // [{t, level, message}]
  const [engines, setEngines] = useState(null); // { music_engine, image_engine } — for the browser-dependency banner

  useEffect(() => {
    if (!activeProjectId) return;
    api.getSettings(activeProjectId)
      .then((s) => setEngines({ music_engine: s?.music_engine || "heartmula", image_engine: s?.image_engine || "flux" }))
      .catch(() => {});
  }, [activeProjectId]);

  const stages = useMemo(() => buildStages({ activeProjectId, includeUpload, includeVideoGen }),
    [activeProjectId, includeUpload, includeVideoGen]);
  const activeJobsCount = jobs.filter((j) => j.status === "queued" || j.status === "running").length;
  // Suno is the only engine driven by the embedded Browser tab (no public API) — everything
  // else here (ACE-Step/HeartMuLa, Midjourney's proxy, ComfyUI/FLUX) is a plain REST job, so a
  // project without Suno as its music engine can run "Run full pipeline" fully unattended.
  const needsBrowser = engines?.music_engine === "suno";

  const pushLog = (message, level = "info") => setLog((prev) => [...prev.slice(-199), { t: Date.now(), level, message }]);

  // ── Background runs ────────────────────────────────────────────────────────
  // The same pipeline, sequenced by the backend from this project's saved JSON. That is the version
  // that survives a project switch, a reload and a crash — nothing about it lives in this component.
  const [backendRun, setBackendRun] = useState(null);
  const [elsewhere, setElsewhere] = useState([]);
  const refreshRuns = async () => {
    try {
      const r = await api.workflowRunStatus(activeProjectId || null);
      setBackendRun(r?.run || null);
      setElsewhere(r?.running_elsewhere || []);
    } catch { /* no runs yet */ }
  };
  useEffect(() => {
    refreshRuns();
    const t = setInterval(refreshRuns, 8000);
    return () => clearInterval(t);
    /* eslint-disable-next-line react-hooks/exhaustive-deps */
  }, [activeProjectId]);

  const startBackgroundRun = async () => {
    if (!activeProjectId) { toast.error("No active project selected."); return; }
    try {
      const r = await api.startWorkflowRun({
        project_id: activeProjectId, stop_after: guidedStopAfter,
        include_upload: includeUpload, include_videogen: includeVideoGen, stop_on_error: stopOnError,
      });
      pushLog(`Background run started: ${(r.steps || []).join(" → ")}`);
      toast.success("Running in the background — you can switch projects freely.");
      refreshRuns();
    } catch (err) { toast.error(`${err}`); }
  };

  const setStage = (id, patch) => setStageState((prev) => ({ ...prev, [id]: { ...prev[id], ...patch } }));

  const runStage = async (stage) => {
    setRunningStageId(stage.id);
    setStage(stage.id, { status: "running", note: null });
    pushLog(`${stage.label}: starting…`);
    try {
      const note = await stage.run(songs);
      setStage(stage.id, { status: "done", note });
      pushLog(`${stage.label}: ${note}`, "success");
      await refreshSongs();
      return true;
    } catch (e) {
      const message = e?.message || String(e);
      setStage(stage.id, { status: "error", note: message });
      pushLog(`${stage.label}: ${message}`, "error");
      return false;
    } finally {
      setRunningStageId(null);
    }
  };

  const runOne = async (stage) => {
    if (runningStageId || runningAll) return;
    // Same question as the full run asks. Running a stage by hand whose input does not exist would
    // otherwise fail somewhere inside the backend call, with a message about the symptom rather than
    // the cause.
    const blocker = stage.requires?.(songs);
    if (blocker) {
      setStage(stage.id, { status: "blocked", note: blocker });
      pushLog(`${stage.label}: ${blocker}`, "error");
      toast.warning(blocker);
      return;
    }
    await runStage(stage);
  };

  // Kaggle GPU engines the app can start on demand (Suno/Midjourney are browser flows, not servers).
  // Switched-off engines are filtered out so the workflow never tries to bring one up.
  const KAGGLE_ENGINES = { music: visibleEngines(["heartmula", "acestep"]), images: ["comfyui", "flux"] };

  // Auto-start the server a stage needs, so a full run is hands-off: start before, stop after (the
  // `finally` below). Idempotent — autoStartKaggleServer skips straight to a health check if the
  // server is already live. Returns false only if it ended in an error state, so the caller can
  // surface a clear message instead of the stage failing cryptically against a dead URL.
  const ensureServerForStage = async (stage) => {
    const engine = stage.id === "music" ? engines?.music_engine
                 : stage.id === "images" ? engines?.image_engine : null;
    if (!engine || !(KAGGLE_ENGINES[stage.id] || []).includes(engine)) return true; // nothing to start
    const st = getKaggleState(engine);
    if (st?.status === "ready") return true; // already up
    pushLog(`${stage.label}: making sure the ${engine} server is running…`);
    try {
      await autoStartKaggleServer(engine);
    } catch (e) {
      pushLog(`${stage.label}: could not start ${engine} (${e}).`, "error");
      return false;
    }
    const after = getKaggleState(engine);
    if (after?.status === "error") {
      pushLog(`${stage.label}: ${engine} server didn't come up — ${after.detail || "see Settings"}.`, "error");
      return false;
    }
    return true;
  };

  const runFullPipeline = async () => {
    if (runningStageId || runningAll) return;
    setRunningAll(true);
    // If the user switches project mid-run, `selectProject` reads this and hands the run to the backend
    // runner rather than letting it die with this component.
    setRunIntent({
      projectId: activeProjectId, stopAfter: guidedStopAfter,
      includeUpload, stopOnError,
    });
    pushLog("── Running full pipeline ──");
    try {
      // The guided run's "how far" answer is a hard stop: everything after the chosen stage is
      // skipped, so "lyrics and music" cannot quietly continue into publishing.
      const order = stages.map((s) => s.id);
      const stopIdx = order.indexOf(guidedStopAfter);
      for (const stage of stages) {
        if (stopIdx >= 0 && order.indexOf(stage.id) > stopIdx) {
          pushLog(`${stage.label}: skipped (this run stops after ${guidedStopAfter}).`);
          continue;
        }
        if (stage.id === "upload" && !includeUpload) {
          pushLog(`${stage.label}: skipped (upload not included in this run).`);
          continue;
        }
        if (stage.id === "videogen" && !includeVideoGen) {
          pushLog(`${stage.label}: skipped (hero clips not included in this run).`);
          continue;
        }
        // Asked before `pending`, because the two produce the same count and mean opposite things.
        // A stage whose input never arrived has not finished — it never started, and every stage
        // after it is about to report the same emptiness as success.
        const blocker = stage.requires?.(songs);
        if (blocker) {
          setStage(stage.id, { status: "blocked", note: blocker });
          pushLog(`${stage.label}: ${blocker}`, "error");
          if (stopOnError) {
            pushLog("Full pipeline stopped — the stage before this one produced nothing.", "error");
            break;
          }
          continue;
        }
        if (stage.pending && stage.pending(songs).length === 0) {
          setStage(stage.id, { status: "done", note: "nothing pending" });
          pushLog(`${stage.label}: nothing pending, skipping.`);
          continue;
        }
        // Bring the stage's GPU server up first (no-op for browser engines / already-live servers).
        const serverOk = await ensureServerForStage(stage);
        if (!serverOk && stopOnError) {
          pushLog("Full pipeline stopped — a required server didn't start.", "error");
          break;
        }
        const ok = await runStage(stage);
        if (!ok && stopOnError) {
          pushLog("Full pipeline stopped after a failed stage.", "error");
          break;
        }
      }
    } finally {
      setRunningAll(false);
      clearRunIntent();
      pushLog("── Full pipeline run finished ──");
      // The GPU servers are no longer needed: shut down anything this session started so the run
      // stops billing against the free weekly Kaggle GPU quota.
      try {
        const stopped = await stopAllStarted("workflow finished");
        if (stopped) pushLog(`Stopped ${stopped} GPU server(s) to save your Kaggle quota.`);
      } catch { /* non-fatal */ }
    }
  };

  if (!activeProjectId) {
    return (
      <div className="p-4 sm:p-6 lg:p-8">
        <Card className="p-10 text-center text-muted-foreground border-dashed">Select a project on the Dashboard first.</Card>
      </div>
    );
  }

  return (
    <div className="p-8 max-w-4xl mx-auto fade-in">
      <div className="flex items-center gap-2 mb-2">
        <WorkflowIcon className="w-6 h-6 text-primary" />
        <h1 className="text-4xl sm:text-5xl font-bold">Workflow</h1>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">
        Drive <strong>{activeProject?.name || "this project"}</strong>'s whole pipeline from one place — lyrics → music →
        analysis → images → overlays → video → upload — either stage by stage or as one run. Every stage calls the same
        backend actions their own pages use, so anything already in progress there shows up here too.
        {activeJobsCount > 0 && <> <Badge variant="secondary" className="ml-1 align-middle">{activeJobsCount} job{activeJobsCount === 1 ? "" : "s"} active</Badge></>}
      </p>

      {engines && (
        <p className="text-xs text-muted-foreground mb-4 flex items-center gap-1.5 flex-wrap">
          <Badge variant={needsBrowser ? "outline" : "secondary"} className="text-[10px]">
            {needsBrowser ? "Needs Browser tab" : "Browser-free"}
          </Badge>
          Engines: {engines.music_engine} (music) + {engines.image_engine} (images).
          {needsBrowser
            ? " Suno has no public API, so the Music stage drives the embedded Browser tab — keep it open (or expect the app to switch you there) while that stage runs."
            : " Every stage here is a plain background job — safe to run unattended."}
          <button className="underline" onClick={() => navigate("/settings")}>Change engines</button>
        </p>
      )}


      <div className="mb-6">
        <GuidedPanel
          flow={workflowFlow}
          projectId={activeProjectId}
          extraCtx={{ stages: [] }}
          actions={{
            // The guide's answers steer the same runner the buttons use; "how far" is honoured by
            // the pipeline's own stage list, so nothing here bypasses the review stops.
            setStopAfter: (stage) => setGuidedStopAfter(stage),
            setProvider: async (id) => { try { await api.saveSettings({ remote_render_provider: id }, activeProjectId); } catch { /* non-fatal */ } },
            run: () => runFullPipeline(),
          }}
        />
      </div>
      {/* A run belonging to another project. Shown here because the whole point of moving the loop to
          the backend is that leaving a project does not stop its run — and that is only reassuring if
          you can see it from wherever you are. */}
      {elsewhere.length > 0 && (
        <Card className="p-3 mb-4 text-sm flex items-start gap-2">
          <Loader2 className="w-4 h-4 text-primary animate-spin mt-0.5 shrink-0" />
          <div>
            <b>{elsewhere.length} other project{elsewhere.length === 1 ? "" : "s"} still running.</b>{" "}
            <span className="text-muted-foreground">
              {elsewhere.map((r) => `${(r.steps || [])[r.cursor] || "finishing"}`).join(", ")} — they carry on
              without this view.
            </span>
          </div>
        </Card>
      )}

      {backendRun && backendRun.status === "running" && (
        <Card className="p-3 mb-4 text-sm space-y-1.5">
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-2">
              <Loader2 className="w-4 h-4 text-primary animate-spin" />
              <b>Background run:</b>
              <span className="text-muted-foreground">
                step {Math.min((backendRun.cursor || 0) + 1, (backendRun.steps || []).length)} of{" "}
                {(backendRun.steps || []).length} — {(backendRun.steps || [])[backendRun.cursor] || "finishing"}
              </span>
            </div>
            <div className="flex gap-1.5">
              <Button size="sm" variant="secondary"
                      onClick={async () => { await api.setWorkflowRunStatus(activeProjectId, "paused"); refreshRuns(); }}>
                Pause
              </Button>
              <Button size="sm" variant="ghost"
                      onClick={async () => { await api.setWorkflowRunStatus(activeProjectId, "cancelled"); refreshRuns(); }}>
                Cancel
              </Button>
            </div>
          </div>
          {(backendRun.log || []).slice(-3).map((l, i) => (
            <div key={i} className="text-xs text-muted-foreground">{l.message}</div>
          ))}
        </Card>
      )}
      {backendRun && backendRun.status === "paused" && (
        <Card className="p-3 mb-4 text-sm flex items-center justify-between gap-2">
          <span className="text-muted-foreground">Background run paused at{" "}
            {(backendRun.steps || [])[backendRun.cursor] || "the end"}.</span>
          <Button size="sm" onClick={async () => { await api.setWorkflowRunStatus(activeProjectId, "running"); refreshRuns(); }}>
            Resume
          </Button>
        </Card>
      )}

      <Card className="p-4 mb-4 flex flex-wrap items-center gap-4">
        <Button size="lg" disabled={runningAll || !!runningStageId} onClick={runFullPipeline} className="shrink-0">
          {runningAll ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <Play className="w-4 h-4 mr-2" />}
          Run full pipeline
        </Button>
        <Button size="lg" variant="secondary" onClick={startBackgroundRun} className="shrink-0"
                title="Sequenced by the backend from this project's saved data — survives switching project, reloading, or a crash">
          <Play className="w-4 h-4 mr-2" />
          Run in background
        </Button>
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <Switch checked={includeUpload} onCheckedChange={setIncludeUpload} />
          Include upload/publish
        </label>
        <label className="flex items-center gap-2 text-xs text-muted-foreground"
               title="One animated clip per song, on its hook section. 10–35 GPU-minutes each out of a 30-hour week.">
          <Switch checked={includeVideoGen} onCheckedChange={setIncludeVideoGen} />
          Include hero clips
        </label>
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <Switch checked={stopOnError} onCheckedChange={setStopOnError} />
          Stop on first failed stage
        </label>
        <Button size="sm" variant="ghost" className="ml-auto" onClick={refreshSongs} title="Refresh song list">
          <RotateCw className="w-3.5 h-3.5 mr-1.5" />Refresh
        </Button>
      </Card>

      <div className="space-y-2">
        {stages.map((stage) => {
          const Icon = stage.icon;
          const state = stageState[stage.id] || { status: "idle" };
          const StatusIcon = STATUS_ICON[state.status];
          const pendingList = stage.pending ? stage.pending(songs) : null;
          const busy = runningStageId === stage.id;
          return (
            <Card key={stage.id} className="p-3 flex items-start gap-3">
              <Icon className="w-5 h-5 text-muted-foreground shrink-0 mt-0.5" />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="font-semibold text-sm">{stage.label}</span>
                  {pendingList != null && (
                    <Badge variant={pendingList.length ? "outline" : "secondary"} className="text-[10px]">
                      {pendingList.length} pending
                    </Badge>
                  )}
                  <StatusIcon className={`w-3.5 h-3.5 ${STATUS_COLOR[busy ? "running" : state.status]}`} />
                </div>
                <p className="text-xs text-muted-foreground mt-0.5">{stage.hint}</p>
                {state.note && (
                  <p className={`text-xs mt-1 font-mono ${state.status === "error" ? "text-red-500" : "text-muted-foreground"}`}>{state.note}</p>
                )}
              </div>
              <Button
                size="sm" variant="outline" className="shrink-0"
                disabled={runningAll || !!runningStageId || (pendingList != null && pendingList.length === 0)}
                onClick={() => runOne(stage)}
              >
                {busy ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" /> : <Play className="w-3.5 h-3.5 mr-1.5" />}
                Run
              </Button>
            </Card>
          );
        })}
      </div>

      {log.length > 0 && (
        <Card className="p-3 mt-4">
          <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-2">Run log</div>
          <div className="space-y-0.5 max-h-64 overflow-y-auto scroll-thin font-mono text-[11px]">
            {log.map((l, i) => (
              <div key={i} className={l.level === "error" ? "text-red-500" : l.level === "success" ? "text-emerald-500" : "text-muted-foreground"}>
                {new Date(l.t).toLocaleTimeString()} — {l.message}
              </div>
            ))}
          </div>
        </Card>
      )}

      <p className="text-[11px] text-muted-foreground mt-4">
        Individual stages still have their own dedicated pages for review and manual tweaks —
        <button className="underline mx-1" onClick={() => navigate("/jobs")}>Jobs</button>
        shows every queued/running job this page kicks off.
      </p>
    </div>
  );
}
