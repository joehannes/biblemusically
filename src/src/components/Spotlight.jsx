import { useEffect, useRef, useState } from "react";
import { locateTarget, rectOf, revealTarget, veilPanels, SPOTLIGHT_PAD, SETTLE_MS } from "../lib/spotlight";

/**
 * Dim the page and ring the one control a guide step is talking about.
 *
 * The welcome tour could already do this for sidebar links; nothing could do it for the actual
 * controls, which is where guidance is worth having. A step that says "set the song length" could
 * only say it — so the reader had to find the field themselves, on a Settings page long enough that
 * finding it *is* the difficulty.
 *
 * Deliberately not modal. The tour's veil swallows every click because the app is a diagram for the
 * length of the tour; here the point is the opposite — the spotlight is pointing at something you
 * are meant to reach out and use, so the veil never captures pointer events and the ring is drawn
 * rather than filled.
 *
 * Renders nothing at all when the target cannot be found. A guide that dims the window and
 * highlights nothing is worse than one that stays quiet: the reader is left looking for a hole that
 * is not there, and the page is harder to read while they do it.
 */
export default function Spotlight({ target, active = true, pad = SPOTLIGHT_PAD }) {
  const [rect, setRect] = useState(null);
  const raf = useRef(0);

  useEffect(() => {
    if (!active || !target) { setRect(null); return; }
    let alive = true;

    const measure = () => {
      const el = locateTarget(target);
      if (!alive) return;
      setRect(el ? rectOf(el) : null);
    };

    // One settle before the first measurement: a step usually becomes active in the same tick that
    // reveals its section, and measuring a node that is still being laid out returns a rectangle
    // that is wrong by the time it is painted.
    const t = setTimeout(async () => {
      const el = locateTarget(target);
      if (!el || !alive) { measure(); return; }
      await revealTarget(el);
      measure();
    }, SETTLE_MS);

    // Keep it on target while the page moves under it. rAF-coalesced because scroll fires far more
    // often than the layout actually changes, and re-measuring per event makes the ring judder.
    const onMove = () => {
      cancelAnimationFrame(raf.current);
      raf.current = requestAnimationFrame(measure);
    };
    window.addEventListener("scroll", onMove, true);
    window.addEventListener("resize", onMove);
    return () => {
      alive = false;
      clearTimeout(t);
      cancelAnimationFrame(raf.current);
      window.removeEventListener("scroll", onMove, true);
      window.removeEventListener("resize", onMove);
    };
  }, [target, active]);

  if (!active || !rect) return null;

  return (
    <>
      {veilPanels(rect, pad).map((p) => (
        <div
          key={p.key}
          className="fixed bg-background/45 transition-all duration-300 pointer-events-none z-[60]"
          style={p.style}
          aria-hidden="true"
        />
      ))}
      <div
        className="fixed rounded-lg ring-2 ring-primary/80 shadow-[0_0_0_5px_hsl(var(--primary)/0.18)] pointer-events-none transition-all duration-300 z-[61]"
        style={{ top: rect.top - pad, left: rect.left - pad, width: rect.width + pad * 2, height: rect.height + pad * 2 }}
        aria-hidden="true"
      />
    </>
  );
}
