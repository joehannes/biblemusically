import defaults from "./templates";

const KEY = "studio:presets";

export function loadPresets() {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return defaults;
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return defaults;
    return parsed;
  } catch (e) { return defaults; }
}

export function savePresets(presets) {
  try { localStorage.setItem(KEY, JSON.stringify(presets)); return true; } catch(e){ return false; }
}

export default { loadPresets, savePresets };
