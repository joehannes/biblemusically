import { createContext, useContext, useEffect, useState, useCallback } from "react";
import { api } from "./api";

const Ctx = createContext(null);

export function StudioProvider({ children }) {
  const [theme, setThemeState] = useState(() => localStorage.getItem("studio:theme") || "obsidian");
  const [activeProjectId, setActiveProjectId] = useState(() => localStorage.getItem("studio:project") || null);
  const [activeSongId, setActiveSongId] = useState(() => localStorage.getItem("studio:song") || null);
  const [projects, setProjects] = useState([]);
  const [activeProject, setActiveProject] = useState(() => {
    try { return JSON.parse(localStorage.getItem("studio:activeProject") || "null"); } catch { return null; }
  });
  const [songs, setSongs] = useState([]);
  const [jobs, setJobs] = useState([]);

  useEffect(() => { document.documentElement.dataset.theme = theme; }, [theme]);
  const setTheme = (t) => { setThemeState(t); localStorage.setItem("studio:theme", t); };

  const perProjectKeys = ["ai-composer-cfg", "studio:composer-profiles", "studio:composer-generated-items", "studio:custom-theme-presets", "studio:bible-selection"];

  const selectProject = async (id) => {
    const prev = activeProjectId;
    // persist relevant drafts for previous project
    try {
      if (prev) {
        for (const k of perProjectKeys) {
          const v = localStorage.getItem(k);
          if (v !== null) localStorage.setItem(`${k}:project:${prev}`, v);
        }
      }
    } catch (e) { console.warn('Failed to persist per-project drafts', e); }

    setActiveProjectId(id);
    localStorage.setItem("studio:project", id || "");
    setActiveSongId(null);
    localStorage.removeItem("studio:song");

    // restore drafts for selected project
    try {
      if (id) {
        for (const k of perProjectKeys) {
          const v = localStorage.getItem(`${k}:project:${id}`);
          if (v !== null) localStorage.setItem(k, v);
        }
      }
    } catch (e) { console.warn('Failed to restore per-project drafts', e); }
  };
  
  // Load full project data when activeProjectId changes and propagate into state
  useEffect(()=>{
    (async ()=>{
      if (!activeProjectId) { setActiveProject(null); localStorage.removeItem("studio:activeProject"); return; }
      try {
        const proj = await api.getProject(activeProjectId);
        setActiveProject(proj);
        try { localStorage.setItem("studio:activeProject", JSON.stringify(proj)); } catch(e){}
        // merge/replace in projects list for immediate UI reflection
        setProjects(prev => {
          const exists = prev.find(p=>p.id===proj.id);
          if (!exists) return [proj, ...prev];
          return prev.map(p => p.id===proj.id ? proj : p);
        });
      } catch(e){ console.warn('Failed to load project', e); }
    })();
  }, [activeProjectId]);

  const saveProjectSettings = async (partial) => {
    if (!activeProjectId) throw new Error('No active project');
    try {
      await api.updateProject(activeProjectId, partial);
      // refresh single project and projects list
      const proj = await api.getProject(activeProjectId);
      setActiveProject(proj);
      try { localStorage.setItem("studio:activeProject", JSON.stringify(proj)); } catch(e){}
      setProjects(prev => prev.map(p => p.id===proj.id ? proj : p));
      return proj;
    } catch(e){ throw e; }
  };
  const selectSong = (id) => { setActiveSongId(id); if (id) localStorage.setItem("studio:song", id); else localStorage.removeItem("studio:song"); };

  const refreshProjects = useCallback(async () => { try { setProjects(await api.listProjects()); } catch(e){} }, []);
  const refreshSongs = useCallback(async () => { if (!activeProjectId) { setSongs([]); return; } try { setSongs(await api.listSongs(activeProjectId)); } catch(e){} }, [activeProjectId]);
  const refreshJobs = useCallback(async () => { try { setJobs(await api.listJobs()); } catch(e){} }, []);

  useEffect(() => { refreshProjects(); }, [refreshProjects]);
  useEffect(() => { refreshSongs(); }, [refreshSongs]);
  useEffect(() => {
    refreshJobs();
    const t = setInterval(refreshJobs, 2500);
    return () => clearInterval(t);
  }, [refreshJobs]);

  const value = {
    theme, setTheme,
    activeProjectId, selectProject,
    activeProject, saveProjectSettings,
    activeSongId, selectSong,
    projects, refreshProjects,
    songs, refreshSongs,
    jobs, refreshJobs,
  };
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export const useStudio = () => useContext(Ctx);
