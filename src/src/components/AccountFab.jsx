import { useEffect, useState, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { subscribeEntitlement, refreshEntitlement } from "../lib/entitlement";
import { LogIn, LogOut, Loader2, UserCircle, Clock, ShieldAlert, ExternalLink } from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// Signing in and out, from the menubar, on every page.
//
// It lives here rather than only on the Account page because being signed out is a state that affects
// everything — half the app refuses to work — and a control for a global state belongs somewhere global.
// Previously the only sign-in button was inside the paywall overlay, which is a place you reach by being
// stopped, and the only sign-out was on a page you had to know existed.
//
// The signed-out state is a labelled button, not an icon. An icon would be honest about the state and
// useless about the remedy: somebody who cannot save their work needs to see the word "Sign in", not
// deduce it from a silhouette.
// ─────────────────────────────────────────────────────────────────────────────

export default function AccountFab() {
  const [ent, setEnt] = useState(null);
  const [busy, setBusy] = useState("");
  const [open, setOpen] = useState(false);
  const ref = useRef(null);
  const navigate = useNavigate();

  useEffect(() => subscribeEntitlement(setEnt), []);
  useEffect(() => {
    if (!open) return;
    const onClick = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    const onKey = (e) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onClick);
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("mousedown", onClick); document.removeEventListener("keydown", onKey); };
  }, [open]);

  const signIn = async () => {
    setBusy("in");
    // The browser opens on Google's own consent screen and this waits for it, which can take as long as
    // the person takes. Said out loud so a window appearing behind the app is not mistaken for a hang.
    toast.message("Opening Google in your browser — finish signing in there.", { duration: 8000 });
    try {
      const r = await api.subsSignInGoogle();
      await refreshEntitlement();
      setOpen(false);
      toast.success(r?.email ? `Signed in as ${r.email}.` : "Signed in.");
    } catch (err) {
      // These failures are configuration problems with multi-line instructions attached, so they need
      // long enough on screen to be read and acted on.
      toast.error(String(err), { duration: 16000 });
    } finally { setBusy(""); }
  };

  const signOut = async () => {
    setBusy("out");
    try {
      await api.subsSignOut();
      await refreshEntitlement();
      setOpen(false);
      // Worth saying: signing out locks the project cache but destroys nothing, and people hesitate over
      // a sign-out button when they are not sure which of those it is.
      toast.success("Signed out. Your projects stay on disk — signing back in opens them again.");
    } catch (err) { toast.error(String(err), { duration: 10000 }); }
    finally { setBusy(""); }
  };

  if (!ent || ent.loading) {
    return <div className="p-1.5 shrink-0"><Loader2 className="w-4 h-4 animate-spin opacity-40" /></div>;
  }

  // ── Signed out: the remedy, spelled out ──────────────────────────────────
  if (!ent.signed_in) {
    return (
      <button
        data-testid="account-fab-signin"
        onClick={signIn}
        disabled={busy === "in"}
        className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium bg-primary/20 text-primary hover:bg-primary/30 disabled:opacity-50 shrink-0"
        title="Sign in with Google — starts the free week, no card asked for"
      >
        {busy === "in" ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <LogIn className="w-3.5 h-3.5" />}
        <span className="hidden sm:inline">Sign in</span>
      </button>
    );
  }

  // ── Signed in: initial, and the details behind it ────────────────────────
  const email = ent.email || "";
  const initial = (ent.username || email || "?").trim().charAt(0).toUpperCase();
  const status = ent.status || "none";
  const statusLine = {
    trial: ent.days_left != null
      ? `Free week · ${ent.days_left} day${ent.days_left === 1 ? "" : "s"} left`
      : "Free week",
    active: ent.plan ? `Subscribed · ${ent.plan}` : "Subscribed",
    expired: "The free week is over",
    blocked: "This account is blocked",
  }[status] || status;

  return (
    <div className="relative shrink-0" ref={ref}>
      <button
        data-testid="account-fab"
        onClick={() => setOpen((o) => !o)}
        className="w-7 h-7 rounded-full bg-primary/20 text-primary text-xs font-semibold flex items-center justify-center hover:bg-primary/30"
        title={email ? `Signed in as ${email}` : "Account"}
        aria-label="Account"
      >
        {initial}
      </button>
      {open && (
        <div className="absolute right-0 top-full mt-2 z-50 w-64 rounded-lg border border-border bg-card shadow-xl p-2.5 space-y-2">
          <div className="flex items-start gap-2">
            <UserCircle className="w-4 h-4 text-primary mt-0.5 shrink-0" />
            <div className="min-w-0">
              {ent.username && <div className="text-sm font-medium truncate">{ent.username}</div>}
              <div className="text-xs text-muted-foreground truncate" data-no-i18n>{email}</div>
            </div>
          </div>

          <div className={`flex items-center gap-1.5 text-xs ${status === "blocked" || status === "expired" ? "text-amber-400" : "text-muted-foreground"}`}>
            {status === "blocked" ? <ShieldAlert className="w-3.5 h-3.5 shrink-0" /> : <Clock className="w-3.5 h-3.5 shrink-0" />}
            <span>{statusLine}</span>
          </div>

          {ent.stale && (
            <div className="text-[11px] text-amber-400">
              Offline — running on the last known licence.
            </div>
          )}

          <div className="pt-1.5 border-t border-border/60 space-y-1">
            <button
              onClick={() => { setOpen(false); navigate("/account"); }}
              className="w-full text-left px-2 py-1.5 rounded-lg text-xs hover:bg-accent flex items-center gap-1.5"
            >
              <ExternalLink className="w-3.5 h-3.5" />Account &amp; subscription
            </button>
            <button
              onClick={signOut}
              disabled={busy === "out"}
              className="w-full text-left px-2 py-1.5 rounded-lg text-xs hover:bg-accent flex items-center gap-1.5 disabled:opacity-50"
            >
              {busy === "out" ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <LogOut className="w-3.5 h-3.5" />}
              Sign out
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
