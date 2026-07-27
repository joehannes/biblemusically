import { useEffect, useState, useRef, useCallback } from "react";
import { NavLink, useLocation, useNavigate } from "react-router-dom";
import { useStudio } from "../lib/store";
import {
  UI_LANGUAGES, getUiLanguage, setUiLanguage, subscribeLanguage, initUiLanguage, isBundledLanguage,
  allLanguages, customLanguages, addCustomLanguage, removeCustomLanguage,
} from "../lib/uiTranslate";
import { useRequireGuideStep } from "./GuideStepDialog";
import ToursFab from "./Tours";
import HealthBanner from "./HealthBanner";
import { api } from "../lib/api";
import { getVersion } from "@tauri-apps/api/app";
// Fallback only, for the very first paint before getVersion() resolves — this file lives
// under vite's project root (`src/`), which has its own long-stale package.json (it lagged
// the real releases for months, e.g. showing 0.34.1 while the app had shipped 0.39.0), so it
// must never be trusted as the source of truth for what's actually running. getVersion() reads
// straight from the built app's own tauri.conf.json and is always correct.
import appPkg from "../../package.json";
import {
  Music2,
  LayoutDashboard,
  BookOpen,
  Bot,
  Sparkles,
  FileJson,
  Mic2,
  Waves,
  Scissors,
  Image as ImageIcon,
  Film,
  Clapperboard,
  Tv,
  UploadCloud,
  Activity,
  Database,
  Share2,
  BarChart3,
  Settings as Cog,
  Palette,
  ChevronLeft,
  ChevronRight,
  ChevronDown,
  Home,
  User,
  ArrowLeft,
  ArrowRight,
  Save,
  CloudUpload,
  HardDrive,
  Download,
  X,
  Keyboard,
  Info,
  Languages,
  Loader2,
  Check,
  Menu,
  GitBranch,
  Folder,
  Globe,
  Wand2,
  Workflow as WorkflowIcon, Megaphone, Plus, Disc3, BookMarked, Shirt, ClipboardList, KeyRound, Users, UserCircle, MessageSquare} from "lucide-react";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import {
  PageActionsContext, ActionRegistryContext, useActionRegistry, splitActions,
} from "../lib/pageActions";
import {
  installAutosave, setAutosaveProject, setAutosaveView, commitPending,
} from "../lib/autosave";
import { subscribePipeline } from "../lib/genPipeline";

// ── Navigation configuration with groups ──
const NAV = [
  {
    to: "/",
    label: "Dashboard",
    icon: LayoutDashboard,
    testid: "nav-dashboard",
    group: "overview",
  },
  {
    to: "/workflow",
    label: "Workflow",
    icon: WorkflowIcon,
    testid: "nav-workflow",
    group: "overview",
  },
  {
    to: "/channels",
    label: "Channel Manager",
    icon: Tv,
    testid: "nav-channels",
    group: "content",
  },
  {
    to: "/bible",
    label: "Bible Sources",
    icon: BookOpen,
    testid: "nav-bible",
    group: "content",
  },
  // Freeform sits BEFORE the AI Composer: like Bible Sources, it is a *source* of raw material
  // that the composer then turns into songs — so it belongs upstream in the flow.
  {
    to: "/freeform",
    label: "Freeform Composer",
    icon: Sparkles,
    testid: "nav-freeform",
    group: "content",
  },
  {
    to: "/composer",
    label: "AI Composer",
    icon: Bot,
    testid: "nav-composer-ai",
    group: "content",
  },
  {
    to: "/lyrics",
    label: "Lyrics Import",
    icon: FileJson,
    testid: "nav-lyrics",
    group: "content",
  },
  {
    to: "/sound",
    label: "Sound Studio",
    icon: Music2,
    testid: "nav-sound",
    group: "ai-generate",
  },
  {
    to: "/music",
    label: "Music Gen",
    icon: Mic2,
    testid: "nav-music",
    group: "ai-generate",
  },
  {
    to: "/analysis",
    label: "Audio Analysis",
    icon: Waves,
    testid: "nav-analysis",
    group: "ai-generate",
  },
  {
    to: "/characters",
    label: "Characters",
    icon: User,
    testid: "nav-characters",
    group: "ai-generate",
  },
  {
    to: "/sections",
    label: "Section Editor",
    icon: Scissors,
    testid: "nav-sections",
    group: "ai-generate",
  },
  {
    to: "/styles",
    label: "Style Studio",
    icon: Palette,
    testid: "nav-styles",
    group: "ai-generate",
  },
  {
    to: "/images",
    label: "Image Gen",
    icon: ImageIcon,
    testid: "nav-images",
    group: "ai-generate",
  },
  {
    to: "/transitions",
    label: "Transitions & FX",
    icon: Clapperboard,
    testid: "nav-transitions",
    group: "publish",
  },
  {
    to: "/overlays",
    label: "Overlay Studio",
    icon: Waves,
    testid: "nav-overlays",
    group: "publish",
  },
  {
    to: "/video",
    label: "Video Composer",
    icon: Film,
    testid: "nav-video",
    group: "publish",
  },
  {
    to: "/publicity",
    label: "Publicity",
    icon: Megaphone,
    testid: "nav-publicity",
    group: "publish",
  },
  {
    to: "/novels",
    label: "Graphic Novels",
    icon: BookMarked,
    testid: "nav-novels",
    group: "publish",
  },
  {
    to: "/distribution",
    label: "Distribution",
    icon: Disc3,
    testid: "nav-distribution",
    group: "publish",
  },
  {
    to: "/print",
    label: "Print on Demand",
    icon: Shirt,
    testid: "nav-print",
    group: "publish",
  },
  {
    to: "/upload",
    label: "Upload",
    icon: UploadCloud,
    testid: "nav-upload",
    group: "publish",
  },
  {
    to: "/browser",
    label: "Web Browser",
    icon: Globe,
    testid: "nav-browser",
    group: "system",
  },
  {
    to: "/account",
    label: "Account",
    icon: UserCircle,
    testid: "nav-account",
    group: "system",
  },
  {
    to: "/feedback",
    label: "Tell me something",
    icon: MessageSquare,
    testid: "nav-feedback",
    group: "system",
  },
  {
    to: "/team",
    label: "Team",
    icon: Users,
    testid: "nav-team",
    group: "system",
  },
  {
    to: "/access",
    label: "Access & Permissions",
    icon: KeyRound,
    testid: "nav-access",
    group: "system",
  },
  {
    to: "/clipboard",
    label: "Clipboard",
    icon: ClipboardList,
    testid: "nav-clipboard",
    group: "system",
  },
  {
    to: "/macros",
    label: "Macro Manager",
    icon: Wand2,
    testid: "nav-macros",
    group: "system",
  },
  {
    to: "/jobs",
    label: "Jobs Monitor",
    icon: Activity,
    testid: "nav-jobs",
    group: "system",
  },
  {
    to: "/insights",
    label: "Insights",
    icon: BarChart3,
    testid: "nav-insights",
    group: "system",
  },
  {
    to: "/social",
    label: "Social Presence",
    icon: Share2,
    testid: "nav-social",
    group: "publish",
  },
  {
    to: "/data",
    label: "Data & Sync",
    icon: Database,
    testid: "nav-data",
    group: "system",
  },
  {
    to: "/settings",
    label: "Settings",
    icon: Cog,
    testid: "nav-settings",
    group: "system",
  },
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
    >
      {label}
    </button>
  );
}

