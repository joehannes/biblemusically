import { useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Input } from "../components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import {
  Shirt, Loader2, Search, Store, CheckCircle2, AlertTriangle, Sparkles,
  ExternalLink, Trash2, RefreshCw,
} from "lucide-react";
import { toast } from "sonner";
import { useNavigate } from "react-router-dom";
import GuidedPanel from "../components/GuidedPanel";
import TypographyStudio from "../components/TypographyStudio";
import { useSectionAction } from "../lib/pageActions";
import { podFlow } from "../lib/guidedFlows";
import { openLoginUrl } from "../lib/openLogin";

// ─────────────────────────────────────────────────────────────────────────────
// Print on demand.
//
// Printify is the only integration in this app with a real, documented API, so this is the one place
// where "publish automatically" means it. That makes the guardrails matter more, not less:
//
//   • products are picked ONCE, so a daily run makes no product decisions;
//   • the print resolution is computed before anything is created, because generated art is 1024–2048
//     pixels and a print area is 3000–4000 — art that prints at 77 DPI is a refund;
//   • publishing is capped by Printify at 200 per 30 minutes, and that ceiling is shown rather than
//     discovered as a wall of errors;
//   • nothing publishes unless the toggle is on. A listing is public, and priced.
// ─────────────────────────────────────────────────────────────────────────────

const cents = (v) => `$${((Number(v) || 0) / 100).toFixed(2)}`;

