import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Progress } from "../components/ui/progress";
import { Copy, RotateCw, X } from "lucide-react";
import { toast } from "sonner";
import { useState } from "react";

const KIND_COLOR = { music: "bg-emerald-500/20 text-emerald-300", analysis: "bg-sky-500/20 text-sky-300", image: "bg-fuchsia-500/20 text-fuchsia-300", video: "bg-amber-500/20 text-amber-300", upload: "bg-rose-500/20 text-rose-300" };
const STATUS_VAR = { queued: "outline", running: "default", done: "secondary", failed: "destructive" };

export default function Jobs() {
  const { jobs, refreshJobs } = useStudio();
  const [open, setOpen] = useState({});

  const retry = async (id) => { await api.retryJob(id); toast.success("Retrying"); refreshJobs(); };
  const cancel = async (id) => { await api.cancelJob(id); refreshJobs(); };
  const copy = async (j) => { await navigator.clipboard.writeText(JSON.stringify(j, null, 2)); toast.success("Job JSON copied"); };

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step 10</div>
      <h1 className="text-4xl sm:text-5xl font-bold mb-2">Jobs Monitor</h1>
      <p className="text-muted-foreground mb-6 max-w-2xl">Live queue, logs and retries. All long-running tasks (Suno, MJ, FFmpeg, YT upload) report here.</p>

      <div className="space-y-3">
        {jobs.map(j => (
          <Card key={j.id} data-testid={`job-row-${j.id}`} className="p-4">
            <div className="flex flex-wrap items-center gap-3">
              <Badge className={`text-mono text-[10px] uppercase ${KIND_COLOR[j.kind] || ""}`}>{j.kind}</Badge>
              <code className="text-mono text-[11px] text-muted-foreground truncate max-w-[160px]">{j.target_id.slice(0,8)}…</code>
              <div className="flex-1 min-w-[200px]"><Progress value={j.progress || 0} /></div>
              <Badge variant={STATUS_VAR[j.status] || "outline"} data-testid={`job-status-${j.id}`}>{j.status} {j.attempts ? `(retry ${j.attempts})` : ""}</Badge>
              <Button size="sm" variant="ghost" onClick={()=>setOpen(o=>({...o,[j.id]:!o[j.id]}))}>{open[j.id]?"hide":"logs"}</Button>
              <Button size="sm" variant="ghost" data-testid={`job-copy-${j.id}`} onClick={()=>copy(j)}><Copy className="w-3 h-3" /></Button>
              {j.status === "failed" && <Button size="sm" data-testid={`job-retry-${j.id}`} onClick={()=>retry(j.id)}><RotateCw className="w-3 h-3" /></Button>}
              <Button size="sm" variant="ghost" data-testid={`job-cancel-${j.id}`} onClick={()=>cancel(j.id)}><X className="w-3 h-3" /></Button>
            </div>
            {open[j.id] && (
              <pre className="mt-3 bg-muted/40 rounded-md p-3 text-mono text-[11px] overflow-auto max-h-64 scroll-thin">{(j.logs || []).join("\n") || "no logs"}{j.error?`\n\nERROR: ${j.error}`:""}</pre>
            )}
          </Card>
        ))}
        {!jobs.length && <Card className="p-10 text-center text-muted-foreground border-dashed">No jobs yet.</Card>}
      </div>
    </div>
  );
}
