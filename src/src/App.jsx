import "@/App.css";
import { useEffect, useState } from "react";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { StudioProvider } from "./lib/store";
import { api } from "./lib/api";
import { initServerLifecycle } from "./lib/serverLifecycle";
import Shell from "./components/Shell";
import Onboarding from "./pages/Onboarding";
import Dashboard from "./pages/Dashboard";
import Workflow from "./pages/Workflow";
import Lyrics from "./pages/Lyrics";
import MusicGen from "./pages/MusicGen";
import Analysis from "./pages/Analysis";
import SectionEditor from "./pages/SectionEditor";
import Images from "./pages/Images";
import Composer from "./pages/Composer";
import BibleSources from "./pages/BibleSources";
import AIComposer from "./pages/AIComposer";
import FreeformComposer from "./pages/FreeformComposer";
import StyleStudio from "./pages/StyleStudio";
import SoundStudio from "./pages/SoundStudio";
import Transitions from "./pages/Transitions";
import OverlayStudio from "./pages/OverlayStudio";
import Channels from "./pages/Channels";
import Characters from "./pages/Characters";
import Upload from "./pages/Upload";
import Jobs from "./pages/Jobs";
import Browser from "./pages/Browser";
import MacroManager from "./pages/MacroManager";
import Settings from "./pages/Settings";
import { Toaster } from "sonner";

// First-run gate: show the guided onboarding wizard until `onboarded` is set in settings, then the
// normal app. On any error reading settings we fail OPEN (show the app) so a settings hiccup can
// never trap the user on the welcome screen.
function RootGate() {
  const [ready, setReady] = useState(false);
  const [onboarded, setOnboarded] = useState(true);
  // Arm the GPU-quota watchdog: idles down any server this session started after 15 min of
  // inactivity, and asks them to stop on shutdown. Servers are never started here — only on demand.
  useEffect(() => { initServerLifecycle(); }, []);
  useEffect(() => {
    (async () => {
      try {
        const s = await api.getSettings();
        setOnboarded(s?.onboarded === true);
      } catch {
        setOnboarded(true);
      } finally {
        setReady(true);
      }
    })();
  }, []);

  if (!ready) return null;
  if (!onboarded) return <Onboarding onDone={() => setOnboarded(true)} />;

  return (
    <Shell>
      <Routes>
        <Route path="/" element={<Dashboard />} />
            <Route path="/workflow" element={<Workflow />} />
            <Route path="/bible" element={<BibleSources />} />
            <Route path="/composer" element={<AIComposer />} />
            <Route path="/freeform" element={<FreeformComposer />} />
            <Route path="/lyrics" element={<Lyrics />} />
            <Route path="/music" element={<MusicGen />} />
            <Route path="/sound" element={<SoundStudio />} />
            <Route path="/analysis" element={<Analysis />} />
            <Route path="/sections" element={<SectionEditor />} />
            <Route path="/images" element={<Images />} />
            <Route path="/styles" element={<StyleStudio />} />
            <Route path="/transitions" element={<Transitions />} />
            <Route path="/overlays" element={<OverlayStudio />} />
            <Route path="/video" element={<Composer />} />
            <Route path="/channels" element={<Channels />} />
            <Route path="/characters" element={<Characters />} />
            <Route path="/upload" element={<Upload />} />
            <Route path="/jobs" element={<Jobs />} />
            <Route path="/browser" element={<Browser />} />
            <Route path="/macros" element={<MacroManager />} />
        <Route path="/settings" element={<Settings />} />
      </Routes>
    </Shell>
  );
}

export default function App() {
  return (
    <StudioProvider>
      <BrowserRouter>
        <RootGate />
        <Toaster position="bottom-right" theme="dark" richColors />
      </BrowserRouter>
    </StudioProvider>
  );
}
