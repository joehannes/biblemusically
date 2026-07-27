import { useEffect, useState, useRef, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import GuidedPanel from "../components/GuidedPanel";
import { imagesFlow } from "../lib/guidedFlows";
import { useStudio } from "../lib/store";
import GenerationProgress from "../components/GenerationProgress";
import ImageLightbox from "../components/ImageLightbox";
import { api } from "../lib/api";
import { usePageActions } from "../lib/pageActions";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Tabs, TabsList, TabsTrigger } from "../components/ui/tabs";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Slider } from "../components/ui/slider";
import { Switch } from "../components/ui/switch";
import { Wand2, Layers, Grid3x3, List as ListIcon, Columns, AlertCircle, CheckCircle2, Clock, Loader2, Globe } from "lucide-react";
import { toast } from "sonner";
import { getStepForPath } from "../lib/pageSteps";
import ImageryStudio from "../components/ImageryStudio";

const FORMATS = [
  { id: "yt", label: "YouTube 16:9", ar: "16:9" },
  { id: "shorts", label: "Shorts 9:16", ar: "9:16" },
  { id: "tiktok", label: "TikTok 9:16", ar: "9:16" },
];

export default function Images() {
  // What the imagery panel last composed. The choice itself reaches the job runner through
  // save_imagery_choice; this is only so the page can say which one is in force, rather than
  // holding a value nothing reads.
  const [imagery, setImagery] = useState(null);
  const { songs, activeSongId, selectSong, activeProjectId } = useStudio();
  const nav = useNavigate();
  const [sections, setSections] = useState([]);
  const [view, setView] = useState("grid");
  const [format, setFormat] = useState("yt");
  const [chaos, setChaos] = useState([0]);
  const [stylize, setStylize] = useState([100]);
  const [video, setVideo] = useState(false);
  const [variantIdx, setVariantIdx] = useState({});
  const [jobs, setJobs] = useState({});  // Track job status per section
  // Guide answers: which sections to cover, which character to carry through, how fine to render.
  const [characters, setCharacters] = useState([]);
  const [coverage, setCoverage] = useState("per_section");
  const [guideCharacter, setGuideCharacter] = useState(null);
  useEffect(() => { api.listCharacters?.().then((r) => setCharacters(Array.isArray(r) ? r : r?.characters || [])).catch(() => {}); }, []);
  const jobPollRef = useRef(null);

  const load = async () => { 
    if (activeSongId) setSections(await api.listSections(activeSongId)); 
  };
  
  useEffect(() => { 
    load(); 
    const t = setInterval(load, 3000); 
    return () => clearInterval(t); 
  }, [activeSongId]);

  useEffect(() => {
    (async () => {
      try {
        await api.ensureMjAutostart();
      } catch (err) {
        console.warn("Midjourney autostart failed", err);
      }
    })();
  }, []);

  // Poll for job status
  useEffect(() => {
    const pollJobs = async () => {
      try {
        const allJobs = await api.listJobs(50);
        const jobMap = {};
        if (Array.isArray(allJobs)) {
          allJobs.forEach(job => {
            // NOTE: list_jobs returns the raw Rust Job — fields are `kind` and `target_id`
            // (NOT `type`/`target`). Reading the wrong names here meant image job status never
            // populated and this page never showed generation progress.
            if (job.target_id && (job.kind === "image" || job.kind === "character_image")) {
              jobMap[job.target_id] = job;
            }
          });
        }
        setJobs(jobMap);
      } catch (e) {
        console.error("Failed to poll jobs", e);
      }
    };
    
    jobPollRef.current = setInterval(pollJobs, 2000);
    return () => clearInterval(jobPollRef.current);
  }, []);

  const gen = async (id) => { 
    try {
      await api.genImage(id); 
      toast.success("Image generation queued"); 
      // Track this in jobs
      setJobs(p => ({ ...p, [id]: { status: "queued", progress: 0, error: null } }));
    } catch (e) {
      toast.error(`Failed to queue image: ${e.message}`);
    }
  };
  
  /** Run the images the guide asked for: every section, the key ones, or a single still. */
  const runGuided = async () => {
    if (!activeSongId) return toast.error("Pick a song first.");
    if (coverage === "per_section") return batch();
    const pick = coverage === "single"
      ? sections.slice(0, 1)
      : sections.filter((s) => /chorus|bridge|hook/i.test(s.name || "")) ;
    const targets = pick.length ? pick : sections.slice(0, 1);
    let queued = 0;
    for (const s of targets) {
      try { await api.genImage(s.id); queued++; } catch (err) { console.warn(err); }
    }
    toast[queued ? "success" : "error"](queued ? `Queued ${queued} image${queued === 1 ? "" : "s"}` : "Could not queue any images");
  };

  const batch = async () => { 
    if (!activeSongId) return; 
    try {
      const r = await api.batchImages(activeSongId); 
      toast.success(`Queued ${r.queued} images`);
    } catch (e) {
      toast.error(`Batch generation failed: ${e.message}`);
    }
  };
  
  const rotateVariant = (id, total) => setVariantIdx(v => ({ ...v, [id]: ((v[id] || 0) + 1) % total }));

  // ── Image explorer (lightbox) ─────────────────────────────────────────────
  const [lightbox, setLightbox] = useState(null); // { sectionId }
  const lbSection = lightbox ? sections.find((s) => s.id === lightbox.sectionId) : null;
  const lbImages = lbSection ? (lbSection.image_variants?.length ? lbSection.image_variants : (lbSection.image_url ? [lbSection.image_url] : [])) : [];
  const lbPrimary = lbSection ? Math.max(0, lbImages.indexOf(lbSection.image_url)) : 0;

  const setSectionPrimary = async (sec, i) => {
    const url = (sec.image_variants || [])[i] || sec.image_url;
    if (!url) return;
    try {
      await api.updateSection(sec.id, { image_url: url });
      // Learn from the pick: the section's mood + prompt keywords are what the user kept.
      const tags = [sec.mood, ...(sec.image_prompt || "").split(/[,\n]/).map((t) => t.trim())].filter(Boolean).slice(0, 4);
      for (const t of tags) api.recordLearningSignal("project", activeProjectId, "kept_image_style", t, null).catch(() => {});
      load();
      toast.success("Set as the primary image.");
    } catch (e) { toast.error(String(e)); }
  };
  const discardSectionVariant = async (sec, i) => {
    const variants = [...(sec.image_variants || [])];
    if (variants.length <= 1) return;
    const removed = variants.splice(i, 1)[0];
    const patch = { image_variants: variants };
    if (sec.image_url === removed) patch.image_url = variants[0];
    try { await api.updateSection(sec.id, patch); load(); }
    catch (e) { toast.error(String(e)); }
  };

  const ar = FORMATS.find(f=>f.id===format).ar;
  const aspect = ar === "16:9" ? "aspect-video" : "aspect-[9/16]";

  const readySongs = songs.filter(s => s.audio_url);
  
  const getStatusIcon = (status, error) => {
    if (error) return <AlertCircle className="w-4 h-4 text-destructive" />;
    if (status === "done") return <CheckCircle2 className="w-4 h-4 text-emerald-500" />;
    if (status === "queued" || status === "pending") return <Clock className="w-4 h-4 text-yellow-500" />;
    if (status === "running") return <Loader2 className="w-4 h-4 text-blue-500 animate-spin" />;
    return null;
  };
  
  const getStatusBadge = (status, error) => {
    if (error) return <Badge variant="destructive">Error</Badge>;
    if (status === "done") return <Badge variant="default">Done</Badge>;
    if (status === "queued" || status === "pending") return <Badge variant="outline">Queued</Badge>;
    if (status === "running") return <Badge variant="outline" className="animate-pulse">Processing...</Badge>;
    return null;
  };

  usePageActions(useMemo(() => (
    <Button size="sm" data-testid="images-batch-btn" onClick={batch} disabled={!sections.length}>
      <Layers className="w-4 h-4 mr-2" />Batch generate
    </Button>
    // eslint-disable-next-line react-hooks/exhaustive-deps
  ), [sections.length]));

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step {getStepForPath("/images")}</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Image Generation</h1>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">Sent to Midjourney via proxy URL. Mix presets &amp; tweak params before generating. Check Settings to verify Midjourney connection.</p>

      {/* Guided path: coverage, character consistency and quality — the rest stays below. */}
      <div className="mb-6">
        <GuidedPanel
          flow={imagesFlow}
          projectId={activeProjectId}
          extraCtx={{ sectionCount: sections.length, characters }}
          actions={{
            setCoverage,
            setCharacter: setGuideCharacter,
            setSteps: async (n) => { try { await api.saveSettings({ flux_steps: n }, activeProjectId); } catch { /* non-fatal */ } },
            // "single" and "key" are narrower passes over the same batch call, so the guide's answer
            // decides how many sections it touches rather than which endpoint it uses.
            run: () => runGuided(),
          }}
        />
      </div>

      {/* Where the picture is going, and how it should feel. See src-tauri/imagery.rs. */}
      <ImageryStudio
        subject={sections[0]?.image_prompt || sections[0]?.line || ""}
        projectId={activeProjectId}
        onCompose={(p) => {
          setImagery(p);
          toast.success(`Next images: ${p.width}×${p.height}, about ${p.seconds}s each.`);
        }}
      />
      {imagery && (
        <p className="text-xs text-muted-foreground -mt-4 mb-6">
          The next generations use the imagery settings above — {imagery.aspect}, {imagery.negative
            ? "with a negative prompt" : "restraint written into the prompt"}.
        </p>
      )}

      <Card className="p-5 mb-6 grid md:grid-cols-6 gap-4 items-center">
        <div className="md:col-span-2 flex flex-col gap-1.5">
          <Select value={activeSongId || ""} onValueChange={selectSong}>
            <SelectTrigger data-testid="images-song-select">
              <SelectValue placeholder={readySongs.length ? "Select song" : "No generated songs available"} />
            </SelectTrigger>
            <SelectContent>
              {readySongs.map(s => <SelectItem key={s.id} value={s.id}>{s.title}</SelectItem>)}
            </SelectContent>
          </Select>
          {readySongs.length === 0 && (
            <span className="text-[11px] text-destructive animate-pulse font-mono pl-1">
              * Generate song audio in Step {getStepForPath("/music")} first!
            </span>
          )}
        </div>
        <Select value={format} onValueChange={setFormat}>
          <SelectTrigger data-testid="images-format-select"><SelectValue /></SelectTrigger>
          <SelectContent>{FORMATS.map(f => <SelectItem key={f.id} value={f.id}>{f.label}</SelectItem>)}</SelectContent>
        </Select>
        <div className="space-y-1"><div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">Chaos {chaos[0]}</div><Slider value={chaos} onValueChange={setChaos} max={100} step={1} /></div>
        <div className="space-y-1"><div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">Stylize {stylize[0]}</div><Slider value={stylize} onValueChange={setStylize} max={1000} step={10} /></div>
        <label className="flex items-center gap-2 text-xs text-mono uppercase tracking-widest text-muted-foreground"><Switch checked={video} onCheckedChange={setVideo} data-testid="images-video-switch" />Video</label>
      </Card>

      <Tabs value={view} onValueChange={setView} className="mb-4">
        <TabsList>
          <TabsTrigger value="grid" data-testid="images-view-grid"><Grid3x3 className="w-4 h-4 mr-1" />Grid</TabsTrigger>
          <TabsTrigger value="list" data-testid="images-view-list"><ListIcon className="w-4 h-4 mr-1" />List</TabsTrigger>
          <TabsTrigger value="column" data-testid="images-view-column"><Columns className="w-4 h-4 mr-1" />Column</TabsTrigger>
        </TabsList>
      </Tabs>

      <div className={view==="grid"?"grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-3" : view==="column" ? "columns-2 md:columns-3 gap-3 space-y-3" : "space-y-3"}>
        {sections.map(s => {
          const vi = variantIdx[s.id] || 0;
          const img = (s.image_variants?.length ? s.image_variants : (s.image_url ? [s.image_url] : []))[vi];
          const jobStatus = jobs[s.id];
          const hasError = s.image_error || jobStatus?.error;
          
          return (
            <Card key={s.id} data-testid={`image-card-${s.index}`} className={`overflow-hidden ${view==="list"?"flex gap-3":""} ${hasError ? "border-destructive/50" : ""}`}>
              <div className={`relative ${view==="list"?"w-48 shrink-0":"w-full"} ${aspect} bg-muted/30 ${view==="list"?"":"rounded-t-md"} overflow-hidden`}>
                {img ? (
                  <img src={img} alt="" className="w-full h-full object-cover cursor-zoom-in"
                    title="Open the image explorer — compare variants, generate alternates"
                    onClick={() => setLightbox({ sectionId: s.id })} />
                ) : jobStatus?.status === "running" ? (
                  <div className="absolute inset-0 flex items-center justify-center text-xs text-muted-foreground text-mono bg-muted/50">
                    <Loader2 className="w-5 h-5 animate-spin mr-2" />generating...
                  </div>
                ) : hasError ? (
                  <div className="absolute inset-0 flex items-center justify-center text-[10px] text-destructive text-mono bg-destructive/10 p-2 text-center">
                    <AlertCircle className="w-4 h-4 mr-1 shrink-0" />{s.image_error || "Generation failed"}
                  </div>
                ) : (
                  <div className="absolute inset-0 flex items-center justify-center text-xs text-muted-foreground text-mono">no image</div>
                )}
                {s.image_variants?.length > 1 && <button onClick={()=>rotateVariant(s.id, s.image_variants.length)} className="absolute bottom-1 right-1 bg-black/60 text-white text-[10px] px-2 py-0.5 rounded text-mono">{vi+1}/{s.image_variants.length}</button>}
              </div>
              <div className="p-3 flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <Badge variant="secondary" className="text-[10px]">{s.mood}</Badge>
                  <span className="text-mono text-[10px] text-muted-foreground">{s.start.toFixed(0)}s</span>
                  {jobStatus && getStatusBadge(jobStatus.status, jobStatus.error)}
                </div>
                <div className="text-xs text-muted-foreground line-clamp-2 italic mb-2">{s.image_prompt}</div>
                {jobStatus && ["queued", "running"].includes(jobStatus.status) && (
                  <div className="mb-2 rounded-lg border border-border/80 bg-muted/20 p-2.5">
                    <GenerationProgress job={jobStatus} label={`${jobStatus.kind === "character_image" ? "Character" : "Image"} generation`} />
                  </div>
                )}
                <div className="flex gap-1">
                  <Button size="sm" variant={hasError ? "destructive" : "secondary"} data-testid={`image-gen-${s.index}`} onClick={()=>gen(s.id)} disabled={jobStatus?.status === "running"}>
                    {getStatusIcon(jobStatus?.status, jobStatus?.error) && <span className="mr-1">{getStatusIcon(jobStatus?.status, jobStatus?.error)}</span>}
                    {jobStatus?.status === "running" ? "Generating..." : img ? "Vary" : "Generate"}
                  </Button>
                  {s.image_prompt && (
                    <Button size="sm" variant="ghost" title="Run this prompt in the embedded Midjourney browser"
                      onClick={() => nav(`/browser?site=midjourney&auto=imagine&prompt=${encodeURIComponent(s.image_prompt)}`)}>
                      <Globe className="w-3.5 h-3.5" />
                    </Button>
                  )}
                </div>
              </div>
            </Card>
          );
        })}
        {!sections.length && <Card className="p-10 col-span-full text-center text-muted-foreground border-dashed">No sections — run analysis first.</Card>}
      </div>

      <ImageLightbox
        open={!!lbSection}
        title={lbSection ? `${lbSection.mood || "Section"} · ${(lbSection.image_prompt || "").slice(0, 60)}` : ""}
        images={lbImages}
        startIndex={lbSection ? (variantIdx[lbSection.id] || 0) : 0}
        primaryIndex={lbPrimary}
        busy={lbSection ? jobs[lbSection.id]?.status === "running" || jobs[lbSection.id]?.status === "queued" : false}
        onClose={() => setLightbox(null)}
        onGenerateAlternate={() => lbSection && gen(lbSection.id)}
        onSetPrimary={(i) => lbSection && setSectionPrimary(lbSection, i)}
        onDiscard={(i) => lbSection && discardSectionVariant(lbSection, i)}
      />
    </div>
  );
}
