import { useEffect, useState, useMemo, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { usePageActions } from "../lib/pageActions";
import { runPipeline, subscribePipeline, cancelPipeline, continuePipeline, suggestChannelForSong } from "../lib/genPipeline";
import GuidedPanel from "../components/GuidedPanel";
import { musicFlow } from "../lib/guidedFlows";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Progress } from "../components/ui/progress";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { 
  DropdownMenu, 
  DropdownMenuContent, 
  DropdownMenuItem, 
  DropdownMenuTrigger 
} from "../components/ui/dropdown-menu";
import { 
  Mic2, 
  Play, 
  Sparkles, 
  Download, 
  Volume2, 
  RefreshCw, 
  CheckCircle2, 
  FileAudio, 
  FolderDown, 
  Loader2, 
  Activity,
  AlertTriangle,
  Search,
  X,
  ChevronDown,
  ChevronUp,
  Save,
  Pencil,
  StopCircle,
  SkipForward,
  HardDriveDownload,
  LogIn,
  Globe,
} from "lucide-react";
import { toast } from "sonner";
import GenerationProgress from "../components/GenerationProgress";
import { getStepForPath } from "../lib/pageSteps";

const ENGINE_NAME = { heartmula: "HeartMuLa", acestep: "ACE-Step", suno: "Suno" };

