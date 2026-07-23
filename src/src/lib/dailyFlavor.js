// A small deterministic-per-day flavor modifier for musical styles — the same catalog/algorithm
// as scheduler.rs's daily_flavor(), kept in sync by hand since they run in different processes.
// Keeps the same configured style from sounding identical every single day: picked from
// (today's date, salt) so a given salt gets one consistent flavor per day, while different
// days/salts diverge.

const FLAVOR_MODIFIERS = [
  "with a touch of warmth", "slightly more percussive energy", "a hint of nostalgic texture",
  "brighter, more optimistic tone", "subtly darker undertone", "a touch more cinematic space",
  "gentle acoustic warmth layered in", "a bit more rhythmic drive", "softened, more intimate mix",
  "extra shimmer on the highs", "a touch of raw, live-band energy", "smoother, more polished production",
  "a dash of unexpected texture", "slightly more minimal arrangement", "richer harmonic layering",
];

function hashSeed(str) {
  let h = 0;
  for (let i = 0; i < str.length; i++) h = (Math.imul(h, 31) + str.charCodeAt(i)) >>> 0;
  return h;
}

export function dailyFlavor(salt = "") {
  const day = new Date().toISOString().slice(0, 10); // YYYY-MM-DD
  const seed = hashSeed(`${day}:${salt}`);
  return FLAVOR_MODIFIERS[seed % FLAVOR_MODIFIERS.length];
}

// Appends today's flavor to a style string (no-op-safe on empty input).
export function appendDailyFlavor(styleText, salt = "") {
  const flavor = dailyFlavor(salt);
  const trimmed = (styleText || "").trim();
  return trimmed ? `${trimmed}, ${flavor}` : flavor;
}
