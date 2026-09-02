import { useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import {
  BookMarked, Loader2, Sparkles, Image as Img, RefreshCw, Trash2, ExternalLink,
  Download, Store, AlertTriangle, Music2, DollarSign, Users,
} from "lucide-react";
import { toast } from "sonner";
import { useNavigate } from "react-router-dom";
import GuidedPanel from "../components/GuidedPanel";
import { useBarAction, useSectionAction } from "../lib/pageActions";
import { novelFlow } from "../lib/guidedFlows";
import { openLoginUrl } from "../lib/openLogin";
import AvatarUniverses from "../components/AvatarUniverses";

// ─────────────────────────────────────────────────────────────────────────────
// Graphic novels → ebooks.
//
// The song's text already exists, and so does its art. What is missing is a *reading* of the text —
// written in a chosen voice rather than "made more poetic" — and a container that holds the words, the
// pictures and the song at once.
//
// That container is EPUB 3, the only mainstream ebook format that carries audio. A PDF of a graphic
// novel about a song cannot play the song, which is most of what makes this worth making.
//
// Four steps in one view because they are one job: write it, illustrate it, bind it, price it. None of
// the ebook stores has a publishing API, so the last step ends in a file and honest instructions
// rather than a button that claims to publish.
// ─────────────────────────────────────────────────────────────────────────────

export default function GraphicNovels() {
  const { songs, activeSongId, selectSong, activeProjectId } = useStudio();
  const nav = useNavigate();
  const [tab, setTab] = useState("write");
  const [styles, setStyles] = useState({ registers: [], formats: [] });
  const [register, setRegister] = useState("free_verse");
  const [format, setFormat] = useState("page");
  const [pages, setPages] = useState(12);
  const [editions, setEditions] = useState([]);
  const [active, setActive] = useState(null);
  const [stores, setStores] = useState([]);
  const [storeNote, setStoreNote] = useState("");
  const [pricing, setPricing] = useState(null);
  const [author, setAuthor] = useState("");
  const [busy, setBusy] = useState("");

  const load = async () => {
    try {
      const r = await api.listEditions(activeProjectId || null, null);
      setEditions(r?.editions || []);
      if (!active && r?.editions?.length) setActive(r.editions[r.editions.length - 1]);
    } catch { setEditions([]); }
  };

  useEffect(() => {
    api.listNovelStyles().then(setStyles).catch(() => {});
    api.listEbookStores().then((r) => {
      setStores(r?.stores || []);
      setStoreNote(r?.no_api_note || "");
    }).catch(() => {});
  }, []);
  useEffect(() => { load(); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [activeProjectId]);

  const write = async () => {
    if (!activeSongId) { toast.error("Pick a song first — the edition is a reading of its text."); return; }
    setBusy("write");
    try {
      const ed = await api.authorEdition({
        song_id: activeSongId, register, format, pages: Number(pages) || 12,
      });
      setEditions((e) => [...e, ed]);
      setActive(ed);
      setTab("pages");
      toast.success(`${ed.pages?.length || 0} pages written in ${ed.register_label}.`);
    } catch (err) { toast.error(`${err}`, { duration: 8000 }); }
    finally { setBusy(""); }
  };

  const queueArt = async () => {
    setBusy("art");
    try {
      const r = await api.generateEditionArt(active.id);
      toast.success(`${r.queued} pages queued at ${r.aspect} — they land in Image Gen.`);
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const collectArt = async () => {
    setBusy("collect");
    try {
      const r = await api.collectEditionArt(active.id);
      toast.success(`${r.with_art} of ${r.pages} pages have art.`);
      const fresh = await api.listEditions(activeProjectId || null, null);
      setEditions(fresh?.editions || []);
      setActive((fresh?.editions || []).find((e) => e.id === active.id) || active);
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const bind = async () => {
    const by = author.trim() || "Unknown";
    setBusy("bind");
    try {
      const r = await api.buildEpub({ edition_id: active.id, author: by, include_audio: true });
      toast.success(`EPUB written: ${(r.bytes / 1048576).toFixed(1)} MB, ${r.pages} pages, ${r.images} images${r.audio ? ", with the song" : ""}.`);
      if (r.note) toast.warning(r.note);
      if (r.missing_art) toast.message(`${r.missing_art} pages have a prompt but no art yet.`);
      setPricing(await api.ebookPricing({ edition_id: active.id }));
      setTab("stores");
    } catch (err) { toast.error(`${err}`, { duration: 8000 }); }
    finally { setBusy(""); }
  };

  const editPage = (i, field, value) => {
    const next = { ...active, pages: active.pages.map((p, idx) => idx === i ? { ...p, [field]: value } : p) };
    setActive(next);
  };
  const savePages = async () => {
    setBusy("save");
    try {
      await api.updateEdition(active.id, { pages: active.pages, title: active.title });
      toast.success("Saved.");
      load();
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  // Editions could be written and built but never removed, so a first attempt at a book stayed in
  // the picker forever. The generated page art lives in the image pipeline under its own sections
  // and is not swept up here — say so, rather than implying a tidier delete than this is.
  const deleteActiveEdition = async () => {
    if (!active) return;
    if (!window.confirm(`Delete the edition “${active.title || "Untitled"}”?\n\nIts ${active.pages?.length || 0} page(s) of text go with it. Any art already generated stays in the image library.`)) return;
    setBusy("delete");
    try {
      await api.deleteEdition(active.id);
      const fresh = await api.listEditions(activeProjectId || null, null);
      const rest = fresh?.editions || [];
      setEditions(rest);
      setActive(rest.length ? rest[rest.length - 1] : null);
      toast.success("Edition deleted.");
    } catch (err) { toast.error(`Could not delete it: ${err}`); }
    finally { setBusy(""); }
  };

  const reg = styles.registers.find((r) => r.id === register);
  const fmt = styles.formats.find((f) => f.id === format);

  // The two big actions live in the top bar rather than at the bottom of a long card — but only while
  // the section they act on is actually on screen. A "Write the edition" button in the bar while the
  // form that configures it is scrolled out of sight is a button whose effect you cannot see.
  const writeRef = useSectionAction({
    id: "novel-write", label: "Write the edition", icon: Sparkles, variant: "primary", priority: 10,
    disabled: busy === "write" || !activeSongId,
    title: activeSongId ? "Write the edition from this song's text" : "Pick a song first",
    onClick: write,
  });
  const bindRef = useSectionAction({
    id: "novel-bind", label: "Build the EPUB", icon: Download, variant: "primary", priority: 10,
    disabled: busy === "bind" || !active,
    onClick: bind,
  });
  // View-wide: useful from anywhere on the page once an edition exists.
  useBarAction({
    id: "novel-collect", label: "Collect art", icon: RefreshCw, priority: 60,
    disabled: !active || busy === "collect",
    title: "Pull finished page art back onto the edition",
    onClick: collectArt,
  });

  return (
    <div className="p-4 sm:p-6 lg:p-8 max-w-6xl mx-auto fade-in space-y-4">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div>
          <h1 className="text-4xl sm:text-5xl font-bold flex items-center gap-3">
            <BookMarked className="w-5 h-5 text-primary" /> Graphic Novels
          </h1>
          <p className="text-sm text-muted-foreground">
            A reading of the text, illustrated, bound as an ebook that plays the song.
          </p>
        </div>
        <div className="flex gap-1">
          {[["write", "Write", Sparkles], ["pages", "Pages", Img], ["readers", "Readers", Users],
            ["book", "Bind", BookMarked], ["stores", "Stores", Store]]
            .map(([id, label, Icon]) => (
              <Button key={id} variant={tab === id ? "default" : "secondary"} size="sm" onClick={() => setTab(id)}
                      disabled={id !== "write" && id !== "readers" && !active}>
                <Icon className="w-3.5 h-3.5 mr-1.5" />{label}
              </Button>
            ))}
        </div>
      </div>

      <GuidedPanel
        flow={novelFlow}
        projectId={activeProjectId}
        extraCtx={{ registers: styles.registers, formats: styles.formats, editions }}
        actions={{
          setRegister: (id) => setRegister(id),
          setFormat: (id) => setFormat(id),
          setPages: (n) => setPages(n),
          write: () => write(),
          setTab: (id) => setTab(id),
        }}
      />

      {/* ── Write ────────────────────────────────────────────────────────── */}
      {tab === "write" && (
        <Card className="p-4 space-y-4" ref={writeRef}>
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
              <label className="text-xs text-muted-foreground">Pages (about)</label>
              <Input type="number" min={4} max={40} value={pages} onChange={(e) => setPages(e.target.value)} />
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-xs text-muted-foreground">Voice</label>
            <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-2">
              {styles.registers.map((r) => (
                <button key={r.id} onClick={() => setRegister(r.id)}
                        className={`text-left rounded-lg border p-2.5 transition-all hover:border-primary/60 ${register === r.id ? "border-primary/60 bg-primary/5" : "border-border"}`}>
                  <div className="text-sm font-medium">{r.label}</div>
                  <div className="text-[11px] text-muted-foreground">{r.hint}</div>
                </button>
              ))}
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-xs text-muted-foreground">Page shape</label>
            <div className="grid sm:grid-cols-3 gap-2">
              {styles.formats.map((f) => (
                <button key={f.id} onClick={() => setFormat(f.id)}
                        className={`text-left rounded-lg border p-2.5 transition-all hover:border-primary/60 ${format === f.id ? "border-primary/60 bg-primary/5" : "border-border"}`}>
                  <div className="text-sm font-medium flex items-center gap-1.5">
                    {f.label} <Badge variant="outline" className="text-[9px]">{f.aspect}</Badge>
                  </div>
                  <div className="text-[11px] text-muted-foreground">{f.hint}</div>
                </button>
              ))}
            </div>
          </div>

          <Button onClick={write} disabled={busy === "write" || !activeSongId}>
            {busy === "write" ? <Loader2 className="w-4 h-4 animate-spin mr-1.5" /> : <Sparkles className="w-4 h-4 mr-1.5" />}
            Write the edition
          </Button>
          {reg && fmt && (
            <p className="text-xs text-muted-foreground">
              {reg.label}, {fmt.aspect} pages. Art prompts are written for that shape — a 16:9 panel in a
              portrait book is a letterboxed strip, so the shape is decided before the art, not after.
            </p>
          )}
        </Card>
      )}

      {/* ── Readers ──────────────────────────────────────────────────────── */}
      {tab === "readers" && (
        <AvatarUniverses
          projectId={activeProjectId}
          editions={editions}
          onRetold={(ed) => { setEditions((list) => [...list, ed]); setActive(ed); setTab("pages"); }}
        />
      )}

      {/* ── Pages ────────────────────────────────────────────────────────── */}
      {tab === "pages" && active && (
        <div className="space-y-3">
          <Card className="p-3 flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-2">
              <Input value={active.title || ""} onChange={(e) => setActive({ ...active, title: e.target.value })}
                     className="w-64 h-8" />
              <Badge variant="outline" className="text-[10px]">{active.register_label}</Badge>
              <Badge variant="outline" className="text-[10px]">{active.aspect}</Badge>
              <span className="text-xs text-muted-foreground">
                {active.pages?.filter((p) => p.image_url).length || 0} of {active.pages?.length || 0} illustrated
              </span>
            </div>
            <div className="flex gap-1.5">
              <Button size="sm" variant="ghost" className="text-muted-foreground hover:text-red-400"
                      onClick={deleteActiveEdition} disabled={busy === "delete"}>
                {busy === "delete" ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Trash2 className="w-3.5 h-3.5" />}
              </Button>
              <Button size="sm" variant="secondary" onClick={savePages} disabled={busy === "save"}>Save text</Button>
              <Button size="sm" variant="secondary" onClick={queueArt} disabled={busy === "art"}>
                {busy === "art" ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : <Img className="w-3.5 h-3.5 mr-1.5" />}
                Make the art
              </Button>
              <Button size="sm" variant="secondary" onClick={collectArt} disabled={busy === "collect"}>
                {busy === "collect" ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : <RefreshCw className="w-3.5 h-3.5 mr-1.5" />}
                Collect finished art
              </Button>
              <Button size="sm" onClick={() => setTab("book")}>Bind it</Button>
            </div>
          </Card>

          {(active.pages || []).map((p, i) => (
            <Card key={i} className="p-3 grid sm:grid-cols-[auto_1fr] gap-3">
              <div className="w-28 shrink-0">
                {p.image_url ? (
                  <img src={p.image_url.startsWith("file://") ? p.image_url : `file://${p.image_url}`}
                       alt="" className="w-28 rounded border border-border object-cover" />
                ) : (
                  <div className="w-28 h-36 rounded border border-dashed border-border flex items-center justify-center text-[10px] text-muted-foreground text-center px-2">
                    {p.art_prompt ? "art not made yet" : "no art on this page"}
                  </div>
                )}
                <div className="text-[10px] text-muted-foreground mt-1">Page {i + 1}</div>
              </div>
              <div className="space-y-2">
                <Input value={p.heading || ""} placeholder="Heading"
                       onChange={(e) => editPage(i, "heading", e.target.value)} className="h-8" />
                <Textarea value={(p.lines || []).join("\n")} className="text-sm h-24"
                          onChange={(e) => editPage(i, "lines", e.target.value.split("\n"))} />
                <Input value={p.caption || ""} placeholder="Caption under the art"
                       onChange={(e) => editPage(i, "caption", e.target.value)} className="h-8 text-xs" />
                <Textarea value={p.art_prompt || ""} placeholder="Art prompt" data-no-i18n
                          onChange={(e) => editPage(i, "art_prompt", e.target.value)}
                          className="text-xs h-16 font-mono" />
              </div>
            </Card>
          ))}
        </div>
      )}

      {/* ── Bind ─────────────────────────────────────────────────────────── */}
      {tab === "book" && active && (
        <Card className="p-4 space-y-3" ref={bindRef}>
          <div>
            <h2 className="font-medium">Bind it as an EPUB</h2>
            <p className="text-xs text-muted-foreground">
              EPUB 3 rather than PDF, because it is the only mainstream ebook format that carries audio —
              the song goes inside the book.
            </p>
          </div>
          <div className="grid sm:grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <label className="text-xs text-muted-foreground">Author, as it appears on the cover</label>
              <Input value={author} onChange={(e) => setAuthor(e.target.value)} placeholder="Your name or artist name" />
            </div>
            <div className="space-y-1.5">
              <label className="text-xs text-muted-foreground">Contents</label>
              <div className="text-sm flex items-center gap-3 pt-1.5">
                <span>{active.pages?.length || 0} pages</span>
                <span>{active.pages?.filter((p) => p.image_url).length || 0} illustrated</span>
                <span className="flex items-center gap-1"><Music2 className="w-3.5 h-3.5" /> song embedded</span>
              </div>
            </div>
          </div>
          <Button onClick={bind} disabled={busy === "bind"}>
            {busy === "bind" ? <Loader2 className="w-4 h-4 animate-spin mr-1.5" /> : <Download className="w-4 h-4 mr-1.5" />}
            Build the EPUB
          </Button>
          {active.epub_path && (
            <p className="text-xs text-muted-foreground break-all">Last built: {active.epub_path}</p>
          )}
        </Card>
      )}

      {/* ── Stores ───────────────────────────────────────────────────────── */}
      {tab === "stores" && (
        <div className="space-y-3">
          {pricing && (
            <Card className="p-4 space-y-2">
              <div className="flex items-center gap-2">
                <DollarSign className="w-4 h-4 text-primary" />
                <span className="font-medium">Price it at ${pricing.recommended_price?.toFixed(2)}</span>
              </div>
              <p className="text-xs text-muted-foreground">{pricing.why}</p>
              <div className="space-y-1">
                {(pricing.per_store || []).map((s) => (
                  <div key={s.store} className="flex items-center justify-between text-xs">
                    <span>{s.name}</span>
                    <span className="text-muted-foreground">
                      {(s.rate * 100).toFixed(0)}%{s.delivery_fee > 0 ? ` − $${s.delivery_fee.toFixed(2)} delivery` : ""}
                      {" → "}<span className="text-foreground">${s.net_per_copy?.toFixed(2)}/copy</span>
                      {!s.in_band && <span className="text-amber-400"> (outside the band)</span>}
                    </span>
                  </div>
                ))}
              </div>
            </Card>
          )}
          {storeNote && (
            <Card className="p-3 text-sm text-muted-foreground flex items-start gap-2">
              <AlertTriangle className="w-4 h-4 text-amber-400 mt-0.5 shrink-0" />
              <span>{storeNote}</span>
            </Card>
          )}
          {stores.map((s) => (
            <Card key={s.id} className="p-4 space-y-1.5">
              <div className="flex items-center justify-between gap-2 flex-wrap">
                <span className="font-medium">{s.name}</span>
                <Button size="sm" variant="secondary"
                        onClick={() => openLoginUrl(s.url, { navigate: nav, label: s.name, recommended: "internal" })}>
                  <ExternalLink className="w-3.5 h-3.5 mr-1.5" />Open
                </Button>
              </div>
              <div className="text-xs"><span className="text-muted-foreground">Royalty:</span> {s.royalty}</div>
              {s.band_high > 0 && (
                <div className="text-xs"><span className="text-muted-foreground">Good-royalty band:</span> ${s.band_low}–${s.band_high}</div>
              )}
              <p className="text-xs text-muted-foreground">{s.band_note}</p>
              <p className="text-xs text-muted-foreground">{s.reach}</p>
              <div className="text-xs"><span className="text-muted-foreground">API:</span> {s.api}</div>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