export default function MusicGen() {
  const { activeProjectId, songs, refreshSongs, selectSong, activeSongId } = useStudio();
  const nav = useNavigate();
  const [jobs, setJobs] = useState([]);
  const [engine, setEngine] = useState("suno");
  const [convertingSongId, setConvertingSongId] = useState(null);
  const [bulkDownloading, setBulkDownloading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  // Which songs the guide's "Start" step should act on.
  const [guidedScope, setGuidedScope] = useState("one");

  // ── AI generation pipeline (Suno) ──
  const [channels, setChannels] = useState([]);
  const [runChannelId, setRunChannelId] = useState("auto"); // "auto" | channel id | ""
  const [pauseBetween, setPauseBetween] = useState(false); // bulk: wait for "Continue" between songs
  const [pipeline, setPipeline] = useState(null);
  const pipelineActive = pipeline && !["idle", "done", "error", "cancelled"].includes(pipeline.status);
  const logEndRef = useRef(null);

  useEffect(() => { api.listChannels().then(setChannels).catch(() => {}); }, []);
  useEffect(() => subscribePipeline(setPipeline), []);
  // The pipeline writes straight to the DB as each song finishes — pick that up so cards
  // update (audio player, download links) without the user having to refresh manually.
  useEffect(() => {
    if (!pipeline) return;
    refreshSongs();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pipeline?.results]);
  useEffect(() => { logEndRef.current?.scrollIntoView({ block: "nearest" }); }, [pipeline?.log?.length]);

  // "auto" prefers the channel the song was actually generated for (carried from AI Composer's
  // targets since import) — the language-match guess is only a fallback for older songs that
  // predate that link, so it can't clobber it or send a channel's downloads to the wrong folder.
  const resolveChannelId = (song) => {
    if (runChannelId === "auto") return song.channel_id || suggestChannelForSong(song, channels)?.id || null;
    return runChannelId || null;
  };

  // ── Per-song expandable details (title / styles / lyrics) ──
  const [expanded, setExpanded] = useState(() => new Set());
  const [drafts, setDrafts] = useState({}); // songId -> { title, styles, lyrics }
  const [savingId, setSavingId] = useState(null);

  const draftFor = (song) => drafts[song.id] || { title: song.title, styles: song.styles, lyrics: song.lyrics || "" };
  const isDirty = (song) => {
    const d = draftFor(song);
    return d.title !== song.title || d.styles !== song.styles || (d.lyrics || "") !== (song.lyrics || "");
  };

  const toggleExpanded = (id) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  const expandSong = (id) => setExpanded((prev) => new Set(prev).add(id));

  const updateDraft = (song, patch) => {
    setDrafts((prev) => ({ ...prev, [song.id]: { ...draftFor(song), ...patch } }));
  };

  const saveDraft = async (song) => {
    const d = draftFor(song);
    if (!d.title.trim()) return toast.error("Title can't be empty.");
    setSavingId(song.id);
    try {
      await api.updateSong(song.id, { title: d.title.trim(), styles: d.styles, lyrics: d.lyrics });
      await refreshSongs();
      toast.success(`Saved "${d.title.trim()}".`);
    } catch (err) {
      toast.error("Failed to save: " + err);
    } finally {
      setSavingId(null);
    }
  };

  // ── Active song title, editable straight from the top bar ──
  const activeSong = songs.find((s) => s.id === activeSongId) || null;
  const [titleDraft, setTitleDraft] = useState("");
  useEffect(() => { setTitleDraft(activeSong?.title || ""); }, [activeSongId, activeSong?.title]);

  const saveActiveTitle = async () => {
    if (!activeSong) return;
    const t = titleDraft.trim();
    if (!t) { toast.error("Title can't be empty."); setTitleDraft(activeSong.title); return; }
    if (t === activeSong.title) return;
    try {
      await api.updateSong(activeSong.id, { title: t });
      await refreshSongs();
      toast.success("Title updated.");
    } catch (err) {
      toast.error("Failed to update title: " + err);
    }
  };

  // Which audio engine is selected — decides whether Generate drives the embedded browser
  // (Suno) or queues an API job (ACE-Step / HeartMuLa).
  useEffect(() => {
    api.getSettings(activeProjectId).then((s) => setEngine(s?.music_engine || "suno")).catch(() => {});
  }, [activeProjectId]);

  const handleSelectVariant = async (sid, variant) => {
    try {
      await api.selectSongVariant(sid, variant);
      await refreshSongs();
      toast.success(`Switched to Version ${variant}!`);
    } catch (err) {
      toast.error(`Failed to switch variant: ${err}`);
    }
  };

  useEffect(() => { 
    refreshSongs(); 
  }, [refreshSongs]);

  // Poll background job queue for real-time progress updates
  useEffect(() => {
    const fetchJobs = () => {
      api.listJobs().then(setJobs).catch(() => {});
    };
    fetchJobs();
    const t = setInterval(fetchJobs, 2000);
    return () => clearInterval(t);
  }, []);

  // A blank-lyrics song still gets enqueued and only fails deep inside engine-specific job
  // code with a generic "generation failed" error — check up front so the failure is instant
  // and points at the actual problem (an empty lyrics field AI Composer/import never filled).
  const missingLyrics = (song) => !song?.lyrics?.trim();

  const trigger = async (sid) => {
    const song = songs.find((s) => s.id === sid);
    if (missingLyrics(song)) {
      expandSong(sid);
      return toast.error(`"${song?.title || "This song"}" has no lyrics yet — expand the card below and add lyrics first.`);
    }
    try {
      await api.genMusic(sid);
      toast.success(`Music generation queued (${engine})`);
      setTimeout(refreshSongs, 2000);
    } catch (err) {
      toast.error(String(err));
    }
  };

  // ── Suno's browser fallback ───────────────────────────────────────────────
  //
  // Suno generates over plain HTTP like every other engine, as a queued job. The visible-browser
  // pipeline is what is left when that cannot work — a cookie Suno has stopped accepting, or a
  // change to their API that the wrapper has not caught up with. It is a fallback and not the route
  // because it needs a webview: it cannot run on a phone, cannot run headless, and holds the app on
  // the Browser tab while it works.
  //
  // Offered rather than taken automatically. Opening a window that drives somebody's own Suno
  // account, unasked, in the middle of a batch, is not a thing to do on their behalf.
  const [browserOffer, setBrowserOffer] = useState(null);   // { songs, why }

  /** Does this failed job look like something the browser could still do? */
  const browserCouldHelp = (log) => {
    const t = String(log || "").toLowerCase();
    return t.includes("cookie") || t.includes("expired") || t.includes("401")
        || t.includes("clerk") || t.includes("wrapper");
  };

  const runInBrowser = (list, why) => {
    if (pipelineActive) return toast.error("A Suno generation is already running — Suno only runs one song at a time.");
    setBrowserOffer(null);
    toast.message(`Running ${list.length === 1 ? `"${list[0].title}"` : `${list.length} songs`} in the browser…`,
                  { description: why });
    runPipeline(list, { projectId: activeProjectId, channelId: resolveChannelId, autoAdvance: !pauseBetween });
  };

  // Watch this project's Suno jobs; when one fails for a reason the browser could get past, offer it.
  useEffect(() => {
    if (engine !== "suno" || pipelineActive || browserOffer) return;
    const failed = (jobs || []).filter(
      (j) => j.kind === "music" && j.status === "failed" && browserCouldHelp((j.logs || []).join(" ")),
    );
    if (!failed.length) return;
    const stuck = songs.filter((s) => !s.audio_url && failed.some((j) => j.target_id === s.id));
    if (stuck.length) {
      setBrowserOffer({
        songs: stuck,
        why: "Suno would not answer over HTTP — usually an expired session.",
      });
    }
  }, [jobs, songs, engine, pipelineActive, browserOffer]);

  const triggerAll = async () => {
    const pending = songs.filter((x) => !x.audio_url);
    const ready = pending.filter((s) => !missingLyrics(s));
    const skipped = pending.filter((s) => missingLyrics(s));
    if (skipped.length) {
      toast.warning(`Skipping ${skipped.length} song${skipped.length > 1 ? "s" : ""} with no lyrics yet: ${skipped.map((s) => s.title).join(", ")}`);
    }
    if (!ready.length) return toast.error(skipped.length ? "Every pending song is missing lyrics." : "Nothing to generate.");

    let queued = 0;
    for (const s of ready) {
      try {
        await api.genMusic(s.id);
        queued++;
      } catch (err) {
        toast.error(`"${s.title}": ${err}`);
      }
    }
    if (queued) toast.success(`Queued ${queued} generation job${queued > 1 ? "s" : ""}`);
    setTimeout(refreshSongs, 2000);
  };

  const handleDownload = async (song, format) => {
    if (!song.audio_url) return;
    
    const safeTitle = song.title.replace(/[^a-zA-Z0-9\s-_]/g, "_").trim();
    const filename = `${safeTitle}.${format}`;
    
    if (format === "mp3") {
      // Direct MP3 download via browser blob download
      toast.promise(
        (async () => {
          const res = await fetch(song.audio_url);
          const blob = await res.blob();
          const blobUrl = URL.createObjectURL(blob);
          const a = document.createElement("a");
          a.href = blobUrl;
          a.download = filename;
          a.click();
          URL.revokeObjectURL(blobUrl);
        })(),
        {
          loading: "Preparing MP3 file for download...",
          success: "MP3 downloaded successfully!",
          error: "Failed to download MP3."
        }
      );
    } else {
      // WAV or FLAC - call the Rust backend conversion
      setConvertingSongId(song.id);
      
      toast.promise(
        (async () => {
          try {
            const destPath = await api.downloadAudio(song.audio_url, format, filename);
            return destPath;
          } finally {
            setConvertingSongId(null);
          }
        })(),
        {
          loading: `Downloading Suno MP3 & converting losslessly to Spotify-compliant ${format.toUpperCase()} (PCM 16-bit, 44.1kHz stereo) via FFmpeg... Please wait.`,
          success: (path) => `Converted successfully! Saved to: ${path}`,
          error: (err) => `Export cancelled or failed: ${err}`
        }
      );
    }
  };

  const handleBulkDownload = async (format) => {
    const readySongs = songs.filter(s => s.audio_url);
    if (!readySongs.length) {
      toast.error("No ready songs found to download.");
      return;
    }

    setBulkDownloading(true);

    const downloadInfo = readySongs.map(s => ({
      audio_url: s.audio_url,
      title: s.title,
      format: format
    }));

    toast.promise(
      (async () => {
        try {
          const count = await api.downloadAllAudio(downloadInfo);
          return count;
        } finally {
          setBulkDownloading(false);
        }
      })(),
      {
        loading: `Opening folder picker... Preparing bulk download and losslessly converting ${readySongs.length} songs to Spotify-compliant ${format.toUpperCase()} format via FFmpeg... Please keep app open.`,
        success: (count) => `Successfully converted & saved ${count} files to your chosen folder!`,
        error: (err) => `Bulk download cancelled or failed: ${err}`
      }
    );
  };

  // Page-wide controls in the top bar instead of the page body — only once a project (and
  // hence the rest of this page) is actually showing.
  usePageActions(useMemo(() => {
    if (!activeProjectId) return null;
    return (
      <>
        {activeSong && (
          <Input
            value={titleDraft}
            onChange={(e) => setTitleDraft(e.target.value)}
            onBlur={saveActiveTitle}
            onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }}
            placeholder="Song title…"
            className="h-8 text-sm w-48"
            title="Title of the selected song — the one AI Composer generated (or set one here if it's blank)"
          />
        )}
        {songs.some((s) => s.audio_url) && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size="sm" variant="outline" disabled={bulkDownloading}>
                {bulkDownloading ? (
                  <>
                    <Loader2 className="w-4 h-4 mr-2 animate-spin text-primary" />
                    Bulk Exporting...
                  </>
                ) : (
                  <>
                    <FolderDown className="w-4 h-4 mr-2 text-primary" />
                    Bulk Export All
                  </>
                )}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-64">
              <DropdownMenuItem onClick={() => handleBulkDownload("mp3")}>
                <FileAudio className="w-4 h-4 mr-2 text-muted-foreground" />
                Download all as MP3
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => handleBulkDownload("wav")}>
                <Sparkles className="w-4 h-4 mr-2 text-primary animate-pulse" />
                Convert &amp; Export all as WAV (Spotify ready)
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => handleBulkDownload("flac")}>
                <Volume2 className="w-4 h-4 mr-2 text-primary" />
                Convert &amp; Export all as FLAC (Lossless)
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        )}
        <Button size="sm" data-testid="musicgen-batch-btn" onClick={triggerAll} disabled={!songs.length || (engine === "suno" && pipelineActive)}>
          {engine === "suno" && pipelineActive ? (
            <>
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
              Generating…
            </>
          ) : (
            <>
              <Sparkles className="w-4 h-4 mr-2" />
              Generate all
            </>
          )}
        </Button>
      </>
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeProjectId, songs, bulkDownloading, activeSong, titleDraft, engine, pipelineActive]));

  if (!activeProjectId) {
    return (
      <div className="p-4 sm:p-6 lg:p-8">
        <Card className="p-10 text-center text-muted-foreground border-dashed">
          Select a project first.
        </Card>
      </div>
    );
  }

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step {getStepForPath("/music")}</div>

      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Music Studio</h1>
      </div>
      <p className="text-muted-foreground mb-8 max-w-2xl">
        Renders each song on the engine you picked, as a queued job you can watch here. Suno goes over
        plain HTTP like the rest; if its session will not answer, the browser route is offered as a
        fallback rather than taken on your behalf.
      </p>

      {/* The browser fallback, offered only when the HTTP route has actually failed for a reason it
          could get past. Dismissible, because "try it in the browser" is not always the answer. */}
      {browserOffer && (
        <Card className="p-4 mb-6 border-amber-500/40 bg-amber-500/5">
          <div className="flex items-start gap-3 flex-wrap">
            <AlertTriangle className="w-4 h-4 text-amber-500 shrink-0 mt-0.5" />
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium">
                {browserOffer.songs.length === 1
                  ? `"${browserOffer.songs[0].title}" did not generate`
                  : `${browserOffer.songs.length} songs did not generate`}
              </div>
              <p className="text-xs text-muted-foreground mt-0.5">
                {browserOffer.why} The browser route signs in as you and fills Suno's own page, so it
                works when the session does not — but it needs this window, and it runs one song at a time.
              </p>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              <Button size="sm" variant="ghost" onClick={() => setBrowserOffer(null)}>Not now</Button>
              <Button size="sm" onClick={() => runInBrowser(browserOffer.songs, browserOffer.why)}>
                <Globe className="w-3.5 h-3.5 mr-1.5" />Run it in the browser
              </Button>
            </div>
          </div>
        </Card>
      )}

      {/* Guided path: engine, scope and length as three questions, with the reasons for each. */}
      <div className="mb-6">
        <GuidedPanel
          flow={musicFlow}
          projectId={activeProjectId}
          extraCtx={{ pendingCount: songs.filter((x) => !x.audio_url).length }}
          actions={{
            // Persisting the engine here is the point: every later step and every prompt the backend
            // builds reads it from settings, not from this page's state.
            setEngine: async (id) => {
              setEngine(id);
              try { await api.saveSettings({ music_engine: id }, activeProjectId); } catch { /* shown by the guide */ }
            },
            setScope: (scope) => setGuidedScope(scope),
            setDuration: async (seconds) => {
              try { await api.saveSettings({ music_duration_s: seconds }, activeProjectId); } catch { /* non-fatal */ }
            },
            run: () => (guidedScope === "one" && activeSongId ? trigger(activeSongId) : triggerAll()),
          }}
        />
      </div>

      {/* Premium Fuzzy Search Bar */}
      <div className="relative mb-6 max-w-md">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground/60" />
        <input
          type="text"
          placeholder="Fuzzy search songs by title or style..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="w-full bg-muted/20 hover:bg-muted/30 focus:bg-muted/40 text-sm pl-9 pr-8 py-2 rounded-lg border border-border/80 outline-none focus:ring-1 focus:ring-primary/60 transition-all text-foreground"
        />
        {searchQuery && (
          <button
            onClick={() => setSearchQuery("")}
            className="absolute right-2.5 top-1/2 -translate-y-1/2 p-0.5 rounded-full hover:bg-muted text-muted-foreground/60 hover:text-foreground transition-all"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        )}
      </div>

      {/* Suno pipeline: destination channel for downloads + live run status/controls */}
      {engine === "suno" && (
        <Card className="p-4 mb-6 space-y-2.5">
          <div className="flex items-center justify-between gap-3 flex-wrap">
            <div className="flex items-center gap-2 text-xs">
              <span className="text-muted-foreground uppercase tracking-widest text-[10px]">Save downloads to channel</span>
              <select
                value={runChannelId}
                onChange={(e) => setRunChannelId(e.target.value)}
                disabled={pipelineActive}
                className="h-8 text-xs rounded-md border border-border bg-background px-2"
                title="Which channel's project subfolder finished clips are downloaded into — Auto picks by matching language, same rule Upload uses"
              >
                <option value="auto">Auto (match language)</option>
                {channels.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
              </select>
              <label className="flex items-center gap-1.5 text-muted-foreground cursor-pointer select-none">
                <input type="checkbox" checked={pauseBetween} onChange={(e) => setPauseBetween(e.target.checked)} disabled={pipelineActive} className="accent-primary" />
                Pause for review between songs (bulk)
              </label>
            </div>
            {pipelineActive && (
              <div className="flex items-center gap-2">
                {pipeline.status === "awaiting-continue" && (
                  <Button size="sm" className="h-8" onClick={continuePipeline}>
                    <SkipForward className="w-3.5 h-3.5 mr-1.5" />Continue to next song
                  </Button>
                )}
                {pipeline.status === "awaiting-login" && (
                  <Badge variant="outline" className="text-amber-500 border-amber-500/50 gap-1.5">
                    <LogIn className="w-3 h-3" />waiting for sign-in in the Browser tab
                  </Badge>
                )}
                <Button size="sm" variant="outline" className="h-8 text-red-500 hover:bg-red-500/10" onClick={cancelPipeline}>
                  <StopCircle className="w-3.5 h-3.5 mr-1.5" />Cancel
                </Button>
              </div>
            )}
          </div>
          {pipeline && pipeline.queue.length > 0 && (pipelineActive || pipeline.log.length > 0) && (
            <div className="space-y-1.5">
              <div className="flex items-center justify-between text-xs">
                <span className="flex items-center gap-1.5 font-medium">
                  {pipelineActive && <Loader2 className="w-3.5 h-3.5 animate-spin text-primary" />}
                  Song {Math.min(pipeline.index + 1, pipeline.queue.length)}/{pipeline.queue.length}
                  {pipeline.currentStep && <span className="text-muted-foreground font-normal">— {pipeline.currentStep}</span>}
                </span>
                <Badge variant={pipeline.status === "error" ? "destructive" : "secondary"} className="text-[10px]">{pipeline.status}</Badge>
              </div>
              <div className="max-h-24 overflow-y-auto scroll-thin text-[10px] font-mono space-y-0.5 bg-muted/30 rounded-md p-2">
                {pipeline.log.slice(-30).map((l, i) => (
                  <div key={i} className={l.level === "error" ? "text-red-500" : l.level === "warn" ? "text-amber-500" : l.level === "success" ? "text-emerald-500" : "text-muted-foreground"}>
                    {l.message}
                  </div>
                ))}
                <div ref={logEndRef} />
              </div>
            </div>
          )}
        </Card>
      )}

      <div className="grid md:grid-cols-2 gap-5">
        {songs
          .filter(s => {
            const q = searchQuery.toLowerCase();
            return s.title.toLowerCase().includes(q) || s.styles.toLowerCase().includes(q);
          })
          .map(s => {
          // Check for active background jobs
          const activeJob = jobs.find(
            j => j.target_id === s.id &&
            j.kind === "music" &&
            (j.status === "queued" || j.status === "running")
          );
          const pipelineRunningThis = pipelineActive && pipeline?.currentSongId === s.id;
          const pipelineQueuedThis = pipelineActive && !pipelineRunningThis && pipeline?.queue?.includes(s.id) && pipeline.queue.indexOf(s.id) > pipeline.index;

          return (
            <Card key={s.id} data-testid={`song-card-${s.id}`}
              className={`p-5 transition-all flex flex-col justify-between ${activeSongId===s.id ? "ring-2 ring-primary bg-muted/10" : "hover:border-primary/40 bg-card/50"}`}
              onClick={() => selectSong(s.id)}>
              
              <div>
                <div className="flex items-start justify-between mb-3 gap-2">
                  <div className="min-w-0">
                    <div className="font-semibold text-lg truncate">{s.title}</div>
                    <div className="text-xs text-muted-foreground italic truncate max-w-[280px]">{s.styles}</div>
                  </div>
                  <div className="flex items-center gap-1.5 shrink-0">
                    {missingLyrics(s) && (
                      <Badge variant="destructive" className="text-[9px]" title="No lyrics — generation will refuse to run">
                        no lyrics
                      </Badge>
                    )}
                    {(() => {
                      const ch = channels.find((c) => c.id === s.channel_id) || suggestChannelForSong(s, channels);
                      return ch ? (
                        <Badge variant="outline" className="text-xs" data-testid={`song-channel-${s.id}`} title={s.channel_id ? "Destination channel" : "Suggested by language match — not yet linked"}>
                          {ch.name}
                        </Badge>
                      ) : null;
                    })()}
                    <Badge variant="secondary" className="text-xs" data-testid={`song-lang-${s.id}`}>
                      {s.language}
                    </Badge>
                  </div>
                </div>

                {/* Retractable details: this song's own title / styles / lyrics — each song is
                    its own language/channel variant, so these can differ (translated lyrics,
                    genre-adapted styles) from every other song generated the same day. */}
                <button
                  type="button"
                  onClick={(e) => { e.stopPropagation(); toggleExpanded(s.id); }}
                  data-testid={`song-expand-${s.id}`}
                  className="w-full flex items-center justify-between text-[11px] text-muted-foreground hover:text-foreground transition-colors py-1 -mt-1 mb-1"
                >
                  <span className="flex items-center gap-1">
                    <Pencil className="w-3 h-3" />
                    Title, styles &amp; lyrics
                  </span>
                  {expanded.has(s.id) ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
                </button>

                {expanded.has(s.id) && (
                  <div className="space-y-2 mb-3 p-3 rounded-lg border border-border/70 bg-muted/10" onClick={(e) => e.stopPropagation()}>
                    <div className="space-y-1">
                      <div className="text-[9px] uppercase tracking-widest text-muted-foreground">Title</div>
                      <Input
                        value={draftFor(s).title}
                        onChange={(e) => updateDraft(s, { title: e.target.value })}
                        className="h-7 text-xs"
                      />
                    </div>
                    <div className="space-y-1">
                      <div className="text-[9px] uppercase tracking-widest text-muted-foreground">Musical styles</div>
                      <Input
                        value={draftFor(s).styles}
                        onChange={(e) => updateDraft(s, { styles: e.target.value })}
                        placeholder="e.g. messianic liquid drum and bass, 174 bpm"
                        className="h-7 text-xs"
                      />
                    </div>
                    <div className="space-y-1">
                      <div className="text-[9px] uppercase tracking-widest text-muted-foreground flex items-center justify-between">
                        <span>Lyrics</span>
                        {!draftFor(s).lyrics?.trim() && <span className="text-destructive normal-case tracking-normal">empty — generation will refuse to run</span>}
                      </div>
                      <Textarea
                        data-testid={`song-lyrics-${s.id}`}
                        value={draftFor(s).lyrics}
                        onChange={(e) => updateDraft(s, { lyrics: e.target.value })}
                        placeholder="No lyrics yet — write or paste them here."
                        rows={6}
                        className="text-xs font-mono"
                      />
                    </div>
                    <div className="flex justify-end">
                      <Button
                        size="sm"
                        className="h-7 text-xs"
                        disabled={!isDirty(s) || savingId === s.id}
                        onClick={() => saveDraft(s)}
                      >
                        <Save className="w-3 h-3 mr-1.5" />
                        {savingId === s.id ? "Saving…" : "Save"}
                      </Button>
                    </div>
                  </div>
                )}

                {/* WAVEFORM OR PROGRESS BOX */}
                <div className="my-4 p-3 rounded-lg border border-border/80 bg-muted/20 relative min-h-[5.5rem] flex flex-col justify-center">
                  {activeJob ? (
                    <GenerationProgress job={activeJob} label={`${ENGINE_NAME[engine] || "Music"} generation`} />
                  ) : pipelineRunningThis ? (
                    <div className="space-y-1.5">
                      <div className="flex items-center gap-1.5 text-xs font-semibold text-primary">
                        <Loader2 className="w-3.5 h-3.5 animate-spin" />
                        Suno pipeline — {pipeline.currentStep || pipeline.status}
                      </div>
                      <div className="text-[10px] text-mono text-muted-foreground truncate">
                        {pipeline.log[pipeline.log.length - 1]?.message || "Working…"}
                      </div>
                    </div>
                  ) : pipelineQueuedThis ? (
                    <div className="flex items-center justify-center gap-1.5 text-xs text-muted-foreground py-2">
                      <Loader2 className="w-3.5 h-3.5" />
                      Queued in the Suno pipeline — song {pipeline.queue.indexOf(s.id) + 1}/{pipeline.queue.length}
                    </div>
                  ) : s.audio_url ? (
                    <div className="flex flex-col gap-2">
                      <div className="flex items-center justify-between">
                        <div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground flex items-center gap-1.5">
                          <Volume2 className="w-3.5 h-3.5 text-primary animate-pulse" />
                          Studio Prelisten
                          {s.local_audio_path && (
                            <span className="flex items-center gap-1 normal-case tracking-normal text-emerald-500" title={s.local_audio_path}>
                              <HardDriveDownload className="w-3 h-3" />saved
                            </span>
                          )}
                        </div>
                        {s.audio_url_alt && (
                          <div className="flex gap-1 p-0.5 rounded bg-muted border border-border/80" onClick={e => e.stopPropagation()}>
                            <Button 
                              size="xs" 
                              variant={s.audio_url === s.audio_url_primary ? "default" : "ghost"}
                              className="h-5 text-[9px] px-1.5 rounded-sm py-0"
                              onClick={() => handleSelectVariant(s.id, 1)}>
                              v1
                            </Button>
                            <Button 
                              size="xs" 
                              variant={s.audio_url === s.audio_url_alt ? "default" : "ghost"}
                              className="h-5 text-[9px] px-1.5 rounded-sm py-0"
                              onClick={() => handleSelectVariant(s.id, 2)}>
                              v2
                            </Button>
                          </div>
                        )}
                      </div>
                      <audio
                        // Prefer the local file the pipeline already downloaded (no network
                        // round-trip, still playable once the CDN url eventually expires) —
                        // falls back to the remote CDN url for songs generated the old way.
                        src={
                          s.audio_url === s.audio_url_primary && s.local_audio_path ? convertFileSrc(s.local_audio_path)
                          : s.audio_url === s.audio_url_alt && s.local_audio_path_alt ? convertFileSrc(s.local_audio_path_alt)
                          : s.audio_url
                        }
                        controls
                        className="w-full h-8 rounded bg-transparent focus:outline-none"
                        onClick={(e) => e.stopPropagation()}
                      />
                    </div>
                  ) : (
                    <div className="flex flex-col items-center justify-center gap-1 text-xs text-muted-foreground text-mono py-2">
                      <AlertTriangle className="w-4 h-4 text-muted-foreground/60 mb-0.5" />
                      — no audio generated yet —
                    </div>
                  )}
                </div>
              </div>

              {/* CARD CONTROLS */}
              <div className="flex items-center justify-between gap-3 mt-2">
                <div className="flex items-center gap-1.5">
                  <Badge variant={s.audio_url ? "default" : "outline"} data-testid={`song-status-${s.id}`}>
                    {activeJob ? "generating" : s.status}
                  </Badge>
                  {s.duration > 0 && (
                    <span className="text-[10px] text-mono text-muted-foreground">
                      {Math.floor(s.duration / 60)}:{(Math.round(s.duration % 60)).toString().padStart(2, '0')}
                    </span>
                  )}
                </div>

                <div className="flex gap-2">
                  {s.audio_url && (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
                        <Button size="sm" variant="secondary" disabled={convertingSongId === s.id}>
                          {convertingSongId === s.id ? (
                            <>
                              <Loader2 className="w-3.5 h-3.5 mr-1 animate-spin text-primary" />
                              Exporting...
                            </>
                          ) : (
                            <>
                              <Download className="w-3.5 h-3.5 mr-1.5" />
                              Download
                            </>
                          )}
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="w-56">
                        <DropdownMenuItem onClick={(e) => { e.stopPropagation(); handleDownload(s, "mp3"); }}>
                          <FileAudio className="w-4 h-4 mr-2 text-muted-foreground" />
                          Download MP3
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={(e) => { e.stopPropagation(); handleDownload(s, "wav"); }}>
                          <Sparkles className="w-4 h-4 mr-2 text-primary animate-pulse" />
                          Spotify WAV (Optimized)
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={(e) => { e.stopPropagation(); handleDownload(s, "flac"); }}>
                          <Volume2 className="w-4 h-4 mr-2 text-primary" />
                          Spotify FLAC (Optimized)
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  )}

                  <Button size="sm" disabled={!!activeJob || (engine === "suno" && pipelineActive)} data-testid={`song-genmusic-${s.id}`}
                    onClick={(e) => { e.stopPropagation(); trigger(s.id); }}>
                    {activeJob || pipelineRunningThis ? (
                      <>
                        <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                        Generating
                      </>
                    ) : pipelineQueuedThis ? (
                      "Queued"
                    ) : (
                      <>
                        <Mic2 className="w-3.5 h-3.5 mr-1.5" />
                        {s.audio_url ? "Re-gen" : "Generate"}
                      </>
                    )}
                  </Button>
                </div>
              </div>

            </Card>
          );
        })}
        
        {!songs.length && (
          <Card className="p-10 col-span-full text-center text-muted-foreground border-dashed">
            No songs found. Please import lyrics first.
          </Card>
        )}
      </div>
    </div>
  );
}
