import { useEffect, useState, useRef, useCallback } from "react";
import { NavLink, useLocation, useNavigate } from "react-router-dom";
import { useStudio } from "../lib/store";
import appPkg from "../../package.json";
import {
  Music2, LayoutDashboard, BookOpen, Bot, FileJson, Mic2, Waves,
  Scissors, Image as ImageIcon, Film, Tv, UploadCloud, Activity,
  Settings as Cog, Palette, ChevronLeft, ChevronRight, ChevronDown,
  Home, ArrowLeft, ArrowRight, Save, Download, X, Keyboard, Check,
} from "lucide-react";
import { toast } from "sonner";

// ── Navigation configuration with groups ──
const NAV = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard, testid: "nav-dashboard", group: "overview" },
  { to: "/channels", label: "Channel Manager", icon: Tv, testid: "nav-channels", group: "content" },
  { to: "/bible", label: "Bible Sources", icon: BookOpen, testid: "nav-bible", group: "content" },
  { to: "/composer", label: "AI Composer", icon: Bot, testid: "nav-composer-ai", group: "content" },
  { to: "/lyrics", label: "Lyrics Import", icon: FileJson, testid: "nav-lyrics", group: "content" },
  { to: "/sections", label: "Section Editor", icon: Scissors, testid: "nav-sections", group: "ai-generate" },
  { to: "/music", label: "Music Gen", icon: Mic2, testid: "nav-music", group: "ai-generate" },
  { to: "/analysis", label: "Audio Analysis", icon: Waves, testid: "nav-analysis", group: "ai-generate" },
  { to: "/images", label: "Image Gen", icon: ImageIcon, testid: "nav-images", group: "ai-generate" },
  { to: "/video", label: "Video Composer", icon: Film, testid: "nav-video", group: "publish" },
  { to: "/upload", label: "Upload", icon: UploadCloud, testid: "nav-upload", group: "publish" },
  { to: "/jobs", label: "Jobs Monitor", icon: Activity, testid: "nav-jobs", group: "system" },
  { to: "/settings", label: "Settings", icon: Cog, testid: "nav-settings", group: "system" },
];

// Group labels for breadcrumb context
const GROUP_LABELS = {
  overview: "Overview",
  content: "Content Creator",
  "ai-generate": "AI Generate",
  publish: "Publish",
  system: "System",
};

// Navigation history stack for back/forward
const navHistory = { stack: [], index: -1 };

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

