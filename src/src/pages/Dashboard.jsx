import { useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Card } from "../components/ui/card";
import { Switch } from "../components/ui/switch";
import { Plus, Trash2, FolderOpen, Calendar, Languages, Palette, ChevronRight } from "lucide-react";
import { toast } from "sonner";

export default function Dashboard() {
  const { projects, refreshProjects, activeProjectId, selectProject } = useStudio();
  const [name, setName] = useState("");
  const [topic, setTopic] = useState("");
  const [schedule, setSchedule] = useState("");

  const create = async () => {
    if (!name.trim()) return toast.error("Project name required");
    await api.createProject({ name, topic, schedule: schedule || null });
    setName(""); setTopic(""); setSchedule("");
    await refreshProjects();
    toast.success("Project created");
  };
  const del = async (id) => { await api.deleteProject(id); if (activeProjectId === id) selectProject(null); refreshProjects(); };
  const toggle = async (p, key) => { await api.updateProject(p.id, { [key]: !p[key] }); refreshProjects(); };

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="mb-10">
        <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">overview</div>
        <h1 className="text-4xl sm:text-5xl lg:text-6xl font-bold tracking-tight">Projects</h1>
        <p className="mt-3 text-muted-foreground max-w-2xl">Each project orchestrates a topic across multiple channels, languages and music styles — from lyrics to scheduled upload.</p>
      </div>

      <Card className="p-6 mb-8 glass">
        <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3">New project</div>
        <div className="grid md:grid-cols-12 gap-3">
          <Input data-testid="project-name-input" placeholder="Project name (e.g. John 1 Multilingual)" value={name} onChange={e=>setName(e.target.value)} className="md:col-span-4" />
          <Input data-testid="project-topic-input" placeholder="Topic / theme" value={topic} onChange={e=>setTopic(e.target.value)} className="md:col-span-4" />
          <Input data-testid="project-schedule-input" placeholder="Schedule (e.g. weekly Sunday 9am)" value={schedule} onChange={e=>setSchedule(e.target.value)} className="md:col-span-3" />
          <Button data-testid="project-create-btn" onClick={create} className="md:col-span-1"><Plus className="w-4 h-4" /></Button>
        </div>
      </Card>

      <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
        {projects.length === 0 && <Card className="p-10 col-span-full text-center text-muted-foreground border-dashed">No projects yet — create one above.</Card>}
        {projects.map(p => (
          <Card key={p.id} data-testid={`project-card-${p.id}`}
            className={`p-5 cursor-pointer transition-all border ${activeProjectId===p.id ? "ring-2 ring-primary border-primary" : "hover:border-primary/40"}`}
            onClick={() => selectProject(p.id)}
          >
            <div className="flex items-start justify-between">
              <div className="min-w-0">
                <h3 className="font-semibold text-lg truncate">{p.name}</h3>
                {p.topic && <p className="text-sm text-muted-foreground truncate">{p.topic}</p>}
              </div>
              <button data-testid={`project-delete-${p.id}`} onClick={e=>{e.stopPropagation();del(p.id);}} className="text-muted-foreground hover:text-destructive p-1"><Trash2 className="w-4 h-4" /></button>
            </div>
            <div className="mt-4 space-y-2 text-sm">
              <div className="flex items-center gap-2 text-muted-foreground"><Calendar className="w-3 h-3" /> {p.schedule || "No schedule"}</div>
              <div className="flex items-center gap-2 text-muted-foreground"><Languages className="w-3 h-3" /> {(p.languages||[]).join(", ") || "—"}</div>
              <div className="flex items-center gap-2 text-muted-foreground"><Palette className="w-3 h-3" /> {(p.styles||[]).length} styles</div>
            </div>
            <div className="mt-4 flex items-center justify-between pt-3 border-t border-border">
              <label className="flex items-center gap-2 text-xs text-muted-foreground" onClick={e=>e.stopPropagation()}>
                <Switch data-testid={`toggle-ml-${p.id}`} checked={!!p.multi_language} onCheckedChange={()=>toggle(p, "multi_language")} /> Multi-lang
              </label>
              <label className="flex items-center gap-2 text-xs text-muted-foreground" onClick={e=>e.stopPropagation()}>
                <Switch data-testid={`toggle-ms-${p.id}`} checked={!!p.multi_style} onCheckedChange={()=>toggle(p, "multi_style")} /> Multi-style
              </label>
              <ChevronRight className="w-4 h-4 text-primary" />
            </div>
          </Card>
        ))}
      </div>

      {activeProjectId && (
        <Card className="mt-8 p-5 border-primary/30 bg-primary/5">
          <div className="flex items-center gap-3">
            <FolderOpen className="w-5 h-5 text-primary" />
            <span className="text-sm">Active project selected — continue to <b>Lyrics Import</b> to bring in your songs.</span>
          </div>
        </Card>
      )}
    </div>
  );
}
