import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Label } from "./ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";
import { Sun, Info, ShieldCheck, Clock, Sparkles } from "lucide-react";

// ─────────────────────────────────────────────────────────────────────────────
// Devotional imagery: pick where it is going, then how it should feel.
//
// Two halves on purpose. A packaged intent ("Chapter opener", "The turn") sets the destination and
// a sensible voice in one click; the controls below adjust the voice without touching the
// destination's constraints, which are platform facts rather than taste.
//
// The panel's real job is the notes. Every decision the backend makes on the user's behalf — that a
// negative prompt would be ignored on this model, that a release cover may not carry text, that
// this shape is not what the model is best at — is shown here rather than discovered later in an
// image that came out wrong. The catalogue and every rule live in src-tauri/imagery.rs, so this
// component holds no copy of its own.
// ─────────────────────────────────────────────────────────────────────────────

const LABELS = {
  light: { dawn: "Dawn", shafts: "Shafts through cloud", candlelit: "Candlelit",
           golden: "Golden hour", storm: "Storm light", starlit: "Starlit" },
  mood: { serene: "Serene", hopeful: "Hopeful", solemn: "Solemn", awe: "Awe",
          held_breath: "Held breath", glory: "Glory" },
  tension: { none: "None", some: "Some", high: "High" },
  era: { ancient: "Ancient Near East", timeless: "Timeless", modern: "Modern parable" },
  symbolism: { none: "None", subtle: "Subtle", overt: "Overt" },
  figures: { no_faces: "No faces shown", faces: "Faces shown", symbolic_only: "Symbolic only" },
};

const HINTS = {
  // Why "no faces" is offered first: it is what many Christian channels actually want, it sidesteps
  // depicting Christ's face, and it happens to sidestep the face-consistency problem entirely.
  figures: "\"No faces\" means backs, silhouettes, hands, feet and robes — the option most channels want.",
  tension: "Tension here is scale, weather and a withheld reveal. It never darkens the scene.",
  symbolism: "Subtle by default: overt reads as clip-art at fifty images a day.",
};

