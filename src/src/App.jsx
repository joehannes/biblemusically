import "@/App.css";
import { useEffect, useState } from "react";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { StudioProvider } from "./lib/store";
import { api } from "./lib/api";
import { initServerLifecycle } from "./lib/serverLifecycle";
import Shell from "./components/Shell";
import Onboarding from "./pages/Onboarding";
import { GUIDE_STEPS } from "./lib/guideSteps";
import Dashboard from "./pages/Dashboard";
import Workflow from "./pages/Workflow";
import Lyrics from "./pages/Lyrics";
import MusicGen from "./pages/MusicGen";
import Analysis from "./pages/Analysis";
import SectionEditor from "./pages/SectionEditor";
import Images from "./pages/Images";
import Composer from "./pages/Composer";
import Publicity from "./pages/Publicity";
import Distribution from "./pages/Distribution";
import GraphicNovels from "./pages/GraphicNovels";
import PrintOnDemand from "./pages/PrintOnDemand";
import Clipboard from "./pages/Clipboard";
import AccessGuides from "./pages/AccessGuides";
import Team from "./pages/Team";
import Account from "./pages/Account";
import Feedback from "./pages/Feedback";
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
import Insights from "./pages/Insights";
import Social from "./pages/Social";
import DataSync from "./pages/DataSync";
import Settings from "./pages/Settings";
import { Toaster, toast } from "sonner";
import ErrorBoundary from "./components/ErrorBoundary";
import SplitLink from "./components/SplitLink";
import { installErrorReporting } from "./lib/errorReporting";
import { installAnalytics, installHotjar } from "./lib/analytics";
import { watchDeepLinks } from "./lib/deepLink";

// First-run gate: show the guided onboarding wizard until `onboarded` is set in settings, then the
// normal app. On any error reading settings we fail OPEN (show the app) so a settings hiccup can
// never trap the user on the welcome screen.
function RootGate() {
  const [ready, setReady] = useState(false);
  const [onboarded, setOnboarded] = useState(true);
  // Steps still outstanding on this launch. First run shows the full wizard; afterwards we re-pop a
  // compact "finish setup" wizard holding ONLY the unfinished steps (skipping anything already
  // ticked off), which the user can dismiss for the session. Once every step is done, nothing pops.
  const [pending, setPending] = useState([]);
  const [dismissed, setDismissed] = useState(false);
  // Arm the GPU-quota watchdog: idles down any server this session started after 15 min of
  // inactivity, and asks them to stop on shutdown. Servers are never started here — only on demand.
  useEffect(() => { initServerLifecycle(); }, []);
  useEffect(() => {
    (async () => {
      try {
        const s = (await api.getSettings()) || {};
        setOnboarded(s.onboarded === true);
        // The user can opt out of the startup re-pop; the guide stays reachable from the Tours button.
        setDismissed(s.onboarding_autopop_disabled === true);
        setPending(GUIDE_STEPS.filter((step) => { try { return !step.isDone(s); } catch { return false; } }));
      } catch {
        setOnboarded(true);
      } finally {
        setReady(true);
      }
    })();
  }, []);

  if (!ready) return null;
  // First run: the full welcome + all steps.
  if (!onboarded) return <Onboarding onDone={() => { setOnboarded(true); setPending([]); }} />;
  // Later runs: re-pop only the still-useful steps, dismissible.
  if (pending.length > 0 && !dismissed) {
    return (
      <Onboarding
        resume
        steps={pending}
        onDone={() => setDismissed(true)}
        onSkip={() => setDismissed(true)}
      />
    );
  }

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
            <Route path="/publicity" element={<Publicity />} />
            <Route path="/distribution" element={<Distribution />} />
            <Route path="/novels" element={<GraphicNovels />} />
            <Route path="/print" element={<PrintOnDemand />} />
            <Route path="/clipboard" element={<Clipboard />} />
            <Route path="/access" element={<AccessGuides />} />
            <Route path="/team" element={<Team />} />
            <Route path="/account" element={<Account />} />
            <Route path="/feedback" element={<Feedback />} />
            <Route path="/channels" element={<Channels />} />
            <Route path="/characters" element={<Characters />} />
            <Route path="/upload" element={<Upload />} />
            <Route path="/jobs" element={<Jobs />} />
            <Route path="/browser" element={<Browser />} />
            <Route path="/macros" element={<MacroManager />} />
            <Route path="/insights" element={<Insights />} />
            <Route path="/social" element={<Social />} />
            <Route path="/data" element={<DataSync />} />
        <Route path="/settings" element={<Settings />} />
      </Routes>
    </Shell>
  );
}

export default function App() {
  // Errors are caught and sent on their own; anything with the user's own words in it is only ever
  // sent because they pressed send. See lib/errorReporting.js — and the terms, which say so plainly.
  // Counting views and steps, batched. The backend decides whether it is allowed to send at all
  // (trial: yes and the terms say so; paying: only if they said yes), because a rule the interface
  // owns is a rule the interface can be talked out of.
  useEffect(() => installAnalytics((payload) => api.trackEvents(payload)), []);

  // Hotjar is a third party watching the interface, so it is opt-in, off by default, and removed
  // again the moment it is switched off — script and cookies both.
  useEffect(() => {
    let stop = () => {};
    (async () => {
      try {
        const [settings, pricing] = await Promise.all([api.getSettings(), api.subsPricing()]);
        stop = installHotjar({
          optedIn: settings?.hotjar_opt_in === true,
          siteId: pricing?.hotjar_site_id || "",
        });
      } catch { /* no analytics is the safe failure */ }
    })();
    return () => stop();
  }, []);

  // A sign-in that finished in the system browser lands back here. See lib/deepLink.js for why it
  // is both an event and a poll.
  useEffect(() => watchDeepLinks((result) => {
    if (result.grant) toast.success(result.message, { duration: 8000 });
    else toast.error(result.message, { duration: 12000 });
  }), []);

  useEffect(() => installErrorReporting({
    send: (report) => api.sendReport(report),
    notify: ({ message, fingerprint, count }) => {
      toast.error("Something went wrong — I have been told about it.", {
        description: count > 1 ? `${message} (${count} times this session)` : message,
        duration: 12000,
        action: {
          label: "Add what you were doing",
          onClick: () => {
            const comment = window.prompt(
              "What were you doing when this happened? It goes straight to the author, with nothing else.");
            if (comment?.trim()) {
              api.sendReport({ kind: "error", fingerprint, comment: comment.trim(), message })
                .then(() => toast.success("Sent. Thank you — that is genuinely how things get fixed."))
                .catch(() => toast.error("That could not be sent. It will be in the next report."));
            }
          },
        },
      });
    },
  }), []);

  return (
    <StudioProvider>
      <BrowserRouter>
        {/* One boundary around the whole interface: a white screen tells nobody anything. */}
        <ErrorBoundary where="app">
          <RootGate />
        </ErrorBoundary>
        {/* Holds a linked page beside the steps that sent you to it. Mounted once, driven by an
            event, because a link rendered deep in a Markdown block should not thread a callback up
            through six components. */}
        <SplitLink />
        <Toaster position="bottom-right" theme="dark" richColors />
      </BrowserRouter>
    </StudioProvider>
  );
}
