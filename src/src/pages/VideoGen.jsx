import { useEffect, useState, useCallback } from "react";
import { api } from "../lib/api";
import { subscribeKaggle, autoStartKaggleServer } from "../lib/kaggleServerPipeline";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Textarea } from "../components/ui/textarea";
import { Label } from "../components/ui/label";
import {
  Film, Loader2, Play, AlertTriangle, Info, Lightbulb, Cpu, Package, Wand2, Server, ShieldCheck,
} from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// Video generation.
//
// The organising idea is that a person choosing a preset is choosing *what they are making*, not a
// checkpoint — so presets are grouped by purpose (B-roll, hero shot, animate a still, loop) and each
// one says what it is for and what it costs. The model behind it is shown, because on a free GPU the
// cost per clip is the constraint everything else bends around, but it is not the thing being picked.
//
// Presets the current hardware cannot run are shown *disabled rather than hidden*. Hiding them would
// make the app look like it has fewer features than it does, and would hide the reason to consider
// better hardware. Disabled with the requirement stated is the honest version.
//
// Everything above the fold is the advisor's output. It is the part that stops somebody spending
// forty minutes of a thirty-hour weekly quota discovering that a 14B model does not fit a T4.
// ─────────────────────────────────────────────────────────────────────────────

const PURPOSE_LABEL = {
  Broll: "B-roll — cheap, plenty of it",
  HeroShot: "Hero shot — the one people look at",
  AnimateStill: "Animate a still you already have",
  Loop: "Loop — seamless, stylised",
};
const PURPOSE_ORDER = ["HeroShot", "Broll", "AnimateStill", "Loop"];

