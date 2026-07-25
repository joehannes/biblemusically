import { useCallback, useEffect, useMemo, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Badge } from "../components/ui/badge";
import { Switch } from "../components/ui/switch";
import { toast } from "sonner";
import {
  Database, FolderOpen, GitCommit, CloudUpload, CloudDownload, KeyRound, Lock, Unlock,
  RefreshCw, Trash2, ShieldCheck, HardDrive, PackageOpen, AlertTriangle, Check, ExternalLink,
} from "lucide-react";

// Bytes → something a human reads without counting digits.
const humanSize = (n) => {
  const bytes = Number(n) || 0;
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) { value /= 1024; i += 1; }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[i]}`;
};

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

function PathRow({ label, path }) {
  if (!path) return null;
  return (
    <div className="flex items-center justify-between gap-2 rounded bg-muted/40 px-2 py-1.5">
      <div className="min-w-0">
        <div className="text-[10px] uppercase tracking-wider text-muted-foreground">{label}</div>
        <div className="text-xs font-mono truncate" title={path}>{path}</div>
      </div>
      <Button size="sm" variant="ghost" onClick={() => api.revealInFileManager(path).catch((e) => toast.error(String(e)))}>
        <FolderOpen className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}

export default function DataSync() {
  const { activeProjectId, activeProject, projects } = useStudio();
  const [info, setInfo] = useState(null);
  const [vault, setVault] = useState(null);
  const [vaultKeys, setVaultKeys] = useState([]);
  const [providers, setProviders] = useState(null);
  const [cfg, setCfg] = useState(null);
  const [assets, setAssets] = useState([]);
  const [busy, setBusy] = useState("");
  const [token, setToken] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [currentPassphrase, setCurrentPassphrase] = useState("");
  const [iaAccess, setIaAccess] = useState("");
  const [iaSecret, setIaSecret] = useState("");
  const [syncLog, setSyncLog] = useState([]);

  const projectId = activeProjectId || null;

  const refreshData = useCallback(async () => {
    try { setInfo(await api.storeInfo()); } catch (e) { toast.error(`Storage info: ${e}`); }
    try { setVault(await api.vaultStatus()); } catch { /* vault is optional */ }
    try { setVaultKeys(await api.vaultList()); } catch { /* ditto */ }
  }, []);

  const refreshProject = useCallback(async () => {
    if (!projectId) { setCfg(null); setAssets([]); return; }
    try { setCfg(await api.getSyncConfig(projectId)); } catch (e) { toast.error(`Sync config: ${e}`); }
    try { setAssets(await api.listProjectAssets(projectId)); } catch { setAssets([]); }
  }, [projectId]);

  useEffect(() => { refreshData(); api.listSyncProviders().then(setProviders).catch(() => {}); }, [refreshData]);
  useEffect(() => { refreshProject(); }, [refreshProject]);

  const counts = useMemo(() => Object.entries(info?.counts || {}).sort((a, b) => b[1] - a[1]), [info]);
  const legacy = info?.legacy_database || {};
  const gitProviders = providers?.git || [];
  const assetProviders = providers?.assets || [];
  const selectedProvider = gitProviders.find((p) => p.id === (cfg?.provider || "huggingface"));
  const selectedAssetProvider = assetProviders.find((p) => p.id === (cfg?.asset_provider || "lfs"));

  const run = async (name, fn, successMessage) => {
    setBusy(name);
    try {
      const result = await fn();
      if (successMessage) toast.success(typeof successMessage === "function" ? successMessage(result) : successMessage);
      return result;
    } catch (err) {
      toast.error(String(err));
      throw err;
    } finally {
      setBusy("");
    }
  };

  const patchCfg = async (patch) => {
    const next = { ...(cfg || {}), ...patch };
    setCfg(next);
    const saved = await api.saveSyncConfig(projectId, next).catch((e) => { toast.error(String(e)); return null; });
    if (saved) setCfg(saved);
  };

  return (
    <div className="p-6 space-y-4 max-w-5xl">
      <div>
        <h1 className="text-2xl font-semibold flex items-center gap-2">
          <Database className="h-6 w-6 text-primary" /> Data &amp; Sync
        </h1>
        <p className="text-sm text-muted-foreground mt-1">
          Everything this app knows is plain JSON files you can open, diff and copy. Each project keeps its own
          data inside its own git repository, and can push itself — media included — to a free remote.
        </p>
      </div>

      {/* ── Where the data is ─────────────────────────────────────────── */}
      <Section
        icon={HardDrive}
        title="Storage"
        subtitle={info ? `Engine: ${info.engine} — no database server, no sidecar.` : "Loading…"}
        action={
          <Button size="sm" variant="outline" onClick={refreshData} disabled={busy === "refresh"}>
            <RefreshCw className="h-3.5 w-3.5 mr-1" /> Refresh
          </Button>
        }
      >
        <div className="space-y-1.5">
          <PathRow label="Global data" path={info?.locations?.global} />
          {(info?.locations?.projects || []).map((p) => {
            const name = projects?.find((x) => x.id === p.project_id)?.name || p.project_id;
            return <PathRow key={p.project_id} label={`Project · ${name}`} path={p.path} />;
          })}
        </div>

        {counts.length > 0 && (
          <div className="flex flex-wrap gap-1.5 pt-1">
            {counts.map(([name, n]) => (
              <Badge key={name} variant="secondary" className="font-mono text-[11px]">
                {name} <span className="ml-1 opacity-70">{n}</span>
              </Badge>
            ))}
          </div>
        )}

        <div className="flex flex-wrap items-center gap-2 pt-1">
          <Button
            size="sm"
            variant="outline"
            disabled={!projectId || busy === "commit"}
            onClick={() => run("commit", () => api.commitProjectData(projectId), (r) =>
              r?.commit ? `Committed ${r.commit}` : "Nothing to commit")}
          >
            <GitCommit className="h-3.5 w-3.5 mr-1" /> Commit this project
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={busy === "commitAll"}
            onClick={() => run("commitAll", () => api.commitAllProjectData(), (r) =>
              `${(r?.committed || []).length} project(s) committed`)}
          >
            <GitCommit className="h-3.5 w-3.5 mr-1" /> Commit all
          </Button>
          {info?.git && !info.git.available && (
            <span className="text-xs text-amber-500 flex items-center gap-1">
              <AlertTriangle className="h-3.5 w-3.5" /> git not found — history and sync are unavailable
            </span>
          )}
          {info?.git?.available && !info.git.lfs_available && (
            <span className="text-xs text-muted-foreground">git-lfs not installed — use asset offloading for media</span>
          )}
        </div>
      </Section>

      {/* ── Legacy database ───────────────────────────────────────────── */}
      {legacy.present && (
        <Section
          icon={PackageOpen}
          title="Legacy database"
          subtitle="Data from the retired MongoDB sidecar. It was imported into JSON on first launch; the original files are left untouched until you remove them."
        >
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <Badge variant={legacy.migrated ? "secondary" : "destructive"}>
              {legacy.migrated ? "Imported" : "Not imported yet"}
            </Badge>
            <span className="font-mono text-muted-foreground truncate">{legacy.path}</span>
            <span className="text-muted-foreground">{humanSize(legacy.size_bytes)}</span>
          </div>
          {info?.migration_report?.documents_imported != null && (
            <p className="text-xs text-muted-foreground">
              {info.migration_report.documents_imported} document(s) imported
              {info.migration_report.per_collection && ` across ${Object.keys(info.migration_report.per_collection).length} collection(s)`}.
              {(info.migration_report.warnings || []).length > 0 &&
                ` ${info.migration_report.warnings.length} warning(s).`}
            </p>
          )}
          {!legacy.mongod_available && !legacy.migrated && (
            <p className="text-xs text-amber-500 flex items-start gap-1">
              <AlertTriangle className="h-3.5 w-3.5 mt-0.5 shrink-0" />
              No <code className="font-mono">mongod</code> binary found to read the old files. Set
              <code className="font-mono mx-1">MONGOD_PATH</code> and retry.
            </p>
          )}
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              disabled={busy === "migrate"}
              onClick={() => run("migrate", () => api.runLegacyMigration(true), (r) =>
                `Import ${r?.status || "finished"} — ${r?.documents_imported ?? 0} document(s)`).then(refreshData)}
            >
              <RefreshCw className="h-3.5 w-3.5 mr-1" /> Import again
            </Button>
            <Button
              size="sm"
              variant="destructive"
              disabled={!legacy.migrated || busy === "purge"}
              title={legacy.migrated ? "" : "Only possible after a successful import"}
              onClick={() => {
                if (!window.confirm(`Permanently delete the old database folder (${humanSize(legacy.size_bytes)})?\n\nEverything in it has already been imported into JSON.`)) return;
                run("purge", () => api.purgeLegacyData(), (r) => `Freed ${humanSize(r?.freed_bytes)}`).then(refreshData);
              }}
            >
              <Trash2 className="h-3.5 w-3.5 mr-1" /> Delete old database
            </Button>
          </div>
        </Section>
      )}

      {/* ── Vault ─────────────────────────────────────────────────────── */}
      <Section
        icon={ShieldCheck}
        title="Credential vault"
        subtitle={vault?.protection}
        action={
          <Badge variant={vault?.unlocked ? "secondary" : "destructive"} className="gap-1">
            {vault?.unlocked ? <Unlock className="h-3 w-3" /> : <Lock className="h-3 w-3" />}
            {vault?.mode === "passphrase" ? (vault?.unlocked ? "Unlocked" : "Locked") : "Machine key"}
          </Badge>
        }
      >
        {vaultKeys.length > 0 && (
          <div className="space-y-1">
            {vaultKeys.map((entry) => (
              <div key={entry.key} className="flex items-center justify-between gap-2 rounded bg-muted/40 px-2 py-1 text-xs">
                <span className="font-mono truncate">{entry.key}</span>
                <span className="flex items-center gap-2 shrink-0">
                  <span className="text-muted-foreground font-mono">{entry.hint}</span>
                  <Button size="sm" variant="ghost" onClick={() => run("vaultDel", () => api.vaultDelete(entry.key), "Removed").then(refreshData)}>
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </span>
              </div>
            ))}
          </div>
        )}

        <div className="flex flex-wrap items-end gap-2">
          {vault?.mode === "passphrase" && !vault?.unlocked ? (
            <>
              <Input
                type="password"
                placeholder="Vault passphrase"
                className="max-w-xs"
                value={passphrase}
                onChange={(ev) => setPassphrase(ev.target.value)}
              />
              <Button size="sm" disabled={!passphrase} onClick={() =>
                run("unlock", () => api.vaultUnlock(passphrase), "Vault unlocked").then(() => { setPassphrase(""); refreshData(); })}>
                <Unlock className="h-3.5 w-3.5 mr-1" /> Unlock
              </Button>
            </>
          ) : (
            <>
              {vault?.mode === "passphrase" && (
                <Input
                  type="password"
                  placeholder="Current passphrase"
                  className="max-w-[200px]"
                  value={currentPassphrase}
                  onChange={(ev) => setCurrentPassphrase(ev.target.value)}
                />
              )}
              <Input
                type="password"
                placeholder={vault?.mode === "passphrase" ? "New passphrase" : "Set a passphrase (optional)"}
                className="max-w-[220px]"
                value={passphrase}
                onChange={(ev) => setPassphrase(ev.target.value)}
              />
              <Button size="sm" variant="outline" disabled={passphrase.length < 8} onClick={() =>
                run("setPass", () => api.vaultSetPassphrase(passphrase, currentPassphrase || null), "Vault re-encrypted")
                  .then(() => { setPassphrase(""); setCurrentPassphrase(""); refreshData(); })}>
                <KeyRound className="h-3.5 w-3.5 mr-1" /> {vault?.mode === "passphrase" ? "Change passphrase" : "Protect with passphrase"}
              </Button>
              {vault?.mode === "passphrase" && (
                <Button size="sm" variant="ghost" onClick={() => run("lock", () => api.vaultLock(), "Locked").then(refreshData)}>
                  <Lock className="h-3.5 w-3.5 mr-1" /> Lock
                </Button>
              )}
            </>
          )}
        </div>
      </Section>

      {/* ── Remote sync ───────────────────────────────────────────────── */}
      <Section
        icon={CloudUpload}
        title="Remote backup & sync"
        subtitle={
          projectId
            ? `Push “${activeProject?.name || projectId}” — data and media — to a free remote.`
            : "Select a project first (top bar) to configure its remote."
        }
      >
        {!projectId ? null : (
          <>
            <div className="grid gap-2 sm:grid-cols-2">
              <div>
                <label className="text-[10px] uppercase tracking-wider text-muted-foreground">Git host</label>
                <select
                  className="w-full mt-1 rounded border border-border bg-background px-2 py-1.5 text-sm"
                  value={cfg?.provider || "huggingface"}
                  onChange={(ev) => patchCfg({ provider: ev.target.value })}
                >
                  {gitProviders.map((p) => (
                    <option key={p.id} value={p.id}>{p.label}{p.recommended ? " — recommended" : ""}</option>
                  ))}
                </select>
              </div>
              <div>
                <label className="text-[10px] uppercase tracking-wider text-muted-foreground">Media</label>
                <select
                  className="w-full mt-1 rounded border border-border bg-background px-2 py-1.5 text-sm"
                  value={cfg?.asset_provider || "lfs"}
                  onChange={(ev) => patchCfg({ asset_provider: ev.target.value })}
                >
                  {assetProviders.map((p) => (
                    <option key={p.id} value={p.id}>{p.label}</option>
                  ))}
                </select>
              </div>
            </div>

            {selectedProvider && (
              <div className="rounded bg-muted/40 p-2 text-xs space-y-1">
                <div><span className="text-muted-foreground">Free storage:</span> {selectedProvider.free_storage}</div>
                {selectedProvider.lfs && <div><span className="text-muted-foreground">Large files:</span> {selectedProvider.lfs}</div>}
                {selectedProvider.best_for && <div><span className="text-muted-foreground">Best for:</span> {selectedProvider.best_for}</div>}
                {selectedProvider.caveats && (
                  <div className="text-amber-500 flex items-start gap-1">
                    <AlertTriangle className="h-3.5 w-3.5 mt-0.5 shrink-0" />{selectedProvider.caveats}
                  </div>
                )}
                {selectedAssetProvider?.caveats && (
                  <div className="text-amber-500 flex items-start gap-1">
                    <AlertTriangle className="h-3.5 w-3.5 mt-0.5 shrink-0" />
                    Media · {selectedAssetProvider.caveats}
                  </div>
                )}
              </div>
            )}

            <div className="flex flex-wrap items-end gap-2">
              <div className="flex-1 min-w-[180px]">
                <label className="text-[10px] uppercase tracking-wider text-muted-foreground">Repository name</label>
                <Input
                  className="mt-1"
                  placeholder="my-project (or owner/my-project)"
                  value={cfg?.repo || ""}
                  onChange={(ev) => setCfg({ ...cfg, repo: ev.target.value })}
                  onBlur={(ev) => patchCfg({ repo: ev.target.value })}
                />
              </div>
              <div className="flex-1 min-w-[200px]">
                <label className="text-[10px] uppercase tracking-wider text-muted-foreground flex items-center gap-1">
                  Access token
                  {cfg?.has_token && <Check className="h-3 w-3 text-emerald-500" />}
                  {selectedProvider?.token_url && (
                    <a href={selectedProvider.token_url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-0.5 text-primary hover:underline normal-case tracking-normal">
                      get one <ExternalLink className="h-3 w-3" />
                    </a>
                  )}
                </label>
                <Input
                  className="mt-1"
                  type="password"
                  placeholder={cfg?.has_token ? "stored — type to replace" : selectedProvider?.token_hint || "token"}
                  value={token}
                  onChange={(ev) => setToken(ev.target.value)}
                />
              </div>
              <Button size="sm" variant="outline" disabled={busy === "test"} onClick={() =>
                run("test", () => api.testSyncConnection(cfg?.provider || "huggingface", token || null),
                  (r) => `Connected as ${r?.name || "your account"}`).then(() => { setToken(""); refreshProject(); })}>
                Test &amp; save
              </Button>
            </div>

            {cfg?.asset_provider === "internet_archive" && (
              <div className="flex flex-wrap items-end gap-2 rounded bg-muted/40 p-2">
                <div className="flex-1 min-w-[160px]">
                  <label className="text-[10px] uppercase tracking-wider text-muted-foreground">Archive.org access key</label>
                  <Input className="mt-1" value={iaAccess} onChange={(ev) => setIaAccess(ev.target.value)} />
                </div>
                <div className="flex-1 min-w-[160px]">
                  <label className="text-[10px] uppercase tracking-wider text-muted-foreground">Secret key</label>
                  <Input className="mt-1" type="password" value={iaSecret} onChange={(ev) => setIaSecret(ev.target.value)} />
                </div>
                <Button size="sm" variant="outline" disabled={!iaAccess || !iaSecret} onClick={() =>
                  run("ia", () => api.saveAssetCredentials("internet_archive", iaAccess, iaSecret), "Keys stored in the vault")
                    .then(() => { setIaAccess(""); setIaSecret(""); refreshProject(); })}>
                  Save keys
                </Button>
              </div>
            )}

            <div className="flex flex-wrap items-center gap-3">
              <label className="flex items-center gap-2 text-xs">
                <Switch checked={cfg?.private !== false} onCheckedChange={(v) => patchCfg({ private: v })} />
                Private repository
              </label>
              <label className="flex items-center gap-2 text-xs">
                <Switch checked={cfg?.auto_sync === true} onCheckedChange={(v) => patchCfg({ auto_sync: v })} />
                Auto-sync every 15 min
              </label>
              {typeof cfg?.unpushed_commits === "number" && cfg.unpushed_commits > 0 && (
                <Badge variant="secondary">{cfg.unpushed_commits} commit(s) not pushed</Badge>
              )}
              {cfg?.last_sync && (
                <span className="text-xs text-muted-foreground">Last sync {new Date(cfg.last_sync).toLocaleString()}</span>
              )}
            </div>

            {cfg?.last_error && (
              <p className="text-xs text-destructive flex items-start gap-1">
                <AlertTriangle className="h-3.5 w-3.5 mt-0.5 shrink-0" />{cfg.last_error}
              </p>
            )}

            <div className="flex flex-wrap gap-2">
              <Button size="sm" disabled={busy === "sync"} onClick={() =>
                run("sync", () => api.syncProjectNow(projectId), "Project pushed").then((r) => {
                  setSyncLog(r?.log || []);
                  refreshProject();
                })}>
                <CloudUpload className="h-3.5 w-3.5 mr-1" /> Sync now
              </Button>
              <Button size="sm" variant="outline" disabled={busy === "pull"} onClick={() =>
                run("pull", () => api.pullProjectNow(projectId), "Pulled from remote").then(refreshProject)}>
                <CloudDownload className="h-3.5 w-3.5 mr-1" /> Pull
              </Button>
              <Button size="sm" variant="outline" disabled={busy === "restore"} onClick={() =>
                run("restore", () => api.restoreProjectAssets(projectId), (r) =>
                  `${r?.restored ?? 0} file(s) restored, ${r?.already_present ?? 0} already here`).then(refreshProject)}>
                <PackageOpen className="h-3.5 w-3.5 mr-1" /> Restore media
              </Button>
              {cfg?.remote_url && (
                <a href={cfg.remote_url} target="_blank" rel="noreferrer" className="text-xs text-primary hover:underline inline-flex items-center gap-1 self-center">
                  open remote <ExternalLink className="h-3 w-3" />
                </a>
              )}
            </div>

            {syncLog.length > 0 && (
              <pre className="rounded bg-muted/40 p-2 text-[11px] font-mono whitespace-pre-wrap max-h-40 overflow-auto">
                {syncLog.join("\n")}
              </pre>
            )}

            {assets.length > 0 && (
              <div className="pt-1">
                <div className="text-[10px] uppercase tracking-wider text-muted-foreground mb-1">
                  Offloaded media ({assets.length})
                </div>
                <div className="max-h-48 overflow-auto space-y-1">
                  {assets.map((a) => (
                    <div key={a.path} className="flex items-center justify-between gap-2 rounded bg-muted/40 px-2 py-1 text-[11px]">
                      <span className="font-mono truncate" title={a.path}>{a.path}</span>
                      <span className="flex items-center gap-2 shrink-0 text-muted-foreground">
                        {humanSize(a.size)}
                        <a href={a.url} target="_blank" rel="noreferrer" className="text-primary hover:underline">open</a>
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
        )}
      </Section>
    </div>
  );
}
