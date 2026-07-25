import { useEffect, useMemo, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Textarea } from "../components/ui/textarea";
import { Input } from "../components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import {
  Megaphone, Sparkles, Loader2, Image as Img, Copy, Trash2, ExternalLink,
  CheckCircle2, AlertTriangle, RefreshCw, Youtube,
} from "lucide-react";
import { toast } from "sonner";
import { useNavigate } from "react-router-dom";
import GuidedPanel from "../components/GuidedPanel";
import { publicityFlow } from "../lib/guidedFlows";
import { openLoginUrl } from "../lib/openLogin";

// ─────────────────────────────────────────────────────────────────────────────
// Publicity — the step after the upload.
//
// A finished video that nobody links to is a finished video nobody watches. Once a song has lyrics,
// a style, analysed sections and rendered images, the studio knows enough to write *about* it: an
// article or post shaped for each platform's own norms, with cover images made for that piece rather
// than lifted from the video, and links back to the YouTube upload and the short.
//
// Nothing here posts by itself. Every piece starts as a draft, and the platforms that have no usable
// posting API say so plainly and hand over a copy button and the in-app browser instead of pretending.
// ─────────────────────────────────────────────────────────────────────────────

const TIER_NOTE = {
  open: { label: "API", cls: "text-emerald-400 border-emerald-500/40" },
  connected: { label: "connected", cls: "text-emerald-400 border-emerald-500/40" },
  review_gated: { label: "needs review", cls: "text-amber-400 border-amber-500/40" },
  paid: { label: "paid API", cls: "text-amber-400 border-amber-500/40" },
  macro: { label: "browser macro", cls: "text-sky-400 border-sky-500/40" },
};

