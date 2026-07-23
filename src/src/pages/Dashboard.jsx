import { useState, useEffect, useCallback } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Card } from "../components/ui/card";
import { Switch } from "../components/ui/switch";
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from "../components/ui/select";
import { Plus, Trash2, FolderOpen, Calendar, Languages, Palette, ChevronRight, ChevronDown, ChevronUp, Download, Upload, GitBranch, Tag, History, Check, Loader2, Sparkles, BookOpen } from "lucide-react";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";
import presets from "../lib/templates";
import presetStore from "../lib/presetStore";
import TemplatesManager from "../components/TemplatesManager";

export default function Dashboard() {
  const { projects, refreshProjects, activeProjectId, selectProject, refreshSongs } = useStudio();
  const [name, setName] = useState("");
  const [topic, setTopic] = useState("");
  const [schedule, setSchedule] = useState("");
  const [template, setTemplate] = useState("default");
  const [remotePresetUrl, setRemotePresetUrl] = useState("");
  const [localPresets, setLocalPresets] = useState(() => presetStore.loadPresets() || presets || []);
  const [availableChannels, setAvailableChannels] = useState([]);
  const [channelsToCopy, setChannelsToCopy] = useState([]);

  // Git panel state: keyed by project id
  const [gitInfo, setGitInfo] = useState({}); // { [pid]: { loading, data, expanded } }
  const [gitExpandedDate, setGitExpandedDate] = useState({}); // { [pid]: dateString } – which day is drilled down
  const [gitBranchInput, setGitBranchInput] = useState({}); // { [pid]: string }
  const [gitCreatingBranch, setGitCreatingBranch] = useState({}); // { [pid]: bool }
  const [gitCheckingOut, setGitCheckingOut] = useState({}); // { [pid]: string } – current checkout in progress

  // Daily Content (scheduler) panel state: keyed by project id
  const [scheduleUi, setScheduleUi] = useState({}); // { [pid]: { expanded, draft, saving, generating } }
  const [bibleBooks, setBibleBooks] = useState([]);
  const [bibleTranslations, setBibleTranslations] = useState({});

  useEffect(() => {
    (async () => {
      try { setBibleBooks((await api.bibleBooks()) || []); } catch (e) { /* ignore */ }
      try { setBibleTranslations((await api.bibleTranslations()) || {}); } catch (e) { /* ignore */ }
    })();
  }, []);

  const defaultScheduleDraft = (p) => ({
    enabled: false,
    frequency: "daily",
    time: "09:00",
    day_of_week: 0,
    book: "",
    translation: "web",
    languages: (p.languages || []).join(", "),
    styles: (p.styles || []).join(", "),
    next_chapter: 1,
    ...(p.schedule_config || {}),
    // schedule_config stores languages/styles as arrays; render them as comma-separated text
    ...(p.schedule_config?.languages ? { languages: p.schedule_config.languages.join(", ") } : {}),
    ...(p.schedule_config?.styles ? { styles: p.schedule_config.styles.join(", ") } : {}),
  });

  const toggleSchedulePanel = (p) => {
    setScheduleUi(prev => {
      const cur = prev[p.id];
      if (cur?.expanded) return { ...prev, [p.id]: { ...cur, expanded: false } };
      return { ...prev, [p.id]: { ...cur, expanded: true, draft: cur?.draft || defaultScheduleDraft(p) } };
    });
  };

  const updateScheduleDraft = (pid, patch) => {
    setScheduleUi(prev => ({ ...prev, [pid]: { ...prev[pid], draft: { ...prev[pid].draft, ...patch } } }));
  };

  const saveSchedule = async (pid) => {
    const draft = scheduleUi[pid]?.draft;
    if (!draft) return;
    setScheduleUi(prev => ({ ...prev, [pid]: { ...prev[pid], saving: true } }));
    try {
      const payload = {
        ...draft,
        languages: draft.languages.split(",").map(s => s.trim()).filter(Boolean),
        styles: draft.styles.split(",").map(s => s.trim()).filter(Boolean),
        next_chapter: Math.max(1, parseInt(draft.next_chapter, 10) || 1),
        day_of_week: Math.max(0, Math.min(6, parseInt(draft.day_of_week, 10) || 0)),
      };
      await api.updateProject(pid, { schedule_config: payload });
      await refreshProjects();
      toast.success("Daily Content schedule saved");
    } catch (err) {
      toast.error(typeof err === "string" ? err : (err?.message || "Failed to save schedule"));
    } finally {
      setScheduleUi(prev => ({ ...prev, [pid]: { ...prev[pid], saving: false } }));
    }
  };

  const generateNow = async (pid) => {
    setScheduleUi(prev => ({ ...prev, [pid]: { ...prev[pid], generating: true } }));
    try {
      const res = await api.generateNextChapterNow(pid);
      const count = res?.created_song_ids?.length || 0;
      if (count > 0) {
        toast.success(`Generated ${count} song draft${count > 1 ? "s" : ""} for ${res.book} ${res.chapter}`);
      } else {
        toast.error((res?.errors && res.errors[0]) || "No songs were generated");
      }
      await refreshProjects();
      if (pid === activeProjectId) await refreshSongs();
    } catch (err) {
      toast.error(typeof err === "string" ? err : (err?.message || "Generation failed"));
    } finally {
      setScheduleUi(prev => ({ ...prev, [pid]: { ...prev[pid], generating: false } }));
    }
  };

  const loadGitInfo = useCallback(async (pid) => {
    setGitInfo(prev => ({ ...prev, [pid]: { ...(prev[pid] || {}), loading: true } }));
    try {
      const data = await api.getProjectGitInfo(pid);
      setGitInfo(prev => ({ ...prev, [pid]: { ...(prev[pid] || {}), loading: false, data } }));
    } catch (err) {
      setGitInfo(prev => ({ ...prev, [pid]: { ...(prev[pid] || {}), loading: false, data: null } }));
    }
  }, []);

  const toggleGitPanel = useCallback(async (pid) => {
    const cur = gitInfo[pid];
    if (!cur?.expanded) {
      setGitInfo(prev => ({ ...prev, [pid]: { ...(prev[pid] || {}), expanded: true } }));
      await loadGitInfo(pid);
    } else {
      setGitInfo(prev => ({ ...prev, [pid]: { ...(prev[pid] || {}), expanded: false } }));
    }
  }, [gitInfo, loadGitInfo]);

  const doCheckoutTag = useCallback(async (pid, tag) => {
    setGitCheckingOut(prev => ({ ...prev, [pid]: tag }));
    try {
      await api.checkoutProjectGitTag(pid, tag);
      toast.success(`Loaded version ${tag}`);
      await loadGitInfo(pid);
      await refreshSongs();
    } catch (err) {
      toast.error(typeof err === "string" ? err : (err?.message || "Checkout failed"));
    } finally {
      setGitCheckingOut(prev => ({ ...prev, [pid]: null }));
    }
  }, [loadGitInfo, refreshSongs]);

  const doCheckoutBranch = useCallback(async (pid, branch) => {
    setGitCheckingOut(prev => ({ ...prev, [pid]: branch }));
    try {
      await api.checkoutProjectGitBranch(pid, branch);
      toast.success(`Switched to branch ${branch}`);
      await loadGitInfo(pid);
      await refreshSongs();
    } catch (err) {
      toast.error(typeof err === "string" ? err : (err?.message || "Branch switch failed"));
    } finally {
      setGitCheckingOut(prev => ({ ...prev, [pid]: null }));
    }
  }, [loadGitInfo, refreshSongs]);

  const doCreateBranch = useCallback(async (pid) => {
    const name = (gitBranchInput[pid] || "").trim();
    if (!name) { toast.error("Branch name is required."); return; }
    setGitCreatingBranch(prev => ({ ...prev, [pid]: true }));
    try {
      await api.createProjectGitBranch(pid, name);
      toast.success(`Branch '${name}' created`);
      setGitBranchInput(prev => ({ ...prev, [pid]: "" }));
      await loadGitInfo(pid);
    } catch (err) {
      toast.error(typeof err === "string" ? err : (err?.message || "Branch creation failed"));
    } finally {
      setGitCreatingBranch(prev => ({ ...prev, [pid]: false }));
    }
  }, [gitBranchInput, loadGitInfo]);

  // Group tags by date prefix (YYYY-MM-DD)
  const groupTagsByDate = (tags) => {
    const groups = {};
    for (const t of (tags || [])) {
      const match = t.tag.match(/^(\d{4}-\d{2}-\d{2})/);
      const key = match ? match[1] : "older";
      if (!groups[key]) groups[key] = [];
      groups[key].push(t);
    }
    return Object.entries(groups).sort((a, b) => b[0].localeCompare(a[0]));
  };

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
            <TemplatesManager onChange={(ps)=>setLocalPresets(ps)} />
            <Button variant="outline" onClick={importProject}><Upload className="w-4 h-4 mr-2" />Import project</Button>
            <Button variant="secondary" onClick={exportActive} disabled={!activeProjectId}><Download className="w-4 h-4 mr-2" />Export active project</Button>
          </div>
        </div>
        <div className="grid md:grid-cols-12 gap-3 items-center">
          <div className="md:col-span-3">
            <Select value={template} onValueChange={v=>setTemplate(v)}>
              <SelectTrigger className="w-full h-10">
                <SelectValue placeholder="Choose template" />
              </SelectTrigger>
              <SelectContent>
                {localPresets.map(p => (
                  <SelectItem key={p.id} value={p.id}>{p.label}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <Input data-testid="project-name-input" placeholder="Project name (e.g. John 1 Multilingual)" value={name} onChange={e=>setName(e.target.value)} className="md:col-span-3 h-10" />
          <Input data-testid="project-topic-input" placeholder="Topic / theme" value={topic} onChange={e=>setTopic(e.target.value)} className="md:col-span-3 h-10" />
          <Input data-testid="project-schedule-input" placeholder="Schedule (e.g. weekly Sunday 9am)" value={schedule} onChange={e=>setSchedule(e.target.value)} className="md:col-span-2 h-10" />
          <Button data-testid="project-create-btn" onClick={create} className="md:col-span-1 h-10"><Plus className="w-4 h-4" /></Button>
        </div>
        {localPresets.find(p => p.id === template)?.description && (
          <p className="mt-2 text-xs text-muted-foreground">{localPresets.find(p => p.id === template)?.description}</p>
        )}
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
            className={`p-5 transition-all border ${activeProjectId===p.id ? "ring-2 ring-primary border-primary" : "hover:border-primary/40"}`}
          >
            {/* Clickable main card area */}
            <div className="cursor-pointer" onClick={() => selectProject(p.id)}>
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
                {p.local_path && (
                  <div className="flex items-center gap-2 text-muted-foreground text-xs truncate">
                    <GitBranch className="w-3 h-3 shrink-0" />
                    <span className="truncate">{p.local_path}</span>
                  </div>
                )}
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
            </div>

            {/* Git History Panel toggle */}
            <div className="mt-3 pt-3 border-t border-border/50">
              <button
                onClick={(e) => { e.stopPropagation(); toggleGitPanel(p.id); }}
                className="w-full flex items-center justify-between text-xs text-muted-foreground hover:text-foreground transition-colors py-1"
              >
                <span className="flex items-center gap-1.5">
                  <History className="w-3 h-3" />
                  Version History & Branches
                </span>
                {gitInfo[p.id]?.loading ? (
                  <Loader2 className="w-3 h-3 animate-spin" />
                ) : gitInfo[p.id]?.expanded ? (
                  <ChevronUp className="w-3 h-3" />
                ) : (
                  <ChevronDown className="w-3 h-3" />
                )}
              </button>

              {gitInfo[p.id]?.expanded && (() => {
                const info = gitInfo[p.id]?.data;
                if (!info) return <div className="text-xs text-muted-foreground py-2">Failed to load git info.</div>;
                if (!info.is_git) return (
                  <div className="text-xs text-muted-foreground py-2 space-y-1">
                    <div>{info.local_path ? `No git repository at ${info.local_path}` : "Project not linked to a local folder. Save first to initialize."}</div>
                  </div>
                );

                const tagGroups = groupTagsByDate(info.tags);
                const isDetached = info.is_detached;

                return (
                  <div className="mt-2 space-y-3" onClick={e => e.stopPropagation()}>

                    {/* Current status badge */}
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-mono ${
                        isDetached ? "bg-amber-500/15 text-amber-400" :
                        info.git_status === "dirty" ? "bg-blue-500/15 text-blue-400" :
                        "bg-emerald-500/15 text-emerald-400"
                      }`}>
                        <GitBranch className="w-2.5 h-2.5" />
                        {info.current_branch}
                      </span>
                      <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                        isDetached ? "bg-amber-500/10 text-amber-500" :
                        info.git_status === "dirty" ? "bg-blue-500/10 text-blue-400" :
                        "bg-muted text-muted-foreground"
                      }`}>{info.git_status}</span>
                    </div>

                    {/* Branches */}
                    {info.branches.length > 0 && (
                      <div>
                        <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1.5">Branches</div>
                        <div className="flex flex-wrap gap-1">
                          {info.branches.map(b => (
                            <button
                              key={b}
                              disabled={gitCheckingOut[p.id] === b}
                              onClick={() => doCheckoutBranch(p.id, b)}
                              className={`flex items-center gap-1 px-2 py-0.5 rounded text-[11px] border transition-colors ${
                                info.current_branch === b
                                  ? "border-primary bg-primary/10 text-primary cursor-default"
                                  : "border-border/60 hover:border-primary/50 hover:bg-accent text-muted-foreground"
                              }`}
                            >
                              {gitCheckingOut[p.id] === b ? <Loader2 className="w-2.5 h-2.5 animate-spin" /> : <GitBranch className="w-2.5 h-2.5" />}
                              {b}
                            </button>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Create new branch (useful on detached HEAD) */}
                    {(isDetached || info.branches.length > 0) && (
                      <div>
                        <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1.5">New Branch</div>
                        <div className="flex gap-1.5">
                          <input
                            className="flex-1 rounded border border-border bg-background px-2 py-1 text-xs outline-none focus:ring-1 focus:ring-primary"
                            placeholder="branch-name"
                            value={gitBranchInput[p.id] || ""}
                            onChange={e => setGitBranchInput(prev => ({ ...prev, [p.id]: e.target.value }))}
                            onKeyDown={e => { if (e.key === "Enter") doCreateBranch(p.id); }}
                          />
                          <button
                            onClick={() => doCreateBranch(p.id)}
                            disabled={gitCreatingBranch[p.id]}
                            className="px-2 py-1 rounded bg-primary text-primary-foreground text-xs hover:bg-primary/90 disabled:opacity-50 transition-colors"
                          >
                            {gitCreatingBranch[p.id] ? <Loader2 className="w-3 h-3 animate-spin" /> : <Plus className="w-3 h-3" />}
                          </button>
                        </div>
                      </div>
                    )}

                    {/* Version tags grouped by date */}
                    {tagGroups.length > 0 && (
                      <div>
                        <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1.5">Save History</div>
                        <div className="space-y-1.5">
                          {tagGroups.map(([date, dateTags]) => {
                            const isExpanded = gitExpandedDate[p.id] === date;
                            const latest = dateTags[0];
                            return (
                              <div key={date} className="rounded-lg border border-border/50 overflow-hidden">
                                {/* Day header: shows latest tag, click to expand/collapse */}
                                <button
                                  className="w-full flex items-center justify-between px-2.5 py-1.5 text-xs hover:bg-accent transition-colors"
                                  onClick={() => setGitExpandedDate(prev => ({ ...prev, [p.id]: isExpanded ? null : date }))}
                                >
                                  <span className="flex items-center gap-1.5">
                                    <Tag className="w-2.5 h-2.5 text-primary" />
                                    <span className="font-mono">{date}</span>
                                    <span className="text-muted-foreground">({dateTags.length} save{dateTags.length > 1 ? "s" : ""})</span>
                                  </span>
                                  <div className="flex items-center gap-1.5">
                                    {/* Quick-load latest */}
                                    <button
                                      onClick={(e) => { e.stopPropagation(); doCheckoutTag(p.id, latest.tag); }}
                                      disabled={gitCheckingOut[p.id] === latest.tag}
                                      className="px-1.5 py-0.5 rounded bg-primary/10 hover:bg-primary/20 text-primary text-[10px] transition-colors"
                                    >
                                      {gitCheckingOut[p.id] === latest.tag ? <Loader2 className="w-2.5 h-2.5 animate-spin" /> : "Load latest"}
                                    </button>
                                    {isExpanded ? <ChevronUp className="w-3 h-3 text-muted-foreground" /> : <ChevronDown className="w-3 h-3 text-muted-foreground" />}
                                  </div>
                                </button>
                                {/* Expanded: list all tags for that day */}
                                {isExpanded && (
                                  <div className="border-t border-border/40 divide-y divide-border/30">
                                    {dateTags.map(t => (
                                      <div key={t.tag} className="flex items-center justify-between px-2.5 py-1.5 text-[11px]">
                                        <span className="flex items-center gap-1.5">
                                          <span className="font-mono text-muted-foreground">{t.tag}</span>
                                        </span>
                                        <button
                                          onClick={() => doCheckoutTag(p.id, t.tag)}
                                          disabled={gitCheckingOut[p.id] === t.tag}
                                          className="px-1.5 py-0.5 rounded text-[10px] border border-border/60 hover:border-primary/50 hover:bg-accent text-muted-foreground transition-colors"
                                        >
                                          {gitCheckingOut[p.id] === t.tag ? <Loader2 className="w-2.5 h-2.5 animate-spin" /> : "Load"}
                                        </button>
                                      </div>
                                    ))}
                                  </div>
                                )}
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    )}
                    {tagGroups.length === 0 && (
                      <div className="text-xs text-muted-foreground">No saved versions yet. Use the Save button to create your first snapshot.</div>
                    )}
                  </div>
                );
              })()}
            </div>

            {/* Daily Content (scheduler) panel toggle */}
            <div className="mt-3 pt-3 border-t border-border/50">
              <button
                onClick={(e) => { e.stopPropagation(); toggleSchedulePanel(p); }}
                className="w-full flex items-center justify-between text-xs text-muted-foreground hover:text-foreground transition-colors py-1"
              >
                <span className="flex items-center gap-1.5">
                  <BookOpen className="w-3 h-3" />
                  Daily Content
                  {p.schedule_config?.enabled && (
                    <span className="px-1.5 py-0.5 rounded-full text-[9px] bg-emerald-500/15 text-emerald-400">on</span>
                  )}
                </span>
                {scheduleUi[p.id]?.expanded ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
              </button>

              {scheduleUi[p.id]?.expanded && (() => {
                const draft = scheduleUi[p.id]?.draft;
                if (!draft) return null;
                const set = (patch) => updateScheduleDraft(p.id, patch);
                const flatTranslations = Object.entries(bibleTranslations).flatMap(([category, list]) =>
                  (list || []).map(t => ({ ...t, category }))
                );
                return (
                  <div className="mt-2 space-y-3" onClick={e => e.stopPropagation()}>
                    <p className="text-xs text-muted-foreground">
                      Automatically progresses through a Bible book, one chapter per run: generates that chapter's lyrics with AI, saves it as a draft song, and starts music generation. Analysis, images, video and upload stay manual so you review before anything goes public.
                    </p>

                    <label className="flex items-center gap-2 text-xs">
                      <Switch checked={!!draft.enabled} onCheckedChange={(v) => set({ enabled: v })} />
                      Enable automatic daily/weekly generation
                    </label>

                    <div className="grid grid-cols-2 gap-2">
                      <div>
                        <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Book</div>
                        <Select value={draft.book || ""} onValueChange={(v) => set({ book: v })}>
                          <SelectTrigger className="h-8 text-xs"><SelectValue placeholder="Choose a book" /></SelectTrigger>
                          <SelectContent>
                            {bibleBooks.map(b => <SelectItem key={b.id} value={b.id}>{b.name}</SelectItem>)}
                          </SelectContent>
                        </Select>
                      </div>
                      <div>
                        <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Translation</div>
                        <Select value={draft.translation || "web"} onValueChange={(v) => set({ translation: v })}>
                          <SelectTrigger className="h-8 text-xs"><SelectValue placeholder="Translation" /></SelectTrigger>
                          <SelectContent>
                            {flatTranslations.map(t => <SelectItem key={t.id} value={t.id}>{t.category} — {t.name}</SelectItem>)}
                          </SelectContent>
                        </Select>
                      </div>
                      <div>
                        <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Frequency</div>
                        <Select value={draft.frequency || "daily"} onValueChange={(v) => set({ frequency: v })}>
                          <SelectTrigger className="h-8 text-xs"><SelectValue /></SelectTrigger>
                          <SelectContent>
                            <SelectItem value="daily">Daily</SelectItem>
                            <SelectItem value="weekly">Weekly</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>
                      <div>
                        <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Time (local)</div>
                        <Input type="time" className="h-8 text-xs" value={draft.time || "09:00"} onChange={(e) => set({ time: e.target.value })} />
                      </div>
                      {draft.frequency === "weekly" && (
                        <div>
                          <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Day of week</div>
                          <Select value={String(draft.day_of_week ?? 0)} onValueChange={(v) => set({ day_of_week: parseInt(v, 10) })}>
                            <SelectTrigger className="h-8 text-xs"><SelectValue /></SelectTrigger>
                            <SelectContent>
                              {["Sunday","Monday","Tuesday","Wednesday","Thursday","Friday","Saturday"].map((d, i) => (
                                <SelectItem key={d} value={String(i)}>{d}</SelectItem>
                              ))}
                            </SelectContent>
                          </Select>
                        </div>
                      )}
                      <div>
                        <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Next chapter</div>
                        <Input type="number" min={1} className="h-8 text-xs" value={draft.next_chapter ?? 1} onChange={(e) => set({ next_chapter: e.target.value })} />
                      </div>
                    </div>

                    <div className="grid grid-cols-2 gap-2">
                      <div>
                        <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Languages (comma-separated)</div>
                        <Input className="h-8 text-xs" placeholder={(p.languages || []).join(", ") || "English"} value={draft.languages || ""} onChange={(e) => set({ languages: e.target.value })} />
                      </div>
                      <div>
                        <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Styles (comma-separated)</div>
                        <Input className="h-8 text-xs" placeholder={(p.styles || []).join(", ") || "Cinematic"} value={draft.styles || ""} onChange={(e) => set({ styles: e.target.value })} />
                      </div>
                    </div>
                    <p className="text-[11px] text-muted-foreground">Up to 4 language × style combinations are generated per run. Leave blank to use the project's configured languages/styles.</p>

                    <div className="flex gap-2">
                      <Button size="sm" disabled={scheduleUi[p.id]?.saving} onClick={() => saveSchedule(p.id)}>
                        {scheduleUi[p.id]?.saving ? <Loader2 className="w-3 h-3 mr-1.5 animate-spin" /> : <Check className="w-3 h-3 mr-1.5" />}
                        Save schedule
                      </Button>
                      <Button size="sm" variant="secondary" disabled={scheduleUi[p.id]?.generating || !draft.book} onClick={() => generateNow(p.id)}>
                        {scheduleUi[p.id]?.generating ? <Loader2 className="w-3 h-3 mr-1.5 animate-spin" /> : <Sparkles className="w-3 h-3 mr-1.5" />}
                        Generate now
                      </Button>
                    </div>
                  </div>
                );
              })()}
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
