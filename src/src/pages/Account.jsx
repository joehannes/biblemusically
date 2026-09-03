import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { subscribeEntitlement, refreshEntitlement } from "../lib/entitlement";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Input } from "../components/ui/input";
import {
  UserCircle, Clock, Monitor, LogOut, RefreshCw, Download, Share2, Copy,
  Loader2, ArrowUpCircle, MapPin, Eye, Lock, Unlock, KeyRound, ShieldAlert, ShieldCheck,
} from "lucide-react";
import { toast } from "sonner";
import { downloadAndInstallUpdate } from "../lib/updateInstall";
import { SubscribePrompt } from "../components/Paywall";
import LearningsPanel from "../components/LearningsPanel";

// ─────────────────────────────────────────────────────────────────────────────
// The account: what you have, what it covers, where it is signed in, and what is new.
//
// Everything here is a fact the user can act on. No usage graphs of their own behaviour, no engagement
// nudges — this is the page somebody opens when something is wrong or when they want to pay, and both of
// those deserve a short page.
//
// The device list is the interesting part. Three concurrent sign-ins is the limit, and a limit with no way
// out of it is a wall — so sessions expire after a week unused, and the list shows approximate cities
// because "Vienna, yesterday" is recognisable in a way a device id is not.
// ─────────────────────────────────────────────────────────────────────────────

