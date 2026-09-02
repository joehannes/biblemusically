// The production pipeline, in the order a song moves through it. This is what the "step N" label at
// the top of a page counts.
//
// NOT the sidebar: Shell's NAV has thirty-five entries and a different order, so the eighth item in
// the sidebar can carry the label "step 5". The thirteen here are the stages a song passes through —
// the studios that shape one of those stages (Sound, Style, Transitions, Overlays, Video Gen,
// Characters) deliberately have no number, because they are not another step to get through.
export const NAV_ORDER = [
  "/",
  "/channels",
  "/bible",
  "/composer",
  "/lyrics",
  "/music",
  "/analysis",
  "/sections",
  "/images",
  "/video",
  "/upload",
  "/jobs",
  "/settings",
];

export function getStepForPath(path) {
  const idx = NAV_ORDER.indexOf(path);
  return idx === -1 ? "" : String(idx + 1);
}
