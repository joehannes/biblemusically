import { useEffect, useState, useRef, useCallback } from "react";
import { api } from "../lib/api";
import OAuthClientsPanel from "../components/OAuthClientsPanel";
import ChannelSettingsPanel from "../components/ChannelSettingsPanel";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Badge } from "../components/ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "../components/ui/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import {
  Tv, Plus, Link as LinkIcon, Trash2, ShieldCheck, KeyRound,
  RefreshCw, User, AtSign, Globe, Hash, Mail, Sparkles, Info,
  ChevronDown, X, Lightbulb, Search, Settings, Languages,
} from "lucide-react";
import { getStepForPath } from "../lib/pageSteps";
import { toast } from "sonner";

// ── Category definitions for discovery input labels ──
const CATEGORIES = {
  handle: {
    label: "Handle",
    icon: AtSign,
    color: "bg-blue-500/15 text-blue-400 border-blue-500/30",
    dot: "bg-blue-400",
    placeholder: "@channelname",
    description: "YouTube handle (e.g. @MrBeast)",
  },
  url: {
    label: "URL",
    icon: Globe,
    color: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30",
    dot: "bg-emerald-400",
    placeholder: "https://youtube.com/@...",
    description: "Full YouTube channel URL",
  },
  channelId: {
    label: "Channel ID",
    icon: Hash,
    color: "bg-purple-500/15 text-purple-400 border-purple-500/30",
    dot: "bg-purple-400",
    placeholder: "UC...",
    description: "YouTube channel ID starting with UC",
  },
  email: {
    label: "Gmail",
    icon: Mail,
    color: "bg-amber-500/15 text-amber-400 border-amber-500/30",
    dot: "bg-amber-400",
    placeholder: "yourname@gmail.com",
    description: "Google account email — discovers all brand channels via OAuth",
    hint: true,
  },
};

const DISCOVERY_STORAGE_KEY = "studio:channel-discovery-tags";