export default function Account() {
  const [ent, setEnt] = useState(null);
  // The entitlement signing key. Minting is local — an Ed25519 pair out of the OS entropy source —
  // so replacing the leaked one is a button rather than a runbook. It only appears for whoever
  // publishes this app: a warning nobody can act on is alarm, not information.
  const [signing, setSigning] = useState(null);
  const [minted, setMinted] = useState(null);
  const [showPrivate, setShowPrivate] = useState(false);
  const [keyForm, setKeyForm] = useState("pkcs8");
  // A packaged release is somebody else's copy; a dev build is the publisher's own checkout, which
  // is also the only place `subscription.rs` can be edited. Vite folds this to `false` in a release
  // so the card never renders and the backend is never asked — the copy still ships in the bundle,
  // as any unrendered branch does, which is fine because it is text rather than a capability.
  const publisher = import.meta.env.DEV;
  const privateHalf = keyForm === "pkcs8"
    ? (minted?.private_key_pkcs8 || minted?.private_key || "")
    : (minted?.private_key || "");

  useEffect(() => {
    if (!import.meta.env.DEV) return;
    api.signingKeyStatus().then(setSigning).catch(() => setSigning(null));
  }, []);

  const copy = async (text, note) => {
    try { await navigator.clipboard.writeText(text); toast.success(note); }
    catch { toast.error("Could not reach the clipboard — select it and copy by hand."); }
  };

  const mintKey = async () => {
    setBusy("mint");
    try {
      const r = await api.mintSigningKey();
      setMinted(r);
      setSigning(await api.signingKeyStatus().catch(() => signing));
    } catch (err) { toast.error(`${err}`, { duration: 9000 }); }
    finally { setBusy(""); }
  };
  const [sessions, setSessions] = useState(null);
  const [update, setUpdate] = useState(null);
  const [referral, setReferral] = useState(null);
  const [cache, setCache] = useState(null);
  const [busy, setBusy] = useState("");

  useEffect(() => subscribeEntitlement(setEnt), []);
  useEffect(() => {
    api.checkUpdate().then(setUpdate).catch(() => {});
    api.subsReferral().then(setReferral).catch(() => {});
    api.subsCacheState().then(setCache).catch(() => {});
    loadSessions();
  }, []);

  const loadSessions = async () => {
    try { setSessions(await api.listSessions?.() ?? null); } catch { /* not signed in */ }
  };

  const act = async (name, fn, after) => {
    setBusy(name);
    try { const r = await fn(); after?.(r); }
    catch (err) { toast.error(`${err}`, { duration: 10000 }); }
    finally { setBusy(""); }
  };

  const status = ent?.status || "none";
  const paid = status === "active" || status === "lifetime";

  return (
    <div className="p-4 sm:p-6 lg:p-8 mx-auto fade-in space-y-4 max-w-2xl">
      <div>
        <h1 className="text-4xl sm:text-5xl font-bold flex items-center gap-3">
          <UserCircle className="w-5 h-5 text-primary" /> Account
        </h1>
        <p className="text-sm text-muted-foreground">What you have, where it is signed in, and what is new.</p>
      </div>

      {/* ── Where you stand ──────────────────────────────────────────────── */}
      <Card className="p-4 space-y-2">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <div>
            <div className="font-medium flex items-center gap-2">
              {ent?.email || "Not signed in"}
              <Badge variant="outline" className={`text-[10px] ${
                status === "lifetime" || status === "active" ? "text-emerald-400 border-emerald-500/40"
                  : status === "trial" ? "text-amber-400 border-amber-500/40" : ""}`}>
                {status === "lifetime" ? "yours for this version" : status}
              </Badge>
              {ent?.stale && (
                <Badge variant="outline" className="text-[10px] text-amber-400 border-amber-500/40">
                  offline — last known licence
                </Badge>
              )}
            </div>
            {ent?.days_left != null && (
              <div className="text-xs text-muted-foreground flex items-center gap-1 mt-0.5">
                <Clock className="w-3 h-3" />
                {ent.days_left} day{ent.days_left === 1 ? "" : "s"} left
                {status === "trial" ? " of the free week" : " on this period"}
              </div>
            )}
          </div>
          <div className="flex gap-1.5">
            <Button size="sm" variant="secondary" disabled={busy === "refresh"}
                    onClick={() => act("refresh", () => refreshEntitlement({ remote: true }),
                      () => toast.success("Checked."))}>
              <RefreshCw className="w-3.5 h-3.5 mr-1.5" />Check
            </Button>
            {ent?.signed_in && (
              <Button size="sm" variant="ghost" disabled={busy === "out"}
                      onClick={() => act("out", () => api.subsSignOut(), async () => {
                        await refreshEntitlement();
                        // Worth saying: signing out is not deleting. People assume the worst here.
                        toast.success("Signed out. Your projects stay exactly where they are.");
                      })}>
                <LogOut className="w-3.5 h-3.5 mr-1.5" />Sign out
              </Button>
            )}
          </div>
        </div>
        {status === "lifetime" && (
          <p className="text-xs text-muted-foreground">
            This licence covers the major version you bought and every update to it, for as long as the
            software exists. A future major version is a separate purchase — and this one will keep
            working when that happens.
          </p>
        )}
      </Card>

      {/* ── Subscribe / plans ────────────────────────────────────────────── */}
      {!paid && <SubscribePrompt ent={ent} compact />}

      {/* ── Updates ──────────────────────────────────────────────────────── */}
      {update && update.kind !== "none" && (
        <Card className={`p-4 space-y-2 ${update.kind === "upgrade" ? "border-primary/40" : ""}`}>
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-2">
              {update.kind === "upgrade"
                ? <ArrowUpCircle className="w-4 h-4 text-primary" />
                : <Download className="w-4 h-4 text-primary" />}
              <span className="font-medium">
                {update.kind === "update" ? `Version ${update.latest} is ready`
                  : update.kind === "upgrade" ? `Version ${update.latest} is out`
                  : "Updates"}
              </span>
              <span className="text-xs text-muted-foreground">you have {update.current}</span>
            </div>
            <div className="flex gap-1.5">
              {/* Same helper as the shell banner: on a phone this downloads *and* hands the file to
                  Android's installer, which is the half that used to be missing. */}
              {update.kind === "update" && (
                <Button size="sm" disabled={busy === "dl"}
                        onClick={() => act("dl", () => downloadAndInstallUpdate())}>
                  {busy === "dl" ? <Loader2 className="w-3.5 h-3.5 animate-spin mr-1.5" /> : null}
                  Update
                </Button>
              )}
              {update.kind === "upgrade" && (
                <>
                  <Button size="sm" onClick={() => toast.message(
                    "The upgrade is a separate purchase — the subscribe screen has the details.",
                    { duration: 9000 })}>
                    See what is new
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => act("dismiss",
                    () => api.dismissUpgrade(Number(String(update.latest).split(".")[0]) || 1),
                    () => { setUpdate({ ...update, kind: "none" }); toast.success("Won't mention it again."); })}>
                    <Eye className="w-3.5 h-3.5 mr-1.5" />Don't show again
                  </Button>
                </>
              )}
            </div>
          </div>
          <p className="text-xs text-muted-foreground">{update.message}</p>
          {update.image && (
            <img src={update.image} alt="" className="rounded-lg max-h-48 object-cover w-full" />
          )}
          {update.notes && (
            <details className="text-xs">
              <summary className="cursor-pointer text-muted-foreground">What changed</summary>
              <pre className="whitespace-pre-wrap mt-1.5 text-[11px]" data-no-i18n>{update.notes}</pre>
            </details>
          )}
        </Card>
      )}

      {/* ── Devices ──────────────────────────────────────────────────────── */}
      {sessions?.sessions && (
        <Card className="p-4 space-y-2">
          <div className="flex items-center justify-between gap-2">
            <h2 className="font-medium flex items-center gap-2">
              <Monitor className="w-4 h-4 text-primary" />
              Signed in on {sessions.sessions.length} of {sessions.max} devices
            </h2>
            {sessions.sessions.length > 1 && (
              <Button size="sm" variant="secondary" disabled={busy === "others"}
                      onClick={() => act("others", () => api.endOtherSessions?.(), () => {
                        loadSessions();
                        toast.success("Every other device signed out.");
                      })}>
                End the others
              </Button>
            )}
          </div>
          {sessions.sessions.map((s) => (
            <div key={s.device} className="flex items-center justify-between gap-2 rounded-lg border border-border/60 p-2">
              <div className="text-sm flex items-center gap-2">
                <MapPin className="w-3.5 h-3.5 text-muted-foreground" />
                {s.place}
                {s.this_device && <Badge variant="outline" className="text-[9px]">this one</Badge>}
                <span className="text-xs text-muted-foreground">
                  last used {new Date(s.last_seen).toLocaleDateString()}
                </span>
              </div>
              {!s.this_device && (
                <Button size="sm" variant="ghost" className="h-6 px-2 text-[10px]"
                        onClick={() => act(`end-${s.device}`, () => api.endSession?.(s.device), () => {
                          loadSessions();
                          toast.success("That device is signed out.");
                        })}>
                  End
                </Button>
              )}
            </div>
          ))}
          <p className="text-[11px] text-muted-foreground">
            A device that has not been used for {sessions.stale_after_days} days frees its slot on its own,
            so three reinstalls can never lock you out of your own licence.
          </p>
        </Card>
      )}

      {/* ── The sealed cache ─────────────────────────────────────────────── */}
      {cache && (
        <Card className="p-4 space-y-2">
          <h2 className="font-medium flex items-center gap-2">
            {cache.active ? <Lock className="w-4 h-4 text-primary" />
                          : <Unlock className="w-4 h-4 text-muted-foreground" />}
            Your projects on this machine
          </h2>
          <p className="text-xs text-muted-foreground">{cache.explanation}</p>
          <label className="flex items-start gap-2 text-sm cursor-pointer">
            <input type="checkbox" className="mt-1 accent-primary"
                   checked={!!cache.sealing}
                   disabled={busy === "seal" || !ent?.signed_in}
                   onChange={(ev) => act("seal",
                     () => api.subsSealProjects(ev.target.checked),
                     async (r) => {
                       setCache(await api.subsCacheState());
                       toast.success(ev.target.checked
                         ? `Sealed — ${r.files} file${r.files === 1 ? "" : "s"} rewritten.`
                         : `Unsealed — ${r.files} file${r.files === 1 ? "" : "s"} are readable again.`);
                     })} />
            <span>
              <b>Encrypt my project files.</b>
              <span className="text-muted-foreground"> The key comes from this account, so a copy of the
              folder opens nowhere else. Turning it off writes them back in plain text — it is a choice
              you can reverse, not a door that locks behind you.</span>
            </span>
          </label>
          {cache.sealing && (
            <p className="text-[11px] text-muted-foreground">
              Sealed files are still committed and synced normally; the same account on another machine
              opens them. What you give up is a readable git diff of your own data.
            </p>
          )}
        </Card>
      )}

      {/* ── Tell a friend ────────────────────────────────────────────────── */}
      {referral?.code && (
        <Card className="p-4 space-y-2">
          <h2 className="font-medium flex items-center gap-2">
            <Share2 className="w-4 h-4 text-primary" />Tell a friend
          </h2>
          <p className="text-xs text-muted-foreground">
            They get the same free week — nothing is taken from them for arriving this way, and you show up
            in the stats as the reason.
          </p>
          <div className="flex gap-2">
            <Input readOnly value={referral.share_url} className="text-xs font-mono" data-no-i18n />
            <Button size="sm" variant="secondary" onClick={() => {
              navigator.clipboard.writeText(referral.share_url);
              toast.success("Copied.");
            }}><Copy className="w-3.5 h-3.5" /></Button>
          </div>
        </Card>
      )}

      {/* ── What it has worked out about you ─────────────────────────────── */}
      {/* Beside the analytics card on purpose: both are "what this app knows about you", and the
          one that shapes your output deserves at least as much visibility as the one that counts
          your clicks. */}
      <LearningsPanel />

      {/* ── The signing key ──────────────────────────────────────────────── */}
      {/* Only in a source build. Which key a release trusts is not a user's business, and a warning
          nobody can act on is alarm rather than information — the person who can act on it is the
          one running from a checkout, which is also the only place the edit it asks for can be made. */}
      {publisher && signing?.compromised && (
        <Card className="p-4 space-y-3 border-destructive/40">
          <div className="flex items-start gap-2">
            <ShieldAlert className="w-4 h-4 text-destructive mt-0.5 shrink-0" />
            <div className="min-w-0">
              <h2 className="font-medium">This build still trusts the leaked signing key</h2>
              <p className="text-xs text-muted-foreground">
                Its private half was committed to the repository in v0.88.0 and is still reachable in
                the history. Anyone who has ever cloned it can mint themselves a lifetime entitlement
                that this app believes. Replacing it is one click here, and one edit.
              </p>
            </div>
          </div>

          {!minted && (
            <>
              <Button size="sm" onClick={mintKey} disabled={busy === "mint"}>
                {busy === "mint" ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                                 : <KeyRound className="w-3.5 h-3.5 mr-1.5" />}
                Mint a new key on this machine
              </Button>
              <p className="text-[11px] text-muted-foreground">
                Made here, not fetched: no network, no account, nothing to install. The private half
                goes into this app's encrypted vault and the public half is shown once, to publish.
                It has to be minted on the machine that will sign with it — a public key whose
                private half lives somewhere that no longer exists is worse than a leaked one,
                because nothing can sign for it and every entitlement stops verifying.
              </p>
            </>
          )}

          {minted && (
            <div className="space-y-2.5">
              <p className="text-xs text-emerald-600 dark:text-emerald-400">
                Minted, and checked against itself: a token was signed with the private half and
                verified with the public half before either was shown.
              </p>

              <div className="space-y-1">
                <div className="text-[10px] uppercase tracking-widest text-muted-foreground">
                  Paste this into SUBS_PUBLIC_KEYS, replacing the one line that is there
                </div>
                <pre className="text-[10px] font-mono whitespace-pre-wrap rounded bg-muted/50 p-2 select-all">{minted.code}</pre>
                <Button size="sm" variant="secondary" onClick={() => copy(minted.code, "Snippet copied.")}>
                  <Copy className="w-3.5 h-3.5 mr-1.5" />Copy the snippet
                </Button>
              </div>

              <div className="space-y-1">
                <div className="text-[10px] uppercase tracking-widest text-muted-foreground">
                  The private half — for whatever signs entitlements. Shown once.
                </div>
                {/* PKCS#8 by default: that is the encoding WebCrypto's importKey takes, and it is
                    what the signing Worker loads. The bare seed is there for anything that wants
                    raw bytes — handing over only one of the two is how somebody ends up with a key
                    their own server cannot load. */}
                <div className="flex gap-1">
                  {[["pkcs8", "PKCS#8"], ["raw", "raw seed"]].map(([id, label]) => (
                    <button key={id} onClick={() => setKeyForm(id)}
                            className={`text-[10px] rounded border px-1.5 py-0.5 transition-colors ${
                              keyForm === id ? "border-primary bg-primary/10 text-primary"
                                             : "border-border text-muted-foreground hover:border-primary/40"}`}>
                      {label}
                    </button>
                  ))}
                </div>
                <div className="flex items-center gap-1.5">
                  <Input readOnly value={showPrivate ? privateHalf : "•".repeat(43)}
                         className="h-8 text-[11px] font-mono" />
                  <Button size="sm" variant="ghost" onClick={() => setShowPrivate((v) => !v)}>
                    {showPrivate ? <Lock className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                  </Button>
                  <Button size="sm" variant="secondary"
                          onClick={() => copy(privateHalf, "Private half copied — paste it into the signer now.")}>
                    <Copy className="w-3.5 h-3.5" />
                  </Button>
                </div>
                <p className="text-[11px] text-muted-foreground">
                  It is already in this machine's vault, so closing this does not lose it. It is never
                  written to the repository — and if it ever ends up in one, the audit that catches
                  that is <span className="text-mono" data-no-i18n>npm run audit:secrets</span>.
                </p>
              </div>

              <p className="text-[11px] text-muted-foreground">
                The old key keeps working for a fortnight. That overlap is not politeness: a token
                issued a minute before the switch is valid for its full term, and refusing it would
                lock out exactly the people who were using the app when the rotation happened.
              </p>
            </div>
          )}

          {signing?.minted && !signing?.in_service && !minted && (
            <p className="text-[11px] text-amber-600 dark:text-amber-400">
              A key was minted on this machine and this build does not list it yet. Until the snippet
              is in <span className="text-mono" data-no-i18n>subscription.rs</span> and shipped, the rotation is
              half-done — this machine can sign for a key the app does not trust.
            </p>
          )}
        </Card>
      )}

      {publisher && signing && !signing.compromised && signing.in_service && (
        <Card className="p-4 flex items-center gap-2">
          <ShieldCheck className="w-4 h-4 text-emerald-500 shrink-0" />
          <div className="text-xs">
            <span className="font-medium">The signing key is this machine's own.</span>
            <span className="text-muted-foreground"> The leaked one is no longer trusted by this build.</span>
          </div>
        </Card>
      )}

      {/* ── Being studied ────────────────────────────────────────────────── */}
      <Card className="p-4 space-y-2">
        <h2 className="font-medium">Helping me improve it</h2>
        <p className="text-xs text-muted-foreground">
          During the free week, anonymous usage counting is part of the deal and it is how a one-person
          project finds out that step four of a guide loses everybody. It records which views you opened
          and where you stopped — never your lyrics, songs, files or keys.
        </p>
        {paid && (
          <label className="flex items-start gap-2 text-sm cursor-pointer">
            <input type="checkbox" className="mt-1 accent-primary"
                   checked={!!ent?.analytics_opt_in}
                   onChange={async (ev) => {
                     await api.saveSettings({ analytics_opt_in: ev.target.checked });
                     await refreshEntitlement();
                     toast.success(ev.target.checked ? "Thank you — that genuinely helps."
                                                     : "Off. Nothing further is sent.");
                   }} />
            <span>
              <b>Keep counting now that I am paying.</b>
              <span className="text-muted-foreground"> Off by default — being studied is a favour, not
              something to extract from a customer.</span>
            </span>
          </label>
        )}
      </Card>
    </div>
  );
}
