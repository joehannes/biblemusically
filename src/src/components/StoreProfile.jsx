import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Input } from "./ui/input";
import { Textarea } from "./ui/textarea";
import { Badge } from "./ui/badge";
import { Store, Check, Loader2, Tag } from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// What kind of shop this is.
//
// A flavour is one choice that sets a dozen defaults: the register the listing copy is written in,
// which products are worth carrying, whether art fills a print area or fits inside it, the markup,
// and how many words may go on the object. Somebody who has not thought about any of it gets
// something coherent instead of something generic — and every default can still be overridden.
//
// The providers are listed with what each is actually for, including the three this app cannot yet
// drive. "Which of these can I use" is the question somebody has, and showing only the one that is
// implemented answers a different one.
// ─────────────────────────────────────────────────────────────────────────────

export default function StoreProfile({ projectId }) {
  const [cat, setCat] = useState({ flavours: [], providers: [] });
  const [profile, setProfile] = useState(null);
  const [busy, setBusy] = useState(false);
  const [cost, setCost] = useState("");
  const [priced, setPriced] = useState(null);

  const load = useCallback(async () => {
    if (!projectId) return;
    try { setProfile(await api.storeProfile(projectId)); } catch { setProfile(null); }
  }, [projectId]);

  useEffect(() => { api.storeFlavours().then((r) => r && setCat(r)).catch(() => {}); }, []);
  useEffect(() => { load(); }, [load]);

  const save = async (patch) => {
    if (!projectId) return;
    setBusy(true);
    try {
      setProfile(await api.saveStoreProfile({ project_id: projectId, patch }));
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(false); }
  };

  const price = async () => {
    const cents = Math.round(Number(cost) * 100);
    if (!Number.isFinite(cents) || cents <= 0) return;
    try { setPriced(await api.storePrice({ cost_cents: cents, project_id: projectId || "" })); }
    catch (err) { toast.error(`${err}`); }
  };

  if (!profile) return null;
  const chosen = cat.flavours.find((f) => f.id === profile.flavour);

  return (
    <div className="space-y-3">
      <Card className="p-4 space-y-3">
        <div className="flex items-start gap-2">
          <div className="p-1.5 rounded-md bg-primary/10 shrink-0"><Store className="w-4 h-4 text-primary" /></div>
          <div className="min-w-0">
            <div className="text-sm font-semibold">What kind of shop this is</div>
            <p className="text-[11px] text-muted-foreground max-w-2xl">
              One choice that sets the copy's register, which products are worth carrying, how the art
              is framed and how it prices. Everything it sets can still be changed.
            </p>
          </div>
          {busy && <Loader2 className="w-3.5 h-3.5 animate-spin text-muted-foreground ml-auto shrink-0" />}
        </div>

        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {cat.flavours.map((f) => {
            const active = profile.flavour === f.id;
            return (
              <button key={f.id} data-testid={`flavour-${f.id}`} onClick={() => save({ flavour: f.id })}
                      className={`text-left rounded-md border p-2.5 transition-colors ${
                        active ? "border-primary bg-primary/5" : "border-border hover:border-primary/50"}`}>
                <div className="text-xs font-medium flex items-center gap-1.5">
                  {f.label}
                  {active && <Check className="w-3 h-3 text-primary" />}
                </div>
                <div className="text-[10px] text-muted-foreground leading-snug">{f.hint}</div>
                <div className="flex flex-wrap gap-1 mt-1">
                  {f.categories.map((c) => (
                    <Badge key={c} variant="secondary" className="text-[9px] font-normal">{c}</Badge>
                  ))}
                </div>
              </button>
            );
          })}
        </div>

        {chosen && (
          <p className="text-[11px] text-muted-foreground">
            <span>Art </span>
            <span className="text-foreground">{chosen.fill ? "fills the print area" : "fits inside the print area"}</span>
            <span>, at most </span>
            <span className="text-mono">{chosen.max_phrase_words}</span>
            <span> words on the object, priced at </span>
            <span className="text-mono">{chosen.markup}×</span>
            <span> cost by default.</span>
          </p>
        )}
      </Card>

      <Card className="p-4 space-y-2.5">
        <div className="text-sm font-semibold">The shop's own words</div>
        <p className="text-[11px] text-muted-foreground">
          Used in every listing this project makes. Without them, two people selling completely
          different things get byte-identical copy.
        </p>
        <div className="grid gap-2.5 sm:grid-cols-2">
          {[["brand", "Shop name"], ["audience", "Who buys here"]].map(([key, label]) => (
            <div key={key} className="space-y-1">
              <label className="text-[11px] text-muted-foreground">{label}</label>
              <Input value={profile[key] ?? ""} className="h-8 text-sm"
                     onChange={(e) => setProfile({ ...profile, [key]: e.target.value })}
                     onBlur={(e) => save({ [key]: e.target.value })} />
            </div>
          ))}
        </div>
        <div className="space-y-1">
          <label className="text-[11px] text-muted-foreground">
            The line under every listing
          </label>
          <Input value={profile.blurb ?? ""} className="h-8 text-sm"
                 placeholder="Printed on demand."
                 onChange={(e) => setProfile({ ...profile, blurb: e.target.value })}
                 onBlur={(e) => save({ blurb: e.target.value })} />
        </div>
        <div className="space-y-1">
          <label className="text-[11px] text-muted-foreground">
            House rules — things the copy must or must never do
          </label>
          <Textarea rows={2} value={profile.note ?? ""} className="text-sm"
                    placeholder="never say 'stunning'; always name the fabric"
                    onChange={(e) => setProfile({ ...profile, note: e.target.value })}
                    onBlur={(e) => save({ note: e.target.value })} />
        </div>
      </Card>

      <Card className="p-4 space-y-2.5">
        <div className="text-sm font-semibold flex items-center gap-2">
          <Tag className="w-3.5 h-3.5 text-primary" />What things cost
        </div>
        <div className="grid gap-2.5 sm:grid-cols-3">
          <div className="space-y-1">
            <label className="text-[11px] text-muted-foreground">Markup on cost</label>
            <Input type="number" step="0.1" min="1" value={profile.markup ?? ""} className="h-8 text-sm"
                   onChange={(e) => setProfile({ ...profile, markup: Number(e.target.value) })}
                   onBlur={(e) => save({ markup: Number(e.target.value) })} />
          </div>
          <div className="space-y-1">
            <label className="text-[11px] text-muted-foreground">Never below (cents)</label>
            <Input type="number" min="0" value={profile.price_floor_cents ?? 0} className="h-8 text-sm"
                   onChange={(e) => setProfile({ ...profile, price_floor_cents: Number(e.target.value) })}
                   onBlur={(e) => save({ price_floor_cents: Number(e.target.value) })} />
          </div>
          <div className="space-y-1">
            <label className="text-[11px] text-muted-foreground">Try a cost</label>
            <Input type="number" step="0.01" min="0" value={cost} className="h-8 text-sm"
                   placeholder="8.40"
                   onChange={(e) => setCost(e.target.value)} onBlur={price} />
          </div>
        </div>
        {priced && (
          <p className="text-[11px] text-muted-foreground">
            <span>Sells at </span>
            <span className="text-mono text-foreground">{(priced.retail_cents / 100).toFixed(2)}</span>
            <span>, leaving </span>
            <span className="text-mono text-foreground">{(priced.margin_cents / 100).toFixed(2)}</span>
            <span> — </span>
            <span className="text-mono">{priced.margin_pct}%</span>
            <span> of the price. The provider's own cut is already inside the cost, so this is the whole of what reaches you.</span>
            {priced.note && <span className="text-amber-600 dark:text-amber-400"> {priced.note}</span>}
          </p>
        )}
      </Card>

      <Card className="p-4 space-y-2">
        <div className="text-sm font-semibold">Who prints it</div>
        <div className="space-y-1.5">
          {cat.providers.map((p) => (
            <div key={p.id} className={`rounded-lg border p-2.5 text-xs ${
              p.wired ? "border-primary/40 bg-primary/5" : "border-border"}`}>
              <div className="flex items-center gap-2 flex-wrap">
                <span className="font-medium">{p.label}</span>
                <Badge variant={p.wired ? "default" : "outline"} className="text-[9px]">
                  {p.wired ? <span>working</span> : <span>not wired up</span>}
                </Badge>
                <span className="text-[10px] text-muted-foreground">{p.region}</span>
              </div>
              <div className="text-[11px] text-muted-foreground mt-0.5">{p.strength}</div>
              {p.what_it_needs && (
                <div className="text-[11px] text-muted-foreground/80 mt-1">
                  <span className="text-foreground/70">What it would take: </span>{p.what_it_needs}
                </div>
              )}
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}