export default function ImageryStudio({ subject = "", projectId, onCompose }) {
  const [cat, setCat] = useState(null);
  const [intent, setIntent] = useState("chapter_opener");
  const [model, setModel] = useState("flux_dev");
  const [controls, setControls] = useState(null);
  const [preview, setPreview] = useState(null);
  const [error, setError] = useState("");

  useEffect(() => { api.imageryCatalogue().then(setCat).catch(() => {}); }, []);

  // Choosing a preset resets the voice to that preset's own, which is what "preset" has to mean.
  useEffect(() => {
    if (!cat) return;
    const chosen = cat.intents.find((i) => i.id === intent);
    if (!chosen) return;
    setControls({ era: "timeless", restraint: true, strict: false, ...chosen.controls });
    if (chosen.model) setModel(chosen.model);
  }, [cat, intent]);

  useEffect(() => {
    if (!cat || !controls || !subject.trim()) { setPreview(null); return; }
    let live = true;
    api.composeImagePrompt({ subject, model, intent, controls })
      .then((r) => { if (live) { setPreview(r); setError(""); } })
      .catch((err) => { if (live) { setPreview(null); setError(String(err)); } });
    return () => { live = false; };
  }, [cat, controls, model, intent, subject]);

  if (!cat) return null;
  const chosen = cat.intents.find((i) => i.id === intent);
  const dest = cat.destinations.find((d) => d.id === chosen?.destination);
  const chosenModel = cat.models.find((m) => m.id === model);
  const set = (key, value) => setControls((c) => ({ ...c, [key]: value }));

  return (
    <Card className="p-5 mb-6 space-y-4">
      <div className="flex items-center gap-2">
        <Sun className="w-4 h-4 text-primary" />
        <h2 className="font-semibold">Imagery</h2>
        {dest && <Badge variant="outline" className="text-[9px]">{dest.aspect} · {dest.width}×{dest.height}</Badge>}
        {preview && (
          <Badge variant="outline" className="text-[9px] flex items-center gap-1">
            <Clock className="w-3 h-3" />~{preview.seconds}s each
          </Badge>
        )}
      </div>

      {/* ── Where it is going ──────────────────────────────────────────────── */}
      <div className="grid md:grid-cols-2 gap-3">
        <div className="space-y-1">
          <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">What this is for</Label>
          <Select value={intent} onValueChange={setIntent}>
            <SelectTrigger data-testid="imagery-intent"><SelectValue /></SelectTrigger>
            <SelectContent>
              {cat.intents.map((i) => <SelectItem key={i.id} value={i.id}>{i.label}</SelectItem>)}
            </SelectContent>
          </Select>
          {chosen && <p className="text-[11px] text-muted-foreground">{chosen.purpose}</p>}
          {/* A preset that silently drops a constraint is worse than no preset. */}
          {chosen && (
            <p className="text-[11px] text-amber-400/90 flex items-start gap-1">
              <ShieldCheck className="w-3 h-3 shrink-0 mt-0.5" />
              <span>{chosen.refuses}</span>
            </p>
          )}
        </div>

        <div className="space-y-1">
          <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Model</Label>
          <Select value={model} onValueChange={setModel}>
            <SelectTrigger data-testid="imagery-model"><SelectValue /></SelectTrigger>
            <SelectContent>
              {cat.models.map((m) => (
                <SelectItem key={m.id} value={m.id}>
                  {m.label} · ~{m.seconds}s{m.negatives ? "" : " · no \"avoid\" field"}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {chosenModel && (
            <>
              <p className="text-[11px] text-muted-foreground">{chosenModel.good_at}</p>
              <p className="text-[11px] text-muted-foreground/80">Weak at: {chosenModel.bad_at}</p>
            </>
          )}
        </div>
      </div>

      {dest && (
        <p className="text-[11px] text-muted-foreground flex items-start gap-1.5 border-l-2 border-border pl-2">
          <Info className="w-3 h-3 shrink-0 mt-0.5" /><span>{dest.constraint}</span>
        </p>
      )}

      {/* ── How it should feel ─────────────────────────────────────────────── */}
      {controls && (
        <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-3">
          {["light", "mood", "tension", "figures", "era", "symbolism"].map((key) => (
            <div key={key} className="space-y-1">
              <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">{key}</Label>
              <Select value={controls[key] || ""} onValueChange={(v) => set(key, v)}>
                <SelectTrigger data-testid={`imagery-${key}`}><SelectValue placeholder="—" /></SelectTrigger>
                <SelectContent>
                  {(cat.controls[key] || []).map((v) => (
                    <SelectItem key={v} value={v}>{LABELS[key]?.[v] || v}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {HINTS[key] && <p className="text-[10px] text-muted-foreground">{HINTS[key]}</p>}
            </div>
          ))}
        </div>
      )}

      {controls && (
        <div className="flex flex-wrap gap-4">
          <label className="flex items-start gap-2 text-xs cursor-pointer">
            <input type="checkbox" className="accent-primary mt-0.5" checked={controls.restraint !== false}
                   data-testid="imagery-restraint"
                   onChange={(e) => set("restraint", e.target.checked)} />
            <span>
              <b>Restraint</b>
              <span className="text-muted-foreground"> — modest clothing, no gore, no occult
              symbolism, nothing sensual. On unless you turn it off.</span>
            </span>
          </label>
          {chosenModel && !chosenModel.negatives && (
            <label className="flex items-start gap-2 text-xs cursor-pointer">
              <input type="checkbox" className="accent-primary mt-0.5" checked={!!controls.strict}
                     data-testid="imagery-strict"
                     onChange={(e) => set("strict", e.target.checked)} />
              <span>
                <b>Strict mode</b>
                <span className="text-muted-foreground"> — turns on the guidance {chosenModel.label} normally
                runs without, so an "avoid" list works. Roughly twice the time.</span>
              </span>
            </label>
          )}
        </div>
      )}

      {/* ── What will actually be sent ─────────────────────────────────────── */}
      {error && <p className="text-xs text-destructive">{error}</p>}
      {preview && (
        <div className="space-y-2">
          <details className="text-xs">
            <summary className="cursor-pointer text-muted-foreground flex items-center gap-1.5">
              <Sparkles className="w-3 h-3" />What will be sent
            </summary>
            <p className="mt-1.5 text-[11px] leading-relaxed" data-no-i18n>{preview.positive}</p>
            {preview.negative && (
              <p className="mt-1.5 text-[11px] text-muted-foreground" data-no-i18n>
                <b>Avoiding:</b> {preview.negative}
              </p>
            )}
          </details>
          {/* The notes are the point of this panel: every decision made on the user's behalf. */}
          {preview.notes?.map((n, i) => (
            <p key={i} className="text-[11px] text-muted-foreground flex items-start gap-1.5">
              <Info className="w-3 h-3 shrink-0 mt-0.5 text-primary" /><span>{n}</span>
            </p>
          ))}
        </div>
      )}

      {onCompose && (
        <Button size="sm" disabled={!preview}
                data-testid="imagery-apply"
                onClick={() => {
                  onCompose(preview);
                  if (projectId) api.saveImageryChoice(projectId, { intent, model, controls }).catch(() => {});
                }}>
          Use this for the next images
        </Button>
      )}
    </Card>
  );
}
