// ────────────────────────────────────────────────────────────────
// Channel Settings Panel with AI Translation
// Global settings, per-channel overrides, and YouTube sync status
// ────────────────────────────────────────────────────────────────

import { useState, useEffect } from "react";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Textarea } from "./ui/textarea";
import { Badge } from "./ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "./ui/dialog";
import { Label } from "./ui/label";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./ui/tabs";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";
import {
  Sparkles, Tag, Type, Save, RefreshCw, CheckCircle2,
  AlertCircle, ChevronRight, ChevronLeft, Languages, Upload,
  Palette, Music2, Wand2, Check, X,
} from "lucide-react";
import { toast } from "sonner";

export default function ChannelSettingsPanel({ projectId, channels, onRefresh }) {
  const [globalSettingsOpen, setGlobalSettingsOpen] = useState(false);
  const [translationDialogOpen, setTranslationDialogOpen] = useState(false);
  const [syncingChannels, setSyncingChannels] = useState(new Set());

  // ── Selected-channel properties panel ──
  // Reflects whichever channel is "current" — the top bar's prev/next cycle through `channels`
  // — so every settable field lives in one always-visible place instead of a per-card dialog.
  const [selectedChannelId, setSelectedChannelId] = useState(null);
  const [channelDraft, setChannelDraft] = useState(null);
  const [channelDraftMeta, setChannelDraftMeta] = useState(null);
  const [loadingChannel, setLoadingChannel] = useState(false);
  const [savingChannel, setSavingChannel] = useState(false);
  const [channelTagInput, setChannelTagInput] = useState("");
  
  // Global settings state
  const [globalSettings, setGlobalSettings] = useState({
    topic_description: "",
    default_tags: [],
    branding_text: "",
    about_section: "",
    layout_preferences: {},
    content_style: "",
    upload_schedule: "",
    default_style_preset_id: "",
    default_genre_preset_id: "",
  });
  const [tagInput, setTagInput] = useState("");
  const [savingGlobal, setSavingGlobal] = useState(false);
  const [translating, setTranslating] = useState(false);

  // ── Musical & visual preset packages (Style Studio / Sound Studio), surfaced here so a
  // channel's whole identity — text, sound, look — is editable from one place. Visual style is
  // its own resource (channel_styles, matched by language at generation time — see
  // styles.rs::channel_style_overrides); musical style is just the channel's existing
  // `musical_style` override field, pre-fillable from a genre preset.
  const [stylePresets, setStylePresets] = useState([]); // visual (ComfyUI) — Style Studio
  const [genrePresets, setGenrePresets] = useState([]); // musical — Sound Studio
  const [channelVisualStyle, setChannelVisualStyle] = useState({ preset_id: "", params: {} });
  const [savingVisualStyle, setSavingVisualStyle] = useState(false);
  const [flavoring, setFlavoring] = useState(null); // "visual" | "musical" | null
  const [visualFlavorPreview, setVisualFlavorPreview] = useState(null); // { flavored_text, rationale }
  const [musicalFlavorPreview, setMusicalFlavorPreview] = useState(null);

  useEffect(() => {
    api.listStylePresets?.().then((r) => setStylePresets(r?.presets || [])).catch(() => {});
    api.listGenrePresets?.().then((r) => setGenrePresets(r?.presets || [])).catch(() => {});
  }, []);

  // Load global settings on mount
  useEffect(() => {
    loadGlobalSettings();
  }, [projectId]);

  const loadGlobalSettings = async () => {
    try {
      const result = await api.getGlobalChannelSettings(projectId);
      if (result && Object.keys(result).length > 0) {
        setGlobalSettings({
          topic_description: result.topic_description || "",
          default_tags: result.default_tags || [],
          branding_text: result.branding_text || "",
          about_section: result.about_section || "",
          layout_preferences: result.layout_preferences || {},
          content_style: result.content_style || "",
          upload_schedule: result.upload_schedule || "",
          default_style_preset_id: result.default_style_preset_id || "",
          default_genre_preset_id: result.default_genre_preset_id || "",
        });
      }
    } catch (err) {
      console.error("Failed to load global settings:", err);
    }
  };

  const saveGlobalSettings = async () => {
    setSavingGlobal(true);
    try {
      await api.saveGlobalChannelSettings(projectId, globalSettings);
      toast.success("Global channel settings saved");
      setGlobalSettingsOpen(false);
    } catch (err) {
      console.error("Failed to save global settings:", err);
      toast.error("Failed to save global settings");
    } finally {
      setSavingGlobal(false);
    }
  };

  const addTag = () => {
    if (tagInput.trim() && !globalSettings.default_tags.includes(tagInput.trim())) {
      setGlobalSettings(prev => ({
        ...prev,
        default_tags: [...prev.default_tags, tagInput.trim()]
      }));
      setTagInput("");
    }
  };

  const removeTag = (tagToRemove) => {
    setGlobalSettings(prev => ({
      ...prev,
      default_tags: prev.default_tags.filter(tag => tag !== tagToRemove)
    }));
  };

  const handleTagKeyDown = (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      addTag();
    }
  };

  const applyTranslationToAll = async () => {
    setTranslating(true);
    try {
      const result = await api.translateAndApplySettings(projectId, null);
      if (result?.ok) {
        const translatedCount = result.translated_count || 0;
        const errorCount = result.error_count || 0;
        toast.success(
          `AI drafted translations for ${translatedCount} channel${translatedCount === 1 ? "" : "s"}${errorCount ? ` (${errorCount} error${errorCount === 1 ? "" : "s"})` : ""} — review below, then Sync to YouTube.`,
          { duration: 7000 },
        );
        if (errorCount) (result.errors || []).forEach((e) => toast.error(`${e.channel_name}: ${e.error}`, { duration: 9000 }));
        if (onRefresh) onRefresh();
        // Re-select the current channel so its freshly-drafted translated_* fields load in.
        if (selectedChannelId) { const id = selectedChannelId; setSelectedChannelId(null); setTimeout(() => setSelectedChannelId(id), 0); }
      } else {
        toast.error("Translation failed");
      }
    } catch (err) {
      console.error("Translation failed:", err);
      toast.error(err?.message || "Translation failed");
    } finally {
      setTranslating(false);
      setTranslationDialogOpen(false);
    }
  };

  // ── Bulk sync: pushes every connected channel's current draft (override, else AI-translated,
  // else global) live to YouTube, one at a time — a bulk write to real public channels, so it
  // gets its own confirmation dialog and runs sequentially with visible per-channel progress
  // rather than firing every request at once (gentler on YouTube's per-minute API quota too).
  const [bulkSyncOpen, setBulkSyncOpen] = useState(false);
  const [bulkSyncing, setBulkSyncing] = useState(false);
  const [bulkSyncProgress, setBulkSyncProgress] = useState(null); // { done, total, current }

  const syncableChannels = (channels || []).filter((c) => c.connected);

  const syncAllToYouTube = async () => {
    setBulkSyncing(true);
    let ok = 0, failed = 0;
    for (let i = 0; i < syncableChannels.length; i++) {
      const ch = syncableChannels[i];
      setBulkSyncProgress({ done: i, total: syncableChannels.length, current: ch.name });
      setSyncingChannels((prev) => new Set(prev).add(ch.id));
      try {
        await api.syncChannelToYouTube(ch.id);
        ok++;
      } catch (err) {
        failed++;
        toast.error(`${ch.name}: ${err?.message || err}`, { duration: 9000 });
      } finally {
        setSyncingChannels((prev) => { const n = new Set(prev); n.delete(ch.id); return n; });
      }
    }
    setBulkSyncProgress({ done: syncableChannels.length, total: syncableChannels.length, current: null });
    setBulkSyncing(false);
    setBulkSyncOpen(false);
    toast.success(`Synced ${ok}/${syncableChannels.length} channel${syncableChannels.length === 1 ? "" : "s"} to YouTube${failed ? ` (${failed} failed)` : ""}.`);
    if (onRefresh) onRefresh();
  };

  // Default to the first channel once the list loads, and re-anchor if the selected one
  // disappears (deleted, or the list hasn't loaded yet).
  useEffect(() => {
    if (!channels?.length) { setSelectedChannelId(null); return; }
    if (!selectedChannelId || !channels.some((c) => c.id === selectedChannelId)) {
      setSelectedChannelId(channels[0].id);
    }
  }, [channels, selectedChannelId]);

  const selectedIndex = channels?.findIndex((c) => c.id === selectedChannelId) ?? -1;
  const selectedChannel = selectedIndex >= 0 ? channels[selectedIndex] : null;

  // Load the draft whenever the selection changes.
  useEffect(() => {
    if (!selectedChannel) { setChannelDraft(null); setChannelDraftMeta(null); return; }
    setLoadingChannel(true);
    (async () => {
      try {
        const result = await api.getChannelSettings(selectedChannel.id);
        setChannelDraft({
          name: selectedChannel.name || "",
          language: selectedChannel.language || "",
          region: selectedChannel.region || "",
          musical_style: selectedChannel.styles || "",
          custom_tags: selectedChannel.custom_tags || [],
          custom_about: selectedChannel.custom_about || "",
        });
        setChannelDraftMeta({
          inherits_from_main: result?.inherits_from_main,
          main_channel: result?.main_channel,
        });
      } catch (err) {
        console.error("Failed to load channel settings:", err);
        toast.error("Failed to load channel settings");
      } finally {
        setLoadingChannel(false);
      }
    })();
    setVisualFlavorPreview(null);
    setMusicalFlavorPreview(null);
    api.getChannelStyle(selectedChannel.id)
      .then((r) => setChannelVisualStyle({ preset_id: r?.preset_id || "", params: r?.params || {} }))
      .catch(() => setChannelVisualStyle({ preset_id: "", params: {} }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedChannelId]);

  // ── Visual style pack (per-channel) ──
  const applyVisualPreset = (presetId) => {
    const p = stylePresets.find((x) => x.id === presetId);
    setChannelVisualStyle({ preset_id: presetId, params: p?.params ? { ...p.params } : {} });
    setVisualFlavorPreview(null);
  };

  const saveVisualStyle = async () => {
    if (!selectedChannel) return;
    setSavingVisualStyle(true);
    try {
      await api.setChannelStyle(selectedChannel.id, { preset_id: channelVisualStyle.preset_id, params: channelVisualStyle.params });
      toast.success("Visual style pinned to this channel.");
    } catch (err) {
      toast.error(err?.message || "Failed to save visual style");
    } finally {
      setSavingVisualStyle(false);
    }
  };

  // ── Musical style pack (per-channel) — pre-fills the existing `musical_style` override field. ──
  const applyGenrePreset = (presetId) => {
    const p = genrePresets.find((x) => x.id === presetId);
    if (p?.styles) setChannelDraft((prev) => ({ ...prev, musical_style: p.styles }));
    setMusicalFlavorPreview(null);
  };

  // ── Cultural flavor: AI-adapts whichever style is currently set for this channel's
  // language/region. Preview-only — Accept copies it into the field, Discard drops it. ──
  const runFlavor = async (kind) => {
    if (!selectedChannel) return;
    const baseText = kind === "visual" ? (channelVisualStyle.params.comfyui_prompt_prefix || "") : (channelDraft?.musical_style || "");
    if (!baseText.trim()) return toast.error(`Pick or type a ${kind} style first.`);
    setFlavoring(kind);
    try {
      const res = await api.aiFlavorStyle(selectedChannel.id, kind, baseText);
      if (kind === "visual") setVisualFlavorPreview(res);
      else setMusicalFlavorPreview(res);
    } catch (err) {
      toast.error(err?.message || "Cultural flavor failed");
    } finally {
      setFlavoring(null);
    }
  };

  const acceptVisualFlavor = () => {
    if (!visualFlavorPreview) return;
    setChannelVisualStyle((prev) => ({ ...prev, params: { ...prev.params, comfyui_prompt_prefix: visualFlavorPreview.flavored_text } }));
    setVisualFlavorPreview(null);
  };

  const acceptMusicalFlavor = () => {
    if (!musicalFlavorPreview) return;
    setChannelDraft((prev) => ({ ...prev, musical_style: musicalFlavorPreview.flavored_text }));
    setMusicalFlavorPreview(null);
  };

  const goToChannel = (dir) => {
    if (!channels?.length) return;
    const idx = selectedIndex < 0 ? 0 : selectedIndex;
    const next = (idx + dir + channels.length) % channels.length;
    setSelectedChannelId(channels[next].id);
  };

  const addChannelTag = () => {
    const v = channelTagInput.trim();
    if (!v || !channelDraft) return;
    if (channelDraft.custom_tags.includes(v)) { setChannelTagInput(""); return; }
    setChannelDraft((prev) => ({ ...prev, custom_tags: [...prev.custom_tags, v] }));
    setChannelTagInput("");
  };

  const removeChannelTag = (tag) => {
    setChannelDraft((prev) => ({ ...prev, custom_tags: prev.custom_tags.filter((t) => t !== tag) }));
  };

  const saveChannelDraft = async () => {
    if (!selectedChannel || !channelDraft) return;
    setSavingChannel(true);
    try {
      await api.updateChannelOverrides(selectedChannel.id, channelDraft);
      toast.success(`Saved "${channelDraft.name || selectedChannel.name}".`);
      if (onRefresh) onRefresh();
    } catch (err) {
      console.error("Failed to save channel:", err);
      toast.error("Failed to save channel properties");
    } finally {
      setSavingChannel(false);
    }
  };

  const syncChannelToYouTube = async (channelId) => {
    setSyncingChannels(prev => new Set(prev).add(channelId));
    try {
      const result = await api.syncChannelToYouTube(channelId);
      if (result?.ok) {
        toast.success("Channel synced to YouTube");
        if (onRefresh) onRefresh();
      }
    } catch (err) {
      console.error("Sync failed:", err);
      toast.error(err?.message || "Sync failed");
    } finally {
      setSyncingChannels(prev => {
        const next = new Set(prev);
        next.delete(channelId);
        return next;
      });
    }
  };

  const getSyncStatus = (channel) => {
    if (!channel.connected) return { status: "not_connected", label: "Not Connected", color: "bg-muted text-muted-foreground" };
    if (channel.last_synced_at) {
      const lastSync = new Date(channel.last_synced_at);
      const daysSinceSync = Math.floor((Date.now() - lastSync.getTime()) / (1000 * 60 * 60 * 24));
      if (daysSinceSync <= 7) {
        return { status: "synced", label: `Synced ${daysSinceSync}d ago`, color: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" };
      } else if (daysSinceSync <= 30) {
        return { status: "stale", label: `Synced ${daysSinceSync}d ago`, color: "bg-amber-500/15 text-amber-400 border-amber-500/30" };
      } else {
        return { status: "outdated", label: `Synced ${daysSinceSync}d ago`, color: "bg-destructive/15 text-destructive border-destructive/30" };
      }
    }
    return { status: "never", label: "Never Synced", color: "bg-muted text-muted-foreground" };
  };

  return (
    <>
      {/* Global Settings & Translation Actions */}
      <Card className="p-5 mb-6 border-primary/30 bg-primary/[0.03]">
        <div className="flex items-center justify-between mb-3">
          <div>
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Global Channel Settings</div>
            <div className="text-sm text-muted-foreground">Manage topic, tags, branding, and AI translation for all channels</div>
          </div>
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={() => setGlobalSettingsOpen(!globalSettingsOpen)}
              className="gap-2"
            >
              <Type className="w-3.5 h-3.5" />
              {globalSettingsOpen ? "Hide" : "Edit"} Global Settings
            </Button>
            <Button
              size="sm"
              onClick={() => setTranslationDialogOpen(true)}
              disabled={!channels?.length}
              className="gap-2"
              title="AI-drafts every channel's description, branding and tags from the fields above, translated and culturally adapted per channel's own language/region/musical style. Stages a draft only — review it below, nothing goes live yet."
            >
              <Sparkles className="w-3.5 h-3.5" />
              AI-draft all channels
            </Button>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => { setBulkSyncProgress(null); setBulkSyncOpen(true); }}
              disabled={!syncableChannels.length}
              className="gap-2"
              title="Pushes every connected channel's current draft (your override, else the AI translation, else the global settings) live to YouTube, one at a time."
            >
              <Upload className="w-3.5 h-3.5" />
              Sync all to YouTube
            </Button>
          </div>
        </div>

        {/* Expanded Global Settings Form */}
        {globalSettingsOpen && (
          <div className="space-y-4 mt-4 pt-4 border-t border-border fade-in">
            <div className="grid md:grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label className="text-xs">Topic Description</Label>
                <Textarea
                  placeholder="Describe your channel's main topic and content focus..."
                  value={globalSettings.topic_description}
                  onChange={(e) => setGlobalSettings(prev => ({ ...prev, topic_description: e.target.value }))}
                  data-autosave="channels:topic_description"
                  rows={3}
                />
              </div>
              <div className="space-y-2">
                <Label className="text-xs">Branding Text</Label>
                <Textarea
                  placeholder="Short branding message or tagline..."
                  value={globalSettings.branding_text}
                  onChange={(e) => setGlobalSettings(prev => ({ ...prev, branding_text: e.target.value }))}
                  data-autosave="channels:branding_text"
                  rows={3}
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label className="text-xs">About Section</Label>
              <Textarea
                placeholder="Detailed 'About' section for your channels..."
                value={globalSettings.about_section}
                onChange={(e) => setGlobalSettings(prev => ({ ...prev, about_section: e.target.value }))}
                  data-autosave="channels:about_section"
                rows={4}
              />
            </div>

            <div className="space-y-2">
              <Label className="text-xs flex items-center gap-2">
                <Tag className="w-3.5 h-3.5" />
                Default Tags
              </Label>
              <div className="flex gap-2">
                <Input
                  placeholder="Add a tag and press Enter..."
                  value={tagInput}
                  onChange={(e) => setTagInput(e.target.value)}
                  onKeyDown={handleTagKeyDown}
                  className="flex-1"
                />
                <Button size="sm" onClick={addTag} variant="outline">Add</Button>
              </div>
              <div className="flex flex-wrap gap-2 mt-2">
                {globalSettings.default_tags.map((tag) => (
                  <Badge
                    key={tag}
                    variant="secondary"
                    className="cursor-pointer hover:bg-destructive/20 hover:text-destructive transition-colors"
                    onClick={() => removeTag(tag)}
                  >
                    {tag} ×
                  </Badge>
                ))}
              </div>
            </div>

            <div className="grid md:grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label className="text-xs">Content Style</Label>
                <Input
                  placeholder="e.g., Educational, Entertainment, Documentary..."
                  value={globalSettings.content_style}
                  onChange={(e) => setGlobalSettings(prev => ({ ...prev, content_style: e.target.value }))}
                  data-autosave="channels:content_style"
                />
              </div>
              <div className="space-y-2">
                <Label className="text-xs">Upload Schedule</Label>
                <Input
                  placeholder="e.g., Daily, Weekly, Bi-weekly..."
                  value={globalSettings.upload_schedule}
                  onChange={(e) => setGlobalSettings(prev => ({ ...prev, upload_schedule: e.target.value }))}
                  data-autosave="channels:upload_schedule"
                />
              </div>
            </div>

            {/* General (project-wide) preset packages — the "general" layer of the general +
                per-channel hierarchy: applies to every channel that hasn't pinned its own pack
                (see channel_style_overrides in styles.rs for visual; the musical one just
                pre-fills a channel's style picker below when nothing's been chosen yet). */}
            <div className="grid md:grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label className="text-xs flex items-center gap-1.5"><Palette className="w-3.5 h-3.5 text-primary" />Default visual style pack (all channels)</Label>
                <Select value={globalSettings.default_style_preset_id || ""} onValueChange={(v) => setGlobalSettings((prev) => ({ ...prev, default_style_preset_id: v }))}>
                  <SelectTrigger className="w-full"><SelectValue placeholder="None — falls back to Settings' ComfyUI defaults" /></SelectTrigger>
                  <SelectContent>
                    {stylePresets.map((p) => <SelectItem key={p.id} value={p.id}>{p.name} · {p.pack}</SelectItem>)}
                  </SelectContent>
                </Select>
                <p className="text-[10px] text-muted-foreground">Used for any channel without its own pinned visual style below.</p>
              </div>
              <div className="space-y-2">
                <Label className="text-xs flex items-center gap-1.5"><Music2 className="w-3.5 h-3.5 text-primary" />Default musical style pack (all channels)</Label>
                <Select value={globalSettings.default_genre_preset_id || ""} onValueChange={(v) => setGlobalSettings((prev) => ({ ...prev, default_genre_preset_id: v }))}>
                  <SelectTrigger className="w-full"><SelectValue placeholder="None" /></SelectTrigger>
                  <SelectContent>
                    {genrePresets.map((p) => <SelectItem key={p.id} value={p.id}>{p.name} · {p.pack}</SelectItem>)}
                  </SelectContent>
                </Select>
                <p className="text-[10px] text-muted-foreground">Suggested when pinning a channel's musical style below, if it doesn't have one yet.</p>
              </div>
            </div>

            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" onClick={() => setGlobalSettingsOpen(false)}>Cancel</Button>
              <Button onClick={saveGlobalSettings} disabled={savingGlobal}>
                <Save className="w-3.5 h-3.5 mr-2" />
                {savingGlobal ? "Saving..." : "Save Global Settings"}
              </Button>
            </div>
          </div>
        )}
      </Card>

      {/* ── Channel Properties: reflects whichever channel is "current" ── */}
      {selectedChannel && (
        <Card className="p-5 mb-6">
          {/* Top bar: current channel + prev/next + jump + basic controls */}
          <div className="flex flex-wrap items-center gap-2 pb-3 mb-4 border-b border-border">
            <Button size="icon" variant="ghost" className="h-8 w-8 shrink-0" onClick={() => goToChannel(-1)} disabled={channels.length < 2} title="Previous channel">
              <ChevronLeft className="w-4 h-4" />
            </Button>
            <div className="min-w-0 flex-1">
              <div className="font-semibold truncate flex items-center gap-2">
                {selectedChannel.name}
                {getSyncStatus(selectedChannel).status === "synced" && <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />}
              </div>
              <div className="text-[11px] text-muted-foreground truncate text-mono">
                {selectedChannel.youtube_channel_id || "no YouTube id"} · channel {selectedIndex + 1} of {channels.length}
              </div>
            </div>
            <Button size="icon" variant="ghost" className="h-8 w-8 shrink-0" onClick={() => goToChannel(1)} disabled={channels.length < 2} title="Next channel">
              <ChevronRight className="w-4 h-4" />
            </Button>
            <select
              value={selectedChannelId || ""}
              onChange={(e) => setSelectedChannelId(e.target.value)}
              className="h-8 text-xs rounded-md border border-border bg-background px-2 max-w-[200px]"
              title="Jump to a channel"
            >
              {channels.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
            </select>
            <Badge variant="outline" className={`border ${getSyncStatus(selectedChannel).color}`}>
              {getSyncStatus(selectedChannel).label}
            </Badge>
            {selectedChannel.connected && (
              <Button
                size="sm"
                variant={getSyncStatus(selectedChannel).status === "synced" ? "secondary" : "default"}
                onClick={() => syncChannelToYouTube(selectedChannel.id)}
                disabled={syncingChannels.has(selectedChannel.id)}
              >
                {syncingChannels.has(selectedChannel.id) ? <RefreshCw className="w-3.5 h-3.5 mr-2 animate-spin" /> : <Upload className="w-3.5 h-3.5 mr-2" />}
                {syncingChannels.has(selectedChannel.id) ? "Syncing…" : "Sync to YouTube"}
              </Button>
            )}
          </div>

          {!channelDraft || loadingChannel ? (
            <div className="text-sm text-muted-foreground py-6 text-center">Loading channel properties…</div>
          ) : (
            <div className="space-y-4">
              {channelDraftMeta?.inherits_from_main && (
                <div className="flex items-start gap-2 p-3 rounded-lg bg-blue-500/10 border border-blue-500/20 text-xs">
                  <AlertCircle className="w-4 h-4 text-blue-400 shrink-0 mt-0.5" />
                  <div>
                    <strong className="text-blue-300">Inherits from main channel:</strong>
                    {" "}{channelDraftMeta.main_channel?.name || "English channel"}
                    <div className="text-muted-foreground mt-1">Modify any field below to override the inherited value.</div>
                  </div>
                </div>
              )}

              <div className="grid md:grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label className="text-xs">Channel Title</Label>
                  <Input
                    placeholder="Channel display name"
                    value={channelDraft.name}
                    onChange={(e) => setChannelDraft((prev) => ({ ...prev, name: e.target.value }))}
                  />
                </div>
                <div className="space-y-2">
                  <Label className="text-xs">YouTube Channel ID</Label>
                  <Input value={selectedChannel.youtube_channel_id || ""} readOnly disabled className="text-mono opacity-70" />
                </div>
              </div>

              <div className="grid md:grid-cols-3 gap-4">
                <div className="space-y-2">
                  <Label className="text-xs">Language</Label>
                  <Input
                    placeholder="e.g., Spanish, French, Japanese..."
                    value={channelDraft.language}
                    onChange={(e) => setChannelDraft((prev) => ({ ...prev, language: e.target.value }))}
                  />
                </div>
                <div className="space-y-2">
                  <Label className="text-xs">Region / Country</Label>
                  <Input
                    placeholder="e.g., ES, FR, JP, US..."
                    value={channelDraft.region}
                    onChange={(e) => setChannelDraft((prev) => ({ ...prev, region: e.target.value }))}
                  />
                </div>
                <div className="space-y-2">
                  <Label className="text-xs">Musical Style / Genre</Label>
                  <Input
                    placeholder="e.g., DnB, Lo-fi, Orchestral..."
                    value={channelDraft.musical_style}
                    onChange={(e) => setChannelDraft((prev) => ({ ...prev, musical_style: e.target.value }))}
                  />
                </div>
              </div>

              {/* Musical & visual preset packages, per channel — the "specialization" layer over
                  the project-wide defaults set in Global Settings above. Each has an optional AI
                  cultural-flavor pass: adapts the chosen preset's descriptor with texture
                  authentic to this channel's language/region, previewed before it's kept. */}
              <div className="grid md:grid-cols-2 gap-4">
                <div className="space-y-2 p-3 rounded-lg border border-border">
                  <Label className="text-xs flex items-center gap-1.5"><Palette className="w-3.5 h-3.5 text-primary" />Visual style pack</Label>
                  <Select value={channelVisualStyle.preset_id || ""} onValueChange={applyVisualPreset}>
                    <SelectTrigger className="w-full"><SelectValue placeholder="None pinned — uses the project default" /></SelectTrigger>
                    <SelectContent>
                      {stylePresets.map((p) => <SelectItem key={p.id} value={p.id}>{p.name} · {p.pack}</SelectItem>)}
                    </SelectContent>
                  </Select>
                  {channelVisualStyle.params?.comfyui_prompt_prefix && (
                    <p className="text-[11px] text-muted-foreground truncate" title={channelVisualStyle.params.comfyui_prompt_prefix}>
                      {channelVisualStyle.params.comfyui_prompt_prefix}
                    </p>
                  )}
                  <div className="flex items-center gap-2">
                    <Button size="sm" variant="outline" className="h-7 text-[11px]" onClick={() => runFlavor("visual")} disabled={flavoring === "visual" || !channelVisualStyle.params?.comfyui_prompt_prefix}>
                      {flavoring === "visual" ? <RefreshCw className="w-3 h-3 mr-1.5 animate-spin" /> : <Wand2 className="w-3 h-3 mr-1.5" />}
                      Cultural flavor
                    </Button>
                    <Button size="sm" className="h-7 text-[11px]" onClick={saveVisualStyle} disabled={savingVisualStyle}>
                      {savingVisualStyle ? "Pinning…" : "Pin to channel"}
                    </Button>
                  </div>
                  {visualFlavorPreview && (
                    <div className="text-[11px] bg-primary/[0.05] border border-primary/20 rounded-md p-2 space-y-1">
                      <p>{visualFlavorPreview.flavored_text}</p>
                      {visualFlavorPreview.rationale && <p className="text-muted-foreground italic">{visualFlavorPreview.rationale}</p>}
                      <div className="flex gap-1.5">
                        <button className="text-primary hover:underline flex items-center gap-0.5" onClick={acceptVisualFlavor}><Check className="w-3 h-3" />Use it</button>
                        <button className="text-muted-foreground hover:underline flex items-center gap-0.5" onClick={() => setVisualFlavorPreview(null)}><X className="w-3 h-3" />Discard</button>
                      </div>
                    </div>
                  )}
                </div>

                <div className="space-y-2 p-3 rounded-lg border border-border">
                  <Label className="text-xs flex items-center gap-1.5"><Music2 className="w-3.5 h-3.5 text-primary" />Musical style pack</Label>
                  <Select value="" onValueChange={applyGenrePreset}>
                    <SelectTrigger className="w-full"><SelectValue placeholder="Start from a preset…" /></SelectTrigger>
                    <SelectContent>
                      {genrePresets.map((p) => <SelectItem key={p.id} value={p.id}>{p.name} · {p.pack}</SelectItem>)}
                    </SelectContent>
                  </Select>
                  <p className="text-[11px] text-muted-foreground">Fills the Musical Style field above — edit freely after, then Save Channel Properties.</p>
                  <Button size="sm" variant="outline" className="h-7 text-[11px]" onClick={() => runFlavor("musical")} disabled={flavoring === "musical" || !channelDraft.musical_style}>
                    {flavoring === "musical" ? <RefreshCw className="w-3 h-3 mr-1.5 animate-spin" /> : <Wand2 className="w-3 h-3 mr-1.5" />}
                    Cultural flavor
                  </Button>
                  {musicalFlavorPreview && (
                    <div className="text-[11px] bg-primary/[0.05] border border-primary/20 rounded-md p-2 space-y-1">
                      <p>{musicalFlavorPreview.flavored_text}</p>
                      {musicalFlavorPreview.rationale && <p className="text-muted-foreground italic">{musicalFlavorPreview.rationale}</p>}
                      <div className="flex gap-1.5">
                        <button className="text-primary hover:underline flex items-center gap-0.5" onClick={acceptMusicalFlavor}><Check className="w-3 h-3" />Use it</button>
                        <button className="text-muted-foreground hover:underline flex items-center gap-0.5" onClick={() => setMusicalFlavorPreview(null)}><X className="w-3 h-3" />Discard</button>
                      </div>
                    </div>
                  )}
                </div>
              </div>

              <div className="space-y-2">
                <Label className="text-xs flex items-center gap-2">
                  <Tag className="w-3.5 h-3.5" />
                  Tags / Keywords
                </Label>
                <div className="flex gap-2">
                  <Input
                    placeholder="Add a tag and press Enter..."
                    value={channelTagInput}
                    onChange={(e) => setChannelTagInput(e.target.value)}
                    onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); addChannelTag(); } }}
                    className="flex-1"
                  />
                  <Button size="sm" onClick={addChannelTag} variant="outline">Add</Button>
                </div>
                <div className="flex flex-wrap gap-2 p-3 border border-border rounded-lg min-h-[52px]">
                  {channelDraft.custom_tags.length > 0 ? (
                    channelDraft.custom_tags.map((tag) => (
                      <Badge
                        key={tag}
                        variant="secondary"
                        className="cursor-pointer hover:bg-destructive/20 hover:text-destructive transition-colors"
                        onClick={() => removeChannelTag(tag)}
                      >
                        {tag} ×
                      </Badge>
                    ))
                  ) : (
                    <span className="text-sm text-muted-foreground italic">No tags yet — using global defaults on sync</span>
                  )}
                </div>
              </div>

              <div className="space-y-2">
                <Label className="text-xs">Description / About</Label>
                <Textarea
                  placeholder="Leave empty to use the global about section..."
                  value={channelDraft.custom_about}
                  onChange={(e) => setChannelDraft((prev) => ({ ...prev, custom_about: e.target.value }))}
                  rows={4}
                />
                <div className="text-[10px] text-muted-foreground text-right">{channelDraft.custom_about.length}/1000 (YouTube's limit)</div>
              </div>

              {/* AI-translated draft: what "AI-draft all channels" generated for this channel,
                  never applied automatically — review it, optionally pull it into the override
                  fields above (where it becomes editable), then Sync to YouTube when it's right. */}
              {(selectedChannel.translated_description || selectedChannel.translated_tags?.length || selectedChannel.translated_branding) && (
                <div className="space-y-2 p-3 rounded-lg border border-primary/20 bg-primary/[0.03]">
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex items-center gap-1.5 text-xs font-medium text-primary">
                      <Sparkles className="w-3.5 h-3.5" />
                      AI-translated draft
                      {selectedChannel.last_translated_at && (
                        <span className="text-[10px] text-muted-foreground font-normal">
                          · {new Date(selectedChannel.last_translated_at).toLocaleString()}
                        </span>
                      )}
                    </div>
                    <Button
                      size="sm" variant="outline" className="h-6 text-[10px]"
                      onClick={() => setChannelDraft((prev) => ({
                        ...prev,
                        custom_about: selectedChannel.translated_description || prev.custom_about,
                        custom_tags: selectedChannel.translated_tags?.length ? selectedChannel.translated_tags : prev.custom_tags,
                      }))}
                    >
                      Use as override
                    </Button>
                  </div>
                  {selectedChannel.translated_description && (
                    <p className="text-xs text-muted-foreground whitespace-pre-wrap">{selectedChannel.translated_description}</p>
                  )}
                  {selectedChannel.translated_tags?.length > 0 && (
                    <div className="flex flex-wrap gap-1">
                      {selectedChannel.translated_tags.map((t) => <Badge key={t} variant="outline" className="text-[10px]">{t}</Badge>)}
                    </div>
                  )}
                  {selectedChannel.translated_cultural_notes && (
                    <p className="text-[11px] text-muted-foreground italic border-t border-border/50 pt-1.5">{selectedChannel.translated_cultural_notes}</p>
                  )}
                </div>
              )}

              <div className="flex justify-end">
                <Button onClick={saveChannelDraft} disabled={savingChannel}>
                  <Save className="w-3.5 h-3.5 mr-2" />
                  {savingChannel ? "Saving..." : "Save Channel Properties"}
                </Button>
              </div>
            </div>
          )}
        </Card>
      )}

      {/* Channel Cards with Sync Status — click a card to bring it up above */}
      {channels?.length > 0 && (
        <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
          {channels.map((channel) => {
            const syncStatus = getSyncStatus(channel);
            const isSyncing = syncingChannels.has(channel.id);
            const isSelected = channel.id === selectedChannelId;

            return (
              <Card
                key={channel.id}
                onClick={() => setSelectedChannelId(channel.id)}
                className={`p-5 cursor-pointer transition-colors ${isSelected ? "border-primary ring-1 ring-primary/40" : "hover:border-primary/40"}`}
              >
                <div className="flex items-start justify-between mb-3">
                  <div className="min-w-0">
                    <div className="font-semibold truncate flex items-center gap-2">
                      {channel.name}
                      {syncStatus.status === "synced" && (
                        <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                      )}
                    </div>
                    <div className="text-xs text-muted-foreground truncate text-mono">
                      {channel.youtube_channel_id || "no id"}
                    </div>
                  </div>
                </div>

                <div className="flex flex-wrap gap-2 mb-3">
                  <Badge variant="secondary">{channel.language}</Badge>
                  {channel.styles && (
                    <Badge variant="secondary" className="bg-primary/20 text-primary">
                      {channel.styles}
                    </Badge>
                  )}
                  {channel.region && <Badge variant="outline">{channel.region}</Badge>}
                  <Badge
                    variant="outline"
                    className={`border ${syncStatus.color}`}
                  >
                    {syncStatus.label}
                  </Badge>
                </div>

                <div className="space-y-2">
                  <div className="text-[11px] text-muted-foreground flex items-center gap-1">
                    <ChevronRight className="w-3 h-3" />
                    {isSelected ? "Shown above" : "Click to edit properties above"}
                  </div>

                  {channel.connected && (
                    <Button
                      size="sm"
                      variant={syncStatus.status === "synced" ? "secondary" : "default"}
                      className="w-full"
                      onClick={(e) => { e.stopPropagation(); syncChannelToYouTube(channel.id); }}
                      disabled={isSyncing}
                    >
                      {isSyncing ? (
                        <RefreshCw className="w-3.5 h-3.5 mr-2 animate-spin" />
                      ) : (
                        <Upload className="w-3.5 h-3.5 mr-2" />
                      )}
                      {isSyncing ? "Syncing..." : "Sync to YouTube"}
                    </Button>
                  )}
                </div>
              </Card>
            );
          })}
        </div>
      )}

      {/* Translation Confirmation Dialog */}
      <Dialog open={translationDialogOpen} onOpenChange={setTranslationDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Sparkles className="w-4 h-4 text-primary" />
              Apply AI Translation to All Channels
            </DialogTitle>
            <DialogDescription>
              This will use OpenRouter AI to translate your global channel settings into each channel's target language and region.
              English channels will use the global settings directly.
            </DialogDescription>
          </DialogHeader>
          
          <div className="py-4">
            <div className="text-sm space-y-2">
              <p>The following will be translated for each non-English channel:</p>
              <ul className="list-disc list-inside text-muted-foreground ml-2">
                <li>Topic Description</li>
                <li>About Section</li>
                <li>Branding Text</li>
                <li>Default Tags (localized for regional search)</li>
              </ul>
              <p className="text-xs text-muted-foreground mt-3">
                Translated content will be stored in each channel document and can be synced to YouTube.
              </p>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setTranslationDialogOpen(false)}>
              Cancel
            </Button>
            <Button onClick={applyTranslationToAll} disabled={translating}>
              {translating ? (
                <>
                  <RefreshCw className="w-3.5 h-3.5 mr-2 animate-spin" />
                  Translating...
                </>
              ) : (
                <>
                  <Languages className="w-3.5 h-3.5 mr-2" />
                  Translate All
                </>
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Bulk Sync Confirmation Dialog — this is the consequential step (real, public YouTube
          channels get updated), so it gets its own explicit confirmation rather than piggybacking
          on the (non-live) translation dialog above. */}
      <Dialog open={bulkSyncOpen} onOpenChange={(o) => { if (!bulkSyncing) setBulkSyncOpen(o); }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Upload className="w-4 h-4 text-primary" />
              Sync {syncableChannels.length} channel{syncableChannels.length === 1 ? "" : "s"} to YouTube
            </DialogTitle>
            <DialogDescription>
              This pushes each connected channel's title, description, keywords and country live to its real YouTube
              channel — your override where you've set one, otherwise the AI-translated draft, otherwise the global
              settings. This is public and visible immediately.
            </DialogDescription>
          </DialogHeader>

          <div className="py-2 text-sm space-y-2">
            {!bulkSyncing && bulkSyncProgress?.done === bulkSyncProgress?.total && bulkSyncProgress?.total > 0 ? (
              <p className="text-emerald-400">Done — synced {bulkSyncProgress.done}/{bulkSyncProgress.total}.</p>
            ) : bulkSyncing ? (
              <div className="space-y-1.5">
                <div className="flex justify-between text-xs text-muted-foreground">
                  <span>{bulkSyncProgress?.current ? `Syncing "${bulkSyncProgress.current}"…` : "Starting…"}</span>
                  <span>{bulkSyncProgress?.done ?? 0}/{bulkSyncProgress?.total ?? syncableChannels.length}</span>
                </div>
                <div className="h-1.5 rounded-full bg-muted overflow-hidden">
                  <div
                    className="h-full bg-primary transition-all"
                    style={{ width: `${bulkSyncProgress?.total ? (100 * (bulkSyncProgress.done) / bulkSyncProgress.total) : 0}%` }}
                  />
                </div>
              </div>
            ) : (
              <p className="text-muted-foreground">{syncableChannels.length} channel{syncableChannels.length === 1 ? " is" : "s are"} connected and will be updated, one at a time.</p>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setBulkSyncOpen(false)} disabled={bulkSyncing}>
              {bulkSyncProgress?.done === bulkSyncProgress?.total && bulkSyncProgress?.total > 0 ? "Close" : "Cancel"}
            </Button>
            {!(bulkSyncProgress?.done === bulkSyncProgress?.total && bulkSyncProgress?.total > 0) && (
              <Button onClick={syncAllToYouTube} disabled={bulkSyncing || !syncableChannels.length}>
                {bulkSyncing ? (
                  <><RefreshCw className="w-3.5 h-3.5 mr-2 animate-spin" />Syncing…</>
                ) : (
                  <><Upload className="w-3.5 h-3.5 mr-2" />Sync all now</>
                )}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
