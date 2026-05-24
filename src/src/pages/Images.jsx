import { useEffect, useState, useRef } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Tabs, TabsList, TabsTrigger } from "../components/ui/tabs";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Slider } from "../components/ui/slider";
import { Switch } from "../components/ui/switch";
import { Wand2, Layers, Grid3x3, List as ListIcon, Columns, AlertCircle, CheckCircle2, Clock, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { getStepForPath } from "../lib/pageSteps";

const FORMATS = [
  { id: "yt", label: "YouTube 16:9", ar: "16:9" },
  { id: "shorts", label: "Shorts 9:16", ar: "9:16" },
  { id: "tiktok", label: "TikTok 9:16", ar: "9:16" },
];

export default function Images() {
  const { songs, activeSongId, selectSong } = useStudio();
  const [sections, setSections] = useState([]);
  const [view, setView] = useState("grid");
  const [format, setFormat] = useState("yt");
  const [chaos, setChaos] = useState([0]);
  const [stylize, setStylize] = useState([100]);
  const [video, setVideo] = useState(false);
  const [variantIdx, setVariantIdx] = useState({});
  const [jobs, setJobs] = useState({});  // Track job status per section
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
            if (job.target && job.type === "image") {
              jobMap[job.target] = {
                id: job.id,
                status: job.status,
                progress: job.progress || 0,
                error: job.error || null,
                logs: job.logs || []
              };
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

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step {getStepForPath("/images")}</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Image Generation</h1>
        <Button data-testid="images-batch-btn" onClick={batch} disabled={!sections.length}><Layers className="w-4 h-4 mr-2" />Batch generate</Button>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">Sent to Midjourney via proxy URL. Mix presets &amp; tweak params before generating. Check Settings to verify Midjourney connection.</p>

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
                  <img src={img} alt="" className="w-full h-full object-cover" />
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
                <div className="flex gap-1">
                  <Button size="sm" variant={hasError ? "destructive" : "secondary"} data-testid={`image-gen-${s.index}`} onClick={()=>gen(s.id)} disabled={jobStatus?.status === "running"}>
                    {getStatusIcon(jobStatus?.status, jobStatus?.error) && <span className="mr-1">{getStatusIcon(jobStatus?.status, jobStatus?.error)}</span>}
                    {jobStatus?.status === "running" ? "Generating..." : img ? "Vary" : "Generate"}
                  </Button>
                </div>
              </div>
            </Card>
          );
        })}
        {!sections.length && <Card className="p-10 col-span-full text-center text-muted-foreground border-dashed">No sections — run analysis first.</Card>}
      </div>
    </div>
  );
}
