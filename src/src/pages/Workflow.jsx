import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { stopAllStarted } from "../lib/serverLifecycle";
import { autoStartKaggleServer, getKaggleState } from "../lib/kaggleServerPipeline";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Switch } from "../components/ui/switch";
import {
  Workflow as WorkflowIcon, BookText, Music2, BarChart3, Image as ImageIcon, Layers, Film,
  UploadCloud, Play, Loader2, CheckCircle2, XCircle, CircleDashed, RotateCw,
} from "lucide-react";
import { toast } from "sonner";
import GuidedPanel from "../components/GuidedPanel";
import { workflowFlow } from "../lib/guidedFlows";

// One row per pipeline stage, in the order they actually have to run. `pending(songs)` — when
// present — returns the subset of songs this stage still needs to touch, which both drives the
// "N songs" badge and is what `run()` iterates; stages without it (lyrics generation, which
// creates a song rather than acting on existing ones; images/overlays, which already have their
// own bulk backend commands scoped to the whole project) leave it out.
function buildStages({ activeProjectId, includeUpload }) {
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
      id: "overlays",
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

const STATUS_ICON = { idle: CircleDashed, running: Loader2, done: CheckCircle2, error: XCircle };
const STATUS_COLOR = { idle: "text-muted-foreground/50", running: "text-primary animate-spin", done: "text-emerald-500", error: "text-red-500" };

export default function Workflow() {
  const { activeProjectId, activeProject, songs, refreshSongs, jobs } = useStudio();
  // How far the guided run should go — read by the pipeline runner below.
  const [guidedStopAfter, setGuidedStopAfter] = useState("video");
  const navigate = useNavigate();
  const [includeUpload, setIncludeUpload] = useState(false);
  const [stopOnError, setStopOnError] = useState(true);
  const [runningStageId, setRunningStageId] = useState(null);
  const [runningAll, setRunningAll] = useState(false);
  const [stageState, setStageState] = useState({}); // id -> { status, note }
  const [log, setLog] = useState([]); // [{t, level, message}]
  const [engines, setEngines] = useState(null); // { music_engine, image_engine } — for the browser-dependency banner

  useEffect(() => {
    if (!activeProjectId) return;
    api.getSettings(activeProjectId)
      .then((s) => setEngines({ music_engine: s?.music_engine || "suno", image_engine: s?.image_engine || "midjourney" }))
      .catch(() => {});
  }, [activeProjectId]);

  const stages = useMemo(() => buildStages({ activeProjectId, includeUpload }), [activeProjectId, includeUpload]);
  const activeJobsCount = jobs.filter((j) => j.status === "queued" || j.status === "running").length;
  // Suno is the only engine driven by the embedded Browser tab (no public API) — everything
  // else here (ACE-Step/HeartMuLa, Midjourney's proxy, ComfyUI/FLUX) is a plain REST job, so a
  // project without Suno as its music engine can run "Run full pipeline" fully unattended.
  const needsBrowser = engines?.music_engine === "suno";

  const pushLog = (message, level = "info") => setLog((prev) => [...prev.slice(-199), { t: Date.now(), level, message }]);

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
    await runStage(stage);
  };

  // Kaggle GPU engines the app can start on demand (Suno/Midjourney are browser flows, not servers).
  const KAGGLE_ENGINES = { music: ["heartmula", "acestep"], images: ["comfyui", "flux"] };

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
      <div className="p-8">
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
      <Card className="p-4 mb-4 flex flex-wrap items-center gap-4">
        <Button size="lg" disabled={runningAll || !!runningStageId} onClick={runFullPipeline} className="shrink-0">
          {runningAll ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <Play className="w-4 h-4 mr-2" />}
          Run full pipeline
        </Button>
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <Switch checked={includeUpload} onCheckedChange={setIncludeUpload} />
          Include upload/publish
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
