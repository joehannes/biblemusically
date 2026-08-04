import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Input } from "./ui/input";
import { openExternalUrl } from "../lib/openLogin";
import {
  Server, ExternalLink, Check, Loader2, Cpu, Cloud, Zap, HardDrive, KeyRound,
} from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// Where the engines run.
//
// Most of this is not new capability — every engine has always taken a server URL, so pointing one
// at a rented RunPod box already worked. It was just never said anywhere, which is the difference
// between a feature and a feature somebody can find. So this panel's job is mostly disclosure:
// here is what exists, what it costs, and the steps from "no account" to "the app can use it".
//
// Grouped by shape rather than by price, because shape is what actually differs in use: a free
// notebook has quota and sessions and a URL that rotates; a rented box has none of those and a bill;
// a serverless API has no server at all. Sorting by price would put those three next to each other
// and hide the only distinction that changes how the day goes.
//
// Free is listed first because that is the order somebody evaluates in — but the note on each says
// plainly that free costs the most machinery and serverless the least, which is the opposite of what
// people expect and the single most useful thing on the page.
// ─────────────────────────────────────────────────────────────────────────────

const SHAPE = {
  NotebookTunnel: { label: "Free notebook + tunnel", icon: Cloud,
    blurb: "Free GPU hours on a metered allowance. Sessions end, and the URL changes every run." },
  Local:          { label: "Your own machine", icon: HardDrive,
    blurb: "No quota, no tunnel, nothing leaves the computer. Needs a GPU big enough for the model." },
  RentedBox:      { label: "Rented by the hour", icon: Server,
    blurb: "A fixed address that stays up. Nothing to monitor, because nothing rotates." },
  Serverless:     { label: "Pay per output", icon: Zap,
    blurb: "No server at all. Costs more per video and removes every session failure." },
};
const SHAPE_ORDER = ["NotebookTunnel", "Local", "RentedBox", "Serverless"];

export default function ComputeProviders({ settings, onChange }) {
  const [data, setData] = useState(null);
  const [open, setOpen] = useState(null);
  const [busy, setBusy] = useState("");

  const load = () => api.computeProviders().then(setData)
    .catch((e) => toast.error(`Could not load providers: ${e?.message || e}`));
  useEffect(() => { load(); }, []);

  const testKey = async (id) => {
    setBusy(id);
    try {
      const r = id === "fal" ? await api.testFal() : null;
      if (!r) return toast.message("No connection test for this provider yet.");
      r.ok ? toast.success(r.detail) : toast.error(`${r.detail}${r.next_step ? ` ${r.next_step}` : ""}`,
                                                   { duration: 12000 });
      load();
    } catch (e) { toast.error(String(e?.message || e)); }
    finally { setBusy(""); }
  };

  if (!data) {
    return <div className="flex items-center gap-2 text-xs text-muted-foreground">
      <Loader2 className="w-3.5 h-3.5 animate-spin" />Loading providers…
    </div>;
  }

  return (
    <div className="space-y-5">
      <p className="text-xs text-muted-foreground">
        Every engine takes a server URL, so anything below that gives you one will work — the free
        notebook hosts are simply the only ones the app can start on its own. Counter-intuitively,
        free costs the most machinery (quota, sessions, rotating URLs) and paying per output costs
        the least (none of it).
      </p>

      {SHAPE_ORDER.map((shape) => {
        const list = data.providers.filter((p) => p.shape === shape);
        if (!list.length) return null;
        const S = SHAPE[shape];
        const Icon = S.icon;
        return (
          <div key={shape} className="space-y-2">
            <div className="flex items-baseline gap-2 flex-wrap">
              <h3 className="text-xs font-semibold flex items-center gap-1.5">
                <Icon className="w-3.5 h-3.5 text-primary" />{S.label}
              </h3>
              <span className="text-[11px] text-muted-foreground">{S.blurb}</span>
            </div>

            <div className="grid sm:grid-cols-2 gap-2">
              {list.map((p) => (
                <div key={p.id} className="rounded-lg border border-border p-3 space-y-2">
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <div className="flex items-center gap-1.5 flex-wrap">
                        <span className="text-sm font-medium">{p.label}</span>
                        {p.ready && <Badge variant="outline"
                          className="text-[9px] text-emerald-500 border-emerald-500/40">
                          <Check className="w-2.5 h-2.5 mr-0.5" />ready</Badge>}
                        {p.automated && <Badge variant="outline" className="text-[9px]">app can start it</Badge>}
                      </div>
                      <div className="text-[11px] font-mono text-muted-foreground mt-0.5" data-no-i18n>
                        {p.price}
                      </div>
                      {p.free_allowance && (
                        <div className="text-[11px] text-emerald-500 mt-0.5">Free: {p.free_allowance}</div>
                      )}
                    </div>
                  </div>

                  <p className="text-[11px] text-muted-foreground">{p.note}</p>

                  <div className="flex gap-1.5 flex-wrap">
                    <Button size="sm" variant="outline" className="h-6 text-[10px] px-2"
                            onClick={() => setOpen(open === p.id ? null : p.id)}>
                      {open === p.id ? "Hide steps" : "How to set up"}
                    </Button>
                    <Button size="sm" variant="ghost" className="h-6 text-[10px] px-2"
                            onClick={() => openExternalUrl(p.signup_url)}>
                      <ExternalLink className="w-3 h-3 mr-1" />Sign up
                    </Button>
                    {p.id === "fal" && (
                      <Button size="sm" variant="ghost" className="h-6 text-[10px] px-2"
                              disabled={busy === p.id} onClick={() => testKey(p.id)}>
                        {busy === p.id ? <Loader2 className="w-3 h-3 animate-spin" />
                                       : <><KeyRound className="w-3 h-3 mr-1" />Test key</>}
                      </Button>
                    )}
                  </div>

                  {open === p.id && (
                    <div className="pt-1.5 border-t border-border/60 space-y-2">
                      <ol className="text-[11px] text-muted-foreground space-y-1 list-decimal ml-4">
                        {p.setup.map((step, i) => <li key={i}>{step}</li>)}
                      </ol>
                      {/* The key providers get their field right here, so setting one up is one
                          uninterrupted motion rather than a hunt through Settings afterwards. */}
                      {p.id === "fal" && (
                        <div className="space-y-1">
                          <label className="text-[9px] uppercase tracking-widest text-muted-foreground">
                            fal.ai API key
                          </label>
                          <Input type="password" className="font-mono text-xs"
                                 placeholder="paste the key from fal.ai → Dashboard → Keys"
                                 value={settings?.fal_api_key || ""}
                                 onChange={(e) => onChange?.({ fal_api_key: e.target.value })} />
                        </div>
                      )}
                      {p.needs?.length > 0 && (
                        <div className="text-[10px] text-muted-foreground">
                          Needs: {p.needs.join(", ")}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        );
      })}

      <div className="rounded-lg border border-primary/30 bg-primary/5 p-3 text-[11px] space-y-1">
        <div className="font-medium flex items-center gap-1.5">
          <Cpu className="w-3.5 h-3.5 text-primary" />Using a rented or local box
        </div>
        <p className="text-muted-foreground">
          There is nothing to install here. Start ComfyUI wherever you like, then paste its address
          into the engine's server-URL field above — <span className="font-mono">http://127.0.0.1:8188</span>
          {" "}for a local one. Set the hardware tier in Video Gen to match the card, so the app
          offers the models it can actually load.
        </p>
      </div>
    </div>
  );
}
