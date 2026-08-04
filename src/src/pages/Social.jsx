import { useCallback, useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Badge } from "../components/ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { toast } from "sonner";
import {
  Share2, UserCheck, Sparkles, Scissors, Send, RefreshCw, Trash2, Plug, Loader2,
  AlertTriangle, Check, Lightbulb, ExternalLink,
} from "lucide-react";
import GuidedPanel from "../components/GuidedPanel";
import { socialFlow } from "../lib/guidedFlows";

// Colour-codes what a platform can actually do for free, so nobody wires up TikTok expecting
// public posts on day one.
const TIER_LABEL = {
  open: { text: "Open API", className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
  connected: { text: "Already connected", className: "bg-sky-500/15 text-sky-400 border-sky-500/30" },
  review_gated: { text: "Needs app review", className: "bg-amber-500/15 text-amber-400 border-amber-500/30" },
  macro: { text: "Browser macro", className: "bg-muted text-muted-foreground border-border" },
};

// The vertical formats build_short knows how to cut, with the one fact per platform that changes
// what you would choose. These mirror `platform_spec` in commands/shorts.rs — the lengths are its,
// not guesses, and the TikTok one is the counter-intuitive case worth saying out loud.
const SHORT_PLATFORMS = [
  { id: "shorts", label: "YouTube Shorts", note: "Under 60s — the Shorts feed cap." },
  { id: "tiktok", label: "TikTok", note: "75s on purpose: under a minute, Creator Rewards pays nothing." },
  { id: "reels", label: "Instagram Reels", note: "Up to 89s; 75s by default." },
  { id: "facebook", label: "Facebook Reels", note: "Pays per view, but only past 10k followers." },
  { id: "snapchat", label: "Snapchat Spotlight", note: "Revenue share with a $100 payout floor." },
  { id: "pinterest", label: "Pinterest", note: "Worth cutting for traffic; it pays nothing per view." },
];

function Section({ icon: Icon, title, subtitle, children, action }) {
  return (
    <section className="rounded-lg border border-border bg-card p-4 space-y-3">
      <header className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-3">
          <Icon className="h-5 w-5 mt-0.5 text-primary shrink-0" />
          <div>
            <h2 className="font-semibold leading-tight">{title}</h2>
            {subtitle && <p className="text-xs text-muted-foreground mt-0.5 max-w-2xl">{subtitle}</p>}
          </div>
        </div>
        {action}
      </header>
      {children}
    </section>
  );
}

function PlatformCard({ platform, account, onConnect, onDisconnect, onVerify, busy }) {
  const [draft, setDraft] = useState({});
  const [open, setOpen] = useState(false);
  const tier = TIER_LABEL[platform.tier] || TIER_LABEL.macro;
  const connected = !!account;

  return (
    <div className="rounded border border-border p-3 space-y-2">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-medium text-sm">{platform.label}</span>
            <span className={`rounded border px-1.5 py-0.5 text-[10px] ${tier.className}`}>{tier.text}</span>
            {connected && account.ok && <Check className="h-3.5 w-3.5 text-emerald-500" />}
          </div>
          <p className="text-xs text-muted-foreground mt-0.5">{platform.cost}</p>
          {connected && account.info?.handle && (
            <p className="text-xs text-muted-foreground">Connected as {String(account.info.handle)}</p>
          )}
          {connected && account.last_error && (
            <p className="text-xs text-destructive flex items-start gap-1 mt-1">
              <AlertTriangle className="h-3 w-3 mt-0.5 shrink-0" />{account.last_error}
            </p>
          )}
        </div>
        <div className="flex gap-1 shrink-0">
          {connected && (
            <>
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => onVerify(platform.id)}>
                <RefreshCw className="h-3.5 w-3.5" />
              </Button>
              <Button size="sm" variant="ghost" onClick={() => onDisconnect(platform.id)}>
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </>
          )}
          {platform.fields.length > 0 && (
            <Button size="sm" variant="outline" onClick={() => setOpen((v) => !v)}>
              <Plug className="h-3.5 w-3.5 mr-1" />{connected ? "Edit" : "Connect"}
            </Button>
          )}
        </div>
      </div>

      {open && platform.fields.length > 0 && (
        <div className="space-y-2 pt-1">
          <p className="text-xs text-muted-foreground">{platform.help}</p>
          {platform.fields.map((f) => (
            <div key={f.key}>
              <label className="text-[10px] uppercase tracking-wider text-muted-foreground">{f.label}</label>
              <Input
                className="mt-0.5"
                type={f.secret ? "password" : "text"}
                placeholder={f.placeholder}
                defaultValue={f.secret ? "" : (account?.[f.key] || "")}
                onChange={(ev) => setDraft((d) => ({ ...d, [f.key]: ev.target.value }))}
              />
            </div>
          ))}
          <Button size="sm" disabled={busy} onClick={() => onConnect(platform.id, draft).then(() => setOpen(false))}>
            {busy ? <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" /> : <Check className="h-3.5 w-3.5 mr-1" />}
            Save &amp; verify
          </Button>
        </div>
      )}
    </div>
  );
}

export default function Social() {
  const { activeProjectId, activeSongId, songs } = useStudio();
  const [platforms, setPlatforms] = useState([]);
  // The guided answers: what these posts are for, where they go, and whether to draft now.
  const [guidedKind, setGuidedKind] = useState("announce");
  const [guidedPlatforms, setGuidedPlatforms] = useState([]);
  const [guidedDrafting, setGuidedDrafting] = useState(false);
  const [accounts, setAccounts] = useState([]);
  const [profile, setProfile] = useState(null);
  const [ideas, setIdeas] = useState(null);
  const [question, setQuestion] = useState("");
  const [derivatives, setDerivatives] = useState([]);
  const [targets, setTargets] = useState(() => new Set(["mastodon"]));
  const [shortPlatform, setShortPlatform] = useState("shorts");
  const [busy, setBusy] = useState("");

  const song = songs?.find((s) => s.id === activeSongId);

  const refresh = useCallback(async () => {
    try { setPlatforms(await api.listSocialPlatforms()); } catch (e) { toast.error(String(e)); }
    try { setAccounts(await api.listSocialAccounts()); } catch { setAccounts([]); }
    try {
      const learnings = await api.getUserLearnings();
      setProfile(learnings?.taste_profile || null);
    } catch { setProfile(null); }
  }, []);

  const refreshDerivatives = useCallback(async () => {
    if (!activeSongId) return setDerivatives([]);
    try { setDerivatives(await api.listDerivatives(activeSongId)); } catch { setDerivatives([]); }
  }, [activeSongId]);

  useEffect(() => { refresh(); }, [refresh]);
  useEffect(() => { refreshDerivatives(); }, [refreshDerivatives]);

  const run = async (name, fn, ok) => {
    setBusy(name);
    try {
      const res = await fn();
      if (ok) toast.success(typeof ok === "function" ? ok(res) : ok);
      return res;
    } catch (err) {
      toast.error(String(err));
      throw err;
    } finally {
      setBusy("");
    }
  };

  const accountFor = (id) => accounts.find((a) => a.platform === id);
  const connectedIds = accounts.map((a) => a.platform);

  const toggleTarget = (id) => {
    setTargets((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  return (
    <div className="p-6 space-y-4 max-w-5xl">
      <div>
        <h1 className="text-2xl font-semibold flex items-center gap-2">
          <Share2 className="h-6 w-6 text-primary" /> Social presence
        </h1>
        <p className="text-sm text-muted-foreground mt-1">
          Learn what you sound like, use it to decide what to make next, cut each song into
          platform-sized versions, and publish them. Nothing is posted without you pressing publish.
        </p>
      </div>

      <GuidedPanel
        flow={socialFlow}
        projectId={activeProjectId}
        extraCtx={{ accounts }}
        actions={{
          setKind: (k) => setGuidedKind(k),
          setPlatforms: (list) => setGuidedPlatforms(list),
          openConnect: () => {},
          draft: () => setGuidedDrafting(true),
        }}
      />

      {/* A. Accounts */}
      <Section
        icon={Plug}
        title="Accounts"
        subtitle="Credentials go into the encrypted vault, never into project files. Platforms are grouped by what a free account can actually do."
      >
        <div className="grid gap-2 sm:grid-cols-2">
          {platforms.map((p) => (
            <PlatformCard
              key={p.id}
              platform={p}
              account={accountFor(p.id)}
              busy={busy === `connect-${p.id}` || busy === `verify-${p.id}`}
              onConnect={(id, fields) =>
                run(`connect-${id}`, () => api.connectSocialAccount(id, fields), "Connected").then(refresh)}
              onVerify={(id) => run(`verify-${id}`, () => api.verifySocialAccount(id), "Verified").then(refresh)}
              onDisconnect={(id) => run("disconnect", () => api.disconnectSocialAccount(id), "Disconnected").then(refresh)}
            />
          ))}
        </div>
      </Section>

      {/* A/B. Taste profile */}
      <Section
        icon={UserCheck}
        title="Your creator profile"
        subtitle="Built from your own posts on the platforms with open APIs, then fed into the composer and the ideation partner so suggestions sound like you."
        action={
          <div className="flex gap-2">
            <Button size="sm" variant="outline" disabled={busy === "ingest"} onClick={() =>
              run("ingest", () => api.ingestSocialActivity(50), (r) => `Read ${r?.ingested ?? 0} post(s)`).then(refresh)}>
              {busy === "ingest" ? <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5 mr-1" />}
              Refresh from platforms
            </Button>
            <Button size="sm" disabled={busy === "profile"} onClick={() =>
              run("profile", () => api.refreshTasteProfile(), "Profile rebuilt").then(refresh)}>
              <Sparkles className="h-3.5 w-3.5 mr-1" /> Rebuild profile
            </Button>
          </div>
        }
      >
        {profile ? (
          <div className="grid gap-2 sm:grid-cols-2 text-xs">
            {[["voice", "Voice"], ["themes", "Themes"], ["audience", "Audience"], ["performs", "What performs"], ["avoid", "Avoid"]].map(([k, label]) =>
              profile[k] ? (
                <div key={k} className="rounded bg-muted/40 p-2">
                  <div className="text-[10px] uppercase tracking-wider text-muted-foreground">{label}</div>
                  <div className="mt-0.5">{profile[k]}</div>
                </div>
              ) : null
            )}
            {profile.generated_at && (
              <p className="text-[11px] text-muted-foreground sm:col-span-2">
                Built {new Date(profile.generated_at).toLocaleString()} from {profile.from_posts} post(s).
              </p>
            )}
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">
            No profile yet. Connect Mastodon or Bluesky above, press &ldquo;Refresh from platforms&rdquo;, then &ldquo;Rebuild profile&rdquo;.
          </p>
        )}
      </Section>

      {/* B. Ideation */}
      <Section icon={Lightbulb} title="What should I make next?" subtitle="Reasons over your profile, your learnings and what this project already produced.">
        <div className="flex gap-2">
          <Input
            placeholder="Optional: ask something specific (e.g. something for Sunday evening)"
            value={question}
            onChange={(ev) => setQuestion(ev.target.value)}
          />
          <Button size="sm" disabled={busy === "ideate"} onClick={() =>
            run("ideate", () => api.ideateNext(activeProjectId, question)).then(setIdeas)}>
            {busy === "ideate" ? <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" /> : <Lightbulb className="h-3.5 w-3.5 mr-1" />}
            Ideate
          </Button>
        </div>
        {ideas?.ideas?.length > 0 && (
          <div className="grid gap-2 sm:grid-cols-2">
            {ideas.ideas.map((idea, i) => (
              <div key={i} className="rounded bg-muted/40 p-2 text-xs space-y-1">
                <div className="font-medium text-sm">{idea.title}</div>
                <div>{idea.angle}</div>
                {idea.why_now && <div className="text-muted-foreground">Why now: {idea.why_now}</div>}
                <div className="flex flex-wrap gap-1 pt-0.5">
                  {idea.styles && <Badge variant="secondary" className="text-[10px]">{idea.styles}</Badge>}
                  {idea.platforms && <Badge variant="outline" className="text-[10px]">{idea.platforms}</Badge>}
                </div>
              </div>
            ))}
          </div>
        )}
        {ideas?.note && <p className="text-xs text-muted-foreground">{ideas.note}</p>}
      </Section>

      {/* C/D. Derivatives */}
      <Section
        icon={Scissors}
        title="Versions for other platforms"
        subtitle={song ? `Cut "${song.title}" into a vertical short, an image post and a teaser.` : "Select a song (top bar) to derive versions of it."}
      >
        {song && (
          <>
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs text-muted-foreground">Targets:</span>
              {platforms.filter((p) => p.tier !== "connected").map((p) => (
                <button
                  key={p.id}
                  onClick={() => toggleTarget(p.id)}
                  className={`px-2 py-1 rounded text-xs transition-colors ${
                    targets.has(p.id) ? "bg-primary text-primary-foreground" : "bg-muted hover:bg-secondary text-muted-foreground"
                  }`}
                  title={connectedIds.includes(p.id) ? "Connected" : "Not connected — you can still prepare the version"}
                >
                  {p.label}
                </button>
              ))}
            </div>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" disabled={busy === "derive" || targets.size === 0} onClick={() =>
                run("derive", () => api.deriveSongVersions(song.id, [...targets]),
                  (r) => `Prepared ${(r?.created || []).length} version(s)`).then(refreshDerivatives)}>
                {busy === "derive" ? <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" /> : <Scissors className="h-3.5 w-3.5 mr-1" />}
                Derive versions
              </Button>
              <Button size="sm" variant="outline" disabled={busy === "publishAll" || derivatives.length === 0} onClick={() => {
                if (!window.confirm("Publish every draft version of this song to its platform?")) return;
                run("publishAll", () => api.publishAllDerivatives(song.id),
                  (r) => `${(r?.published || []).length} published, ${(r?.failed || []).length} failed`).then(refreshDerivatives);
              }}>
                <Send className="h-3.5 w-3.5 mr-1" /> Publish all drafts
              </Button>
            </div>

            {/* "Derive versions" cuts a short with its own simple helper: the platform's window,
                taken from the top. `build_short` is the better one and had no caller — it finds the
                *hook* (the chorus if the analysis found one, else the peak-mood section, else a third
                of the way in) and burns a "full video" card into the closing seconds. One button
                each rather than replacing the other, because the cheap cut is still the right answer
                when a song has no analysis to reason about. */}
            <div className="flex flex-wrap items-center gap-2 pt-2 border-t border-border/50">
              <span className="text-xs text-muted-foreground">Hook cut:</span>
              <Select value={shortPlatform} onValueChange={setShortPlatform}>
                <SelectTrigger className="h-8 w-40 text-xs"><SelectValue /></SelectTrigger>
                <SelectContent>
                  {SHORT_PLATFORMS.map((p) => (
                    <SelectItem key={p.id} value={p.id}>{p.label}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button size="sm" variant="secondary" disabled={busy === "hook"} onClick={() =>
                run("hook", () => api.buildShort({ song_id: song.id, platform: shortPlatform }),
                  (r) => `Cut ${r?.seconds ? `${Math.round(r.seconds)}s` : "a short"} from ${r?.hook_start != null ? `${Math.round(r.hook_start)}s in` : "the hook"}`)
                  .then(refreshDerivatives)}>
                {busy === "hook" ? <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" /> : <Scissors className="h-3.5 w-3.5 mr-1" />}
                Cut from the hook
              </Button>
              <span className="text-[11px] text-muted-foreground">
                Needs a rendered video. {SHORT_PLATFORMS.find((p) => p.id === shortPlatform)?.note}
              </span>
            </div>

            <div className="space-y-1">
              {derivatives.map((d) => (
                <div key={d.id} className="rounded border border-border p-2 space-y-1">
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2 text-xs">
                      <Badge variant="secondary" className="text-[10px]">{d.platform}</Badge>
                      <Badge variant="outline" className="text-[10px]">{d.kind}</Badge>
                      <span className={
                        d.status === "published" ? "text-emerald-500"
                          : d.status === "failed" ? "text-destructive" : "text-muted-foreground"
                      }>{d.status}</span>
                      {d.published_url && (
                        <a href={d.published_url} target="_blank" rel="noreferrer" className="text-primary hover:underline inline-flex items-center gap-0.5">
                          view <ExternalLink className="h-3 w-3" />
                        </a>
                      )}
                    </div>
                    <div className="flex gap-1">
                      <Button size="sm" variant="ghost" disabled={busy === `pub-${d.id}` || d.status === "published"} onClick={() =>
                        run(`pub-${d.id}`, () => api.publishDerivative(d.id), "Published").then(refreshDerivatives)}>
                        <Send className="h-3.5 w-3.5" />
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() =>
                        run("del", () => api.deleteDerivative(d.id), "Removed").then(refreshDerivatives)}>
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  </div>
                  <Textarea
                    className="text-xs min-h-[60px]"
                    defaultValue={d.caption}
                    onBlur={(ev) => api.updateDerivative(d.id, { caption: ev.target.value }).catch((e) => toast.error(String(e)))}
                  />
                  {d.media_path && <p className="text-[11px] font-mono text-muted-foreground truncate">{d.media_path}</p>}
                  {d.last_error && (
                    <p className="text-[11px] text-destructive flex items-start gap-1">
                      <AlertTriangle className="h-3 w-3 mt-0.5 shrink-0" />{d.last_error}
                    </p>
                  )}
                </div>
              ))}
            </div>
          </>
        )}
      </Section>
    </div>
  );
}