// ── Breadcrumb dropdown showing ALL nav options with group separators ──
function BreadcrumbDropdown({ currentPath }) {
  const [open, setOpen] = useState(false);
  const ref = useRef(null);
  const navigate = useNavigate();

  const currentItem = NAV.find((n) => n.to === currentPath);

  useEffect(() => {
    if (!open) return;
    const handler = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  if (!currentItem) return null;

  // Build items with separators between groups
  let lastGroup = null;
  const dropdownItems = [];
  NAV.forEach((item) => {
    if (lastGroup !== null && lastGroup !== item.group) {
      dropdownItems.push({ type: "separator" });
    }
    dropdownItems.push({ type: "item", ...item });
    lastGroup = item.group;
  });

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1 text-mono text-[11px] uppercase tracking-[0.25em] text-muted-foreground hover:text-foreground transition-colors"
      >
        {currentItem.label}
        <ChevronDown className="w-3 h-3" />
      </button>
      {open && (
        <div className="absolute z-[9999] top-full mt-1 left-0 bg-popover backdrop-blur-xl border-2 border-border/80 rounded-xl shadow-2xl p-1.5 min-w-[240px] fade-in max-h-[80vh] overflow-y-auto">
          {dropdownItems.map((entry, idx) => {
            if (entry.type === "separator") {
              return <div key={`sep-${idx}`} className="h-px bg-border/60 mx-2 my-1" />;
            }
            const Icon = entry.icon;
            const isActive = entry.to === currentPath;
            return (
              <button
                key={entry.to}
                onClick={() => { navigate(entry.to); setOpen(false); }}
                className={`w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-sm transition-colors ${
                  isActive
                    ? "bg-primary text-primary-foreground font-bold"
                    : "font-semibold text-muted-foreground hover:bg-accent hover:text-foreground"
                }`}
              >
                <Icon className="w-3.5 h-3.5 shrink-0" />
                {entry.label}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ── Main Shell component ──
export default function Shell({ children }) {
  const { theme, setTheme, activeProjectId, projects, jobs } = useStudio();
  const project = projects.find((p) => p.id === activeProjectId);
  const running = jobs.filter((j) => j.status === "running" || j.status === "queued").length;
  const loc = useLocation();
  const navigate = useNavigate();
  const [collapsed, setCollapsed] = useState(false);
  const [historyStack, setHistoryStack] = useState([]);
  const [historyIdx, setHistoryIdx] = useState(-1);

  // Track navigation history for back/forward
  useEffect(() => {
    setHistoryStack((prev) => {
      const newStack = [...prev.slice(0, historyIdx + 1), loc.pathname];
      return newStack;
    });
    setHistoryIdx((prev) => {
      const newStack = historyStack.slice(0, prev + 1);
      return newStack.length - 1;
    });
  }, [loc.pathname]);

  // Update document title with current page name
  useEffect(() => {
    const current = NAV.find((n) => n.to === loc.pathname);
    const title = current ? `${current.label} — Lightkid AI Studio` : "Lightkid AI Studio";
    document.title = title;
  }, [loc.pathname]);

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e) => {
      // Don't navigate if an input/textarea is focused
      const tag = document.activeElement?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || document.activeElement?.contentEditable === "true") return;

      const currentIdx = NAV.findIndex((item) => item.to === loc.pathname);
      if (currentIdx === -1) return;

      if (e.ctrlKey) {
        if (e.key === "ArrowRight") {
          // Next page
          e.preventDefault();
          const nextIdx = (currentIdx + 1) % NAV.length;
          navigate(NAV[nextIdx].to);
        } else if (e.key === "ArrowLeft") {
          // Previous page
          e.preventDefault();
          const prevIdx = (currentIdx - 1 + NAV.length) % NAV.length;
          navigate(NAV[prevIdx].to);
        } else if (e.key === "ArrowDown") {
          // Last page
          e.preventDefault();
          navigate(NAV[NAV.length - 1].to);
        } else if (e.key === "ArrowUp") {
          // First page (Dashboard)
          e.preventDefault();
          navigate(NAV[0].to);
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [loc.pathname, navigate]);

  const goBack = () => {
    if (historyIdx > 0) {
      setHistoryIdx((prev) => prev - 1);
      navigate(historyStack[historyIdx - 1]);
    }
  };

  const goForward = () => {
    if (historyIdx < historyStack.length - 1) {
      setHistoryIdx((prev) => prev + 1);
      navigate(historyStack[historyIdx + 1]);
    }
  };

  // Save button visual feedback
  const [saveFeedback, setSaveFeedback] = useState(null);
  useEffect(() => {
    const handler = () => {
      setSaveFeedback("saved");
      toast.success("Settings saved");
      setTimeout(() => setSaveFeedback(null), 1500);
    };
    window.addEventListener("studio:save", handler);
    return () => window.removeEventListener("studio:save", handler);
  }, []);

  // Export all settings, configs, templates, etc.
  const handleExport = useCallback(() => {
    try {
      const exportData = {
        exportedAt: new Date().toISOString(),
        version: appPkg.version,
        settings: (() => {
          try { return JSON.parse(localStorage.getItem("studio:settings") || "{}"); } catch { return {}; }
        })(),
        composerConfig: (() => {
          try { return JSON.parse(localStorage.getItem("studio:composer-config") || "{}"); } catch { return {}; }
        })(),
        composerProfiles: (() => {
          try { return JSON.parse(localStorage.getItem("studio:composer-profiles") || "{}"); } catch { return {}; }
        })(),
        customThemePresets: (() => {
          try { return JSON.parse(localStorage.getItem("studio:custom-theme-presets") || "{}"); } catch { return {}; }
        })(),
        chapterDrafts: (() => {
          try { return JSON.parse(localStorage.getItem("studio:pasted-chapters") || "{}"); } catch { return {}; }
        })(),
        channelDiscoveryTags: (() => {
          try { return JSON.parse(localStorage.getItem("studio:channel-discovery-tags") || "[]"); } catch { return []; }
        })(),
        templates: (() => {
          try { return JSON.parse(localStorage.getItem("studio:templates") || "[]"); } catch { return []; }
        })(),
      };
      const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `studio-export-${Date.now()}.json`;
      a.click();
      URL.revokeObjectURL(url);
      toast.success("Studio data exported");
    } catch (err) {
      console.error("Export failed", err);
      toast.error("Export failed");
    }
  }, []);

  useEffect(() => {
    window.addEventListener("studio:export", handleExport);
    return () => window.removeEventListener("studio:export", handleExport);
  }, [handleExport]);

  const currentNav = NAV.find((n) => n.to === loc.pathname);
  const groupLabel = currentNav ? GROUP_LABELS[currentNav.group] : "";

  return (
    <div className="min-h-screen flex bg-background text-foreground">
      <aside className={`${collapsed ? "w-16" : "w-64"} shrink-0 border-r border-border bg-card/40 flex flex-col sticky top-0 h-screen transition-all`}>
        <div className="px-3 py-3 border-b border-border flex items-center gap-2">
          <div className="w-9 h-9 flex items-center justify-center">
            <img src="/icon.png" alt="Lightkid AI" className="w-9 h-9 object-contain" />
          </div>
          {!collapsed && (
            <div>
              <div className="text-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">Studio</div>
              <div className="font-semibold text-base leading-tight flex items-baseline gap-2">Lightkid AI <span className="text-xs text-muted-foreground">v{appPkg.version}</span></div>
            </div>
          )}
        </div>

        <nav className="flex-1 overflow-y-auto py-3 scroll-thin">
          {NAV.map(({ to, label, icon: Icon, testid, group }) => {
            const prevGroup = NAV[NAV.indexOf(NAV.find((n) => n.to === to)) - 1]?.group;
            const showGroupHeader = group !== prevGroup;
            return (
              <div key={to}>
                {showGroupHeader && !collapsed && (
                  <div className="px-5 pt-3 pb-1 text-[9px] uppercase tracking-[0.2em] text-muted-foreground/60 font-medium">
                    {GROUP_LABELS[group]}
                  </div>
                )}
                <NavLink
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
              </div>
            );
          })}
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
          {!collapsed && (
            <div className="text-[9px] text-muted-foreground/50 space-y-0.5 pt-2 border-t border-border/50">
              <div className="flex items-center gap-1"><Keyboard className="w-2.5 h-2.5" /> Navigation shortcuts</div>
              <div>Ctrl+← → prev/next page</div>
              <div>Ctrl+↑ ↓ first/last page</div>
            </div>
          )}
        </div>
      </aside>

      <main className="flex-1 flex flex-col min-w-0">
        {/* ── Top navigation bar (sticky, always on top) ── */}
        <header className="sticky top-0 z-30 border-b border-border bg-card shadow-sm flex items-center justify-between px-6 py-3 gap-4 shrink-0">
          <div className="flex items-center gap-2 min-w-0">
            {/* Sidebar toggle */}
            <button className="p-1.5 rounded hover:bg-muted/30 shrink-0" onClick={() => setCollapsed((c) => !c)} aria-label="Toggle sidebar">
              {collapsed ? <ChevronRight className="w-4 h-4" /> : <ChevronLeft className="w-4 h-4" />}
            </button>

            {/* Back/Forward nav buttons */}
            <div className="flex items-center gap-0.5 shrink-0">
              <button
                onClick={goBack}
                disabled={historyIdx <= 0}
                className="p-1.5 rounded hover:bg-muted/30 disabled:opacity-30 disabled:cursor-not-allowed"
                title="Back (history)"
              >
                <ArrowLeft className="w-4 h-4" />
              </button>
              <button
                onClick={goForward}
                disabled={historyIdx >= historyStack.length - 1}
                className="p-1.5 rounded hover:bg-muted/30 disabled:opacity-30 disabled:cursor-not-allowed"
                title="Forward (history)"
              >
                <ArrowRight className="w-4 h-4" />
              </button>
              <button
                onClick={() => navigate("/")}
                className="p-1.5 rounded hover:bg-muted/30"
                title="Home (Dashboard)"
              >
                <Home className="w-4 h-4" />
              </button>
            </div>

            {/* Breadcrumb separator */}
            <div className="w-px h-5 bg-border mx-1" />

            {/* Breadcrumbs */}
            <div className="flex items-center gap-2 min-w-0">
              <span className="text-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground/60 shrink-0">
                {groupLabel}
              </span>
              {groupLabel && <span className="text-muted-foreground/40">/</span>}
              <BreadcrumbDropdown currentPath={loc.pathname} />
              {project && (
                <>
                  <span className="text-muted-foreground/40">/</span>
                  <span data-testid="active-project-name" className="text-sm font-medium truncate max-w-[200px]">{project.name}</span>
                </>
              )}
            </div>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            {/* Running jobs indicator */}
            {running > 0 && (
              <div className="flex items-center gap-2 text-xs text-mono mr-2">
                <span className="relative flex h-2 w-2"><span className="absolute inline-flex h-full w-full rounded-full bg-primary opacity-75 animate-ping"></span><span className="relative inline-flex rounded-full h-2 w-2 bg-primary"></span></span>
                <span className="text-muted-foreground">{running} job{running > 1 ? "s" : ""}</span>
              </div>
            )}

            {/* Action buttons */}
            <button
              onClick={() => window.dispatchEvent(new CustomEvent("studio:save"))}
              className={`p-1.5 rounded transition-all duration-300 ${
                saveFeedback === "saved"
                  ? "bg-emerald-500/20 text-emerald-400"
                  : "hover:bg-muted/30 text-muted-foreground hover:text-foreground"
              }`}
              title="Save (Ctrl+S)"
            >
              {saveFeedback === "saved" ? (
                <Check className="w-4 h-4 animate-in zoom-in duration-200" />
              ) : (
                <Save className="w-4 h-4" />
              )}
            </button>
            <button
              onClick={() => window.dispatchEvent(new CustomEvent("studio:export"))}
              className="p-1.5 rounded hover:bg-muted/30 text-muted-foreground hover:text-foreground"
              title="Export all settings & data"
            >
              <Download className="w-4 h-4" />
            </button>
          </div>
        </header>

        <div className="flex-1 overflow-auto scroll-thin">{children}</div>
      </main>
    </div>
  );
}