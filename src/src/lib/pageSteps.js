// Centralized page ordering derived from the main app sidebar (Shell).
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