export default function VideoGen() {
  const [cat, setCat] = useState(null);
  const [advice, setAdvice] = useState(null);
  const [preset, setPreset] = useState("");
  const [prompt, setPrompt] = useState("");
  const [clips, setClips] = useState(1);
  const [busy, setBusy] = useState("");
  const [results, setResults] = useState([]);
  const [verify, setVerify] = useState(null);
  const [kaggle, setKaggle] = useState({});

  useEffect(() => subscribeKaggle(setKaggle), []);

  const load = useCallback(async () => {
    try {
      const c = await api.videoCatalogue();
      setCat(c);
      if (!preset && c.selected_preset) setPreset(c.selected_preset);
      else if (!preset) {
        const first = (c.presets || []).find((p) => p.runs);
        if (first) setPreset(first.id);
      }
    } catch (e) { toast.error(`Could not load the video catalogue: ${e?.message || e}`); }
  }, [preset]);
  useEffect(() => { load(); }, [load]);

  // The advisor re-runs whenever anything it reasons about changes, so its answer is never stale
  // advice about a setting the user has already moved on from.
  useEffect(() => {
    let alive = true;
    api.videoAdvice(preset || null, cat?.selected_pack || null, clips)
      .then((a) => { if (alive) setAdvice(a); })
      .catch(() => {});
    return () => { alive = false; };
  }, [preset, clips, cat?.selected_pack, kaggle?.video?.status]);

  const chosen = cat?.presets?.find((p) => p.id === preset);
  const chosenModel = cat?.models?.find((m) => m.id === chosen?.model);
  const blocked = advice?.blocked;

  const setTier = async (tier) => {
    try {
      await api.saveSettings({ video_tier: tier });
      toast.success("Hardware tier saved.");
      load();
    } catch (e) { toast.error(String(e)); }
  };

  const choose = async (id) => {
    setPreset(id);
    try { await api.saveSettings({ video_preset: id }); } catch { /* non-fatal */ }
  };

  const generate = async () => {
    if (!prompt.trim()) return toast.error("Describe the shot first.");
    setBusy("gen");
    const made = [];
    try {
      for (let i = 0; i < clips; i++) {
        toast.message(`Rendering clip ${i + 1} of ${clips}…`, { duration: 6000 });
        const r = await api.generateVideo({ preset, prompt: prompt.trim() });
        made.push(...(r.urls || []));
        setResults((prev) => [...(r.urls || []), ...prev]);
      }
      toast.success(`${made.length} clip(s) rendered.`);
    } catch (e) {
      toast.error(String(e?.message || e), { duration: 16000 });
    } finally { setBusy(""); }
  };

  const verifyGraphs = async () => {
    setBusy("verify");
    try {
      const r = await api.verifyVideoGraphs();
      setVerify(r);
      if (!r.reachable) toast.error(`${r.detail}${r.next_step ? ` ${r.next_step}` : ""}`, { duration: 12000 });
      else if (r.ok) toast.success(r.detail);
      else toast.warning(r.detail, { duration: 12000 });
    } catch (e) { toast.error(String(e?.message || e), { duration: 12000 }); }
    finally { setBusy(""); }
  };

  const startServer = () => {
    autoStartKaggleServer("video");
    toast.message("Bringing up the video server on Kaggle — first run takes ~10 min (ComfyUI + models).");
  };

  if (!cat) {
    return <div className="p-6 flex items-center gap-2 text-muted-foreground">
      <Loader2 className="w-4 h-4 animate-spin" />Loading the video catalogue…
    </div>;
  }

  const tier = cat.tiers.find((t) => t.id === cat.tier);

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-xl font-semibold flex items-center gap-2">
          <Film className="w-5 h-5 text-primary" /> Video Gen
        </h1>
        <p className="text-sm text-muted-foreground">
          Short clips from open models on a free GPU. Pick what you are making; the model follows from that.
        </p>
      </div>

      {/* ── What will go wrong, before it does ──────────────────────────── */}
      {advice?.hints?.length > 0 && (
        <div className="space-y-2">
          {advice.hints.map((h, i) => <HintCard key={`${h.code}-${i}`} hint={h} onApply={choose} />)}
        </div>
      )}

      {/* ── Hardware ────────────────────────────────────────────────────── */}
      <Card className="p-4 space-y-3">
        <div className="flex items-center gap-2"><Cpu className="w-4 h-4 text-primary" />
          <h2 className="font-medium">Hardware</h2>
          <Badge variant="outline" className="text-[10px]">{tier?.label}</Badge>
        </div>
        <p className="text-xs text-muted-foreground">
          Kaggle's free tier is its only tier — there is no GPU upgrade to buy there. Set this higher
          only if you are pointing the video server at rented or local hardware (Colab Pro,
          Lightning.ai, RunPod, your own card); it decides which models are offered, and claiming more
          than you have just moves the failure to the middle of a render.
        </p>
        <div className="flex flex-wrap gap-1.5">
          {cat.tiers.map((t) => (
            <Button key={t.id} size="sm" variant={t.id === cat.tier ? "default" : "outline"}
                    className="h-7 text-[11px]" onClick={() => setTier(t.id)}>
              {t.label}
            </Button>
          ))}
        </div>
      </Card>

      {/* ── Server ──────────────────────────────────────────────────────── */}
      <Card className="p-4 space-y-2">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <div className="flex items-center gap-2"><Server className="w-4 h-4 text-primary" />
            <h2 className="font-medium">Video server</h2>
            {advice?.server_live
              ? <Badge variant="outline" className="text-[10px] text-emerald-500 border-emerald-500/40">live</Badge>
              : <Badge variant="outline" className="text-[10px] text-amber-400 border-amber-500/40">not running</Badge>}
          </div>
          <Button size="sm" variant="secondary" onClick={startServer}
                  disabled={kaggle?.video?.status === "waiting" || kaggle?.video?.status === "starting"}>
            {kaggle?.video?.status === "waiting" || kaggle?.video?.status === "starting"
              ? <><Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />{kaggle?.video?.phase || "starting"}…</>
              : <><Play className="w-3.5 h-3.5 mr-1.5" />Start &amp; connect</>}
          </Button>
        </div>
        {kaggle?.video?.detail && (
          <p className="text-xs text-muted-foreground">{kaggle.video.detail}</p>
        )}
        {/* Checks every graph against this server's own node list before a generation is spent on
            one. The image-to-video graph is the one that needs it — it was not transcribed from an
            official ComfyUI example, so a missing node is a real possibility. */}
        <div className="flex items-center gap-2 flex-wrap">
          <Button size="sm" variant="outline" className="h-7 text-[11px]"
                  disabled={busy === "verify"} onClick={verifyGraphs}>
            {busy === "verify" ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                               : <ShieldCheck className="w-3.5 h-3.5 mr-1.5" />}
            Check the graphs against this server
          </Button>
          {verify && (
            <span className={`text-[11px] ${verify.ok ? "text-emerald-500" : "text-amber-400"}`}>
              {verify.detail}
            </span>
          )}
        </div>
        {verify?.results?.filter((r) => !r.ok).map((r) => (
          <div key={r.preset} className="text-[11px] text-amber-400">
            <b>{r.label}</b>: {r.missing_nodes?.length > 0
              ? `this server has no ${r.missing_nodes.join(", ")} node.`
              : r.missing_inputs?.length > 0
                ? `missing inputs — ${r.missing_inputs.join(", ")}.`
                : r.problem}
          </div>
        ))}
        <p className="text-[11px] text-muted-foreground">
          A separate Kaggle session from the image server, so stills and clips render at the same time
          on separate quota.
        </p>
      </Card>

      {/* ── Presets, by what they are for ───────────────────────────────── */}
      {PURPOSE_ORDER.map((purpose) => {
        const list = cat.presets.filter((p) => p.purpose === purpose);
        if (!list.length) return null;
        return (
          <Card key={purpose} className="p-4 space-y-2">
            <h2 className="font-medium text-sm flex items-center gap-2">
              <Package className="w-4 h-4 text-primary" />{PURPOSE_LABEL[purpose] || purpose}
            </h2>
            <div className="grid sm:grid-cols-2 gap-2">
              {list.map((p) => {
                const m = cat.models.find((x) => x.id === p.model);
                const active = p.id === preset;
                return (
                  <button
                    key={p.id}
                    disabled={!p.runs}
                    onClick={() => choose(p.id)}
                    className={`text-left rounded-lg border p-2.5 space-y-1 transition-colors ${
                      active ? "border-primary bg-primary/10"
                             : p.runs ? "border-border hover:bg-accent" : "border-border/50 opacity-55 cursor-not-allowed"
                    }`}
                  >
                    <div className="flex items-center gap-1.5 flex-wrap">
                      <span className="text-sm font-medium">{p.label}</span>
                      {p.vertical && <Badge variant="outline" className="text-[9px]">9:16</Badge>}
                      {!p.runs && (
                        <Badge variant="outline" className="text-[9px] text-amber-400 border-amber-500/40">
                          needs {cat.tiers.find((t) => t.id === m?.min_tier)?.label || "more VRAM"}
                        </Badge>
                      )}
                    </div>
                    <div className="text-[11px] text-muted-foreground">{p.use_for}</div>
                    <div className="text-[10px] text-muted-foreground font-mono" data-no-i18n>
                      {p.width}×{p.height} · {p.seconds?.toFixed?.(1)}s · {m?.label}
                      {m?.minutes_per_clip ? ` · ~${m.minutes_per_clip[0]}-${m.minutes_per_clip[1]} min` : ""}
                    </div>
                  </button>
                );
              })}
            </div>
          </Card>
        );
      })}

      {/* ── The prompt ──────────────────────────────────────────────────── */}
      <Card className="p-4 space-y-3">
        <div className="flex items-center gap-2"><Wand2 className="w-4 h-4 text-primary" />
          <h2 className="font-medium">Describe the shot</h2>
        </div>
        {chosenModel && (
          <p className="text-[11px] text-muted-foreground">{chosenModel.character}</p>
        )}
        <Textarea rows={3} value={prompt} onChange={(e) => setPrompt(e.target.value)}
                  placeholder="slow drift over still water at dawn, mist, low sun, cinematic" />
        <div className="flex items-end gap-3 flex-wrap">
          <div className="space-y-1">
            <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Clips</Label>
            <div className="flex gap-1">
              {[1, 2, 4, 8].map((n) => (
                <Button key={n} size="sm" variant={clips === n ? "default" : "outline"}
                        className="h-7 w-9 text-[11px]" onClick={() => setClips(n)}>{n}</Button>
              ))}
            </div>
          </div>
          <Button onClick={generate} disabled={busy === "gen" || blocked || !prompt.trim()}>
            {busy === "gen" ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <Film className="w-4 h-4 mr-2" />}
            Render {clips > 1 ? `${clips} clips` : "clip"}
          </Button>
          {blocked && (
            <span className="text-[11px] text-amber-400 flex items-center gap-1">
              <AlertTriangle className="w-3.5 h-3.5" />Fix the blocker above first.
            </span>
          )}
        </div>
      </Card>

      {results.length > 0 && (
        <Card className="p-4 space-y-2">
          <h2 className="font-medium text-sm">Rendered</h2>
          <div className="grid sm:grid-cols-2 gap-2">
            {results.map((u, i) => (
              <video key={`${u}-${i}`} src={u} controls loop className="w-full rounded-lg border border-border" />
            ))}
          </div>
        </Card>
      )}
    </div>
  );
}

/** One piece of advice, with its remedy as a button when the app can just do it. */
function HintCard({ hint, onApply }) {
  const tone = {
    Blocker: "border-red-500/40 bg-red-500/5 text-red-400",
    Warn: "border-amber-500/40 bg-amber-500/5 text-amber-400",
    Note: "border-border bg-muted/30 text-muted-foreground",
  }[hint.level] || "border-border";
  const Icon = { Blocker: AlertTriangle, Warn: AlertTriangle, Note: Info }[hint.level] || Lightbulb;

  return (
    <div className={`rounded-lg border p-2.5 flex items-start gap-2 text-xs leading-relaxed ${tone}`}>
      <Icon className="w-4 h-4 shrink-0 mt-0.5" />
      <div className="space-y-1.5 min-w-0">
        <div className="font-medium">{hint.problem}</div>
        <div className="text-muted-foreground">{hint.fix}</div>
        {hint.apply && (
          <Button size="sm" variant="outline" className="h-6 text-[11px]"
                  onClick={() => onApply(hint.apply)}>
            Switch to that
          </Button>
        )}
      </div>
    </div>
  );
}
