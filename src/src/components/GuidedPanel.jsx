import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { useGuidedView, GuidedHeader, GuidedBar } from "./GuidedView";

// What a page mounts to become guided, when it has no collapsible sections of its own to manifest.
//
// It loads the shared context every flow needs (project brief, channels, selected engines) and hands
// it to the same controller the AI Composer uses — so these pages get the same template-first start,
// the same persisted progress, and the same resume bar, without each page repeating the wiring.
//
//   <GuidedPanel flow={musicFlow} projectId={id} actions={{ setEngine, run: triggerAll }} />
//
// Pages that DO have sections should call `useGuidedView` directly and ask it about each section
// (see AIComposer) — that is what turns manifestation and the collapse rule on.
export default function GuidedPanel({ flow, actions = {}, extraCtx = {}, projectId, showBottomBar = false }) {
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

  const ctx = useMemo(
    () => ({ projectId, project, channels, settings, ...extraCtx, ...actions }),
    [projectId, project, channels, settings, extraCtx, actions],
  );

  // Without a page-owned section list, the flow's own `reveals` ids are the manifestation order.
  const sectionOrder = useMemo(
    () => [...new Set((flow.steps || []).map((s) => s.reveals || s.id))],
    [flow],
  );

  const guide = useGuidedView({ view: flow.id, projectId, flow, sectionOrder, ctx });

  return (
    <>
      <GuidedHeader guide={guide} flow={flow} ctx={ctx} />
      {showBottomBar && <GuidedBar guide={guide} position="bottom" />}
    </>
  );
}
