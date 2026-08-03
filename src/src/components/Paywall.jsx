import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { subscribeEntitlement, refreshEntitlement, isLocked } from "../lib/entitlement";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Input } from "./ui/input";
import { Lock, Loader2, Gift, ExternalLink, LogIn, Clock, Check, ShieldAlert } from "lucide-react";
import { toast } from "sonner";
import Markdown from "./Markdown";

// ─────────────────────────────────────────────────────────────────────────────
// The paywall, and what it is honest about.
//
// The blur is an explanation, not a lock. The real gate is in the backend: every restricted command
// verifies a signed entitlement before doing any work, so removing this overlay in devtools reveals a
// page whose buttons still refuse. That distinction is worth being clear about, because a paywall that is
// only CSS teaches people that the app is dishonest about its own limits.
//
// The overlay is deliberately *readable through*. Somebody deciding whether to pay should be able to see
// what they would be paying for — a black box does not sell anything. So the page behind stays
// recognisable and stops being usable.
//
// Trial users are never shown this. They have the whole app; what they cannot do is take their work out
// of it, and that refusal happens where they try, with its own sentence, rather than as a wall on arrival.
// ─────────────────────────────────────────────────────────────────────────────

export default function Paywall({ children }) {
  const [ent, setEnt] = useState(null);
  useEffect(() => subscribeEntitlement(setEnt), []);

  if (!ent || ent.loading) return children;
  // Reading is always allowed, and a trial is not locked — so this only fires for expired, blocked, or
  // never-signed-in.
  if (!isLocked()) return children;

  return (
    <div className="relative">
      {/* Still legible: 3px of blur makes text unreadable but leaves the shape of the page. Pointer
          events off so nothing behind can be clicked even before the backend refuses it. */}
      <div className="pointer-events-none select-none" style={{ filter: "blur(3px)", opacity: 0.55 }}
           aria-hidden="true">
        {children}
      </div>
      <div className="absolute inset-0 flex items-start justify-center pt-10 px-4 overflow-y-auto">
        <SubscribePrompt ent={ent} />
      </div>
    </div>
  );
}