export default function PrintOnDemand() {
  // The rendered phrase art, so the product step can composite it onto the picture.
  const [textArt, setTextArt] = useState(null);
  const { songs, activeSongId, selectSong, activeProjectId } = useStudio();
  const nav = useNavigate();
  const [tab, setTab] = useState("shop");
  const [shops, setShops] = useState([]);
  const [shopId, setShopId] = useState("");
  const [storefronts, setStorefronts] = useState([]);
  const [storeNote, setStoreNote] = useState("");
  const [search, setSearch] = useState("shirt");
  const [blueprints, setBlueprints] = useState([]);
  const [detail, setDetail] = useState(null);
  // The imagery layer's verdicts on the blueprint currently open: will it print, and may words go
  // inside the picture at all. Both are refusals rather than preferences, so they are shown next to
  // "Use this product" rather than buried.
  const [printCheck, setPrintCheck] = useState(null);
  const [textRule, setTextRule] = useState(null);
  const [picked, setPicked] = useState([]);
  const [price, setPrice] = useState(2499);
  const [made, setMade] = useState([]);
  const [pacing, setPacing] = useState(null);
  const [phrase, setPhrase] = useState("");
  const [publish, setPublish] = useState(false);
  const [busy, setBusy] = useState("");

  useEffect(() => {
    api.printifyStorefronts().then((r) => {
      setStorefronts(r?.storefronts || []);
      setStoreNote(r?.note || "");
    }).catch(() => {});
    api.printifySelection().then((r) => {
      setPicked(r?.products || []);
      if (r?.shop_id) setShopId(r.shop_id);
    }).catch(() => {});
    loadMade();
    /* eslint-disable-next-line react-hooks/exhaustive-deps */
  }, [activeProjectId]);

  const loadMade = async () => {
    try {
      const r = await api.listPrintifyProducts(activeProjectId || null);
      setMade(r?.products || []);
      setPacing(r?.pacing || null);
    } catch { /* nothing made yet */ }
  };

  const connect = async () => {
    setBusy("connect");
    try {
      const r = await api.printifyConnect();
      setShops(r?.shops || []);
      if (r?.shops?.length === 1) setShopId(String(r.shops[0].id));
      toast.success(`Connected — ${r?.shops?.length || 0} shop(s).`);
      setTab("products");
    } catch (err) { toast.error(`${err}`, { duration: 12000 }); }
    finally { setBusy(""); }
  };

  const findProducts = async () => {
    setBusy("catalog");
    try {
      const r = await api.printifyCatalog({ search, limit: 24 });
      setBlueprints(r?.blueprints || []);
      if (!r?.blueprints?.length) toast.message("Nothing matched that word.");
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  // What the app will actually generate for product art. The image engines top out around here on a
  // free GPU, and the whole point of checking is to compare that against the blueprint's real print
  // area — not against the 4000×4000 the destination nominally asks for.
  const GENERATED_ART_PX = 2048;

  const openBlueprint = async (b) => {
    setBusy(`bp-${b.id}`);
    try {
      const d = await api.printifyBlueprintDetail(b.id);
      setDetail({ ...d, title: b.title });
      setPrintCheck(null);
      setTextRule(null);

      // Two guardrails the imagery layer already knows how to apply, asked here — before the product
      // is picked — because afterwards the GPU time and the listing are already spent. The page used
      // to state a rule of thumb ("at least half that") instead of the real DPI arithmetic.
      const front = (d.variants || [])[0]?.placeholders?.find((p) => p.position === "front")
                 || (d.variants || [])[0]?.placeholders?.[0];
      if (front?.width && front?.height) {
        api.imageryPrintCheck("product", GENERATED_ART_PX, GENERATED_ART_PX, front.width, front.height)
          .then(setPrintCheck).catch(() => setPrintCheck(null));
      }
      api.imageryTextAllowed("product", "flux_dev").then(setTextRule).catch(() => setTextRule(null));
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const pick = async () => {
    const variants = (detail.variants || []).slice(0, 12);
    if (!variants.length) { toast.error("This provider returned no variants."); return; }
    const front = variants[0].placeholders?.find((p) => p.position === "front") || variants[0].placeholders?.[0];
    const entry = {
      blueprint_id: detail.blueprint_id,
      print_provider_id: detail.print_provider_id,
      title: detail.title,
      variant_ids: variants.map((v) => v.id),
      position: front?.position || "front",
      price_cents: Number(price) || 2499,
      placeholder_width: front?.width || 0,
      placeholder_height: front?.height || 0,
    };
    const next = [...picked.filter((p) => p.blueprint_id !== entry.blueprint_id), entry];
    setBusy("pick");
    try {
      await api.printifyPickProducts({ products: next, shop_id: shopId || null });
      setPicked(next);
      setDetail(null);
      toast.success(`${entry.title} added — ${entry.variant_ids.length} variants at ${cents(entry.price_cents)}.`);
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const unpick = async (blueprintId) => {
    const next = picked.filter((p) => p.blueprint_id !== blueprintId);
    await api.printifyPickProducts({ products: next, shop_id: shopId || null });
    setPicked(next);
  };

  const make = async () => {
    if (!shopId) { toast.error("Pick a shop first."); setTab("shop"); return; }
    if (!activeSongId) { toast.error("Pick a song — the wording and the art come from it."); return; }
    setBusy("make");
    try {
      const r = await api.printifyMakeProducts({
        shop_id: shopId,
        song_id: activeSongId,
        phrase: phrase.trim() || null,
        publish,
      });
      toast.success(`"${r.phrase}" applied to ${r.created?.length || 0} product(s)${r.published ? " and published" : " as drafts"}.`);
      (r.errors || []).forEach((err) => toast.warning(err, { duration: 10000 }));
      if (r.note) toast.message(r.note);
      const soft = (r.created || []).filter((c) => c.print_quality?.ok === false);
      if (soft.length) {
        toast.warning(`${soft.length} product(s) print below 150 DPI — ${soft[0].print_quality?.advice}`, { duration: 12000 });
      }
      loadMade();
      setTab("made");
    } catch (err) { toast.error(`${err}`, { duration: 12000 }); }
    finally { setBusy(""); }
  };

  // The one action that publishes real products belongs in the bar — but only while the card that
  // configures it is on screen. Making products from a tab that is not showing the wording or the
  // product list is exactly the click nobody means to make.
  const makeRef = useSectionAction({
    id: "pod-make", label: "Make the products", icon: Sparkles, variant: "primary", priority: 10,
    disabled: busy === "make" || !picked.length || !activeSongId,
    title: picked.length ? "Apply today's wording and art to the picked products" : "Pick products first",
    onClick: make,
  });

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div>
          <h1 className="text-xl font-semibold flex items-center gap-2">
            <Shirt className="w-5 h-5 text-primary" /> Print on Demand
          </h1>
          <p className="text-sm text-muted-foreground">
            The wording and the art on real products, through Printify's own API.
          </p>
        </div>
        <div className="flex gap-1">
          {[["shop", "Shop", Store], ["products", "Products", Shirt], ["make", "Make", Sparkles], ["made", "Made", CheckCircle2]]
            .map(([id, label, Icon]) => (
              <Button key={id} variant={tab === id ? "default" : "secondary"} size="sm" onClick={() => setTab(id)}>
                <Icon className="w-3.5 h-3.5 mr-1.5" />{label}
              </Button>
            ))}
        </div>
      </div>

      <GuidedPanel
        flow={podFlow}
        projectId={activeProjectId}
        extraCtx={{ picked, storefronts, shops, made }}
        actions={{ setTab: (id) => setTab(id), connect: () => connect(), make: () => make() }}
      />

      {/* ── Shop ─────────────────────────────────────────────────────────── */}
      {tab === "shop" && (
        <div className="space-y-3">
          <Card className="p-4 space-y-3">
            <div>
              <h2 className="font-medium">Connect Printify</h2>
              <p className="text-xs text-muted-foreground">
                Paste a Personal Access Token in Settings → Print on demand (Printify → My Profile →
                Connections), then connect. This is the only integration here with a real API — the
                distributors and ebook stores all need a browser.
              </p>
            </div>
            <div className="flex gap-2 flex-wrap">
              <Button onClick={connect} disabled={busy === "connect"}>
                {busy === "connect" ? <Loader2 className="w-4 h-4 animate-spin mr-1.5" /> : <RefreshCw className="w-4 h-4 mr-1.5" />}
                Connect and list shops
              </Button>
              <Button variant="secondary"
                      onClick={() => openLoginUrl("https://printify.com/app/account/connections", { navigate: nav, label: "Printify", recommended: "internal" })}>
                <ExternalLink className="w-4 h-4 mr-1.5" />Get a token
              </Button>
            </div>
            {shops.length > 0 && (
              <div className="space-y-1.5">
                <label className="text-xs text-muted-foreground">Shop to publish into</label>
                <Select value={shopId} onValueChange={setShopId}>
                  <SelectTrigger className="w-72"><SelectValue placeholder="Pick a shop" /></SelectTrigger>
                  <SelectContent>
                    {shops.map((s) => (
                      <SelectItem key={s.id} value={String(s.id)}>
                        {s.title} — {s.sales_channel}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}
          </Card>

          <Card className="p-4 space-y-2">
            <h2 className="font-medium">Where a shop can sell</h2>
            {storeNote && <p className="text-xs text-muted-foreground">{storeNote}</p>}
            {storefronts.map((s) => (
              <div key={s.id} className="rounded-lg border border-border p-2.5">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium">{s.name}</span>
                  <Badge variant="outline" className={`text-[10px] ${s.recommended ? "text-emerald-400 border-emerald-500/40" : ""}`}>
                    {s.cost}
                  </Badge>
                </div>
                <p className="text-xs text-muted-foreground mt-0.5">{s.detail}</p>
              </div>
            ))}
          </Card>
        </div>
      )}

      {/* ── Products ─────────────────────────────────────────────────────── */}
      {tab === "products" && (
        <div className="space-y-3">
          <Card className="p-4 space-y-3">
            <div>
              <h2 className="font-medium">Pick the products once</h2>
              <p className="text-xs text-muted-foreground">
                A daily run should make no product decisions. Pick them here, with a price, and the run
                only applies the day's wording and art.
              </p>
            </div>
            <div className="flex gap-2 flex-wrap">
              <Input value={search} onChange={(e) => setSearch(e.target.value)}
                     onKeyDown={(e) => { if (e.key === "Enter") findProducts(); }}
                     placeholder="shirt, mug, poster, tote, hoodie" className="w-64" />
              <Input type="number" value={price} onChange={(e) => setPrice(e.target.value)}
                     className="w-32" title="Retail price in cents" />
              <span className="text-xs text-muted-foreground self-center">{cents(price)} retail</span>
              <Button onClick={findProducts} disabled={busy === "catalog"}>
                {busy === "catalog" ? <Loader2 className="w-4 h-4 animate-spin mr-1.5" /> : <Search className="w-4 h-4 mr-1.5" />}
                Search the catalogue
              </Button>
            </div>
            <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-2">
              {blueprints.map((b) => (
                <button key={b.id} onClick={() => openBlueprint(b)}
                        className="text-left rounded-lg border border-border p-2.5 hover:border-primary/60 transition-all">
                  <div className="text-sm font-medium truncate">{b.title}</div>
                  <div className="text-[11px] text-muted-foreground truncate">{b.brand} {b.model}</div>
                </button>
              ))}
            </div>
          </Card>

          {detail && (
            <Card className="p-4 space-y-2">
              <div className="flex items-center justify-between gap-2 flex-wrap">
                <div>
                  <div className="font-medium">{detail.title}</div>
                  <div className="text-xs text-muted-foreground">
                    {(detail.print_providers || []).length} print providers ·
                    {" "}{(detail.variants || []).length} variants
                  </div>
                </div>
                <div className="flex gap-1.5">
                  <Button size="sm" onClick={pick} disabled={busy === "pick"}>Use this product</Button>
                  <Button size="sm" variant="ghost" onClick={() => setDetail(null)}>Close</Button>
                </div>
              </div>
              {(detail.variants || []).slice(0, 1).map((v) => (
                <div key={v.id} className="text-xs text-muted-foreground">
                  Print areas: {(v.placeholders || []).map((p) => `${p.position} ${p.width}×${p.height}`).join(", ")}
                </div>
              ))}

              {/* The real arithmetic, not the rule of thumb this used to print. `imagery_print_check`
                  refuses below the destination's own minimum DPI and says what to do instead. */}
              {printCheck && (
                printCheck.refused || printCheck.ok === false ? (
                  <div className="text-xs rounded-md border border-amber-500/40 bg-amber-500/10 p-2 text-amber-200 flex items-start gap-1.5">
                    <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-px text-amber-400" />
                    <span>{printCheck.reason || "This would not print sharply at the size the art is generated."}</span>
                  </div>
                ) : printCheck.checked === false ? null : (
                  <div className="text-xs text-emerald-500">
                    Prints at {printCheck.dpi} DPI from {GENERATED_ART_PX}px art — above the 150 DPI floor.
                  </div>
                )
              )}

              {/* Product art carries no words by design: DTG loses thin strokes, and the typography
                  layer sets them in a real font afterwards, which never misspells anything. */}
              {textRule && textRule.allowed === false && (
                <div className="text-[11px] text-muted-foreground">{textRule.reason}</div>
              )}
            </Card>
          )}

          <Card className="p-4 space-y-2">
            <h2 className="font-medium">Picked ({picked.length})</h2>
            {!picked.length && <p className="text-sm text-muted-foreground">Nothing picked yet.</p>}
            {picked.map((p) => (
              <div key={p.blueprint_id} className="flex items-center justify-between gap-2 rounded-lg border border-border p-2.5">
                <div>
                  <div className="text-sm font-medium">{p.title}</div>
                  <div className="text-xs text-muted-foreground">
                    {p.variant_ids?.length} variants · {cents(p.price_cents)} · {p.position}
                    {p.placeholder_width ? ` · print area ${p.placeholder_width}×${p.placeholder_height}` : ""}
                  </div>
                </div>
                <Button size="sm" variant="ghost" onClick={() => unpick(p.blueprint_id)}>
                  <Trash2 className="w-3.5 h-3.5" />
                </Button>
              </div>
            ))}
          </Card>
        </div>
      )}

      {/* ── Make ─────────────────────────────────────────────────────────── */}
      {/* The words are set in a real font and composited on. An image model that spells a mug
          wrong has produced a returned order, not a retry. See src-tauri/typography.rs. */}
      {tab === "make" && (
        <TypographyStudio
          initialPhrase={phrase}
          onRendered={(r) => setTextArt(r)}
        />
      )}

      {tab === "make" && (
        <Card className="p-4 space-y-3" ref={makeRef}>
          <div>
            <h2 className="font-medium">Apply today's wording and art</h2>
            <p className="text-xs text-muted-foreground">
              The line is written from the song — at most eight words, because it has to stand alone on a
              mug. The art is the song's own first finished image.
            </p>
          </div>
          <div className="grid sm:grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <label className="text-xs text-muted-foreground">Song</label>
              <Select value={activeSongId || ""} onValueChange={selectSong}>
                <SelectTrigger><SelectValue placeholder="Pick a song" /></SelectTrigger>
                <SelectContent>
                  {(songs || []).slice(0, 100).map((s) => (
                    <SelectItem key={s.id} value={s.id}>{s.title}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <label className="text-xs text-muted-foreground">Wording (blank = let it write one)</label>
              <Input value={phrase} onChange={(e) => setPhrase(e.target.value)} placeholder="Be still and know" />
            </div>
          </div>
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={publish} onChange={(e) => setPublish(e.target.checked)} />
            Publish to the connected storefront (otherwise they stay drafts in Printify)
          </label>
          {pacing && !pacing.room && (
            <div className="text-xs text-amber-400 flex items-start gap-1.5">
              <AlertTriangle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
              Printify's publish window is full — {pacing.used} of {pacing.cap} in the last
              {" "}{pacing.window_minutes} minutes. New products will be created as drafts.
            </div>
          )}
          <Button onClick={make} disabled={busy === "make" || !picked.length}>
            {busy === "make" ? <Loader2 className="w-4 h-4 animate-spin mr-1.5" /> : <Sparkles className="w-4 h-4 mr-1.5" />}
            Make {picked.length || "the"} product{picked.length === 1 ? "" : "s"}
          </Button>
        </Card>
      )}

      {/* ── Made ─────────────────────────────────────────────────────────── */}
      {tab === "made" && (
        <div className="space-y-3">
          {pacing && (
            <Card className="p-3 text-sm flex items-center gap-2">
              {pacing.room ? <CheckCircle2 className="w-4 h-4 text-emerald-400" /> : <AlertTriangle className="w-4 h-4 text-amber-400" />}
              <span className="text-muted-foreground">
                {pacing.remaining} of {pacing.cap} publishes left in this {pacing.window_minutes}-minute window
              </span>
            </Card>
          )}
          {!made.length && <Card className="p-4 text-sm text-muted-foreground">Nothing made yet.</Card>}
          {made.map((p) => (
            <Card key={p.id} className="p-3 space-y-1">
              <div className="flex items-center justify-between gap-2 flex-wrap">
                <div className="text-sm font-medium">{p.title}</div>
                <div className="flex items-center gap-1.5">
                  <Badge variant="outline" className="text-[10px]">{cents(p.price_cents)}</Badge>
                  <Badge variant="outline" className={`text-[10px] ${p.published_at ? "text-emerald-400 border-emerald-500/40" : ""}`}>
                    {p.published_at ? "published" : "draft"}
                  </Badge>
                </div>
              </div>
              <div className="text-xs text-muted-foreground">"{p.phrase}"</div>
              {p.print_quality && (
                <div className={`text-xs ${p.print_quality.ok ? "text-muted-foreground" : "text-amber-400"}`}>
                  {p.print_quality.dpi} DPI — {p.print_quality.verdict}. {p.print_quality.advice}
                </div>
              )}
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
