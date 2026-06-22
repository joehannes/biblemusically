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
import { 
  Sparkles, Globe, Tag, Type, Save, RefreshCw, CheckCircle2, 
  AlertCircle, ChevronRight, Languages, Upload, KeyRound 
} from "lucide-react";
import { toast } from "sonner";

export default function ChannelSettingsPanel({ projectId, channels, onRefresh }) {
  const [globalSettingsOpen, setGlobalSettingsOpen] = useState(false);
  const [translationDialogOpen, setTranslationDialogOpen] = useState(false);
  const [channelOverrideDialog, setChannelOverrideDialog] = useState(null);
  const [syncingChannels, setSyncingChannels] = useState(new Set());
  
  // Global settings state
  const [globalSettings, setGlobalSettings] = useState({
    topic_description: "",
    default_tags: [],
    branding_text: "",
    about_section: "",
    layout_preferences: {},
    content_style: "",
    upload_schedule: "",
  });
  const [tagInput, setTagInput] = useState("");
  const [savingGlobal, setSavingGlobal] = useState(false);
  const [translating, setTranslating] = useState(false);

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
        const syncedCount = result.synced_count || 0;
        const errorCount = result.error_count || 0;
        toast.success(`AI translation applied to ${syncedCount} channels (${errorCount} errors)`);
        if (onRefresh) onRefresh();
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

  const openChannelOverride = async (channel) => {
    try {
      const result = await api.getChannelSettings(channel.id);
      setChannelOverrideDialog({
        channel,
        data: result,
        overrides: {
          language: channel.language || "",
          region: channel.region || "",
          musical_style: channel.styles || "",
          custom_tags: channel.custom_tags || [],
          custom_about: channel.custom_about || "",
        }
      });
    } catch (err) {
      console.error("Failed to load channel settings:", err);
      toast.error("Failed to load channel settings");
    }
  };

  const saveChannelOverrides = async () => {
    if (!channelOverrideDialog) return;
    try {
      await api.updateChannelOverrides(
        channelOverrideDialog.channel.id,
        channelOverrideDialog.overrides
      );
      toast.success("Channel overrides saved");
      setChannelOverrideDialog(null);
      if (onRefresh) onRefresh();
    } catch (err) {
      console.error("Failed to save overrides:", err);
      toast.error("Failed to save channel overrides");
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
            >
              <Sparkles className="w-3.5 h-3.5" />
              Apply AI Translation
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
                  rows={3}
                />
              </div>
              <div className="space-y-2">
                <Label className="text-xs">Branding Text</Label>
                <Textarea
                  placeholder="Short branding message or tagline..."
                  value={globalSettings.branding_text}
                  onChange={(e) => setGlobalSettings(prev => ({ ...prev, branding_text: e.target.value }))}
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
                />
              </div>
              <div className="space-y-2">
                <Label className="text-xs">Upload Schedule</Label>
                <Input
                  placeholder="e.g., Daily, Weekly, Bi-weekly..."
                  value={globalSettings.upload_schedule}
                  onChange={(e) => setGlobalSettings(prev => ({ ...prev, upload_schedule: e.target.value }))}
                />
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

      {/* Channel Cards with Sync Status */}
      {channels?.length > 0 && (
        <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
          {channels.map((channel) => {
            const syncStatus = getSyncStatus(channel);
            const isSyncing = syncingChannels.has(channel.id);
            
            return (
              <Card key={channel.id} className="p-5">
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
                  <Button
                    size="sm"
                    variant="outline"
                    className="w-full justify-start"
                    onClick={() => openChannelOverride(channel)}
                  >
                    <ChevronRight className="w-3.5 h-3.5 mr-2" />
                    Edit Overrides
                  </Button>
                  
                  {channel.connected && (
                    <Button
                      size="sm"
                      variant={syncStatus.status === "synced" ? "secondary" : "default"}
                      className="w-full"
                      onClick={() => syncChannelToYouTube(channel.id)}
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

      {/* Channel Override Dialog */}
      {channelOverrideDialog && (
        <Dialog open={!!channelOverrideDialog} onOpenChange={() => setChannelOverrideDialog(null)}>
          <DialogContent className="max-w-2xl">
            <DialogHeader>
              <DialogTitle className="flex items-center gap-2">
                <Globe className="w-4 h-4" />
                Channel Overrides: {channelOverrideDialog.channel.name}
              </DialogTitle>
              <DialogDescription>
                Customize settings for this specific channel. These override the global settings.
              </DialogDescription>
            </DialogHeader>

            <div className="space-y-4 py-4">
              {channelOverrideDialog.data?.inherits_from_main && (
                <div className="flex items-start gap-2 p-3 rounded-lg bg-blue-500/10 border border-blue-500/20 text-xs">
                  <AlertCircle className="w-4 h-4 text-blue-400 shrink-0 mt-0.5" />
                  <div>
                    <strong className="text-blue-300">Inherits from main channel:</strong>
                    {" "}{channelOverrideDialog.data.main_channel?.name || "English channel"}
                    <div className="text-muted-foreground mt-1">
                      Modify any field below to override the inherited value.
                    </div>
                  </div>
                </div>
              )}

              <div className="grid md:grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label className="text-xs">Language</Label>
                  <Input
                    placeholder="e.g., Spanish, French, Japanese..."
                    value={channelOverrideDialog.overrides.language}
                    onChange={(e) => setChannelOverrideDialog(prev => ({
                      ...prev,
                      overrides: { ...prev.overrides, language: e.target.value }
                    }))}
                  />
                </div>
                <div className="space-y-2">
                  <Label className="text-xs">Region</Label>
                  <Input
                    placeholder="e.g., ES, FR, JP, US..."
                    value={channelOverrideDialog.overrides.region}
                    onChange={(e) => setChannelOverrideDialog(prev => ({
                      ...prev,
                      overrides: { ...prev.overrides, region: e.target.value }
                    }))}
                  />
                </div>
              </div>

              <div className="space-y-2">
                <Label className="text-xs">Musical Style</Label>
                <Input
                  placeholder="e.g., DnB, Lo-fi, Orchestral..."
                  value={channelOverrideDialog.overrides.musical_style}
                  onChange={(e) => setChannelOverrideDialog(prev => ({
                    ...prev,
                    overrides: { ...prev.overrides, musical_style: e.target.value }
                  }))}
                />
              </div>

              <div className="space-y-2">
                <Label className="text-xs flex items-center gap-2">
                  <Tag className="w-3.5 h-3.5" />
                  Custom Tags (optional)
                </Label>
                <div className="flex flex-wrap gap-2 p-3 border border-border rounded-lg min-h-[60px]">
                  {channelOverrideDialog.overrides.custom_tags?.length > 0 ? (
                    channelOverrideDialog.overrides.custom_tags.map((tag, idx) => (
                      <Badge
                        key={idx}
                        variant="secondary"
                        className="cursor-pointer hover:bg-destructive/20 hover:text-destructive"
                        onClick={() => {
                          const newTags = [...channelOverrideDialog.overrides.custom_tags];
                          newTags.splice(idx, 1);
                          setChannelOverrideDialog(prev => ({
                            ...prev,
                            overrides: { ...prev.overrides, custom_tags: newTags }
                          }));
                        }}
                      >
                        {tag} ×
                      </Badge>
                    ))
                  ) : (
                    <span className="text-sm text-muted-foreground italic">No custom tags — using global defaults</span>
                  )}
                </div>
              </div>

              <div className="space-y-2">
                <Label className="text-xs">Custom About Section (optional)</Label>
                <Textarea
                  placeholder="Leave empty to use global about section..."
                  value={channelOverrideDialog.overrides.custom_about || ""}
                  onChange={(e) => setChannelOverrideDialog(prev => ({
                    ...prev,
                    overrides: { ...prev.overrides, custom_about: e.target.value }
                  }))}
                  rows={3}
                />
              </div>
            </div>

            <DialogFooter>
              <Button variant="outline" onClick={() => setChannelOverrideDialog(null)}>
                Cancel
              </Button>
              <Button onClick={saveChannelOverrides}>
                <Save className="w-3.5 h-3.5 mr-2" />
                Save Overrides
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}
    </>
  );
}
