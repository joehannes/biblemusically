import { useEffect, useMemo, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import {
  Disc3, Loader2, FolderOpen, Trash2, ExternalLink, AlertTriangle, CheckCircle2,
  Image as Img, Layers, Film, Users, RefreshCw,
} from "lucide-react";
import { toast } from "sonner";
import { useNavigate } from "react-router-dom";
import GuidedPanel from "../components/GuidedPanel";
import { useBarAction } from "../lib/pageActions";
import { distributionFlow } from "../lib/guidedFlows";
import { openLoginUrl } from "../lib/openLogin";

// ─────────────────────────────────────────────────────────────────────────────
// Distribution — the songs themselves, not the videos.
//
// Two outputs live here because both are book-scale rather than chapter-scale:
//
//   • Releases: a chapter is a single, a finished book is an album, and each goes to Spotify and the
//     rest through a distributor. No self-service distributor publishes an upload API (checked July
//     2026), so the app assembles a complete, correctly named release package and the upload itself
//     is a macro or a person. Pretending otherwise would just fail slower.
//   • Compilations: every chapter of a book joined into one long video with a timestamped tracklist,
//     which is what makes a two-hour upload navigable.
//
// The distributor comparison leads with each one's AI policy, because that is the fact that decides
// everything else here: TuneCore and CD Baby refuse fully AI-generated music outright, and two of the
// ones that accept it cap how often you may release.
// ─────────────────────────────────────────────────────────────────────────────

const AI_BADGE = {
  accepts: { label: "accepts AI", cls: "text-emerald-400 border-emerald-500/40" },
  limited: { label: "AI with limits", cls: "text-amber-400 border-amber-500/40" },
  rejects: { label: "refuses AI music", cls: "text-red-400 border-red-500/40" },
};

const fmtRuntime = (secs) => {
  const t = Math.round(secs || 0);
  const h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), s = t % 60;
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
               : `${m}:${String(s).padStart(2, "0")}`;
};

