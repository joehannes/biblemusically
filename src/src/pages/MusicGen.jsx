import { useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Progress } from "../components/ui/progress";
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
  X
} from "lucide-react";
import { toast } from "sonner";
import { getStepForPath } from "../lib/pageSteps";

export default function MusicGen() {
  const { activeProjectId, songs, refreshSongs, selectSong, activeSongId } = useStudio();
  const [jobs, setJobs] = useState([]);
  const [convertingSongId, setConvertingSongId] = useState(null);
  const [bulkDownloading, setBulkDownloading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");

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

  const trigger = async (sid) => { 
    await api.genMusic(sid); 
    toast.success("Music generation queued (Suno)"); 
    setTimeout(refreshSongs, 2000); 
  };

  const triggerAll = async () => { 
    for (const s of songs.filter(x => !x.audio_url)) {
      await api.genMusic(s.id); 
    }
    toast.success(`Queued ${songs.length} generation jobs`); 
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

  if (!activeProjectId) {
    return (
      <div className="p-8">
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
        
        <div className="flex items-center gap-3">
          {songs.some(s => s.audio_url) && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" disabled={bulkDownloading}>
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

          <Button data-testid="musicgen-batch-btn" onClick={triggerAll} disabled={!songs.length}>
            <Sparkles className="w-4 h-4 mr-2" />
            Generate all
          </Button>
        </div>
      </div>
      <p className="text-muted-foreground mb-8 max-w-2xl">
        Generate premium song clips using Suno AI. Listen to studio previews, monitor real-time generation progress, and export files optimized directly for Spotify distribution.
      </p>

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
                  <Badge variant="secondary" className="text-xs shrink-0" data-testid={`song-lang-${s.id}`}>
                    {s.language}
                  </Badge>
                </div>

                {/* WAVEFORM OR PROGRESS BOX */}
                <div className="my-4 p-3 rounded-lg border border-border/80 bg-muted/20 relative min-h-[5.5rem] flex flex-col justify-center">
                  {activeJob ? (
                    <div className="space-y-2.5">
                      <div className="flex items-center justify-between text-xs font-semibold text-mono">
                        <span className="flex items-center gap-1 text-primary">
                          <Activity className="w-3.5 h-3.5 animate-pulse" />
                          Suno Generation
                        </span>
                        <span>{activeJob.progress}%</span>
                      </div>
                      
                      <Progress value={activeJob.progress} className="h-1.5" />
                      
                      <div className="text-[10px] text-mono text-muted-foreground truncate">
                        Status: {activeJob.logs && activeJob.logs.length > 0 ? activeJob.logs[activeJob.logs.length - 1].replace(/\[.*\]\s*/, "") : "Queueing job..."}
                      </div>
                    </div>
                  ) : s.audio_url ? (
                    <div className="flex flex-col gap-2">
                      <div className="flex items-center justify-between">
                        <div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground flex items-center gap-1.5">
                          <Volume2 className="w-3.5 h-3.5 text-primary animate-pulse" />
                          Studio Prelisten
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
                        src={s.audio_url} 
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

                  <Button size="sm" disabled={!!activeJob} data-testid={`song-genmusic-${s.id}`} 
                    onClick={(e) => { e.stopPropagation(); trigger(s.id); }}>
                    {activeJob ? (
                      <>
                        <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                        Generating
                      </>
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
