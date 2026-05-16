import "@/App.css";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { StudioProvider } from "./lib/store";
import Shell from "./components/Shell";
import Dashboard from "./pages/Dashboard";
import Lyrics from "./pages/Lyrics";
import MusicGen from "./pages/MusicGen";
import Analysis from "./pages/Analysis";
import SectionEditor from "./pages/SectionEditor";
import Images from "./pages/Images";
import Composer from "./pages/Composer";
import Channels from "./pages/Channels";
import Upload from "./pages/Upload";
import Jobs from "./pages/Jobs";
import Settings from "./pages/Settings";
import { Toaster } from "sonner";

export default function App() {
  return (
    <StudioProvider>
      <BrowserRouter>
        <Shell>
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/lyrics" element={<Lyrics />} />
            <Route path="/music" element={<MusicGen />} />
            <Route path="/analysis" element={<Analysis />} />
            <Route path="/sections" element={<SectionEditor />} />
            <Route path="/images" element={<Images />} />
            <Route path="/composer" element={<Composer />} />
            <Route path="/channels" element={<Channels />} />
            <Route path="/upload" element={<Upload />} />
            <Route path="/jobs" element={<Jobs />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </Shell>
        <Toaster position="bottom-right" theme="dark" richColors />
      </BrowserRouter>
    </StudioProvider>
  );
}
