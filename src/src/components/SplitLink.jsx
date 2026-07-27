import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { planFor, splitFor, watchFrame, refusalMessage, linkPreference } from "../lib/splitScreen";
import { openLoginUrl } from "../lib/openLogin";
import { X, ExternalLink } from "lucide-react";
import { toast } from "sonner";

// The panel that holds a linked page beside the steps that sent you to it.
//
// Mounted once, at the app root, and driven by an event rather than by props — a link is rendered
// deep inside a Markdown block and should not have to thread a callback up through six components to
// open something.
//
// The whole design turns on one fact: the pages this app links to for keys and logins refuse to be
// framed. So the plan is decided *before* anything renders (lib/splitScreen.js), and when a frame
// does load nothing but grey, the panel closes itself and says why in a sentence. A blank half-screen
// is worse than a browser jump, and silence about it is worse still.

export function openBesideSteps(url, label) {
  window.dispatchEvent(new CustomEvent("bm:open-beside", { detail: { url, label } }));
}

export default function SplitLink() {
  const [open, setOpen] = useState(null);   // { url, label }
  const [orientation, setOrientation] = useState("vertical");
  const frameRef = useRef(null);

  useEffect(() => {
    const onOpen = (event) => {
      const { url, label } = event.detail || {};
      if (!url) return;
      const mobile = /android|iphone|ipad/i.test(navigator.userAgent) || window.innerWidth < 720;
      const plan = planFor(url, { mobile, preference: linkPreference() });
      if (plan === "split") {
        setOrientation(splitFor());
        setOpen({ url, label });
      } else if (plan === "tab") {
        openLoginUrl(url, { label });
      } else {
        // The system browser, with the reason — because being sent away without one feels like a bug.
        openLoginUrl(url, { label, force: "external" });
      }
    };
    window.addEventListener("bm:open-beside", onOpen);
    return () => window.removeEventListener("bm:open-beside", onOpen);
  }, []);

  // A phone turned sideways mid-task should not have to be reopened.
  useEffect(() => {
    if (!open) return;
    const onResize = () => setOrientation(splitFor());
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [open]);

  useEffect(() => {
    if (!open || !frameRef.current) return;
    return watchFrame(frameRef.current, (loaded) => {
      if (loaded) return;
      // It refused after all. Close, explain, and hand it to the browser rather than leaving grey.
      setOpen(null);
      toast.message(refusalMessage(open.url), { duration: 12000 });
      openLoginUrl(open.url, { label: open.label, force: "external" });
    });
  }, [open]);

  if (!open) return null;
  const vertical = orientation !== "horizontal";

  return createPortal(
    <div className={`fixed z-40 bg-background border-border shadow-2xl flex flex-col ${
      vertical ? "left-0 right-0 bottom-0 h-1/2 border-t" : "top-0 bottom-0 right-0 w-1/2 border-l"}`}>
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border/60 shrink-0">
        <span className="text-xs truncate flex-1" data-no-i18n>{open.label || open.url}</span>
        <button className="p-1 rounded hover:bg-muted" title="Open in my browser instead"
                onClick={() => { openLoginUrl(open.url, { label: open.label, force: "external" }); setOpen(null); }}>
          <ExternalLink className="w-4 h-4" />
        </button>
        <button className="p-1 rounded hover:bg-muted" title="Close" onClick={() => setOpen(null)}>
          <X className="w-4 h-4" />
        </button>
      </div>
      <iframe ref={frameRef} src={open.url} title={open.label || open.url}
              className="flex-1 w-full border-0 bg-white"
              sandbox="allow-forms allow-scripts allow-same-origin allow-popups" />
    </div>,
    document.body,
  );
}
