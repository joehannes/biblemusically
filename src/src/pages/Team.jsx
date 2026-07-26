import { useEffect, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import {
  Users, UserPlus, Share2, GitMerge, History, Loader2, Trash2, AlertTriangle,
  RotateCcw, Copy, LogIn, LogOut, ShieldAlert,
} from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// Working with other people.
//
// Sharing is a shared git remote: the project already *is* a repo, so two people pulling and pushing it
// is the mechanism rather than a bolt-on. Identity is a social sign-in plus a username — no password is
// stored here at all.
//
// The limit is stated on this page rather than buried: **roles are advisory.** Anyone with push access
// to the remote can push anything, whatever this app shows. Enforcement is the host's job (GitHub teams,
// branch protection); what the app does is stop its own buttons from doing what your role does not
// cover, which prevents accidents and not adversaries.
//
// Merging is three-way on the JSON, so two people editing different fields are never asked anything, and
// nobody's afternoon is silently overwritten. Rewinding writes a new commit rather than rewriting
// history — destruction stays visible, which is what makes it discussable.
// ─────────────────────────────────────────────────────────────────────────────

const ROLE_NOTE = {
  owner: "Everything, including members and deleting the project.",
  editor: "Read and write. Cannot manage members or delete the project.",
  viewer: "Read, pull and export only.",
};

/**
 * Moving channel sign-ins between devices.
 *
 * A phone cannot use the desktop's loopback OAuth redirect, and creating an Android OAuth client is a
 * trip through the Google console. So the default is simply to carry the tokens across: a refresh token
 * is exactly the thing designed to be stored and reused.
 *
 * Sealed with a passphrase, because a refresh token in plain text in a chat message is a token somebody
 * else can use for a year.
 */
function DeviceTransfer() {
  const [caps, setCaps] = useState(null);
  const [pass, setPass] = useState("");
  const [blob, setBlob] = useState("");
  const [incoming, setIncoming] = useState("");
  const [busy, setBusy] = useState("");

  useEffect(() => { api.platformCapabilities().then(setCaps).catch(() => {}); }, []);

  return (
    <Card className="p-4 space-y-3">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <h2 className="font-medium">Sign-ins on another device</h2>
        {caps && (
          <Badge variant="outline" className="text-[10px]">
            {caps.desktop ? "desktop · loopback sign-in" : `phone · ${caps.oauth_mode} sign-in`}
          </Badge>
        )}
      </div>
      <p className="text-xs text-muted-foreground">
        A phone cannot use the desktop's loopback redirect. The simplest route is to carry the channel
        sign-ins across — a refresh token stays valid until it is revoked, so nothing has to be
        re-consented. Sealed with a passphrase, so it is safe to send through a normal chat.
      </p>

      <Input type="password" value={pass} onChange={(e) => setPass(e.target.value)}
             placeholder="a passphrase, at least 8 characters" className="text-sm" />

      <div className="grid sm:grid-cols-2 gap-3">
        <div className="space-y-1.5">
          <div className="text-[10px] uppercase tracking-widest text-muted-foreground">Export from here</div>
          <Button size="sm" variant="secondary" disabled={busy === "exp" || pass.trim().length < 8}
                  onClick={async () => {
                    setBusy("exp");
                    try {
                      const r = await api.exportChannelTokens(pass);
                      setBlob(r.blob);
                      toast.success(`${r.channels} sign-in(s) sealed.`);
                      toast.warning(r.caveat, { duration: 11000 });
                    } catch (err) { toast.error(`${err}`, { duration: 9000 }); }
                    finally { setBusy(""); }
                  }}>
            {busy === "exp" ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : null}
            Seal my sign-ins
          </Button>
          {blob && (
            <>
              <Textarea readOnly value={blob} className="text-[10px] h-20 font-mono" data-no-i18n />
              <Button size="sm" variant="ghost" onClick={() => {
                navigator.clipboard.writeText(blob); toast.success("Copied.");
              }}><Copy className="w-3.5 h-3.5 mr-1.5" />Copy</Button>
            </>
          )}
        </div>

        <div className="space-y-1.5">
          <div className="text-[10px] uppercase tracking-widest text-muted-foreground">Bring them here</div>
          <Textarea value={incoming} onChange={(e) => setIncoming(e.target.value)}
                    placeholder="bmstudio-sealed:…" className="text-[10px] h-20 font-mono" data-no-i18n />
          <Button size="sm" disabled={busy === "imp" || !incoming.trim() || pass.trim().length < 8}
                  onClick={async () => {
                    setBusy("imp");
                    try {
                      const r = await api.importChannelTokens({ blob: incoming.trim(), passphrase: pass });
                      toast.success(`${r.imported} channel(s) can upload from this device now.`);
                      setIncoming("");
                    } catch (err) { toast.error(`${err}`, { duration: 9000 }); }
                    finally { setBusy(""); }
                  }}>
            Import
          </Button>
        </div>
      </div>

      {caps && !caps.desktop && (
        <p className="text-[11px] text-muted-foreground">
          {caps.android_client_configured
            ? `This phone can also sign in on its own — redirect ${caps.deeplink_redirect}.`
            : "This phone can sign in on its own once an Android OAuth client is set up (Settings → Mobile sign-in). Until then, carrying sign-ins across is the way."}
        </p>
      )}
      {caps && (
        <div className="text-[11px] text-muted-foreground space-y-0.5">
          {!caps.local_cli && <div>· {caps.local_cli_note}</div>}
          {!caps.hidden_webview && <div>· {caps.hidden_webview_note}</div>}
        </div>
      )}
    </Card>
  );
}

export default function Team() {
  const { activeProjectId, projects } = useStudio();
  const [me, setMe] = useState({ signed_in: false });
  const [username, setUsername] = useState("");
  const [email, setEmail] = useState("");
  const [members, setMembers] = useState([]);
  const [role, setRole] = useState(null);
  const [who, setWho] = useState("");
  const [newRole, setNewRole] = useState("editor");
  const [invite, setInvite] = useState("");
  const [incoming, setIncoming] = useState("");
  const [folder, setFolder] = useState("");
  const [conflicts, setConflicts] = useState([]);
  const [history, setHistory] = useState([]);
  const [busy, setBusy] = useState("");

  const project = projects?.find((p) => p.id === activeProjectId);

  const load = async () => {
    try { setMe(await api.currentUser()); } catch { /* not signed in */ }
    if (!activeProjectId) return;
    try {
      const [m, r] = await Promise.all([
        api.listProjectMembers(activeProjectId),
        api.myProjectRole(activeProjectId),
      ]);
      setMembers(m?.members || []);
      setRole(r);
    } catch { /* single-user install */ }
  };
  useEffect(() => { load(); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [activeProjectId]);

  const act = async (name, fn, after) => {
    setBusy(name);
    try { const r = await fn(); after?.(r); }
    catch (err) { toast.error(`${err}`, { duration: 10000 }); }
    finally { setBusy(""); }
  };

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-xl font-semibold flex items-center gap-2">
          <Users className="w-5 h-5 text-primary" /> Team
        </h1>
        <p className="text-sm text-muted-foreground">
          Share a project through a git remote, merge each other's work, and rewind anything.
        </p>
      </div>

      {/* ── Who you are ──────────────────────────────────────────────────── */}
      <Card className="p-4 space-y-3">
        <h2 className="font-medium">You</h2>
        {me.signed_in ? (
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="text-sm">
              <b>{me.user?.username}</b>
              <span className="text-muted-foreground"> · {me.user?.email} · via {me.user?.provider}</span>
            </div>
            <Button size="sm" variant="secondary" disabled={busy === "out"}
                    onClick={() => act("out", () => api.signOut(), () => { setMe({ signed_in: false }); toast.success("Signed out."); })}>
              <LogOut className="w-3.5 h-3.5 mr-1.5" />Sign out
            </Button>
          </div>
        ) : (
          <div className="space-y-2">
            <p className="text-xs text-muted-foreground">
              Sign in with the Google account you already use for YouTube, and pick a username the team
              will see. No password is stored — not hashed, none. One less thing to lose.
            </p>
            <div className="grid sm:grid-cols-2 gap-2">
              <Input value={email} onChange={(e) => setEmail(e.target.value)}
                     placeholder="your Google address" />
              <Input value={username} onChange={(e) => setUsername(e.target.value)}
                     placeholder="username the team sees" />
            </div>
            <Button size="sm" disabled={busy === "in" || !email.trim() || username.trim().length < 2}
                    onClick={() => act("in",
                      () => api.signIn({ provider: "google", email: email.trim(), username: username.trim() }),
                      () => { load(); toast.success("Signed in."); })}>
              <LogIn className="w-3.5 h-3.5 mr-1.5" />Sign in
            </Button>
          </div>
        )}
      </Card>

      {/* ── Bring sign-ins to another device ─────────────────────────────── */}
      <DeviceTransfer />

      {!activeProjectId ? (
        <Card className="p-4 text-sm text-muted-foreground">Pick a project to share it.</Card>
      ) : (
        <>
          {/* ── Roles ────────────────────────────────────────────────────── */}
          <Card className="p-4 space-y-3">
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <h2 className="font-medium">Who can work on {project?.name || "this project"}</h2>
              {role && <Badge variant="outline" className="text-[10px]">you are {role.role}</Badge>}
            </div>

            <div className="text-xs text-amber-400 flex items-start gap-1.5">
              <ShieldAlert className="w-3.5 h-3.5 mt-0.5 shrink-0" />
              <span>
                These roles are what this app shows and what it stops its own buttons from doing. They
                cannot stop somebody who has push access to the remote — that is the git host's job
                (GitHub teams, branch protection). Said plainly because a role that looks enforced and
                is not would be worse than none.
              </span>
            </div>

            <div className="flex gap-2 flex-wrap">
              <Input value={who} onChange={(e) => setWho(e.target.value)}
                     placeholder="their email or username" className="flex-1 min-w-[14rem]" />
              <select value={newRole} onChange={(e) => setNewRole(e.target.value)}
                      className="bg-muted/30 border border-border rounded px-2 text-sm">
                <option value="owner">owner</option>
                <option value="editor">editor</option>
                <option value="viewer">viewer</option>
              </select>
              <Button size="sm" disabled={busy === "add" || !who.trim()}
                      onClick={() => act("add",
                        () => api.addProjectMember({ project_id: activeProjectId, who: who.trim(), role: newRole }),
                        () => { setWho(""); load(); toast.success("Added."); })}>
                <UserPlus className="w-3.5 h-3.5 mr-1.5" />Add
              </Button>
            </div>
            <div className="text-[11px] text-muted-foreground">{ROLE_NOTE[newRole]}</div>

            {members.map((m) => (
              <div key={m.id} className="flex items-center justify-between gap-2 rounded-lg border border-border/60 p-2">
                <div className="text-sm">{m.who} <span className="text-muted-foreground">· {m.role}</span></div>
                <Button size="sm" variant="ghost"
                        onClick={() => act(`rm-${m.id}`,
                          () => api.removeProjectMember(activeProjectId, m.who), load)}>
                  <Trash2 className="w-3.5 h-3.5" />
                </Button>
              </div>
            ))}
          </Card>

          {/* ── Invite ───────────────────────────────────────────────────── */}
          <Card className="p-4 space-y-2">
            <h2 className="font-medium">Send an invitation</h2>
            <p className="text-xs text-muted-foreground">
              One line, containing the repo and the role. There is no server to be down — accepting it is
              a clone.
            </p>
            <div className="flex gap-2 flex-wrap">
              <select value={newRole} onChange={(e) => setNewRole(e.target.value)}
                      className="bg-muted/30 border border-border rounded px-2 py-1 text-sm">
                <option value="editor">as editor</option>
                <option value="viewer">as viewer</option>
                <option value="owner">as owner</option>
              </select>
              <Button size="sm" disabled={busy === "share"}
                      onClick={() => act("share", () => api.shareProject(activeProjectId, newRole),
                        (r) => { setInvite(r.invite); toast.success("Invitation ready — copy it to them."); })}>
                <Share2 className="w-3.5 h-3.5 mr-1.5" />Create
              </Button>
              {invite && (
                <Button size="sm" variant="secondary" onClick={() => {
                  navigator.clipboard.writeText(invite);
                  toast.success("Copied.");
                }}>
                  <Copy className="w-3.5 h-3.5 mr-1.5" />Copy
                </Button>
              )}
            </div>
            {invite && <Textarea readOnly value={invite} className="text-xs h-16 font-mono" data-no-i18n />}
          </Card>

          {/* ── Accept ───────────────────────────────────────────────────── */}
          <Card className="p-4 space-y-2">
            <h2 className="font-medium">Accept an invitation</h2>
            <Textarea value={incoming} onChange={(e) => setIncoming(e.target.value)}
                      placeholder="bmstudio-invite:…" className="text-xs h-16 font-mono" data-no-i18n />
            <Input value={folder} onChange={(e) => setFolder(e.target.value)}
                   placeholder="an empty folder to clone into" className="text-sm" />
            <Button size="sm" disabled={busy === "accept" || !incoming.trim() || !folder.trim()}
                    onClick={() => act("accept",
                      () => api.acceptInvite({ invite: incoming.trim(), folder: folder.trim() }),
                      (r) => { setIncoming(""); toast.success(`Cloned ${r.project?.name}. ${r.note}`, { duration: 9000 }); })}>
              Clone the shared project
            </Button>
          </Card>

          {/* ── Pull and merge ───────────────────────────────────────────── */}
          <Card className="p-4 space-y-2">
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <h2 className="font-medium">Pull their changes</h2>
              <Button size="sm" disabled={busy === "pull"}
                      onClick={() => act("pull", () => api.teamPull(activeProjectId), (r) => {
                        setConflicts(r.conflicts || []);
                        if (r.ok) toast.success("Merged cleanly.");
                        else toast.warning(`${r.conflicts?.length || 0} file(s) need settling. Your work was committed first, so nothing is lost.`, { duration: 10000 });
                      })}>
                {busy === "pull" ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : <GitMerge className="w-3.5 h-3.5 mr-1.5" />}
                Pull
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              Your work is committed before the pull, always — pulling over uncommitted changes is how
              people lose an afternoon.
            </p>
            {conflicts.map((f) => (
              <div key={f} className="rounded-lg border border-amber-500/40 p-2 space-y-1">
                <div className="text-xs font-mono break-all" data-no-i18n>{f}</div>
                <div className="flex gap-1.5 flex-wrap">
                  {[
                    ["merge", "Merge field by field"],
                    ["mine", "Keep mine"],
                    ["theirs", "Keep theirs"],
                  ].map(([how, label]) => (
                    <Button key={how} size="sm" variant={how === "merge" ? "default" : "secondary"}
                            className="h-6 px-2 text-[10px]"
                            onClick={() => act(`res-${f}-${how}`,
                              () => api.resolveConflict({ project_id: activeProjectId, file: f, how }),
                              (r) => {
                                setConflicts((c) => c.filter((x) => x !== f));
                                if (r.field_conflicts?.length) {
                                  toast.warning(`${r.note} Fields: ${r.field_conflicts.join(", ")}`, { duration: 12000 });
                                } else toast.success(r.note || "Settled.");
                              })}>
                      {label}
                    </Button>
                  ))}
                </div>
              </div>
            ))}
            {conflicts.length === 0 && (
              <Button size="sm" variant="secondary" disabled={busy === "finish"}
                      onClick={() => act("finish", () => api.finishMerge(activeProjectId),
                        () => toast.success("Merge committed."))}>
                Finish the merge
              </Button>
            )}
          </Card>

          {/* ── History and rewind ───────────────────────────────────────── */}
          <Card className="p-4 space-y-2">
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <h2 className="font-medium">History</h2>
              <Button size="sm" variant="secondary" disabled={busy === "hist"}
                      onClick={() => act("hist", () => api.projectHistory(activeProjectId, 40),
                        (r) => setHistory(r.commits || []))}>
                <History className="w-3.5 h-3.5 mr-1.5" />Load
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              Rewinding writes a <b>new</b> commit holding the old files. Nothing is rewritten, so what
              you rewound from is still there — which is what makes it possible to talk about.
            </p>
            {history.map((c) => (
              <div key={c.hash} className="flex items-center justify-between gap-2 text-xs rounded border border-border/50 p-1.5">
                <div className="min-w-0">
                  <span className="font-mono" data-no-i18n>{c.hash}</span>
                  <span className="text-muted-foreground"> · {c.author} · {c.when}</span>
                  <div className="truncate">{c.subject}</div>
                </div>
                <Button size="sm" variant="ghost" className="h-6 px-2 text-[10px] shrink-0"
                        onClick={() => act(`rw-${c.hash}`,
                          () => api.rewindProject(activeProjectId, c.hash),
                          (r) => toast.success(`Restored as ${r.restored_as || "a new commit"}. ${r.note}`, { duration: 10000 }))}>
                  <RotateCcw className="w-3 h-3 mr-1" />Rewind here
                </Button>
              </div>
            ))}
          </Card>
        </>
      )}
    </div>
  );
}
