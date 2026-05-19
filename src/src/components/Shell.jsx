import { useEffect, useState } from "react";
import { NavLink, useLocation, useNavigate } from "react-router-dom";
import { useStudio } from "../lib/store";
import { Music2, LayoutDashboard, BookOpen, Bot, FileJson, Mic2, Waves, Scissors, Image as ImageIcon, Film, Tv, UploadCloud, Activity, Settings as Cog, Palette, Sparkles, Menu, ChevronLeft, ChevronRight } from "lucide-react";

const NAV = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard, testid: "nav-dashboard" },
  { to: "/channels", label: "Channel Manager", icon: Tv, testid: "nav-channels" },
  { to: "/bible", label: "Bible Sources", icon: BookOpen, testid: "nav-bible" },
  { to: "/composer", label: "AI Composer", icon: Bot, testid: "nav-composer-ai" },
  { to: "/lyrics", label: "Lyrics Import", icon: FileJson, testid: "nav-lyrics" },
  { to: "/music", label: "Music Gen", icon: Mic2, testid: "nav-music" },
  { to: "/analysis", label: "Audio Analysis", icon: Waves, testid: "nav-analysis" },
  { to: "/sections", label: "Section Editor", icon: Scissors, testid: "nav-sections" },
  { to: "/images", label: "Image Gen", icon: ImageIcon, testid: "nav-images" },
  { to: "/video", label: "Video Composer", icon: Film, testid: "nav-video" },
  { to: "/upload", label: "Upload", icon: UploadCloud, testid: "nav-upload" },
  { to: "/jobs", label: "Jobs Monitor", icon: Activity, testid: "nav-jobs" },
  { to: "/settings", label: "Settings", icon: Cog, testid: "nav-settings" },
];

function ThemeBtn({ id, label, current, setTheme }) {
  const active = current === id;
  return (
    <button
      data-testid={`theme-${id}`}
      onClick={() => setTheme(id)}
      className={`px-2 py-1 rounded text-xs text-mono uppercase tracking-wider transition-colors ${active ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground hover:bg-secondary"}`}
    >{label}</button>
  );
}

export default function Shell({ children }) {
  const { theme, setTheme, activeProjectId, projects, jobs } = useStudio();
  const project = projects.find(p => p.id === activeProjectId);
  const running = jobs.filter(j => j.status === "running" || j.status === "queued").length;
  const loc = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    const handleKeyDown = (e) => {
      if (e.ctrlKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
        e.preventDefault();
        const currentIdx = NAV.findIndex(item => item.to === loc.pathname);
        if (currentIdx === -1) return;
        let nextIdx;
        if (e.key === "ArrowUp") {
          nextIdx = (currentIdx - 1 + NAV.length) % NAV.length;
        } else {
          nextIdx = (currentIdx + 1) % NAV.length;
        }
        navigate(NAV[nextIdx].to);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [loc.pathname, navigate]);

  const [collapsed, setCollapsed] = useState(false);

  return (
    <div className="min-h-screen flex bg-background text-foreground">
      <aside className={`${collapsed?"w-16":"w-64"} shrink-0 border-r border-border bg-card/40 flex flex-col sticky top-0 h-screen`}>
        <div className="px-3 py-3 border-b border-border flex items-center gap-2">
          <div className="w-9 h-9 flex items-center justify-center">
            <img src="/src-tauri/icons/icon.png" alt="Lightkid AI" className="w-9 h-9 object-contain" />
          </div>
          {!collapsed && (
            <div>
              <div className="text-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">Studio</div>
              <div className="font-semibold text-base leading-tight">Lightkid AI</div>
            </div>
          )}
        </div>

        <nav className="flex-1 overflow-y-auto py-3 scroll-thin">
          {NAV.map(({ to, label, icon: Icon, testid }) => (
            <NavLink
              key={to}
              to={to}
              data-testid={testid}
              className={({ isActive }) =>
                `flex items-center gap-3 px-5 py-2 text-sm transition-colors border-l-2 ${
                  isActive ? "border-primary bg-primary/10 text-foreground" : "border-transparent text-muted-foreground hover:text-foreground hover:bg-secondary/50"
                }`
              }
            >
              <Icon className="w-4 h-4" />
              {!collapsed && <span>{label}</span>}
            </NavLink>
          ))}
        </nav>

        <div className="border-t border-border p-3 space-y-3">
          {!collapsed && (
            <div>
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground flex items-center gap-2">
                <Palette className="w-3 h-3" /> Theme
              </div>
              <div className="flex gap-2 mt-2">
                <ThemeBtn id="obsidian" label="Obsidian" current={theme} setTheme={setTheme} />
                <ThemeBtn id="aurora" label="Aurora" current={theme} setTheme={setTheme} />
                <ThemeBtn id="vellum" label="Vellum" current={theme} setTheme={setTheme} />
              </div>
            </div>
          )}
        </div>
      </aside>

      <main className="flex-1 flex flex-col min-w-0">
        <header className="border-b border-border bg-card/30 backdrop-blur flex items-center justify-between px-8 py-4">
          <div className="flex items-center gap-3 min-w-0">
            <button className="p-1 rounded hover:bg-muted/30" onClick={()=>setCollapsed(c=>!c)} aria-label="Toggle sidebar">
              {collapsed ? <ChevronRight className="w-4 h-4" /> : <ChevronLeft className="w-4 h-4" />}
            </button>
            <Music2 className="w-4 h-4 text-primary" />
            <div className="text-mono text-[11px] uppercase tracking-[0.25em] text-muted-foreground">
              {loc.pathname === "/" ? "dashboard" : loc.pathname.slice(1)}
            </div>
            {project && (
              <>
                <span className="text-muted-foreground">/</span>
                <span data-testid="active-project-name" className="text-sm font-medium truncate">{project.name}</span>
              </>
            )}
          </div>
          <div className="flex items-center gap-3">
            {running > 0 && (
              <div className="flex items-center gap-2 text-xs text-mono">
                <span className="relative flex h-2 w-2"><span className="absolute inline-flex h-full w-full rounded-full bg-primary opacity-75 animate-ping"></span><span className="relative inline-flex rounded-full h-2 w-2 bg-primary"></span></span>
                <span className="text-muted-foreground">{running} job{running>1?"s":""} active</span>
              </div>
            )}
          </div>
        </header>
        <div className="flex-1 overflow-auto scroll-thin">{children}</div>
      </main>
    </div>
  );
}