function loadPersistedTags() {
  try {
    const raw = window.localStorage.getItem(DISCOVERY_STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch { return []; }
}

function persistTags(tags) {
  try { window.localStorage.setItem(DISCOVERY_STORAGE_KEY, JSON.stringify(tags)); } catch {}
}

let tagIdSeq = Date.now();
function makeTag(category, value) {
  return { id: ++tagIdSeq, category, value: value.trim() };
}

// ── Tag pill component ──
function TagPill({ tag, onRemove }) {
  const cat = CATEGORIES[tag.category] || CATEGORIES.handle;
  const Icon = cat.icon;
  return (
    <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border ${cat.color} transition-all`}>
      <Icon className="w-3 h-3 shrink-0" />
      <span className="truncate max-w-[200px]">{tag.value}</span>
      <button onClick={() => onRemove(tag.id)} className="ml-0.5 hover:bg-white/10 rounded-full p-0.5 transition-colors">
        <X className="w-3 h-3" />
      </button>
    </span>
  );
}

// ── Add-tag dropdown ──
function AddTagDropdown({ onAdd }) {
  const [open, setOpen] = useState(false);
  const [inputVal, setInputVal] = useState("");
  const [selectedCat, setSelectedCat] = useState(null);
  const inputRef = useRef(null);
  const dropdownRef = useRef(null);

  const handleSelect = (catKey) => {
    setSelectedCat(catKey);
    setInputVal("");
    setTimeout(() => inputRef.current?.focus(), 50);
  };

  const handleSubmit = () => {
    if (!selectedCat || !inputVal.trim()) return;
    onAdd(makeTag(selectedCat, inputVal));
    setInputVal("");
    setSelectedCat(null);
    setOpen(false);
  };

  // Close dropdown on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target)) {
        setOpen(false);
        setSelectedCat(null);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  return (
    <div className="relative" ref={dropdownRef}>
      <Button
        size="sm"
        variant="outline"
        onClick={() => { setOpen(!open); setSelectedCat(null); setInputVal(""); }}
        className="gap-1.5"
      >
        <Plus className="w-3.5 h-3.5" /> Add <ChevronDown className="w-3 h-3" />
      </Button>

      {open && (
        <div className="absolute z-50 top-full mt-1 left-0 bg-popover border border-border rounded-lg shadow-lg p-2 min-w-[280px] fade-in">
          {!selectedCat ? (
            <div className="space-y-0.5">
              <div className="text-[10px] uppercase tracking-wider text-muted-foreground px-2 py-1">Input type</div>
              {Object.entries(CATEGORIES).map(([key, cat]) => {
                const Icon = cat.icon;
                return (
                  <button
                    key={key}
                    onClick={() => handleSelect(key)}
                    className="w-full flex items-center gap-2.5 px-2 py-1.5 rounded-md hover:bg-accent text-left text-sm transition-colors"
                  >
                    <span className={`w-2 h-2 rounded-full ${cat.dot} shrink-0`} />
                    <Icon className="w-3.5 h-3.5 shrink-0 text-muted-foreground" />
                    <span className="flex-1">{cat.label}</span>
                    {cat.hint && <Badge variant="outline" className="text-[9px] px-1 py-0 ml-1 border-amber-500/30 text-amber-400">★ recommended</Badge>}
                  </button>
                );
              })}
            </div>
          ) : (
            <div className="space-y-2">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span className={`w-2 h-2 rounded-full ${CATEGORIES[selectedCat].dot}`} />
                {CATEGORIES[selectedCat].description}
              </div>
              <div className="flex gap-1.5">
                <input
                  ref={inputRef}
                  value={inputVal}
                  onChange={(e) => setInputVal(e.target.value)}
                  onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); handleSubmit(); } if (e.key === "Escape") { setOpen(false); setSelectedCat(null); } }}
                  placeholder={CATEGORIES[selectedCat].placeholder}
                  className="flex-1 px-2.5 py-1.5 text-sm bg-background border border-border rounded-md outline-none focus:ring-1 focus:ring-ring"
                />
                <Button size="sm" onClick={handleSubmit} disabled={!inputVal.trim()}>Add</Button>
              </div>
              <button onClick={() => setSelectedCat(null)} className="text-[11px] text-muted-foreground hover:text-foreground">← back to types</button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ══════════════════════════════════════════════════════════════════
// Main Channels component
// ══════════════════════════════════════════════════════════════════
export default function Channels() {
  const [channels, setChannels] = useState([]);
  const [oauthClients, setOauthClients] = useState([]);
  const [name, setName] = useState(""); const [ytId, setYtId] = useState(""); const [lang, setLang] = useState(""); const [region, setRegion] = useState("");
  const [styles, setStyles] = useState("");
  const [oauthDialog, setOauthDialog] = useState(null);
  const [refresh, setRefresh] = useState(""); const [yt, setYt] = useState(""); const [subs, setSubs] = useState("");
  const [pickedClient, setPickedClient] = useState({});
  // Discovery tags (replaces the old textarea)
  const [discoveryTags, setDiscoveryTags] = useState(loadPersistedTags);
  const [discoverResults, setDiscoverResults] = useState([]);
  const [discoverStatus, setDiscoverStatus] = useState("");
  const [discoverErrors, setDiscoverErrors] = useState([]);
  const [isDiscovering, setIsDiscovering] = useState(false);
  const [isDiscoveringSwitcher, setIsDiscoveringSwitcher] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isConnectingAll, setIsConnectingAll] = useState(false);
  const [importLoading, setImportLoading] = useState(false);
  const [importResults, setImportResults] = useState(null);
  const [selectedImportClient, setSelectedImportClient] = useState("");
  const [selectedClientValid, setSelectedClientValid] = useState(null); // null | 'loading' | { ok, missing } | { error }
  const [importError, setImportError] = useState(null);
  
  // Channel settings panel state
  const [showChannelSettings, setShowChannelSettings] = useState(false);

  const load = async () => setChannels(await api.listChannels());
  useEffect(() => { load(); }, []);

  // Persist tags whenever they change
  useEffect(() => { persistTags(discoveryTags); }, [discoveryTags]);

  // Re-resolve picked clients for each channel
  useEffect(() => {
    (async () => {
      const next = {};
      for (const c of channels) {
        try { const r = await api.channelPickedClient(c.id); next[c.id] = r.client; } catch {}
      }
      setPickedClient(next);
    })();
  }, [channels, oauthClients]);

  // ── Tag management ──
  const addTag = useCallback((tag) => {
    setDiscoveryTags((prev) => {
      // Deduplicate by category + value
      if (prev.some((t) => t.category === tag.category && t.value.toLowerCase() === tag.value.toLowerCase())) {
        toast.info("Already added.");
        return prev;
      }
      return [...prev, tag];
    });
  }, []);

  const removeTag = useCallback((id) => {
    setDiscoveryTags((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const clearAllTags = useCallback(() => {
    setDiscoveryTags([]);
    setDiscoverResults([]);
    setDiscoverStatus("");
    setDiscoverErrors([]);
  }, []);

  // ── Create channel ──
  const create = async () => {
    if (!name) return toast.error("Channel name required");
    await api.createChannel({ name, youtube_channel_id: ytId, language: lang || "English", region, styles });
    setName(""); setYtId(""); setLang(""); setRegion(""); setStyles(""); load(); toast.success("Channel added");
  };

  // ── OAuth ──
  const startOauth = async (c, forceClientId) => {
    // Use the loopback server approach which handles the full callback automatically
    // This fixes the "network error" issue by spinning up a local server to catch the redirect
    try {
      toast.info("Opening browser for OAuth consent. Please authorize in the browser window...");
      const r = await api.oauthStartForChannel(c.id, forceClientId);
      if (r.error) {
        toast.error(r.error);
        return false;
      }
      if (r.ok) {
        toast.success(`Channel connected! ${r.channel_title || r.youtube_channel_id || ""}`);
        await load();
        return true;
      }
      return false;
    } catch (err) {
      console.error("OAuth loopback failed:", err);
      toast.error("OAuth failed: " + (err?.toString?.() || "Unknown error"));
      return false;
    }
  };

  const connectAllChannels = async () => {
    const targets = channels.filter((c) => !c.connected || !c.refresh_token);
    if (!targets.length) {
      await load();
      return toast.success("All channels already connected");
    }

    setIsConnectingAll(true);
    try {
      // Try the one-shot approach first: single OAuth → all channels via YouTube API
      const firstTarget = targets[0];
      const forcedClientId = pickedClient[firstTarget.id];
      toast.info(`Connecting ${targets.length} channel${targets.length === 1 ? "" : "s"} in a single OAuth flow...`);

      try {
        const result = await api.connectAllChannelsOneShot(forcedClientId || null);
        if (result?.ok) {
          const count = result.connected_count || 0;
          if (count > 0) {
            toast.success(`Connected ${count} channel${count > 1 ? 's' : ''} in one go! (via ${result.oauth_client_label || 'OAuth'})`);
          } else if (result.already_connected > 0) {
            toast.success(`All ${result.already_connected} channel${result.already_connected > 1 ? 's' : ''} already connected.`);
          }
          await load();
          return;
        }
      } catch (oneShotErr) {
        // One-shot failed — fall through to sequential approach
        console.warn("One-shot connect-all failed, falling back to sequential OAuth:", oneShotErr?.toString?.());
      }

      // Fallback: sequential per-channel OAuth with progress feedback
      toast.info(`Falling back: connecting ${targets.length} channel${targets.length === 1 ? "" : "s"} one at a time...`);
      let connected = 0;
      for (let i = 0; i < targets.length; i++) {
        const channel = targets[i];
        const progressMsg = `Connecting channel ${i + 1}/${targets.length}: ${channel.name || channel.youtube_channel_id || "..."}`;
        toast.info(progressMsg, { duration: 3000 });
        const ok = await startOauth(channel);
        if (ok) connected += 1;
      }
      await load();
      toast.success(`Connected ${connected} of ${targets.length} channel${targets.length === 1 ? "" : "s"}.`);
    } finally {
      setIsConnectingAll(false);
    }
  };

  const completeOauth = async () => {
    if (!oauthDialog) return;
    await api.oauthComplete(oauthDialog.id, { refresh_token: refresh, youtube_channel_id: yt, subscriber_count: subs });
    setOauthDialog(null); setRefresh(""); setYt(""); setSubs(""); load(); toast.success("Channel connected");
  };

  // ── Discovery ──
  const discoverChannels = async () => {
    const queries = discoveryTags.map((t) => t.value).filter(Boolean);
    if (!queries.length) return toast.error("Add at least one handle, URL, channel ID, or email first.");
    setIsDiscovering(true);
    setDiscoverStatus("Scanning YouTube for channels...");
    setDiscoverErrors([]);
    try {
      const result = await api.discoverYoutubeChannels(queries, 240);
      if (!result?.ok) {
        setDiscoverStatus("Discovery failed.");
        return toast.error(result?.error || "YouTube discovery failed.");
      }
      const discovered = Array.isArray(result.discovered)
        ? result.discovered.flatMap((entry) => entry.channels.map((ch) => ({ ...ch, query: entry.query, source: entry.final_url || entry.source })))
        : [];
      setDiscoverResults(discovered);
      setDiscoverStatus(`Found ${discovered.length} channels across ${queries.length} entries.`);
      setDiscoverErrors(Array.isArray(result.errors) ? result.errors : []);
    } catch (err) {
      console.error(err);
      setDiscoverStatus("Discovery failed.");
      toast.error(err?.toString?.() || "Discovery failed.");
    } finally {
      setIsDiscovering(false);
    }
  };

  // ── Channel Switcher discovery ──
  const discoverChannelSwitcher = async () => {
    setIsDiscoveringSwitcher(true);
    setDiscoverErrors([]);
    setDiscoverResults([]);
    setDiscoverStatus("Opening YouTube channel switcher in browser…");
    try {
      const result = await api.discoverFromChannelSwitcher(null, 120);
      if (!result?.ok) {
        setDiscoverStatus("Channel switcher discovery failed.");
        return toast.error(result?.error || "Channel switcher discovery failed.");
      }
      const channels = Array.isArray(result.channels) ? result.channels : [];
      if (!channels.length) {
        setDiscoverStatus("No channels found in account switcher. Make sure you're logged into YouTube.");
        toast.info("No channels found. Ensure you're logged into your main YouTube account.");
        return;
      }
      // Map channel_switcher results into the same format as discover results for unified display
      const mapped = channels.map((ch) => ({
        channel_id: ch.channel_id || "",
        title: ch.title || "Unknown channel",
        handle: ch.handle || "",
        avatar: ch.avatar || "",
        source: "youtube.com/channel_switcher",
        query: ch.handle || ch.channel_id || "",
      }));
      setDiscoverResults(mapped);
      setDiscoverStatus(`Found ${mapped.length} available channel${mapped.length > 1 ? 's' : ''} in your YouTube account.`);
      toast.success(`Found ${mapped.length} channel${mapped.length > 1 ? 's' : ''} from channel switcher.`);
    } catch (err) {
      console.error(err);
      setDiscoverStatus("Channel switcher discovery failed.");
      toast.error(err?.toString?.() || "Channel switcher discovery failed.");
    } finally {
      setIsDiscoveringSwitcher(false);
    }
  };

  const importDiscoveredChannels = async () => {
    if (!discoverResults.length) return toast.error("No discovered channels to import.");
    try {
      const res = await api.importDiscoveredChannels(discoverResults);
      await load();
      toast.success(`Imported ${res.inserted || 0} channels. ${res.skipped || 0} skipped.`);
    } catch (err) {
      console.error(err);
      toast.error("Failed to import discovered channels.");
    }
  };

  const importDiscoveredChannel = async (channel) => {
    try {
      await api.createChannel({
        name: channel.title || channel.name || "Imported channel",
        youtube_channel_id: channel.channel_id || "",
        language: "English", region: "", styles: "",
      });
      await load();
      toast.success(`Imported ${channel.title || channel.channel_id}`);
    } catch (err) {
      console.error(err);
      toast.error("Failed to import channel.");
    }
  };

  // ── Google Account import ──
  // Validate the selected OAuth client whenever it changes
  useEffect(() => {
    if (!selectedImportClient) {
      setSelectedClientValid(null);
      return;
    }
    (async () => {
      setSelectedClientValid("loading");
      try {
        const result = await api.validateOauthClient(selectedImportClient);
        setSelectedClientValid(result);
      } catch (err) {
        setSelectedClientValid({ ok: false, error: err.toString() });
      }
    })();
  }, [selectedImportClient, oauthClients]);

  const importFromGoogleAccount = async (oauthClientId) => {
    if (!oauthClientId) return toast.error("Select an OAuth client first.");
    if (selectedClientValid && !selectedClientValid.ok && selectedClientValid !== "loading") {
      const missing = selectedClientValid.missing?.join(", ") || "unknown fields";
      toast.error(`Selected client is missing: ${missing}. Edit it in the OAuth client pool above.`);
      return;
    }
    setImportLoading(true);
    setImportResults(null);
    setImportError(null);
    try {
      const result = await api.importFromGoogleAccount(oauthClientId);
      if (!result?.ok) {
        toast.error(result?.error || "Failed to import channels from Google account.");
        return;
      }
      setImportResults(result.channels || []);
      // Show auto-creation summary
      const created = result.created_count || 0;
      const existing = result.existing_count || 0;
      const hasTokens = result.tokens_available;
      const parts = [`Found ${result.count || 0} channels from Google account.`];
      if (created > 0) parts.push(`Automatically created ${created} new channel${created > 1 ? 's' : ''}.`);
      if (existing > 0) parts.push(`${existing} already existed (tokens refreshed).`);
      if (hasTokens && created + existing > 0) {
        parts.push('These channels are now connected and ready for upload.');
      } else if (!hasTokens && created + existing > 0) {
        parts.push('Note: No refresh token was returned. You may need to OAuth-connect each channel individually.');
      }
      toast.success(parts.join(' '));
      await load(); // Refresh the channel list
    } catch (err) {
      console.error(err);
      const errMsg = err?.toString?.() || "Unknown error";
      setImportError(errMsg);
      toast.error("Failed to import channels from Google account: " + errMsg);
    } finally {
      setImportLoading(false);
    }
  };

  const importGoogleChannel = async (channel) => {
    try {
      await api.createChannel({
        name: channel.title || "Imported Channel",
        youtube_channel_id: channel.channel_id || "",
        language: "English", region: "", styles: "",
      });
      await load();
      toast.success(`Imported ${channel.title || channel.channel_id}`);
    } catch (err) {
      console.error(err);
      toast.error("Failed to import channel.");
    }
  };

  const refreshMetadata = async () => {
    setIsRefreshing(true);
    try {
      const result = await api.refreshAllChannelMetadata();
      toast.success(`Updated ${result.updated || 0} channels.`);
      if (result.failed?.length) console.warn("Refresh failures", result.failed);
      await load();
    } catch (err) {
      console.error(err);
      toast.error("Failed to refresh channel metadata.");
    } finally {
      setIsRefreshing(false);
    }
  };

  // ── Helpers ──
  const emailTags = discoveryTags.filter((t) => t.category === "email");
  const nonEmailTags = discoveryTags.filter((t) => t.category !== "email");

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step {getStepForPath("/channels")}</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Channel Manager</h1>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={() => setShowChannelSettings(!showChannelSettings)} className="gap-2">
            <Settings className="w-4 h-4" />
            {showChannelSettings ? "Hide" : "Channel Settings"}
          </Button>
          <Button variant="outline" size="sm" onClick={refreshMetadata} disabled={isRefreshing}>
            <RefreshCw className={`w-4 h-4 mr-2 ${isRefreshing ? "animate-spin" : ""}`} />Refresh metadata
          </Button>
          <Button data-testid="channels-connect-all-btn" onClick={connectAllChannels} disabled={isConnectingAll || !channels.length}>
            <ShieldCheck className="w-4 h-4 mr-2" />{isConnectingAll ? "Connecting..." : "Connect all"}
          </Button>
        </div>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">Add YouTube channels and connect each via Google OAuth. Manage multiple OAuth clients below to spread upload quota across language groups.</p>

      <OAuthClientsPanel defaultOpen={false} onChange={setOauthClients} />

      {/* ── Add Channel (manual) ── */}
      <Card className="p-5 mb-6">
        <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3">Add channel</div>
        <div className="grid md:grid-cols-12 gap-3">
          <Input data-testid="channel-name-input" placeholder="Channel name" value={name} onChange={(e) => setName(e.target.value)} className="md:col-span-3" />
          <Input data-testid="channel-ytid-input" placeholder="YouTube channel id (UC...)" value={ytId} onChange={(e) => setYtId(e.target.value)} className="md:col-span-2" />
          <Input data-testid="channel-lang-input" placeholder="Language" value={lang} onChange={(e) => setLang(e.target.value)} className="md:col-span-2" />
          <Input data-testid="channel-styles-input" placeholder="Styles (e.g. DnB)" value={styles} onChange={(e) => setStyles(e.target.value)} className="md:col-span-3" />
          <Input data-testid="channel-region-input" placeholder="Region" value={region} onChange={(e) => setRegion(e.target.value)} className="md:col-span-1" />
          <Button data-testid="channel-add-btn" onClick={create} className="md:col-span-1"><Plus className="w-4 h-4" /></Button>
        </div>
      </Card>

      {/* ── Import from Google Account (★ recommended — discovers ALL brand channels) ── */}
      <Card className="p-5 mb-6 border-primary/30 bg-primary/[0.03]">
        <div className="mb-3">
          <div className="flex items-center gap-2 mb-1">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Import from Google Account</div>
            <Badge variant="outline" className="text-[9px] border-primary/40 text-primary gap-1"><Sparkles className="w-2.5 h-2.5" /> recommended</Badge>
          </div>
          <div className="text-sm text-muted-foreground max-w-2xl">Select an OAuth client token, then click <strong className="text-foreground">Discover channels</strong> to import <strong className="text-foreground">all YouTube channels you manage</strong> — including brand channels.</div>
        </div>

        <div className="flex items-center gap-3 flex-wrap mb-3">
          <Select value={selectedImportClient} onValueChange={(v) => setSelectedImportClient(v)}>
            <SelectTrigger className="w-[260px]" disabled={importLoading}>
              <SelectValue placeholder="Select OAuth token…" />
            </SelectTrigger>
            <SelectContent>
              {oauthClients.map((oc) => (
                <SelectItem key={oc.id} value={oc.id}>
                  <KeyRound className="w-3 h-3 mr-2 inline" />{oc.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {oauthClients.length === 0 && discoveryTags.filter(t => t.category === "email").length > 0 && (
            <div className="text-[10px] text-amber-400 italic">
              ⚠️ You added Gmail addresses to the discovery field below, but you still need to create an OAuth client above first.
            </div>
          )}
          <Button
            size="sm"
            onClick={() => importFromGoogleAccount(selectedImportClient)}
            disabled={importLoading || !selectedImportClient}
            className="gap-2"
          >
            {importLoading ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <Sparkles className="w-3.5 h-3.5" />}
            {importLoading ? "Authenticating…" : "Discover channels"}
          </Button>
        </div>

        {/* How it works explanation */}
        <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-500/5 border border-amber-500/20 text-xs text-muted-foreground mb-2">
          <Lightbulb className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
          <div>
            <strong className="text-amber-300">How this works:</strong>{" "}
            First select the OAuth token (API credentials set up in Google Cloud Console) linked to your Google account, then click "Discover channels".
            This opens your browser to sign in. After you authorize, the app discovers <em>every</em> channel you manage — including all brand channels.
            {discoveryTags.filter(t => t.category === "email").length > 0 && (
              <div className="mt-1.5 text-amber-300/80">
                💡 <strong>Tip:</strong> You added {discoveryTags.filter(t => t.category === "email").length} Gmail address{discoveryTags.filter(t => t.category === "email").length > 1 ? "es" : ""} in the "YouTube Channel Discovery" section below.
                No need to re-enter them here — just configure one OAuth client per Google Cloud project above, then select it here.
              </div>
            )}
          </div>
        </div>

        {oauthClients.length === 0 && (
          <div className="flex items-start gap-2 p-3 rounded-lg bg-destructive/5 border border-destructive/20 text-xs text-muted-foreground mb-2">
            <Info className="w-4 h-4 text-destructive shrink-0 mt-0.5" />
            <div>
              <strong className="text-destructive">No OAuth clients configured.</strong>{" "}
              You need to set up at least one Google OAuth client first. Open the "YouTube OAuth client pool" section above to add one.
              You'll need a Google Cloud Console project with the YouTube Data API enabled and OAuth credentials created.
              Make sure the <strong>redirect_uri</strong> is set to <code className="text-foreground">http://127.0.0.1:3335</code> (must match what you enter in Google Cloud Console).
            </div>
          </div>
        )}

        {importResults && importResults.length > 0 && (
          <div className="mt-3">
            <div className="text-sm text-muted-foreground mb-3">Found {importResults.length} channels. Click "Import" to add them:</div>
            <div className="grid gap-2">
              {importResults.map((ch) => {
                const already = channels.some((c) => c.youtube_channel_id === ch.channel_id);
                return (
                  <div key={ch.channel_id} className="flex items-center justify-between p-3 border border-border rounded-lg">
                    <div className="flex items-center gap-3 min-w-0">
                      {ch.thumbnail && <img src={ch.thumbnail} alt="" className="w-10 h-10 rounded-full" />}
                      <div className="min-w-0">
                        <div className="font-medium truncate text-sm">{ch.title}</div>
                        <div className="text-xs text-muted-foreground truncate">{ch.channel_id}</div>
                        <div className="text-xs text-muted-foreground">
                          {ch.subscriber_count > 0 && <span>{ch.subscriber_count.toLocaleString()} subscribers</span>}
                          {ch.video_count > 0 && <span> · {ch.video_count.toLocaleString()} videos</span>}
                        </div>
                      </div>
                    </div>
                    <Button size="sm" variant={already ? "secondary" : "default"} disabled={already} onClick={() => importGoogleChannel(ch)}>
                      {already ? "Added" : "Import"}
                    </Button>
                  </div>
                );
              })}
            </div>
            <div className="mt-3 flex gap-2">
              <Button size="sm" variant="secondary" onClick={() => {
                const newChannels = importResults.filter((ch) => !channels.some((c) => c.youtube_channel_id === ch.channel_id));
                if (newChannels.length) {
                  Promise.all(newChannels.map((ch) => importGoogleChannel(ch)));
                } else {
                  toast.info("All channels already added.");
                }
              }}>Import All New</Button>
              <Button size="sm" variant="ghost" onClick={() => setImportResults(null)}>Clear</Button>
            </div>
          </div>
        )}
      </Card>

      {/* ── YouTube Channel Discovery (tag-based input) ── */}
      <Card className="p-5 mb-6">
        <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-3 mb-3">
          <div>
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">YouTube channel discovery</div>
            <div className="text-sm text-muted-foreground max-w-2xl">Add handles, URLs, channel IDs, or Gmail addresses to discover and import channels.</div>
          </div>
          <div className="flex gap-2 flex-wrap">
            <Button size="sm" onClick={discoverChannels} disabled={isDiscovering || !discoveryTags.length}>
              <RefreshCw className={`w-4 h-4 mr-2 ${isDiscovering ? "animate-spin" : ""}`} />{isDiscovering ? "Discovering…" : "Discover channels"}
            </Button>
            <Button size="sm" variant="secondary" onClick={discoverChannelSwitcher} disabled={isDiscoveringSwitcher}>
              <Search className={`w-4 h-4 mr-2 ${isDiscoveringSwitcher ? "animate-pulse" : ""}`} />{isDiscoveringSwitcher ? "Opening channel switcher…" : "Auto Discover (Available Channels)"}
            </Button>
            <Button size="sm" variant="secondary" onClick={importDiscoveredChannels} disabled={!discoverResults.length}>
              Import discovered
            </Button>
            <Button size="sm" variant="ghost" onClick={clearAllTags}>Clear</Button>
          </div>
        </div>

        {/* Tag area */}
        <div className="min-h-[80px] border border-border rounded-lg p-3 flex flex-wrap items-start gap-2 focus-within:ring-1 focus-within:ring-ring transition-shadow">
          {discoveryTags.length === 0 && (
            <div className="text-sm text-muted-foreground italic py-1 select-none">No inputs yet — click "Add" to start adding handles, URLs, channel IDs, or Gmail addresses.</div>
          )}
          {discoveryTags.map((tag) => (
            <TagPill key={tag.id} tag={tag} onRemove={removeTag} />
          ))}
          <AddTagDropdown onAdd={addTag} />
        </div>

        {/* Gmail hint when email tags are present */}
        {emailTags.length > 0 && (
          <div className="mt-3 flex items-start gap-2 p-3 rounded-lg bg-amber-500/5 border border-amber-500/20 text-xs text-muted-foreground">
            <Info className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
            <div>
              <strong className="text-amber-300">Gmail inputs are handled differently:</strong>{" "}
              Email addresses can't be directly scraped for channels. Instead, use the <strong>"Import from Google Account"</strong> section above
              to authenticate with that Gmail and automatically discover all brand channels it manages.
              Alternatively, enter the associated YouTube handle or channel URL above to discover specific channels.
            </div>
          </div>
        )}

        {/* Category legend */}
        {discoveryTags.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-3 text-[10px] text-muted-foreground">
            {Object.entries(CATEGORIES).map(([key, cat]) => {
              const count = discoveryTags.filter((t) => t.category === key).length;
              if (count === 0) return null;
              return (
                <span key={key} className="flex items-center gap-1">
                  <span className={`w-1.5 h-1.5 rounded-full ${cat.dot}`} />
                  {count} {cat.label}{count !== 1 ? "s" : ""}
                </span>
              );
            })}
          </div>
        )}

        {discoverStatus && <div className="mt-3 text-sm text-muted-foreground">{discoverStatus}</div>}
        {discoverErrors.length > 0 && (
          <div className="mt-3 text-sm text-destructive">
            {discoverErrors.map((err, idx) => <div key={idx}>{err.query}: {err.detail || err.error}</div>)}
          </div>
        )}

        {discoverResults.length > 0 && (
          <div className="mt-5 grid gap-3">
            {discoverResults.map((item, idx) => {
              const already = channels.some((c) => c.youtube_channel_id === item.channel_id);
              return (
                <Card key={`${item.channel_id}-${idx}`} className="p-3 border border-border">
                  <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-3">
                    <div className="min-w-0">
                      <div className="font-semibold truncate">{item.title || item.channel_id || item.name || "Untitled channel"}</div>
                      <div className="text-xs text-muted-foreground truncate">{item.channel_id || item.channel_url}</div>
                      <div className="text-xs text-muted-foreground mt-1">Source: {item.source || item.final_url || item.query}</div>
                    </div>
                    <div className="flex items-center gap-2 flex-wrap">
                      <Badge variant={already ? "secondary" : "outline"}>{already ? "Existing" : "New"}</Badge>
                      <Button size="sm" variant="outline" onClick={() => importDiscoveredChannel(item)} disabled={already}>Import</Button>
                    </div>
                  </div>
                </Card>
              );
            })}
          </div>
        )}
      </Card>

      {/* ── Channel Cards Grid ── */}
      <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
        {channels.map((c) => {
          const picked = pickedClient[c.id];
          return (
            <Card key={c.id} data-testid={`channel-card-${c.id}`} className="p-5">
              <div className="flex items-start justify-between mb-3">
                <div className="min-w-0">
                  <div className="font-semibold truncate flex items-center gap-2"><Tv className="w-4 h-4 text-primary" />{c.name}</div>
                  <div className="text-xs text-muted-foreground truncate text-mono">{c.youtube_channel_id || "no id"}</div>
                </div>
                <button onClick={() => { api.deleteChannel(c.id).then(load); }} className="text-muted-foreground hover:text-destructive"><Trash2 className="w-4 h-4" /></button>
              </div>
              <div className="flex flex-wrap gap-2 mb-3">
                <Badge variant="secondary">{c.language}</Badge>
                {c.styles && <Badge variant="secondary" className="bg-primary/20 text-primary">{c.styles}</Badge>}
                {c.region && <Badge variant="outline">{c.region}</Badge>}
                <Badge variant={c.connected ? "default" : "outline"} data-testid={`channel-status-${c.id}`}>{c.connected ? "connected" : "not connected"}</Badge>
              </div>
              {picked && (
                <div className="mb-3 text-[10px] text-mono text-muted-foreground flex items-center gap-1"><KeyRound className="w-3 h-3" />will use: <span className="text-foreground">{picked.label}</span></div>
              )}
              {oauthClients.length > 1 ? (
                <div className="flex gap-2">
                  <Select onValueChange={(v) => startOauth(c, v === "_auto" ? undefined : v)}>
                    <SelectTrigger data-testid={`channel-pickclient-${c.id}`} className="flex-1"><SelectValue placeholder="Connect via..." /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="_auto">Auto-pick by language</SelectItem>
                      {oauthClients.map((oc) => <SelectItem key={oc.id} value={oc.id}>{oc.label}</SelectItem>)}
                    </SelectContent>
                  </Select>
                </div>
              ) : (
                <Button size="sm" variant={c.connected ? "secondary" : "default"} data-testid={`channel-oauth-${c.id}`} onClick={() => startOauth(c)} className="w-full">
                  <ShieldCheck className="w-3 h-3 mr-2" />{c.connected ? "Re-connect" : "Connect OAuth"}
                </Button>
              )}
            </Card>
          );
        })}
        {!channels.length && <Card className="p-10 col-span-full text-center text-muted-foreground border-dashed">No channels yet.</Card>}
      </div>

      {/* Channel Settings Panel (Global settings, AI translation, overrides) */}
      {showChannelSettings && (
        <div className="mt-8 fade-in">
          <div className="flex items-center gap-2 mb-4">
            <Languages className="w-5 h-5 text-primary" />
            <h2 className="text-2xl font-bold">AI Translation & Channel Settings</h2>
          </div>
          <ChannelSettingsPanel
            projectId={null}
            channels={channels}
            onRefresh={load}
          />
        </div>
      )}

      {/* ── OAuth manual fallback dialog ── */}
      <Dialog open={!!oauthDialog} onOpenChange={(v) => !v && setOauthDialog(null)}>
        <DialogContent>
          <DialogHeader><DialogTitle>Complete OAuth for {oauthDialog?.name}</DialogTitle></DialogHeader>
          <p className="text-sm text-muted-foreground">After authorizing in your system browser, the callback updates this channel automatically via the local server. If the redirect fails, paste the refresh token manually below.</p>
          <Input data-testid="oauth-refresh-input" placeholder="refresh_token (manual fallback)" value={refresh} onChange={(e) => setRefresh(e.target.value)} />
          <Input data-testid="oauth-ytid-input" placeholder="youtube channel id" value={yt} onChange={(e) => setYt(e.target.value)} />
          <Input data-testid="oauth-subs-input" placeholder="subscriber count" value={subs} onChange={(e) => setSubs(e.target.value)} />
          <Button data-testid="oauth-complete-btn" onClick={completeOauth}><LinkIcon className="w-4 h-4 mr-2" />Save manual token</Button>
        </DialogContent>
      </Dialog>
    </div>
  );
}