/** The prompt itself — also used as a full screen from the Account view. */
export function SubscribePrompt({ ent, compact = false }) {
  const [pricing, setPricing] = useState(null);
  const [busy, setBusy] = useState("");
  const [code, setCode] = useState("");
  const [terms, setTerms] = useState("");
  const [showTerms, setShowTerms] = useState(false);

  useEffect(() => { api.subsPricing().then(setPricing).catch(() => {}); }, []);

  const openTerms = async () => {
    if (!terms) {
      try { setTerms((await api.subsTerms())?.markdown || "Could not load the terms."); }
      catch { setTerms("Could not load the terms."); }
    }
    setShowTerms((s) => !s);
  };

  const signIn = async () => {
    setBusy("in");
    try {
      // The Google sign-in the app already performs for YouTube; the id token proves who they are, and no
      // password is ever created or stored. One call, because this used to be two — a `?.` optional call
      // to a command that did not exist, so the button's only effect was a toast telling people to sign
      // in somewhere that could not sign them in either.
      const r = await api.subsSignInGoogle();
      await refreshEntitlement();
      toast.success(r?.email ? `Signed in as ${r.email} — your free week starts now.`
                             : "Signed in — your free week starts now.");
    } catch (err) {
      // The session limit is a refusal with a way out, so it reads differently from a failure.
      const msg = String(err);
      if (msg.includes("limit")) toast.warning(msg, { duration: 14000 });
      else toast.error(msg, { duration: 14000 });
    } finally { setBusy(""); }
  };

  const redeem = async () => {
    setBusy("code");
    try {
      await api.subsRedeem(code.trim().toUpperCase());
      await refreshEntitlement();
      toast.success("Redeemed — welcome in.");
      setCode("");
    } catch (err) { toast.error(`${err}`, { duration: 9000 }); }
    finally { setBusy(""); }
  };

  const status = ent?.status || "none";
  const headline = {
    none: "Seven days free, and no card asked for",
    expired: "The free week is over",
    blocked: "This account is blocked",
    trial: "You are on the free week",
  }[status] || "A subscription unlocks this";

  return (
    <Card className={`w-full ${compact ? "" : "max-w-lg"} p-5 space-y-4 shadow-2xl border-primary/30`}>
      <div className="flex items-start gap-3">
        {status === "blocked" ? <ShieldAlert className="w-5 h-5 text-red-400 mt-0.5 shrink-0" />
                              : <Lock className="w-5 h-5 text-primary mt-0.5 shrink-0" />}
        <div>
          <h2 className="font-semibold">{headline}</h2>
          {status === "expired" && (
            <p className="text-sm text-muted-foreground">
              Your projects are still here and still intact — nothing was deleted. A subscription unlocks
              them exactly where they are.
            </p>
          )}
          {status === "none" && (
            <p className="text-sm text-muted-foreground">
              The trial is the whole application. Two things wait: exporting your projects, and saving
              copies out of the app.
            </p>
          )}
          {status === "blocked" && (
            <p className="text-sm text-muted-foreground">
              If that is a mistake, reply to any report you have sent and I will look at it.
            </p>
          )}
        </div>
      </div>

      {status === "none" && (
        <Button onClick={signIn} disabled={busy === "in"} className="w-full">
          {busy === "in" ? <Loader2 className="w-4 h-4 animate-spin mr-2" /> : <LogIn className="w-4 h-4 mr-2" />}
          Start the free week
        </Button>
      )}

      {pricing?.plans && status !== "none" && (
        <div className="space-y-2">
          <div className="grid grid-cols-2 gap-2">
            {["monthly", "quarterly", "annual", "lifetime"].filter((k) => pricing.plans[k]).map((k) => {
              const p = pricing.plans[k];
              const per = p.months > 1 ? `${(p.cents / p.months / 100).toFixed(2)}/mo` : "";
              return (
                <div key={k} className="rounded-lg border border-border p-2.5">
                  <div className="text-lg font-semibold">
                    {(p.cents / 100).toFixed(2)}
                    <span className="text-xs text-muted-foreground ml-1">{pricing.currency}</span>
                  </div>
                  <div className="text-[11px] text-muted-foreground">{p.label}{per ? ` · ${per}` : ""}</div>
                </div>
              );
            })}
          </div>
          <p className="text-[11px] text-muted-foreground">
            Paid through Lemon Squeezy, who handle tax wherever you are. A lifetime licence covers this
            major version and every update to it — a future major version is a separate purchase, and your
            licence never stops working when one appears.
          </p>
        </div>
      )}

      {status === "trial" && ent?.days_left != null && (
        <div className="flex items-center gap-2 text-sm">
          <Clock className="w-4 h-4 text-primary" />
          <span>{ent.days_left} day{ent.days_left === 1 ? "" : "s"} left of the free week.</span>
        </div>
      )}

      <div className="space-y-1.5 pt-1 border-t border-border/60">
        <div className="text-[10px] uppercase tracking-widest text-muted-foreground">Have a code?</div>
        <div className="flex gap-2">
          <Input value={code} onChange={(ev) => setCode(ev.target.value)} placeholder="GIFT-CODE-HERE"
                 className="font-mono text-sm" data-no-i18n />
          <Button variant="secondary" onClick={redeem} disabled={busy === "code" || code.trim().length < 4}>
            {busy === "code" ? <Loader2 className="w-4 h-4 animate-spin" /> : <Gift className="w-4 h-4" />}
          </Button>
        </div>
      </div>

      <div className="flex items-center gap-3 text-[11px]">
        <button className="underline text-muted-foreground" onClick={openTerms}>
          {showTerms ? "Hide the terms" : "Read the terms"}
        </button>
        {ent?.stale && (
          <Badge variant="outline" className="text-[10px] text-amber-400 border-amber-500/40">
            offline — using the last known licence
          </Badge>
        )}
      </div>
      {showTerms && (
        <div className="max-h-72 overflow-y-auto rounded-lg border border-border p-3">
          <Markdown text={terms} />
        </div>
      )}
    </Card>
  );
}

/**
 * A small inline refusal, for a single control rather than a whole page.
 *
 * Used where a trial user is allowed to be — the button is visible and disabled, with the reason next to
 * it. Hiding it instead would leave somebody wondering whether the feature exists at all.
 */
export function FeatureLock({ feature, children }) {
  const [reason, setReason] = useState(null);
  useEffect(() => {
    let alive = true;
    api.subsCan(feature).then((r) => { if (alive) setReason(r.allowed ? null : r.reason); }).catch(() => {});
    return () => { alive = false; };
  }, [feature]);

  if (!reason) return children;
  return (
    <div className="space-y-1">
      <div className="opacity-50 pointer-events-none">{children}</div>
      <p className="text-[11px] text-amber-400 flex items-start gap-1">
        <Lock className="w-3 h-3 mt-0.5 shrink-0" />{reason}
      </p>
    </div>
  );
}