export default function Distribution() {
  const { activeProjectId, channels } = useStudio();
  const nav = useNavigate();
  const [tab, setTab] = useState("releases");
  const [dists, setDists] = useState([]);
  const [distNote, setDistNote] = useState("");
  const [plan, setPlan] = useState({ singles: [], albums: [] });
  const [releases, setReleases] = useState([]);
  const [pacing, setPacing] = useState({});
  const [artists, setArtists] = useState([]);
  const [books, setBooks] = useState([]);
  const [comps, setComps] = useState([]);
  const [busy, setBusy] = useState("");
  const [artistDraft, setArtistDraft] = useState({ name: "", bio: "", channel_id: "", distributor: "" });

  const load = async () => {
    if (!activeProjectId) return;
    try {
      const [p, r] = await Promise.all([
        api.planReleases(activeProjectId),
        api.listReleases(activeProjectId),
      ]);
      setPlan({ singles: p?.singles || [], albums: p?.albums || [] });
      setReleases(r?.releases || []);
      setPacing(r?.pacing || {});
    } catch (err) { toast.error(`Could not read releases: ${err}`); }
  };

  useEffect(() => {
    api.listDistributors().then((r) => {
      setDists(r?.distributors || []);
      setDistNote(r?.no_api_anywhere || "");
    }).catch(() => {});
    api.listArtistProfiles().then((r) => setArtists(r?.artists || [])).catch(() => {});
  }, []);
  useEffect(() => { load(); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [activeProjectId]);
  useEffect(() => {
    if (!activeProjectId) return;
    api.listCompilations(activeProjectId).then((r) => setComps(r?.compilations || [])).catch(() => {});
  }, [activeProjectId]);

  const loadBooks = async () => {
    setBusy("plan-comp");
    try {
      const r = await api.compilationPlan({ project_id: activeProjectId });
      setBooks(r?.books || []);
      if (!r?.books?.length) {
        toast.message("No songs in this project have a chapter reference in their title yet.");
      }
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const createRelease = async (proposal) => {
    const artist = artists[0]?.name || channels?.[0]?.title || "";
    if (!artist) {
      toast.error("Add an artist first — a release needs somebody to be by.");
      setTab("artists");
      return;
    }
    setBusy(`create-${proposal.song_id || proposal.book}`);
    try {
      const tracks = proposal.kind === "album" ? proposal.tracks : [{
        song_id: proposal.song_id, title: proposal.title, duration: proposal.duration,
        language: proposal.language, audio: proposal.audio,
      }];
      await api.saveRelease({
        project_id: activeProjectId,
        kind: proposal.kind,
        title: proposal.title,
        artist,
        book: proposal.book || null,
        tracks,
      });
      toast.success(`${proposal.kind === "album" ? "Album" : "Single"} drafted.`);
      await load();
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const exportPackage = async (rel) => {
    setBusy(`export-${rel.id}`);
    try {
      const r = await api.exportReleasePackage(rel.id);
      toast.success(`Package written: ${r.audio_files} audio files in ${r.folder}`);
      if (r.note) toast.warning(r.note);
      if (r.missing_audio?.length) {
        toast.warning(`Missing audio for: ${r.missing_audio.join(", ")}`);
      }
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const makeCover = async (rel) => {
    setBusy(`cover-${rel.id}`);
    try {
      await api.generateReleaseCover({ release_id: rel.id });
      toast.success("Cover queued — it appears in Image Gen when it finishes.");
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  const buildComp = async (book, from, to, label) => {
    setBusy(`comp-${book}`);
    try {
      const r = await api.buildCompilation({
        project_id: activeProjectId, book,
        from: from || null, to: to || null, label: label || null,
      });
      toast.success(`Compilation built: ${r.runtime}`);
      setComps((c) => [...c, r]);
    } catch (err) { toast.error(`${err}`, { duration: 10000 }); }
    finally { setBusy(""); }
  };

  const saveArtist = async () => {
    if (!artistDraft.name.trim()) { toast.error("An artist needs a name."); return; }
    setBusy("artist");
    try {
      await api.saveArtistProfile({ ...artistDraft, project_id: activeProjectId });
      setArtists(await api.listArtistProfiles().then((r) => r?.artists || []));
      setArtistDraft({ name: "", bio: "", channel_id: "", distributor: "" });
      toast.success("Artist saved.");
    } catch (err) { toast.error(`${err}`); }
    finally { setBusy(""); }
  };

  // View-wide: refreshing is useful from any tab, and it is the action people reach for most.
  useBarAction({
    id: "dist-refresh", label: "Refresh", icon: RefreshCw, priority: 60,
    disabled: !activeProjectId, onClick: load,
  });

  const chosen = useMemo(() => {
    const ids = new Set(releases.map((r) => r.distributor).filter(Boolean));
    return dists.filter((d) => ids.has(d.id));
  }, [releases, dists]);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <div>
          <h1 className="text-xl font-semibold flex items-center gap-2">
            <Disc3 className="w-5 h-5 text-primary" /> Distribution
          </h1>
          <p className="text-sm text-muted-foreground">
            The songs on streaming services, and whole books as one long video.
          </p>
        </div>
        <div className="flex gap-1">
          {[["releases", "Releases", Disc3], ["compilations", "Compilations", Film],
            ["distributors", "Distributors", Layers], ["artists", "Artists", Users]].map(([id, label, Icon]) => (
            <Button key={id} variant={tab === id ? "default" : "secondary"} size="sm" onClick={() => setTab(id)}>
              <Icon className="w-3.5 h-3.5 mr-1.5" />{label}
            </Button>
          ))}
        </div>
      </div>

      <GuidedPanel
        flow={distributionFlow}
        projectId={activeProjectId}
        extraCtx={{ distributors: dists, plan, releases, artists }}
        actions={{ setTab: (id) => setTab(id) }}
      />

      {/* ── Releases ─────────────────────────────────────────────────────── */}
      {tab === "releases" && (
        <div className="space-y-4">
          {chosen.map((d) => {
            const p = pacing[d.id];
            if (!p?.capped) return null;
            return (
              <Card key={d.id} className={`p-3 text-sm ${p.room ? "" : "border-amber-500/50"}`}>
                <div className="flex items-center gap-2">
                  {p.room ? <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                          : <AlertTriangle className="w-4 h-4 text-amber-400" />}
                  <span className="font-medium">{d.name}</span>
                  <span className="text-muted-foreground">
                    {p.used} of {p.cap} releases used in the last {p.days} days
                    {p.room ? "" : " — the next one has to wait, or it gets rejected."}
                  </span>
                </div>
              </Card>
            );
          })}

          <Card className="p-4 space-y-3">
            <div className="flex items-center justify-between">
              <h2 className="font-medium">Drafted releases</h2>
              <Button variant="secondary" size="sm" onClick={load}>
                <RefreshCw className="w-3.5 h-3.5 mr-1.5" />Refresh
              </Button>
            </div>
            {!releases.length && (
              <p className="text-sm text-muted-foreground">
                Nothing drafted yet. The proposals below are built from songs that already have audio.
              </p>
            )}
            {releases.map((rel) => (
              <div key={rel.id} className="rounded-lg border border-border p-3 space-y-2">
                <div className="flex items-start justify-between gap-2 flex-wrap">
                  <div>
                    <div className="font-medium flex items-center gap-2">
                      {rel.title}
                      <Badge variant="outline" className="text-[10px]">{rel.kind}</Badge>
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {rel.artist} · {rel.tracks?.length || 0} track{(rel.tracks?.length || 0) === 1 ? "" : "s"}
                      {rel.cover_path ? " · cover attached" : " · no cover yet"}
                    </div>
                  </div>
                  <div className="flex gap-1.5 flex-wrap">
                    <Select
                      value={rel.distributor || ""}
                      onValueChange={async (v) => {
                        await api.saveRelease({ ...rel, distributor: v, project_id: activeProjectId });
                        load();
                      }}
                    >
                      <SelectTrigger className="w-40 h-8 text-xs"><SelectValue placeholder="Distributor" /></SelectTrigger>
                      <SelectContent>
                        {dists.map((d) => (
                          <SelectItem key={d.id} value={d.id} className="text-xs">
                            {d.name}{d.ai === "rejects" ? " — refuses AI" : ""}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <Button size="sm" variant="secondary" onClick={() => makeCover(rel)} disabled={busy === `cover-${rel.id}`}>
                      {busy === `cover-${rel.id}` ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Img className="w-3.5 h-3.5" />}
                      <span className="ml-1.5">Cover</span>
                    </Button>
                    <Button size="sm" onClick={() => exportPackage(rel)} disabled={busy === `export-${rel.id}`}>
                      {busy === `export-${rel.id}` ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <FolderOpen className="w-3.5 h-3.5" />}
                      <span className="ml-1.5">Export package</span>
                    </Button>
                    <Button size="sm" variant="ghost" onClick={async () => {
                      await api.deleteRelease(rel.id); load();
                    }}><Trash2 className="w-3.5 h-3.5" /></Button>
                  </div>
                </div>
                {rel.distributor && dists.find((d) => d.id === rel.distributor)?.ai === "rejects" && (
                  <div className="text-xs text-red-400 flex items-start gap-1.5">
                    <AlertTriangle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
                    This distributor refuses fully AI-generated music. Delivering here risks the whole
                    account, not just this release.
                  </div>
                )}
              </div>
            ))}
          </Card>

          <Card className="p-4 space-y-3">
            <h2 className="font-medium">Ready to release</h2>
            {!plan.albums.length && !plan.singles.length && (
              <p className="text-sm text-muted-foreground">
                Nothing has audio yet — generate music first, then come back.
              </p>
            )}
            {plan.albums.map((a) => (
              <div key={a.book} className="flex items-center justify-between gap-3 rounded-lg border border-border p-3">
                <div>
                  <div className="font-medium text-sm">{a.title}</div>
                  <div className="text-xs text-muted-foreground">
                    {a.chapters} of {a.chapters_total} chapters · {fmtRuntime(a.seconds)}
                    {a.complete ? " · complete book" : " · partial"}
                  </div>
                </div>
                <Button size="sm" onClick={() => createRelease(a)} disabled={busy === `create-${a.book}`}>
                  {busy === `create-${a.book}` ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : null}
                  Draft album
                </Button>
              </div>
            ))}
            {plan.singles.slice(0, 30).map((s) => (
              <div key={s.song_id} className="flex items-center justify-between gap-3 rounded-lg border border-border/60 p-2">
                <div className="text-sm truncate">{s.title}</div>
                <Button size="sm" variant="secondary" onClick={() => createRelease(s)} disabled={busy === `create-${s.song_id}`}>
                  Draft single
                </Button>
              </div>
            ))}
            {plan.singles.length > 30 && (
              <p className="text-xs text-muted-foreground">
                {plan.singles.length - 30} more songs could be singles — release them as an album instead.
              </p>
            )}
          </Card>
        </div>
      )}

      {/* ── Compilations ─────────────────────────────────────────────────── */}
      {tab === "compilations" && (
        <div className="space-y-4">
          <Card className="p-4 space-y-3">
            <div className="flex items-center justify-between">
              <div>
                <h2 className="font-medium">Whole books, one video</h2>
                <p className="text-xs text-muted-foreground">
                  Chapters in scripture order, with a timestamped tracklist YouTube turns into chapter markers.
                </p>
              </div>
              <Button size="sm" variant="secondary" onClick={loadBooks} disabled={busy === "plan-comp"}>
                {busy === "plan-comp" ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : <RefreshCw className="w-3.5 h-3.5 mr-1.5" />}
                What's ready?
              </Button>
            </div>
            {books.map((b) => (
              <div key={b.book} className="rounded-lg border border-border p-3 space-y-2">
                <div className="flex items-center justify-between gap-2 flex-wrap">
                  <div>
                    <div className="font-medium text-sm">{b.book_name}</div>
                    <div className="text-xs text-muted-foreground">
                      {b.tracks.length} rendered of {b.chapters_total} chapters · {b.estimated_runtime}
                      {b.missing_video?.length ? ` · ${b.missing_video.length} without video` : ""}
                    </div>
                  </div>
                  <div className="flex items-center gap-1.5">
                    {b.over_track_cap && (
                      <span className="text-[10px] text-amber-400">
                        over {b.track_cap} — release in volumes
                      </span>
                    )}
                    <Button size="sm" onClick={() => buildComp(b.book)} disabled={busy === `comp-${b.book}` || b.tracks.length < 2 || b.over_track_cap}>
                      {busy === `comp-${b.book}` ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : <Film className="w-3.5 h-3.5 mr-1.5" />}
                      Build
                    </Button>
                  </div>
                </div>
                {b.over_track_cap && (
                  <VolumeBuilder book={b} onBuild={buildComp} busy={busy === `comp-${b.book}`} cap={b.track_cap} />
                )}
              </div>
            ))}
          </Card>

          {comps.length > 0 && (
            <Card className="p-4 space-y-2">
              <h2 className="font-medium">Built</h2>
              {comps.map((c) => (
                <div key={c.id} className="rounded-lg border border-border p-3 space-y-2">
                  <div className="flex items-center justify-between gap-2">
                    <div>
                      <div className="text-sm font-medium">{c.title}</div>
                      <div className="text-xs text-muted-foreground">
                        {c.runtime} · {c.tracks?.length || 0} chapters
                      </div>
                    </div>
                    <Button size="sm" variant="ghost" onClick={async () => {
                      await api.deleteCompilation(c.id);
                      setComps((x) => x.filter((y) => y.id !== c.id));
                    }}><Trash2 className="w-3.5 h-3.5" /></Button>
                  </div>
                  <Textarea readOnly value={c.description || ""} className="text-xs h-32 font-mono" data-no-i18n />
                  <Button size="sm" variant="secondary" onClick={() => {
                    navigator.clipboard.writeText(c.description || "");
                    toast.success("Description copied — paste it into the upload.");
                  }}>Copy description</Button>
                </div>
              ))}
            </Card>
          )}
        </div>
      )}

      {/* ── Distributors ─────────────────────────────────────────────────── */}
      {tab === "distributors" && (
        <div className="space-y-3">
          {distNote && (
            <Card className="p-3 text-sm text-muted-foreground flex items-start gap-2">
              <AlertTriangle className="w-4 h-4 text-amber-400 mt-0.5 shrink-0" />
              <span>{distNote}</span>
            </Card>
          )}
          {dists.map((d) => {
            const badge = AI_BADGE[d.ai] || AI_BADGE.limited;
            return (
              <Card key={d.id} className={`p-4 space-y-2 ${d.ai === "rejects" ? "opacity-70" : ""}`}>
                <div className="flex items-center justify-between gap-2 flex-wrap">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{d.name}</span>
                    <Badge variant="outline" className={`text-[10px] ${badge.cls}`}>{badge.label}</Badge>
                    {d.recommended && (
                      <Badge variant="outline" className="text-[10px] text-primary border-primary/40">no rate cap</Badge>
                    )}
                  </div>
                  <Button size="sm" variant="secondary" onClick={() => openLoginUrl(d.url, { navigate: nav, label: d.name, recommended: "internal" })}>
                    <ExternalLink className="w-3.5 h-3.5 mr-1.5" />Open
                  </Button>
                </div>
                <div className="grid sm:grid-cols-2 gap-x-6 gap-y-1 text-xs">
                  <div><span className="text-muted-foreground">Price:</span> {d.price}</div>
                  <div><span className="text-muted-foreground">Royalty:</span> {d.royalty}</div>
                  <div><span className="text-muted-foreground">API:</span> {d.api}</div>
                  {d.cap_releases > 0 && (
                    <div><span className="text-muted-foreground">Rate limit:</span> {d.cap_releases} per {d.cap_days} days</div>
                  )}
                </div>
                <p className="text-xs text-muted-foreground">{d.ai_note}</p>
                {d.excludes && <p className="text-xs text-amber-400/90">Excluded: {d.excludes}</p>}
              </Card>
            );
          })}
          <p className="text-xs text-muted-foreground">
            Prices and policies checked July 2026 — verify on each site before paying. Spotify's DDEX AI
            credits standard requires disclosing AI involvement; every exported package includes that
            disclosure, filled in from what actually generated the song.
          </p>
        </div>
      )}

      {/* ── Artists ──────────────────────────────────────────────────────── */}
      {tab === "artists" && (
        <div className="space-y-4">
          <Card className="p-4 space-y-3">
            <div>
              <h2 className="font-medium">Channels as artists</h2>
              <p className="text-xs text-muted-foreground">
                One channel releases as one artist. The store links land here once the distributor
                confirms them, which is what keeps royalties attached to the thing that earns them.
              </p>
            </div>
            <div className="grid sm:grid-cols-2 gap-2">
              <Input placeholder="Artist name" value={artistDraft.name}
                     onChange={(e) => setArtistDraft({ ...artistDraft, name: e.target.value })} />
              <Select value={artistDraft.channel_id}
                      onValueChange={(v) => setArtistDraft({ ...artistDraft, channel_id: v })}>
                <SelectTrigger><SelectValue placeholder="Channel (optional)" /></SelectTrigger>
                <SelectContent>
                  {(channels || []).map((c) => (
                    <SelectItem key={c.id} value={c.id}>{c.title || c.handle || c.id}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <Textarea placeholder="Bio — what this artist is, in the words a listener would read."
                      value={artistDraft.bio}
                      onChange={(e) => setArtistDraft({ ...artistDraft, bio: e.target.value })} />
            <Button onClick={saveArtist} disabled={busy === "artist"}>
              {busy === "artist" ? <Loader2 className="w-4 h-4 animate-spin mr-1.5" /> : null}Save artist
            </Button>
          </Card>
          {artists.map((a) => (
            <Card key={a.id} className="p-3">
              <div className="font-medium text-sm">{a.name}</div>
              {a.bio && <p className="text-xs text-muted-foreground mt-1">{a.bio}</p>}
              <div className="text-xs text-muted-foreground mt-1">
                {a.distributor || "no distributor chosen"}
                {a.spotify_url ? " · Spotify linked" : ""}
              </div>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}

/// A book past the per-encode cap is released in volumes, so offer the ranges rather than a dead button.
function VolumeBuilder({ book, onBuild, busy, cap }) {
  const chapters = book.tracks.map((t) => t.chapter);
  const volumes = [];
  for (let i = 0; i < chapters.length; i += cap) {
    const slice = chapters.slice(i, i + cap);
    volumes.push({ from: slice[0], to: slice[slice.length - 1], n: volumes.length + 1 });
  }
  return (
    <div className="flex flex-wrap gap-1.5 pt-1 border-t border-border/50">
      {volumes.map((v) => (
        <Button key={v.n} size="sm" variant="secondary" disabled={busy}
                onClick={() => onBuild(book.book, v.from, v.to, `Vol. ${v.n}`)}>
          Vol. {v.n} — chapters {v.from}–{v.to}
        </Button>
      ))}
    </div>
  );
}