const THEMES = [
  { id: "obsidian", label: "Obsidian" },
  { id: "aurora", label: "Aurora" },
  { id: "twilight", label: "Twilight" },
  { id: "midnight", label: "Midnight" },
  { id: "forest", label: "Forest" },
  { id: "rosewood", label: "Rosewood" },
  { id: "vellum", label: "Vellum" },
  { id: "dawn", label: "Dawn" },
  { id: "daybreak", label: "Daybreak" },
  { id: "sandstone", label: "Sandstone" },
  { id: "arctic", label: "Arctic" },
  { id: "ember", label: "Ember" },
  { id: "cosmos", label: "Cosmos" },
  { id: "slate", label: "Slate" },
  { id: "mint", label: "Mint" },
];

// Theme picker as a menubar action button (moved out of the sidebar). A Palette button that opens
// a small popover of the theme swatches; closes on outside click / Escape.
function ThemeFab({ theme, setTheme }) {
  const [open, setOpen] = useState(false);
  const ref = useRef(null);
  useEffect(() => {
    if (!open) return;
    const onClick = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    const onKey = (e) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onClick);
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("mousedown", onClick); document.removeEventListener("keydown", onKey); };
  }, [open]);
  return (
    <div className="relative" ref={ref}>
      <button
        data-testid="theme-fab"
        onClick={() => setOpen((o) => !o)}
        className="p-1.5 rounded hover:bg-muted/30 shrink-0"
        title="Theme"
        aria-label="Theme"
      >
        <Palette className="w-4 h-4" />
      </button>
      {open && (
        <div className="absolute right-0 top-full mt-2 z-50 w-44 rounded-lg border border-border bg-card shadow-xl p-2">
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground px-1 pb-1.5 flex items-center gap-1.5">
            <Palette className="w-3 h-3" /> Theme
          </div>
          <div className="flex flex-wrap gap-1.5">
            {THEMES.map((t) => (
              <ThemeBtn key={t.id} id={t.id} label={t.label} current={theme} setTheme={(id) => { setTheme(id); }} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// Keyboard-shortcut hint behind an info icon in the sidebar header (previously a permanent block
// taking up space at the bottom of the sidebar). Shows on hover OR click so it's discoverable
// either way; click toggles so it stays open long enough to read on touch input.
function ShortcutHint({ collapsed }) {
  const [open, setOpen] = useState(false);
  const [pinned, setPinned] = useState(false);
  const ref = useRef(null);
  useEffect(() => {
    if (!pinned) return;
    const onClick = (e) => { if (ref.current && !ref.current.contains(e.target)) { setPinned(false); setOpen(false); } };
    const onKey = (e) => { if (e.key === "Escape") { setPinned(false); setOpen(false); } };
    document.addEventListener("mousedown", onClick);
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("mousedown", onClick); document.removeEventListener("keydown", onKey); };
  }, [pinned]);

  return (
    <div className={`relative ${collapsed ? "" : "ml-auto"}`} ref={ref}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => { if (!pinned) setOpen(false); }}
    >
      <button
        data-testid="shortcut-hint"
        onClick={() => { setPinned((p) => !p); setOpen(true); }}
        className="p-1 rounded hover:bg-muted/40 text-muted-foreground hover:text-foreground shrink-0"
        title="Navigation shortcuts"
        aria-label="Navigation shortcuts"
      >
        <Info className="w-3.5 h-3.5" />
      </button>
      {open && (
        <div className="absolute left-0 top-full mt-1.5 z-50 w-56 rounded-lg border border-border bg-card shadow-xl p-2.5">
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground pb-1.5 flex items-center gap-1.5">
            <Keyboard className="w-3 h-3" /> Navigation shortcuts
          </div>
          <div className="space-y-1 text-[11px] text-muted-foreground">
            <div className="flex items-center justify-between gap-2">
              <span>Previous / next page</span>
              <kbd className="text-mono text-[10px] px-1.5 py-0.5 rounded bg-muted border border-border shrink-0">Ctrl ← →</kbd>
            </div>
            <div className="flex items-center justify-between gap-2">
              <span>First / last page</span>
              <kbd className="text-mono text-[10px] px-1.5 py-0.5 rounded bg-muted border border-border shrink-0">Ctrl ↑ ↓</kbd>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// Language picker. Switching translates the rendered interface with the configured free AI
// provider (see lib/uiTranslate.js) — translations are cached per language, so a repeat switch is
// instant and costs no AI call.
function LanguageFab() {
  const [open, setOpen] = useState(false);
  const [lang, setLang] = useState(getUiLanguage());
  const [busy, setBusy] = useState(false);
  const [custom, setCustom] = useState(() => customLanguages());
  const [typed, setTyped] = useState("");
  const ref = useRef(null);

  useEffect(() => subscribeLanguage(setLang), []);
  useEffect(() => {
    if (!open) return;
    const onClick = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    const onKey = (e) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onClick);
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("mousedown", onClick); document.removeEventListener("keydown", onKey); };
  }, [open]);

  const pick = async (code) => {
    setOpen(false);
    if (code === lang) return;
    setBusy(true);
    // Notify only AFTER the UI is actually translated & in place (or truthfully report a failure) —
    // never a premature "translated" toast.
    const t = code !== "en" ? toast.loading("Translating the interface…") : null;
    try {
      const r = await setUiLanguage(code);
      if (code === "en") { toast.success("Back to English."); return; }
      if (r?.ok && r.applied > 0) {
        // A bundled language never touched the AI — say so, it explains why it was instant.
        toast.success(
          r.bundled
            ? `Interface translated (${r.applied} labels, built-in catalog).`
            : `Interface translated (${r.applied} labels).`,
          { id: t },
        );
      } else if (r?.error) {
        toast.error(`Couldn't translate: ${r.error}. Check your AI provider key in Settings.`, { id: t });
      } else {
        toast.message("Nothing to translate right now.", { id: t });
      }
    } catch (e) {
      toast.error(`Translation failed: ${e}`, { id: t });
    } finally { setBusy(false); }
  };

  const cur = allLanguages().find((l) => l.code === lang) || UI_LANGUAGES[0];
  const addOwn = async () => {
    const r = addCustomLanguage(typed);
    if (!r.ok) { toast.error(r.error); return; }
    setTyped("");
    setCustom(customLanguages());
    if (r.existing === "bundled") toast.message("That one already ships with the app — switching to it.");
    await pick(r.code);
  };
  const drop = (l) => (e) => {
    e.stopPropagation();
    removeCustomLanguage(l.code);
    setCustom(customLanguages());
    if (l.code === lang) pick("en");
  };
  return (
    <div className="relative" ref={ref} data-no-i18n>
      <button
        data-testid="language-fab"
        onClick={() => setOpen((o) => !o)}
        className="p-1.5 rounded hover:bg-muted/30 shrink-0 flex items-center gap-1"
        title={`Interface language — ${cur.native}`}
        aria-label="Interface language"
        disabled={busy}
      >
        {busy ? <Loader2 className="w-4 h-4 animate-spin" /> : <Languages className="w-4 h-4" />}
        <span className="text-[10px] uppercase tracking-wide text-muted-foreground">{cur.code}</span>
      </button>
      {open && (
        <div className="absolute right-0 top-full mt-2 z-50 w-52 max-h-80 overflow-y-auto scroll-thin rounded-lg border border-border bg-card shadow-xl p-1.5">
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground px-1.5 pb-1 flex items-center gap-1.5">
            <Languages className="w-3 h-3" /> Interface language
          </div>
          {[...UI_LANGUAGES, ...custom].map((l) => (
            <button
              key={l.code}
              onClick={() => pick(l.code)}
              className={`w-full text-left px-2 py-1.5 rounded text-sm hover:bg-muted/50 flex items-center justify-between gap-2 ${l.code === lang ? "text-primary font-medium" : ""}`}
            >
              <span className="truncate">{l.native}</span>
              <span className="flex items-center gap-1.5 shrink-0">
                {/* Built-in means the catalog ships with the app: instant, offline, no AI request. */}
                {isBundledLanguage(l.code) && l.code !== lang && (
                  <span className="text-[9px] uppercase tracking-wide text-muted-foreground">built-in</span>
                )}
                {l.custom && (
                  <>
                    <span className="text-[9px] uppercase tracking-wide text-muted-foreground">yours</span>
                    <span onClick={drop(l)} title="Forget this language" className="hover:text-destructive">
                      <X className="w-3 h-3" />
                    </span>
                  </>
                )}
                {l.code === lang && <Check className="w-3.5 h-3.5" />}
              </span>
            </button>
          ))}
          {/* Sixteen languages is not every language. Type any name and the AI translates into it. */}
          <div className="border-t border-border/50 mt-1 pt-1.5 px-1.5">
            <div className="text-[10px] uppercase tracking-widest text-muted-foreground pb-1">Your language</div>
            <div className="flex gap-1">
              <input
                value={typed}
                onChange={(e) => setTyped(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") addOwn(); }}
                placeholder="e.g. Tagalog, Swiss German"
                className="flex-1 min-w-0 bg-muted/30 border border-border rounded px-2 py-1 text-xs"
              />
              <button
                onClick={addOwn}
                disabled={busy || typed.trim().length < 2}
                className="px-2 py-1 rounded bg-primary/20 text-primary text-xs disabled:opacity-40"
                title="Translate the interface into this language"
              >
                <Plus className="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
          <div className="text-[10px] text-muted-foreground px-1.5 pt-1.5 leading-snug border-t border-border/50 mt-1">
            The listed languages ship with the app — instant and offline. A language you add is translated
            by your AI provider once and then cached. Your own content (lyrics, titles, text you typed) is
            left untouched.
          </div>
        </div>
      )}
    </div>
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
    const handler = (e) => {
      if (ref.current && !ref.current.contains(e.target)) setOpen(false);
    };
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
              return (
                <div
                  key={`sep-${idx}`}
                  className="h-px bg-border/60 mx-2 my-1"
                />
              );
            }
            const Icon = entry.icon;
            const isActive = entry.to === currentPath;
            return (
              <button
                key={entry.to}
                onClick={() => {
                  navigate(entry.to);
                  setOpen(false);
                }}
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
  // Re-apply a previously chosen interface language (cached strings paint with no AI call).
  useEffect(() => { initUiLanguage(); }, []);
  // When a Kaggle run is denied a GPU and no other account can be rotated to, the server pipeline
  // fires this event; Shell is always mounted, so it's the reliable place to pop the guided step
  // that connects another free account (GPU quota is per-account).
  const { dialog: kaggleAccountGuide, openStep: openKaggleAccountGuide } = useRequireGuideStep("kaggle");
  useEffect(() => {
    const onNeedsAccount = () => openKaggleAccountGuide();
    window.addEventListener("bm:kaggle-needs-account", onNeedsAccount);
    return () => window.removeEventListener("bm:kaggle-needs-account", onNeedsAccount);
  }, [openKaggleAccountGuide]);
  const project = projects.find((p) => p.id === activeProjectId);
  // Non-christian projects work from freeform material only — Bible Sources disappears for them
  // (and christian-specific musical layers hide elsewhere off the same flag).
  const visibleNav = NAV.filter((n) => n.to !== "/bible" || project?.is_christian !== false);
  const running = jobs.filter(
    (j) => j.status === "running" || j.status === "queued",
  ).length;
  const loc = useLocation();
  const navigate = useNavigate();
  const [collapsed, setCollapsed] = useState(false);
  const [mobileOpen, setMobileOpen] = useState(false);
  const [historyStack, setHistoryStack] = useState([]);
  const [historyIdx, setHistoryIdx] = useState(-1);
  const historyNavPathRef = useRef(null);
  // The version actually running, straight from Tauri — corrects the appPkg fallback (see
  // import above) as soon as it resolves.
  const [appVersion, setAppVersion] = useState(appPkg.version);
  useEffect(() => { getVersion().then(setAppVersion).catch(() => {}); }, []);

  // AI generation pipeline (genPipeline.js) runs as a module-level singleton so it survives
  // route changes, but Suno click-automation needs the embedded Browser tab actually mounted
  // (native webviews paint over its placeholder stage). This bounces the app to wherever the
  // running pipeline currently needs to be visible: the Browser tab while it's loading/filling/
  // signing in, back to Music Studio once a clip is downloaded so the preview is right there.
  // Fires only on needsView *transitions* (idle → browser → preview → browser → …), not on
  // every pipeline state tick or unrelated route change — so a user who manually navigates
  // elsewhere mid-run isn't fought back to the Browser tab until the pipeline actually needs it
  // to change again.
  // ── Autosave ───────────────────────────────────────────────────────────────
  // Values are staged as they lose focus (see lib/autosave.js); the commit happens when the view
  // changes, because that is the moment a person believes their work is safe. The view committed is the
  // one being LEFT, not the one arriving — a commit labelled with the destination would be a lie about
  // where the edits came from.
  useEffect(() => { installAutosave(); }, []);
  // Entitlement refreshed hourly and on refocus; the paywall reads the same store.
  useEffect(() => { installEntitlement(); }, []);
  useEffect(() => { setAutosaveProject(activeProjectId); }, [activeProjectId]);
  const leavingViewRef = useRef(null);
  useEffect(() => {
    const label = (NAV.find((n) => n.to === loc.pathname)?.label) || loc.pathname.replace(/^\//, "") || "dashboard";
    const previous = leavingViewRef.current;
    leavingViewRef.current = label;
    setAutosaveView(label);
    if (previous && previous !== label) {
      commitPending("autosave", previous).then((r) => {
        if (r?.committed) toast.success(`Saved ${previous} — ${r.commit}`, { duration: 2000 });
      });
    }
  }, [loc.pathname]);

  const lastNeedsViewRef = useRef(null);
  useEffect(() => subscribePipeline((s) => {
    if (s.needsView === lastNeedsViewRef.current) return;
    lastNeedsViewRef.current = s.needsView;
    if (s.needsView === "browser") navigate("/browser?site=suno&pipeline=1");
    else if (s.needsView === "preview") navigate("/music");
  }), [navigate]);
  // Whatever the active page published via usePageActions() — its final Generate/Render/
  // Publish button and other page-wide controls, rendered in the top bar below. Cleared by
  // usePageActions' own unmount cleanup when you navigate away, so no route-based reset here
  // is needed (a parent-owned effect on `loc.pathname` would fire after the new page's own
  // effect and wipe out whatever it just published).
  const [pageActions, setPageActions] = useState(null);
  // Registered actions: view-wide ones stay while the view is mounted, section ones come and go with
  // the viewport. Order is by priority then id, never by when they registered — see pageActions.jsx.
  const { registry: actionRegistry, ordered: barActions } = useActionRegistry();
  const [overflowOpen, setOverflowOpen] = useState(false);

  // Track navigation history for back/forward
  useEffect(() => {
    if (historyNavPathRef.current === loc.pathname) {
      historyNavPathRef.current = null;
      setMobileOpen(false);
      return;
    }
    setHistoryStack((prev) => {
      const last = prev[prev.length - 1];
      if (last === loc.pathname) return prev;
      const base = historyIdx >= 0 ? prev.slice(0, historyIdx + 1) : prev;
      const next = [...base, loc.pathname];
      setHistoryIdx(next.length - 1);
      return next;
    });
    setMobileOpen(false);
  }, [loc.pathname, historyIdx]);

  // Update document title with current page name
  useEffect(() => {
    const current = NAV.find((n) => n.to === loc.pathname);
    const title = current
      ? `${current.label} — Lightkid AI Studio`
      : "Lightkid AI Studio";
    document.title = title;
  }, [loc.pathname]);

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e) => {
      // Don't navigate if an input/textarea is focused
      const tag = document.activeElement?.tagName;
      if (
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        document.activeElement?.contentEditable === "true"
      )
        return;

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
      historyNavPathRef.current = historyStack[historyIdx - 1];
      setHistoryIdx((prev) => prev - 1);
      navigate(historyStack[historyIdx - 1]);
    }
  };

  const goForward = () => {
    if (historyIdx < historyStack.length - 1) {
      historyNavPathRef.current = historyStack[historyIdx + 1];
      setHistoryIdx((prev) => prev + 1);
      navigate(historyStack[historyIdx + 1]);
    }
  };

  // ── Save dropdown state ──
  const [saveOpen, setSaveOpen] = useState(false);
  const saveRef = useRef(null);
  const [saving, setSaving] = useState(false);
  const [saveFeedback, setSaveFeedback] = useState(null);

  useEffect(() => {
    if (!saveOpen) return;
    const handler = (e) => { if (saveRef.current && !saveRef.current.contains(e.target)) setSaveOpen(false); };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [saveOpen]);

  // Branch creation dialog state
  const [branchDialogOpen, setBranchDialogOpen] = useState(false);
  const [branchDialogPendingSaveType, setBranchDialogPendingSaveType] = useState(null);
  const [branchName, setBranchName] = useState("");

  const doSave = useCallback(async (saveType, opts = {}) => {
    if (!activeProjectId) { toast.error("No active project selected."); return; }
    setSaving(true);
    setSaveOpen(false);
    try {
      // Commit whatever is staged first, so the tagged version actually contains the last edit rather
      // than everything up to it.
      await commitPending("manual", leavingViewRef.current || "app");
      const res = await api.saveProjectVersion(activeProjectId, saveType, opts.branchToCreate || null);

      if (res.status === "needs_path") {
        // First save: ask for a folder
        const folder = await open({ directory: true, multiple: false, title: "Choose a folder to save this project" });
        if (!folder) { setSaving(false); return; }
        await api.updateProject(activeProjectId, { local_path: folder });
        // Retry now that path is set
        const res2 = await api.saveProjectVersion(activeProjectId, saveType, opts.branchToCreate || null);
        if (res2.status === "needs_branch_creation") {
          setBranchDialogPendingSaveType(saveType);
          setBranchDialogOpen(true);
          setSaving(false);
          return;
        }
        if (res2.status === "success") {
          setSaveFeedback("saved");
          toast.success(`Saved & tagged ${res2.tag}`);
          setTimeout(() => setSaveFeedback(null), 2000);
        }
        setSaving(false);
        return;
      }

      if (res.status === "needs_branch_creation") {
        setBranchDialogPendingSaveType(saveType);
        setBranchDialogOpen(true);
        setSaving(false);
        return;
      }

      if (res.status === "success") {
        setSaveFeedback("saved");
        const dest = saveType === "local" ? "Local disk" : saveType === "gdrive_exclude" ? "Google Drive (JSON only)" : "Google Drive (full)";
        toast.success(`Saved to ${dest} — tag ${res.tag}`);
        setTimeout(() => setSaveFeedback(null), 2000);
      }
    } catch (err) {
      console.error("Save failed", err);
      toast.error(typeof err === "string" ? err : (err?.message || "Save failed."));
    } finally {
      setSaving(false);
    }
  }, [activeProjectId]);

  /// Commit everything pending and push to the project's git remote.
  ///
  /// Separate from Save because pushing is a different promise: it puts the work somewhere the user does
  /// not control. So it is an explicit choice, and it says plainly when no remote is configured instead
  /// of looking like it worked.
  const doSaveAndPush = useCallback(async () => {
    if (!activeProjectId) { toast.error("No active project selected."); return; }
    setSaving(true);
    setSaveOpen(false);
    const t = toast.loading("Committing and pushing…");
    try {
      await commitPending("manual", leavingViewRef.current || "app");
      const res = await api.saveAndPush(activeProjectId, leavingViewRef.current || "app");
      if (res?.pushed) {
        toast.success(`Pushed${res.committed ? ` — commit ${res.committed}` : ""}.`, { id: t });
        setSaveFeedback("saved");
        setTimeout(() => setSaveFeedback(null), 2000);
      } else {
        toast.warning(res?.reason || "Nothing was pushed.", { id: t, duration: 9000 });
      }
    } catch (err) {
      toast.error(typeof err === "string" ? err : (err?.message || "Push failed."), { id: t });
    } finally { setSaving(false); }
  }, [activeProjectId]);

  const handleBranchConfirm = useCallback(async () => {
    if (!branchName.trim()) { toast.error("Branch name is required."); return; }
    setBranchDialogOpen(false);
    await doSave(branchDialogPendingSaveType, { branchToCreate: branchName.trim() });
    setBranchName("");
  }, [branchName, branchDialogPendingSaveType, doSave]);

  // Export all settings, configs, templates, etc.
  const handleExport = useCallback(() => {
    try {
      const exportData = {
        exportedAt: new Date().toISOString(),
        version: appVersion,
        settings: (() => {
          try {
            return JSON.parse(localStorage.getItem("studio:settings") || "{}");
          } catch {
            return {};
          }
        })(),
        composerConfig: (() => {
          try {
            return JSON.parse(
              localStorage.getItem("studio:composer-config") || "{}",
            );
          } catch {
            return {};
          }
        })(),
        composerProfiles: (() => {
          try {
            return JSON.parse(
              localStorage.getItem("studio:composer-profiles") || "{}",
            );
          } catch {
            return {};
          }
        })(),
        customThemePresets: (() => {
          try {
            return JSON.parse(
              localStorage.getItem("studio:custom-theme-presets") || "{}",
            );
          } catch {
            return {};
          }
        })(),
        chapterDrafts: (() => {
          try {
            return JSON.parse(
              localStorage.getItem("studio:pasted-chapters") || "{}",
            );
          } catch {
            return {};
          }
        })(),
        channelDiscoveryTags: (() => {
          try {
            return JSON.parse(
              localStorage.getItem("studio:channel-discovery-tags") || "[]",
            );
          } catch {
            return [];
          }
        })(),
        templates: (() => {
          try {
            return JSON.parse(localStorage.getItem("studio:templates") || "[]");
          } catch {
            return [];
          }
        })(),
      };
      const blob = new Blob([JSON.stringify(exportData, null, 2)], {
        type: "application/json",
      });
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
  }, [appVersion]);

  useEffect(() => {
    window.addEventListener("studio:export", handleExport);
    return () => window.removeEventListener("studio:export", handleExport);
  }, [handleExport]);

  const currentNav = NAV.find((n) => n.to === loc.pathname);
  const groupLabel = currentNav ? GROUP_LABELS[currentNav.group] : "";
  const mobileQuickNav = NAV.filter((n) =>
    ["/", "/composer", "/sections", "/jobs", "/settings"].includes(n.to),
  );

  return (
    // h-screen + overflow-hidden pins the chrome (sidebar, header) to the viewport and makes
    // the content area below a fixed, non-scrolling box that pages scroll *inside*. The
    // Browser page's native child webview is positioned over a placeholder in that box, and a
    // window-level scroll would slide the placeholder out from under it.
    <div className="h-screen overflow-hidden flex bg-background text-foreground">
      {kaggleAccountGuide}
      <aside
        className={`${collapsed ? "w-16" : "w-64"} hidden md:flex shrink-0 border-r border-sidebar-border bg-sidebar text-sidebar-foreground flex-col h-full transition-all`}
      >
        {/* h-16 matches the main header exactly, so the top bar reads as one continuous strip.
            The header inverts to the sidebar's complementary color (see --sidebar-bar tokens). */}
        <div className={`h-16 shrink-0 px-3 border-b border-sidebar-border flex items-center gap-2 bg-sidebar-bar text-sidebar-bar-foreground ${collapsed ? "justify-center" : ""}`}>
          <div className="w-9 h-9 flex items-center justify-center shrink-0">
            <img
              src="/icon.png"
              alt="Lightkid AI"
              className="w-9 h-9 object-contain"
            />
          </div>
          {!collapsed && (
            <div>
              <div className="text-mono text-[11px] uppercase tracking-[0.2em] opacity-70">
                Studio
              </div>
              <div className="font-semibold text-base leading-tight flex items-baseline gap-2">
                Lightkid AI{" "}
                <span className="text-xs opacity-70">
                  v{appVersion}
                </span>
              </div>
            </div>
          )}
          {/* Keyboard-shortcut hint behind an info icon — hidden when collapsed so the logo icon
              keeps the narrow rail to itself and stays reasonably big. */}
          {!collapsed && <ShortcutHint collapsed={collapsed} />}
        </div>

        <nav className="flex-1 overflow-y-auto py-3 scroll-thin">
          {visibleNav.map(({ to, label, icon: Icon, testid, group }) => {
            const prevGroup =
              visibleNav[visibleNav.indexOf(visibleNav.find((n) => n.to === to)) - 1]?.group;
            const showGroupHeader = group !== prevGroup;
            return (
              <div key={to}>
                {showGroupHeader && !collapsed && (
                  <div className="px-5 pt-4 pb-1.5 text-[9px] uppercase tracking-[0.2em] text-sidebar-foreground/45 font-semibold">
                    {GROUP_LABELS[group]}
                  </div>
                )}
                <NavLink
                  to={to}
                  data-testid={testid}
                  className={({ isActive }) =>
                    `flex items-center gap-3 px-5 py-2.5 text-sm transition-colors border-l-2 ${
                      isActive
                        ? "border-sidebar-accent bg-sidebar-accent/15 text-sidebar-foreground font-medium"
                        : "border-transparent text-sidebar-foreground/65 hover:text-sidebar-foreground hover:bg-sidebar-accent/10"
                    }`
                  }
                >
                  <Icon className="w-4 h-4 shrink-0" />
                  {!collapsed && <span>{label}</span>}
                </NavLink>
              </div>
            );
          })}
        </nav>

      </aside>

      {mobileOpen && (
        <div className="fixed inset-0 z-50 md:hidden">
          <button
            className="absolute inset-0 bg-background/70 backdrop-blur-sm"
            aria-label="Close navigation"
            onClick={() => setMobileOpen(false)}
          />
          <div className="absolute inset-y-0 left-0 w-[min(82vw,320px)] bg-sidebar text-sidebar-foreground border-r border-sidebar-border shadow-2xl flex flex-col">
            <div className="h-16 shrink-0 px-4 border-b border-sidebar-border flex items-center justify-between">
              <div className="flex items-center gap-2 min-w-0">
                <img
                  src="/icon.png"
                  alt="Lightkid AI"
                  className="w-9 h-9 object-contain"
                />
                <div className="min-w-0">
                  <div className="text-mono text-[10px] uppercase text-muted-foreground">
                    Studio
                  </div>
                  <div className="font-semibold truncate">Lightkid AI</div>
                </div>
              </div>
              <button
                className="p-2 rounded-md hover:bg-muted"
                onClick={() => setMobileOpen(false)}
                aria-label="Close navigation"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            <nav className="flex-1 overflow-y-auto py-3 scroll-thin">
              {visibleNav.map(({ to, label, icon: Icon, testid, group }, idx) => {
                const prevGroup = visibleNav[idx - 1]?.group;
                const showGroupHeader = group !== prevGroup;
                return (
                  <div key={to}>
                    {showGroupHeader && (
                      <div className="px-5 pt-4 pb-1.5 text-[9px] uppercase tracking-[0.2em] text-sidebar-foreground/45 font-semibold">
                        {GROUP_LABELS[group]}
                      </div>
                    )}
                    <NavLink
                      to={to}
                      data-testid={`${testid}-mobile`}
                      onClick={() => setMobileOpen(false)}
                      className={({ isActive }) =>
                        `flex items-center gap-3 px-5 py-3 text-sm border-l-2 ${
                          isActive
                            ? "border-sidebar-accent bg-sidebar-accent/15 text-sidebar-foreground font-medium"
                            : "border-transparent text-sidebar-foreground/65 hover:text-sidebar-foreground hover:bg-sidebar-accent/10"
                        }`
                      }
                    >
                      <Icon className="w-4 h-4 shrink-0" />
                      <span>{label}</span>
                    </NavLink>
                  </div>
                );
              })}
            </nav>
          </div>
        </div>
      )}

      <main className="flex-1 flex flex-col min-w-0 min-h-0 pb-16 md:pb-0">
        {/* ── Top navigation bar (always on top) ── */}
        {/* The top bar carries the COMPLEMENTARY colour of the main region below it (see --main-bar),
            so both top strips (this + the sidebar header) contrast their own body rather than each
            other. */}
        <header className="h-16 z-30 border-b border-border bg-mainbar text-mainbar-foreground shadow-sm flex items-center justify-between px-3 sm:px-6 gap-3 sm:gap-4 shrink-0">
          <div className="flex items-center gap-2 min-w-0">
            <button
              className="p-2 rounded hover:bg-muted/30 shrink-0 md:hidden"
              onClick={() => setMobileOpen(true)}
              aria-label="Open navigation"
            >
              <Menu className="w-4 h-4" />
            </button>
            {/* Sidebar toggle */}
            <button
              className="hidden md:inline-flex p-1.5 rounded hover:bg-muted/30 shrink-0"
              onClick={() => setCollapsed((c) => !c)}
              aria-label="Toggle sidebar"
            >
              {collapsed ? (
                <ChevronRight className="w-4 h-4" />
              ) : (
                <ChevronLeft className="w-4 h-4" />
              )}
            </button>

            {/* Back/Forward nav buttons */}
            <div className="flex items-center gap-0.5 shrink-0">
              <button
                onClick={goBack}
                disabled={historyIdx <= 0}
                className="hidden sm:inline-flex p-1.5 rounded hover:bg-muted/30 disabled:opacity-30 disabled:cursor-not-allowed"
                title="Back (history)"
              >
                <ArrowLeft className="w-4 h-4" />
              </button>
              <button
                onClick={goForward}
                disabled={historyIdx >= historyStack.length - 1}
                className="hidden sm:inline-flex p-1.5 rounded hover:bg-muted/30 disabled:opacity-30 disabled:cursor-not-allowed"
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
            <div className="hidden sm:block w-px h-5 bg-border mx-1" />

            {/* Breadcrumbs */}
            <div className="flex items-center gap-2 min-w-0">
              <span className="hidden sm:inline text-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground/60 shrink-0">
                {groupLabel}
              </span>
              {groupLabel && (
                <span className="hidden sm:inline text-muted-foreground/40">
                  /
                </span>
              )}
              <BreadcrumbDropdown currentPath={loc.pathname} />
              {project && (
                <>
                  <span className="text-muted-foreground/40">/</span>
                  <span
                    data-testid="active-project-name"
                    className="text-sm font-medium truncate max-w-[200px]"
                  >
                    {project.name}
                  </span>
                </>
              )}
            </div>
          </div>

          <div className="flex items-center gap-1.5 sm:gap-2 shrink-0">
            {/* Theme picker (moved here from the sidebar) */}
            <ToursFab />
            <LanguageFab />
              <ThemeFab theme={theme} setTheme={setTheme} />

            {/* Page-wide controls the active page published via usePageActions() — its final
                Generate/Render/Publish button and the like, so it's reachable without scrolling
                the page body. */}
            {pageActions && (
              <div className="flex items-center gap-1.5 sm:gap-2 pr-1.5 sm:pr-2 mr-0.5 border-r border-border/60">
                {pageActions}
              </div>
            )}

            {/* Registered actions. Section-scoped ones are only here while their section is on screen,
                so a Generate button never sits in the bar with its subject scrolled out of sight. */}
            {barActions.length > 0 && (() => {
              const { visible, overflow } = splitActions(barActions);
              return (
                <div className="flex items-center gap-1 pr-1.5 sm:pr-2 mr-0.5 border-r border-border/60">
                  {visible.map((a) => {
                    const Icon = a.icon;
                    return (
                      <button
                        key={a.id}
                        data-testid={`bar-action-${a.id}`}
                        onClick={() => (a._live?.current?.onClick || a.onClick)?.()}
                        disabled={a.disabled}
                        title={a.title || a.label}
                        className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${
                          a.variant === "primary"
                            ? "bg-primary/20 text-primary hover:bg-primary/30"
                            : "bg-muted/40 hover:bg-muted/70 text-foreground"
                        }`}
                      >
                        {Icon ? <Icon className="w-3.5 h-3.5" /> : null}
                        <span className="hidden md:inline">{a.label}</span>
                      </button>
                    );
                  })}
                  {overflow.length > 0 && (
                    <div className="relative">
                      <button
                        onClick={() => setOverflowOpen((o) => !o)}
                        title={`${overflow.length} more action${overflow.length === 1 ? "" : "s"}`}
                        className="px-1.5 py-1.5 rounded-lg bg-muted/40 hover:bg-muted/70 text-muted-foreground"
                      >
                        <ChevronDown className="w-3.5 h-3.5" />
                      </button>
                      {overflowOpen && (
                        <div className="absolute z-[9999] top-full mt-1.5 right-0 bg-popover border border-border/80 rounded-xl shadow-2xl p-1.5 min-w-[200px]">
                          {overflow.map((a) => (
                            <button
                              key={a.id}
                              onClick={() => { setOverflowOpen(false); (a._live?.current?.onClick || a.onClick)?.(); }}
                              disabled={a.disabled}
                              className="w-full text-left px-3 py-2 rounded-lg text-sm hover:bg-accent disabled:opacity-40"
                            >
                              {a.label}
                            </button>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })()}

            {/* Running jobs indicator */}
            {running > 0 && (
              <div className="flex items-center gap-2 text-xs text-mono mr-2">
                <span className="relative flex h-2 w-2">
                  <span className="absolute inline-flex h-full w-full rounded-full bg-primary opacity-75 animate-ping"></span>
                  <span className="relative inline-flex rounded-full h-2 w-2 bg-primary"></span>
                </span>
                <span className="hidden sm:inline text-muted-foreground">
                  {running} job{running > 1 ? "s" : ""}
                </span>
              </div>
            )}

            {/* Action buttons */}
            {/* ── Save Dropdown ── */}
            <div className="relative" ref={saveRef}>
              <div className="flex items-center rounded-lg overflow-hidden border border-border/60">
                {/* Default save action (local) */}
                <button
                  onClick={() => doSave("local")}
                  disabled={saving}
                  className={`flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium transition-all duration-200 ${
                    saveFeedback === "saved"
                      ? "bg-emerald-500/20 text-emerald-400"
                      : "bg-muted/40 hover:bg-muted/70 text-foreground"
                  } disabled:opacity-50 disabled:cursor-not-allowed`}
                  title="Save to local file system"
                >
                  {saveFeedback === "saved" ? (
                    <Check className="w-3.5 h-3.5" />
                  ) : saving ? (
                    <span className="w-3.5 h-3.5 border-2 border-current border-t-transparent rounded-full animate-spin" />
                  ) : (
                    <Save className="w-3.5 h-3.5" />
                  )}
                  <span className="hidden sm:inline">{saving ? "Saving…" : "Save"}</span>
                </button>
                {/* Dropdown chevron */}
                <button
                  onClick={() => setSaveOpen((o) => !o)}
                  disabled={saving}
                  className="px-1.5 py-1.5 bg-muted/40 hover:bg-muted/70 border-l border-border/60 text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
                  title="More save options"
                >
                  <ChevronDown className="w-3 h-3" />
                </button>
              </div>
              {saveOpen && (
                <div className="absolute z-[9999] top-full mt-1.5 right-0 bg-popover backdrop-blur-xl border border-border/80 rounded-xl shadow-2xl p-1.5 min-w-[230px] fade-in">
                  <div className="px-2 py-1 mb-1">
                    <div className="text-[10px] uppercase tracking-widest text-muted-foreground font-mono">
                      {activeProjectId ? "Save Project Version" : "No active project"}
                    </div>
                  </div>
                  <button
                    onClick={() => doSave("local")}
                    className="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm hover:bg-accent transition-colors text-left"
                  >
                    <HardDrive className="w-4 h-4 text-primary shrink-0" />
                    <div>
                      <div className="font-medium">Save to Local Disk</div>
                      <div className="text-[11px] text-muted-foreground">Full project as .tar archive</div>
                    </div>
                  </button>
                  <div className="h-px bg-border/50 mx-2 my-1" />
                  <button
                    onClick={() => doSave("gdrive_exclude")}
                    className="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm hover:bg-accent transition-colors text-left"
                  >
                    <CloudUpload className="w-4 h-4 text-blue-400 shrink-0" />
                    <div>
                      <div className="font-medium">Save to Google Drive</div>
                      <div className="text-[11px] text-muted-foreground">JSON + git only (no media)</div>
                    </div>
                  </button>
                  <button
                    onClick={() => doSave("gdrive_include")}
                    className="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm hover:bg-accent transition-colors text-left"
                  >
                    <CloudUpload className="w-4 h-4 text-violet-400 shrink-0" />
                    <div>
                      <div className="font-medium">Save to Google Drive (Full)</div>
                      <div className="text-[11px] text-muted-foreground">Everything incl. media files</div>
                    </div>
                  </button>
                  <div className="h-px bg-border/50 mx-2 my-1" />
                  <button
                    onClick={doSaveAndPush}
                    className="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm hover:bg-accent transition-colors text-left"
                  >
                    <GitBranch className="w-4 h-4 text-emerald-400 shrink-0" />
                    <div>
                      <div className="font-medium">Save &amp; Push</div>
                      <div className="text-[11px] text-muted-foreground">
                        Commit, then push to this project's git remote
                      </div>
                    </div>
                  </button>
                  <div className="h-px bg-border/50 mx-2 my-1" />
                  <button
                    onClick={() => { setSaveOpen(false); window.dispatchEvent(new CustomEvent("studio:export")); }}
                    className="w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm hover:bg-accent transition-colors text-left text-muted-foreground"
                  >
                    <Download className="w-4 h-4 shrink-0" />
                    <div className="font-medium">Export Settings JSON</div>
                  </button>
                </div>
              )}
            </div>

            {/* Branch-creation modal */}
            {branchDialogOpen && (
              <div className="fixed inset-0 z-[10000] flex items-center justify-center bg-background/60 backdrop-blur-sm">
                <div className="bg-card border border-border rounded-2xl shadow-2xl p-6 w-full max-w-sm mx-4 fade-in">
                  <div className="flex items-center gap-2 mb-3">
                    <GitBranch className="w-5 h-5 text-primary" />
                    <h2 className="font-semibold text-base">Create a Branch to Save</h2>
                  </div>
                  <p className="text-sm text-muted-foreground mb-4">
                    You are in a detached HEAD state. To save, create a new branch from this point.
                  </p>
                  <input
                    autoFocus
                    className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm mb-4 outline-none focus:ring-2 focus:ring-primary"
                    placeholder="my-new-branch"
                    value={branchName}
                    onChange={(e) => setBranchName(e.target.value)}
                    onKeyDown={(e) => { if (e.key === "Enter") handleBranchConfirm(); if (e.key === "Escape") setBranchDialogOpen(false); }}
                  />
                  <div className="flex gap-2 justify-end">
                    <button
                      onClick={() => setBranchDialogOpen(false)}
                      className="px-4 py-2 rounded-lg text-sm text-muted-foreground hover:bg-muted transition-colors"
                    >Cancel</button>
                    <button
                      onClick={handleBranchConfirm}
                      className="px-4 py-2 rounded-lg text-sm bg-primary text-primary-foreground hover:bg-primary/90 transition-colors font-medium"
                    >Create & Save</button>
                  </div>
                </div>
              </div>
            )}
          </div>
        </header>

        {/* The view box: exactly the window minus sidebar and header. Pages scroll in here;
            a page can also fill it exactly with `h-full` (see Browser). Wrapped in the page-
            actions provider so any page below can publish controls into the header above via
            usePageActions() — e.g. a page-wide "Generate" button next to the running-jobs
            indicator instead of buried in the page body. */}
        {/* Engine health: only speaks up when the engines this setup actually uses are failing. */}
        <HealthBanner />

        <PageActionsContext.Provider value={setPageActions}>
        <ActionRegistryContext.Provider value={actionRegistry}>
          {/* pb-16 on mobile clears the fixed bottom nav so the last of a page's content isn't
              hidden behind it; safe-area inset keeps it above a phone's home indicator. */}
          <div className="flex-1 min-h-0 overflow-auto scroll-thin pb-16 md:pb-0 [padding-bottom:calc(4rem+env(safe-area-inset-bottom))] md:[padding-bottom:0]">{children}</div>
        </ActionRegistryContext.Provider>
        </PageActionsContext.Provider>
      </main>

      <nav className="fixed bottom-0 left-0 right-0 z-40 md:hidden border-t border-border bg-card/95 backdrop-blur supports-[backdrop-filter]:bg-card/85 [padding-bottom:env(safe-area-inset-bottom)]">
        <div className="grid grid-cols-5">
          {mobileQuickNav.map(({ to, label, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              className={({ isActive }) =>
                `flex min-h-14 flex-col items-center justify-center gap-1 text-[10px] ${
                  isActive ? "text-primary" : "text-muted-foreground"
                }`
              }
            >
              <Icon className="w-4 h-4" />
              <span className="max-w-full truncate px-1">
                {label.replace(" Manager", "").replace("AI ", "")}
              </span>
            </NavLink>
          ))}
        </div>
      </nav>
    </div>
  );
}
