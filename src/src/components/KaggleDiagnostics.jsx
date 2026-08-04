import { useState } from "react";
import { api } from "../lib/api";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Stethoscope, Loader2, Copy, AlertTriangle, CheckCircle2 } from "lucide-react";
import { toast } from "sonner";

// ─────────────────────────────────────────────────────────────────────────────
// What the app is actually doing with Kaggle, on demand.
//
// This exists because of a bug that was undiagnosable from the interface. Every Kaggle operation
// addressed a kernel slug of the form `<owner>/<name>` where the owner was compiled in — so once the
// signed-in account was anybody else, the app pushed to a kernel it could not write to, polled a
// kernel it did not own, and streamed that other kernel's *previous* log. The interface reported "the
// run ended before a server came up" and showed a log of a server coming up. Both were true. Nothing
// on screen connected them, because the one fact that explained it — which account, which kernel —
// was never displayed anywhere.
//
// So: show the wiring. Owner, slug, whether that kernel exists, what its status says, and whether the
// stored URL still answers. And make it copyable, because the second thing anybody does with an
// answer like this is paste it into a bug report.
// ─────────────────────────────────────────────────────────────────────────────

export default function KaggleDiagnostics() {
  const [diag, setDiag] = useState(null);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    setBusy(true);
    try {
      setDiag(await api.kaggleDiagnostics());
    } catch (e) {
      toast.error(`Diagnostics failed: ${e?.message || e}`, { duration: 10000 });
    } finally { setBusy(false); }
  };

  const copy = () => {
    navigator.clipboard?.writeText(JSON.stringify(diag, null, 2));
    toast.success("Diagnostics copied — paste it into a bug report.");
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2 flex-wrap">
        <Button size="sm" variant="outline" onClick={run} disabled={busy} data-testid="kaggle-diagnose">
          {busy ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" /> : <Stethoscope className="w-3.5 h-3.5 mr-1.5" />}
          Check my Kaggle setup
        </Button>
        {diag && (
          <Button size="sm" variant="ghost" onClick={copy}>
            <Copy className="w-3.5 h-3.5 mr-1.5" />Copy for a bug report
          </Button>
        )}
      </div>

      {diag && (
        <div className="space-y-2 text-xs">
          {!diag.cli_present && (
            <Row bad>The kaggle CLI was not found at <code data-no-i18n>{diag.cli_path}</code>. Install it with <code>pipx install kaggle</code>.</Row>
          )}
          {!diag.cli_username && (
            <Row bad>No <code>~/.kaggle/kaggle.json</code> — nothing can authenticate. Kaggle → Settings → Create New API Token.</Row>
          )}
          {diag.cli_username && (
            <Row>Signed in to Kaggle as <b data-no-i18n>{diag.cli_username}</b>, and the app addresses that account's notebooks.</Row>
          )}
          {/* The silent no-op: a cached OAuth token outranks the key file, so pasting a new
              kaggle.json switches nothing and reports nothing. Named first because it makes every
              other symptom below make sense. */}
          {diag.token_overrides_key_file && (
            <Row bad>
              The Kaggle CLI is signed in as <b data-no-i18n>{diag.cli_username}</b> using a cached
              access token (<code>~/.kaggle/access_token</code>), which <b>overrides</b> the API key
              in <code>~/.kaggle/kaggle.json</code> — that file names{" "}
              <b data-no-i18n>{diag.kaggle_json_username}</b>. Adding an account by pasting a
              kaggle.json therefore changes nothing. To switch accounts, delete{" "}
              <code>~/.kaggle/access_token</code>, or run <code>kaggle auth login</code> as the
              account you want.
            </Row>
          )}
          {diag.auth_method && (
            <Row>Authenticating by <b data-no-i18n>{diag.auth_method === "ACCESS_TOKEN"
              ? "cached access token" : "API key"}</b>.</Row>
          )}
          {diag.identity_mismatch && (
            <Row bad>
              The app had <b data-no-i18n>{diag.stored_active}</b> recorded as active while the CLI signs in as{" "}
              <b data-no-i18n>{diag.cli_username}</b>. That is now reconciled — reopening this page adopts the CLI's account.
            </Row>
          )}

          <div className="rounded-md border border-border/70 divide-y divide-border/60">
            {(diag.engines || []).map((e) => (
              <div key={e.engine} className="px-2.5 py-2 space-y-1">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="font-medium">{e.engine}</span>
                  {e.kernel_exists
                    ? <Badge variant="outline" className="text-[9px]">notebook exists</Badge>
                    : <Badge variant="outline" className="text-[9px] text-amber-400 border-amber-500/40">not created yet</Badge>}
                  {e.url_alive === true && <Badge variant="outline" className="text-[9px] text-emerald-500 border-emerald-500/40">URL answering</Badge>}
                  {e.url_alive === false && <Badge variant="outline" className="text-[9px] text-amber-400 border-amber-500/40">stored URL dead</Badge>}
                </div>
                <div className="text-muted-foreground font-mono text-[10px] break-all" data-no-i18n>{e.slug}</div>
                {!e.kernel_exists && (
                  <div className="text-[11px] text-muted-foreground">
                    Normal before the first run — "Start &amp; connect" copies the published notebook to your account.
                  </div>
                )}
                {e.status_raw && (
                  <div className="text-[10px] text-muted-foreground break-all" data-no-i18n>{e.status_raw}</div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function Row({ children, bad }) {
  return (
    <div className={`flex items-start gap-1.5 ${bad ? "text-amber-400" : "text-muted-foreground"}`}>
      {bad ? <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
           : <CheckCircle2 className="w-3.5 h-3.5 shrink-0 mt-0.5 text-emerald-500" />}
      <span>{children}</span>
    </div>
  );
}
