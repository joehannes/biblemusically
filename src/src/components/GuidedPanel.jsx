import { useEffect, useState } from "react";
import { api } from "../lib/api";
import GuidedFlow from "./GuidedFlow";
import { Wand2 } from "lucide-react";

// A page mounts the guided layer with this: it owns the on/off preference, loads the shared context
// every flow needs (project brief, channels, selected engines) and gets out of the way when the user
// turns it off. Pages only pass the flow and their own action handlers.
//
//   <GuidedPanel flow={musicFlow} actions={{ setEngine, run: triggerAll }} />
//
// Kept separate from GuidedFlow so that component stays purely presentational and testable, and so a
// page never has to repeat the context-loading effect.
export default function GuidedPanel({ flow, actions = {}, extraCtx = {}, projectId, onRevealSection, onFinish }) {
  const key = `studio:guided-${flow.id}`;
  const [on, setOn] = useState(() => localStorage.getItem(key) !== "off");
  const [project, setProject] = useState(null);
  const [channels, setChannels] = useState([]);
  const [settings, setSettings] = useState({});

  useEffect(() => {
    (async () => {
      try {
        const [projects, chans, s] = await Promise.all([
          api.listProjects().catch(() => []),
          api.listChannels().catch(() => []),
          api.getSettings(projectId).catch(() => ({})),
        ]);
        setProject((projects || []).find((p) => p.id === projectId) || (projects || [])[0] || null);
        setChannels(Array.isArray(chans) ? chans : chans?.channels || []);
        setSettings(s || {});
      } catch { /* the flow falls back to static defaults */ }
    })();
  }, [projectId]);

  const setMode = (next) => {
    setOn(next);
    localStorage.setItem(key, next ? "on" : "off");
  };

  if (!on) {
    return (
      <button
        onClick={() => setMode(true)}
        className="text-[11px] text-muted-foreground hover:text-primary inline-flex items-center gap-1.5"
      >
        <Wand2 className="w-3.5 h-3.5" />Guide me through this
      </button>
    );
  }

  return (
    <GuidedFlow
      flow={flow}
      ctx={{ projectId, project, channels, settings, ...extraCtx, ...actions }}
      onRevealSection={onRevealSection}
      onShowAll={() => setMode(false)}
      onFinish={onFinish}
      compact
    />
  );
}
