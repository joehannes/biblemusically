import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { subscribeEntitlement, refreshEntitlement } from "../lib/entitlement";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Input } from "../components/ui/input";
import {
  UserCircle, Clock, Monitor, LogOut, RefreshCw, Download, Share2, Copy,
  Loader2, ArrowUpCircle, MapPin, Eye,
} from "lucide-react";
import { toast } from "sonner";
import { SubscribePrompt } from "../components/Paywall";

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
  const [sessions, setSessions] = useState(null);
  const [update, setUpdate] = useState(null);
  const [referral, setReferral] = useState(null);
  const [busy, setBusy] = useState("");

  useEffect(() => subscribeEntitlement(setEnt), []);
  useEffect(() => {
    api.checkUpdate().then(setUpdate).catch(() => {});
    api.subsReferral().then(setReferral).catch(() => {});
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
    <div className="space-y-4 max-w-2xl">
      <div>
        <h1 className="text-xl font-semibold flex items-center gap-2">
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
              {update.kind === "update" && (
                <Button size="sm" disabled={busy === "dl"}
                        onClick={() => act("dl", () => api.downloadUpdate(), (r) => {
                          toast.success(`Downloaded to ${r.path}`, { duration: 12000 });
                          toast.message(r.next, { duration: 14000 });
                        })}>
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