export default function Publicity() {
  const { songs, activeSongId, selectSong, activeProjectId } = useStudio();
  const nav = useNavigate();
  const [platforms, setPlatforms] = useState([]);
  const [accounts, setAccounts] = useState([]);
  const [pieces, setPieces] = useState([]);
  const [platform, setPlatform] = useState("");
  const [angle, setAngle] = useState("");
  const [busy, setBusy] = useState("");
  const [editing, setEditing] = useState({});      // id -> { title, body }

  const load = async () => {
    try { setPieces(await api.listPublicityPieces(activeSongId || null, activeProjectId || null)); }
    catch { setPieces([]); }
  };

  useEffect(() => {
    api.listSocialPlatforms().then((r) => setPlatforms(Array.isArray(r) ? r : r?.platforms || [])).catch(() => {});
    api.listSocialAccounts().then(setAccounts).catch(() => setAccounts([]));
  }, []);
  useEffect(() => { load(); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [activeSongId, activeProjectId]);
  // Covers finish rendering in the background, so the list refreshes while they land.
  useEffect(() => {
    const t = setInterval(() => { if (pieces.some((p) => (p.cover_prompts || []).length > (p.cover_urls || []).length)) load(); }, 5000);
    return () => clearInterval(t);
    /* eslint-disable-next-line react-hooks/exhaustive-deps */
  }, [pieces]);

  const connected = useMemo(() => new Set(accounts.map((a) => a.platform)), [accounts]);
  const song = songs.find((s) => s.id === activeSongId);
  const platformInfo = (id) => platforms.find((p) => p.id === id);

  const write = async (target) => {
    if (!activeSongId) return toast.error("Pick a song first.");
    setBusy(target || "one");
    try {
      const piece = await api.authorPublicityPiece({
        song_id: activeSongId, platform: target, angle: angle.trim() || null, covers: 2,
      });
      toast.success(`Wrote "${piece.title}" for ${platformInfo(target)?.label || target}.`);
      await load();
    } catch (err) { toast.error(String(err), { duration: 12000 }); }
    finally { setBusy(""); }
  };

  const writeAll = async () => {
    if (!activeSongId) return toast.error("Pick a song first.");
    setBusy("all");
    try {
      const r = await api.authorPublicitySet(activeSongId, null);
      const ok = r?.written?.length || 0;
      toast[ok ? "success" : "error"](
        ok ? `Wrote ${ok} piece${ok === 1 ? "" : "s"}.` : "Nothing could be written.",
        { description: r?.failed?.length ? `${r.failed.length} platform(s) failed — see the console.` : undefined },
      );
      if (r?.failed?.length) console.warn("[publicity] failures:", r.failed);
      await load();
    } catch (err) { toast.error(String(err), { duration: 12000 }); }
    finally { setBusy(""); }
  };

  const covers = async (id) => {
    setBusy(id);
    try {
      const r = await api.generatePublicityCovers(id);
      toast.success(`Queued ${r.queued} cover image${r.queued === 1 ? "" : "s"} — they appear here when they render.`);
    } catch (err) { toast.error(String(err)); }
    finally { setBusy(""); }
  };

  const save = async (piece) => {
    const draft = editing[piece.id];
    if (!draft) return;
    try {
      await api.updatePublicityPiece(piece.id, draft);
      setEditing((e) => ({ ...e, [piece.id]: undefined }));
      toast.success("Saved.");
      await load();
    } catch (err) { toast.error(String(err)); }
  };

  const copyOut = async (piece) => {
    const text = [piece.title, "", piece.body, "", (piece.tags || []).join(" ")].filter(Boolean).join("\n");
    try { await navigator.clipboard.writeText(text); toast.success("Copied — paste it into the site."); }
    catch { toast.error("Could not reach the clipboard."); }
  };

  const readySongs = songs.filter((s) => s.lyrics);

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-3xl sm:text-5xl font-bold flex items-center gap-3">
          <Megaphone className="w-8 h-8 text-primary" />Publicity
        </h1>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">
        Articles and posts written about a finished song — one per platform, in that platform's own
        register, with cover images made for the piece and links back to the video. Everything starts
        as a draft; nothing posts on its own.
      </p>

      <div className="mb-6">
        <GuidedPanel
          flow={publicityFlow}
          projectId={activeProjectId}
          extraCtx={{ accounts, platforms, pieceCount: pieces.length }}
          actions={{
            setPlatform: (id) => setPlatform(id),
            setAngle: (a) => setAngle(a),
            write: () => (platform ? write(platform) : writeAll()),
            writeAll: () => writeAll(),
          }}
        />
      </div>

      {/* ── what to write about ─────────────────────────────────────────── */}
      <Card className="p-5 mb-6 space-y-3">
        <div className="grid md:grid-cols-3 gap-3">
          <div className="space-y-1.5">
            <div className="text-[10px] uppercase tracking-widest text-muted-foreground">Song</div>
            <Select value={activeSongId || ""} onValueChange={selectSong}>
              <SelectTrigger data-testid="publicity-song"><SelectValue placeholder={readySongs.length ? "Select a song" : "No songs with lyrics yet"} /></SelectTrigger>
              <SelectContent>
                {readySongs.map((s) => <SelectItem key={s.id} value={s.id}>{s.title}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <div className="text-[10px] uppercase tracking-widest text-muted-foreground">Platform</div>
            <Select value={platform} onValueChange={setPlatform}>
              <SelectTrigger data-testid="publicity-platform"><SelectValue placeholder="All connected" /></SelectTrigger>
              <SelectContent>
                {platforms.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.label}{connected.has(p.id) ? " ✓" : ""}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1.5">
            <div className="text-[10px] uppercase tracking-widest text-muted-foreground">Angle (optional)</div>
            <Input value={angle} onChange={(e) => setAngle(e.target.value)}
              placeholder="e.g. lead with the Hebrew imagery" />
          </div>
        </div>
        <div className="flex gap-2 flex-wrap">
          <Button size="sm" onClick={() => (platform ? write(platform) : writeAll())} disabled={!activeSongId || !!busy}>
            {busy ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" /> : <Sparkles className="w-3.5 h-3.5 mr-1.5" />}
            {platform ? `Write for ${platformInfo(platform)?.label || platform}` : "Write for every connected platform"}
          </Button>
          <Button size="sm" variant="ghost" onClick={load}><RefreshCw className="w-3.5 h-3.5 mr-1.5" />Refresh</Button>
          {song?.video_url && (
            <a href={song.video_url} target="_blank" rel="noreferrer"
              className="text-[11px] text-primary inline-flex items-center gap-1 self-center">
              <Youtube className="w-3.5 h-3.5" />the video this is about<ExternalLink className="w-3 h-3" />
            </a>
          )}
        </div>
        {!accounts.length && (
          <div className="text-[11px] text-amber-400 flex items-start gap-1.5">
            <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
            No social accounts are connected yet — connect one in Social Presence, or pick a platform
            above to write a piece you post by hand.
          </div>
        )}
      </Card>

      {/* ── the pieces ──────────────────────────────────────────────────── */}
      <div className="space-y-4">
        {pieces.map((p) => {
          const info = platformInfo(p.platform);
          const tier = TIER_NOTE[info?.tier] || TIER_NOTE.macro;
          const draft = editing[p.id];
          return (
            <Card key={p.id} className="p-5 space-y-3" data-testid={`publicity-piece-${p.id}`}>
              <div className="flex items-start justify-between gap-3 flex-wrap">
                <div className="min-w-0">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="font-semibold">{p.title}</span>
                    <Badge variant="outline" className={`text-[9px] ${tier.cls}`}>{info?.label || p.platform} · {tier.label}</Badge>
                    {p.status === "posted" && <Badge variant="outline" className="text-[9px] text-emerald-400 border-emerald-500/40">posted</Badge>}
                  </div>
                  {p.summary && <div className="text-[11px] text-muted-foreground mt-0.5">{p.summary}</div>}
                </div>
                <div className="flex gap-1.5">
                  <Button size="sm" variant="ghost" onClick={() => copyOut(p)} title="Copy title, body and tags">
                    <Copy className="w-3.5 h-3.5" />
                  </Button>
                  <Button size="sm" variant="ghost" onClick={async () => {
                    if (!window.confirm("Delete this piece and its covers?")) return;
                    await api.deletePublicityPiece(p.id); load();
                  }}><Trash2 className="w-3.5 h-3.5" /></Button>
                </div>
              </div>

              {p.posting_notes && (
                <div className="text-[11px] text-amber-400/90 flex items-start gap-1.5">
                  <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" /><span>{p.posting_notes}</span>
                </div>
              )}

              <Textarea
                value={draft?.body ?? p.body}
                onChange={(ev) => setEditing((e) => ({ ...e, [p.id]: { ...(e[p.id] || { title: p.title }), body: ev.target.value } }))}
                className="min-h-[10rem] text-sm font-normal leading-relaxed"
                data-no-i18n
              />

              <div className="flex items-center gap-2 flex-wrap">
                {(p.tags || []).map((t) => <Badge key={t} variant="secondary" className="text-[10px]">{t}</Badge>)}
              </div>

              {/* covers */}
              <div className="flex items-center gap-2 flex-wrap">
                <Button size="sm" variant="secondary" onClick={() => covers(p.id)} disabled={busy === p.id || !(p.cover_prompts || []).length}>
                  {busy === p.id ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" /> : <Img className="w-3.5 h-3.5 mr-1.5" />}
                  Render {(p.cover_prompts || []).length} cover{(p.cover_prompts || []).length === 1 ? "" : "s"}
                </Button>
                {draft && <Button size="sm" onClick={() => save(p)}><CheckCircle2 className="w-3.5 h-3.5 mr-1.5" />Save edits</Button>}
                {info?.tier !== "open" && info?.tier !== "connected" && (
                  <Button size="sm" variant="ghost" onClick={() => {
                    const url = p.platform === "reddit" ? "https://www.reddit.com/submit"
                      : p.platform === "devto" ? "https://dev.to/new"
                      : p.platform === "x" ? "https://x.com/compose/post"
                      : p.platform === "linkedin" ? "https://www.linkedin.com/feed/"
                      : p.platform === "pinterest" ? "https://www.pinterest.com/pin-builder/"
                      : p.platform === "tumblr" ? "https://www.tumblr.com/new/text"
                      : "";
                    if (!url) return toast.message("No posting page known for this platform.");
                    openLoginUrl(url, { navigate: nav, label: `${info?.label || p.platform} — post` });
                  }}>
                    <ExternalLink className="w-3.5 h-3.5 mr-1.5" />Open the posting page
                  </Button>
                )}
                {p.youtube_url && (
                  <a href={p.youtube_url} target="_blank" rel="noreferrer" className="text-[11px] text-primary inline-flex items-center gap-1">
                    <Youtube className="w-3.5 h-3.5" />linked video
                  </a>
                )}
              </div>

              {(p.cover_urls || []).length > 0 && (
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
                  {p.cover_urls.map((u) => (
                    <img key={u} src={u} alt="" className="rounded-md border border-border w-full h-28 object-cover" />
                  ))}
                </div>
              )}
            </Card>
          );
        })}

        {!pieces.length && (
          <Card className="p-10 text-center text-muted-foreground border-dashed">
            Nothing written yet. Pick a song above and let the studio write about it.
          </Card>
        )}
      </div>
    </div>
  );
}
