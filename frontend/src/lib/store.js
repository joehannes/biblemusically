import { createContext, useContext, useEffect, useState, useCallback } from "react";
import { api } from "./api";

const Ctx = createContext(null);

export function StudioProvider({ children }) {
  const [theme, setThemeState] = useState(() => localStorage.getItem("studio:theme") || "obsidian");
  const [activeProjectId, setActiveProjectId] = useState(() => localStorage.getItem("studio:project") || null);
  const [activeSongId, setActiveSongId] = useState(() => localStorage.getItem("studio:song") || null);
  const [projects, setProjects] = useState([]);
  const [songs, setSongs] = useState([]);
  const [jobs, setJobs] = useState([]);

  useEffect(() => { document.documentElement.dataset.theme = theme; }, [theme]);
  const setTheme = (t) => { setThemeState(t); localStorage.setItem("studio:theme", t); };

  const selectProject = (id) => { setActiveProjectId(id); localStorage.setItem("studio:project", id || ""); setActiveSongId(null); localStorage.removeItem("studio:song"); };
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
    activeSongId, selectSong,
    projects, refreshProjects,
    songs, refreshSongs,
    jobs, refreshJobs,
  };
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export const useStudio = () => useContext(Ctx);
