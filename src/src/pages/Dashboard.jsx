import { useState, useEffect } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Card } from "../components/ui/card";
import { Switch } from "../components/ui/switch";
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from "../components/ui/select";
import { Plus, Trash2, FolderOpen, Calendar, Languages, Palette, ChevronRight, Download, Upload } from "lucide-react";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";
import presets from "../lib/templates";
import presetStore from "../lib/presetStore";
import TemplatesManager from "../components/TemplatesManager";

export default function Dashboard() {
  const { projects, refreshProjects, activeProjectId, selectProject } = useStudio();
  const [name, setName] = useState("");
  const [topic, setTopic] = useState("");
  const [schedule, setSchedule] = useState("");
  const [template, setTemplate] = useState("default");
  const [remotePresetUrl, setRemotePresetUrl] = useState("");
  const [localPresets, setLocalPresets] = useState(() => presetStore.loadPresets() || presets || []);
  const [availableChannels, setAvailableChannels] = useState([]);
  const [channelsToCopy, setChannelsToCopy] = useState([]);

  useEffect(() => {
    // If a template provides defaults, apply them to the form
    const t = localPresets.find(p => p.id === template);
    if (t) {
      if (t.defaultName) setName(t.defaultName);
      if (t.defaultTopic) setTopic(t.defaultTopic);
      if (t.defaultSchedule) setSchedule(t.defaultSchedule);
    }
  }, [template]);

  const [defaults, setDefaults] = useState({ multi_language: true, multi_style: true });
  useEffect(()=>{
    (async ()=>{
      try { const s = await api.getSettings(); setDefaults(Object.assign({}, defaults, s || {})); } catch(e){/*ignore*/}
    })();
  },[]);

  useEffect(()=>{
    (async ()=>{
      try { const ch = await api.listChannels(); setAvailableChannels(ch || []); } catch(e){}
    })();
  },[]);

  const create = async () => {
    if (!name.trim()) return toast.error("Project name required");
    // Build project payload from selected template
    const t = localPresets.find(p => p.id === template) || {};
    const payload = { name, topic, schedule: schedule || null, ...(t.projectOverrides || {}) };
    const created = await api.createProject(payload);

    // If template requests reset of global settings or channels, perform actions
    try {
      if (t.resetDefaults) {
        if (t.defaults) {
          await api.saveSettings(t.defaults);
        }
      }
      if (t.resetChannels) {
        // delete all channels (user requested behaviour)
        const channels = await api.listChannels();
        for (const ch of channels) {
          try { await api.deleteChannel(ch.id); } catch (e) { console.warn(e); }
        }
      }
      // copy selected channels into new project context (duplicate entries)
      if (channelsToCopy && channelsToCopy.length) {
        for (const cid of channelsToCopy) {
          const ch = availableChannels.find(c=>c.id===cid);
          if (!ch) continue;
          try {
            await api.createChannel({ name: `${ch.name} (copy)`, youtube_channel_id: ch.youtube_channel_id||"", language: ch.language||'en', styles: ch.styles||[], region: ch.region||null });
          } catch(e){ console.warn('Failed to copy channel', e); }
        }
      }
    } catch (err) {
      console.warn("Post-create template actions failed", err);
    }

    setName(""); setTopic(""); setSchedule("");
    await refreshProjects();
    toast.success("Project created");
  };
  const del = async (id) => { await api.deleteProject(id); if (activeProjectId === id) selectProject(null); refreshProjects(); };
  const toggle = async (p, key) => { await api.updateProject(p.id, { [key]: !p[key] }); refreshProjects(); };

  const exportActive = async () => {
    if (!activeProjectId) return toast.error("Select a project first to export");
    try {
      const res = await api.exportProject(activeProjectId);
      if (res?.export_folder) {
        toast.success(`Exported project to ${res.export_folder}`);
      } else {
        toast.success("Project exported successfully");
      }
    } catch (err) {
      console.error(err);
      toast.error("Project export cancelled or failed");
    }
  };

  const importProject = async () => {
    try {
      const selected = await open({ multiple: false, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!selected || Array.isArray(selected)) return;
      const content = await readTextFile(selected);
      const payload = JSON.parse(content);
      const sourceDir = selected.replace(/\\/g, "/").replace(/\/[^\/]*$/, "");
      await api.importProject(payload, sourceDir);
      await refreshProjects();
      toast.success("Project imported successfully");
    } catch (err) {
      console.error(err);
      toast.error("Failed to import project");
    }
  };

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="mb-10">
        <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">overview</div>
        <h1 className="text-4xl sm:text-5xl lg:text-6xl font-bold tracking-tight">Projects</h1>
        <p className="mt-3 text-muted-foreground max-w-2xl">Each project orchestrates a topic across multiple channels, languages and music styles — from lyrics to scheduled upload.</p>
      </div>

      <Card className="p-6 mb-8 glass">
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-3">
          <div>
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">New project</div>
            <div className="text-sm text-muted-foreground">Create a project and export / import it later with all related data.</div>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={importProject}><Upload className="w-4 h-4 mr-2" />Import project</Button>
            <Button variant="secondary" onClick={exportActive} disabled={!activeProjectId}><Download className="w-4 h-4 mr-2" />Export active project</Button>
          </div>
        </div>
        <div className="grid md:grid-cols-12 gap-3">
          <div className="md:col-span-3">
            <div className="text-[11px] text-muted-foreground mb-1">Template</div>
            <Select value={template} onValueChange={v=>setTemplate(v)}>
              <SelectTrigger className="w-full h-9">
                <SelectValue placeholder="Choose template" />
              </SelectTrigger>
              <SelectContent>
                {localPresets.map(p => (
                  <SelectItem key={p.id} value={p.id}>{p.label}</SelectItem>
                ))}
              </SelectContent>
            </Select>
            {localPresets.find(p => p.id === template)?.description && (
              <p className="mt-2 text-xs text-muted-foreground">{localPresets.find(p => p.id === template)?.description}</p>
            )}
          </div>
          <div className="md:col-span-1 flex items-end">
            <TemplatesManager onChange={(ps)=>setLocalPresets(ps)} />
          </div>
          <Input data-testid="project-name-input" placeholder="Project name (e.g. John 1 Multilingual)" value={name} onChange={e=>setName(e.target.value)} className="md:col-span-3" />
          <Input data-testid="project-topic-input" placeholder="Topic / theme" value={topic} onChange={e=>setTopic(e.target.value)} className="md:col-span-3" />
          <Input data-testid="project-schedule-input" placeholder="Schedule (e.g. weekly Sunday 9am)" value={schedule} onChange={e=>setSchedule(e.target.value)} className="md:col-span-2" />
          <Button data-testid="project-create-btn" onClick={create} className="md:col-span-1"><Plus className="w-4 h-4" /></Button>
        </div>
        <div className="mt-3 grid md:grid-cols-12 gap-2 items-center">
          <Input placeholder="Import preset JSON from URL" value={remotePresetUrl} onChange={e=>setRemotePresetUrl(e.target.value)} className="md:col-span-9" />
          <Button className="md:col-span-3" onClick={async ()=>{
            if (!remotePresetUrl) return toast.error('Preset URL required');
            try {
              const res = await fetch(remotePresetUrl);
              const json = await res.json();
              setLocalPresets(ps => [...ps, json]);
              toast.success('Preset imported');
            } catch (err) { console.error(err); toast.error('Failed to import preset'); }
          }}>Import preset</Button>
        </div>
        <div className="mt-4">
          <div className="text-sm text-muted-foreground mb-2">Optional: copy existing channels into new project</div>
          <div className="grid gap-2 md:grid-cols-4">
            {availableChannels.map(ch=> (
              <label key={ch.id} className="flex items-center gap-2 p-2 border rounded">
                <input type="checkbox" checked={channelsToCopy.includes(ch.id)} onChange={(e)=>{
                  if (e.target.checked) setChannelsToCopy(a=>[...a,ch.id]); else setChannelsToCopy(a=>a.filter(x=>x!==ch.id));
                }} />
                <div className="flex-1 text-sm">{ch.name} <span className="text-xs text-muted-foreground">{ch.youtube_channel_id}</span></div>
              </label>
            ))}
          </div>
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

      <Card className="mt-6 p-5">
        <div className="flex items-center justify-between mb-3">
          <div>
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Global controls</div>
            <div className="text-sm text-muted-foreground">Reset app-wide settings, manage channels and presets.</div>
          </div>
          <div className="flex gap-2">
            <Button variant="outline" onClick={async ()=>{
              if (!confirm('Reset app settings to defaults?')) return;
              try { await api.saveSettings({}); toast.success('Settings reset to defaults'); } catch (e) { toast.error('Failed to reset settings'); }
            }}>Reset settings</Button>
            <Button variant="destructive" onClick={async ()=>{
              if (!confirm('Delete ALL YouTube channels? This is irreversible.')) return;
              try {
                const channels = await api.listChannels();
                for (const c of channels) { try { await api.deleteChannel(c.id); } catch(e){console.warn(e);} }
                toast.success('All channels deleted');
              } catch (e) { toast.error('Failed to clear channels'); }
            }}>Clear channels</Button>
            <Button onClick={async ()=>{
              try {
                const channels = await api.listChannels();
                const blob = new Blob([JSON.stringify(channels, null, 2)], { type: 'application/json' });
                const url = URL.createObjectURL(blob);
                const a = document.createElement('a');
                a.href = url; a.download = 'channels.json'; document.body.appendChild(a); a.click(); a.remove(); URL.revokeObjectURL(url);
                toast.success('Channels exported');
              } catch (e) { toast.error('Failed to export channels'); }
            }}>Export channels</Button>
            <Button variant="outline" onClick={async ()=>{
              try {
                const selected = await open({ multiple: false, filters: [{ name: 'JSON', extensions: ['json'] }] });
                if (!selected || Array.isArray(selected)) return;
                const content = await readTextFile(selected);
                const arr = JSON.parse(content);
                for (const ch of arr) {
                  try { await api.createChannel({ name: ch.name||ch.label||'Imported', youtube_channel_id: ch.youtube_channel_id||ch.youtubeChannelId||'', language: ch.language||'en', styles: ch.styles||[], region: ch.region||null }); }
                  catch(e){console.warn(e);} 
                }
                toast.success('Channels imported');
              } catch (e) { console.error(e); toast.error('Failed to import channels'); }
            }}>Import channels</Button>
          </div>
        </div>
        <div className="flex items-center gap-4">
          <label className="flex items-center gap-2 text-sm"><Switch checked={!!defaults.multi_language} onCheckedChange={async (v)=>{ setDefaults(d=>({...d, multi_language: v})); try{ await api.saveSettings({...defaults, multi_language: v}); toast.success('Saved'); } catch(e){toast.error('Save failed');} }} /> Default multi-language</label>
          <label className="flex items-center gap-2 text-sm"><Switch checked={!!defaults.multi_style} onCheckedChange={async (v)=>{ setDefaults(d=>({...d, multi_style: v})); try{ await api.saveSettings({...defaults, multi_style: v}); toast.success('Saved'); } catch(e){toast.error('Save failed');} }} /> Default multi-style</label>
        </div>
      </Card>
    </div>
  );
}
