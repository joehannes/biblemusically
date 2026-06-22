import { useEffect, useState, useCallback } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Badge } from "../components/ui/badge";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "../components/ui/dialog";
import { Tabs, TabsList, TabsTrigger } from "../components/ui/tabs";
import {
  User,
  Plus,
  Trash2,
  Sparkles,
  RefreshCw,
  Check,
  X,
  Image as ImageIcon,
  ChevronLeft,
  ChevronRight,
  Loader2,
  AlertCircle,
  Wand2,
  Eye,
  EyeOff,
} from "lucide-react";
import { toast } from "sonner";
import { getStepForPath } from "../lib/pageSteps";

export default function Characters() {
  const { songs, activeSongId, selectSong } = useStudio();
  const [characters, setCharacters] = useState([]);
  const [loading, setLoading] = useState(false);
  const [proposing, setProposing] = useState(false);
  const [generating, setGenerating] = useState({});
  const [editingChar, setEditingChar] = useState(null);
  const [editDraft, setEditDraft] = useState({
    name: "",
    description: "",
    image_prompt: "",
  });
  const [createOpen, setCreateOpen] = useState(false);
  const [createDraft, setCreateDraft] = useState({
    name: "",
    description: "",
    image_prompt: "",
  });
  const [viewMode, setViewMode] = useState("grid");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const chars = await api.listCharacters(activeSongId || null);
      setCharacters(chars);
    } catch (err) {
      console.error("Failed to load characters", err);
    } finally {
      setLoading(false);
    }
  }, [activeSongId]);

  useEffect(() => {
    load();
  }, [load]);

  const handleCreate = async () => {
    if (!createDraft.name.trim()) return toast.error("Character name required");
    try {
      await api.createCharacter({
        name: createDraft.name.trim(),
        description: createDraft.description.trim(),
        image_prompt: createDraft.image_prompt.trim(),
        song_id: activeSongId || null,
      });
      setCreateOpen(false);
      setCreateDraft({ name: "", description: "", image_prompt: "" });
      toast.success("Character created");
      load();
    } catch (err) {
      toast.error(
        "Failed to create character: " + (err?.toString?.() || "Unknown error"),
      );
    }
  };

  const handleUpdate = async () => {
    if (!editingChar) return;
    try {
      await api.updateCharacter(editingChar.id, editDraft);
      setEditingChar(null);
      toast.success("Character updated");
      load();
    } catch (err) {
      toast.error(
        "Failed to update character: " + (err?.toString?.() || "Unknown error"),
      );
    }
  };

  const handleDelete = async (id) => {
    try {
      await api.deleteCharacter(id);
      toast.success("Character deleted");
      load();
    } catch (err) {
      toast.error("Failed to delete character");
    }
  };

  const handleGenerate = async (id) => {
    setGenerating((prev) => ({ ...prev, [id]: true }));
    try {
      await api.generateCharacterImage(id);
      toast.success("Character image generation queued");
      // Poll for completion
      const poll = setInterval(async () => {
        try {
          const chars = await api.listCharacters(activeSongId || null);
          const updated = chars.find((c) => c.id === id);
          if (updated && updated.image_variants?.length > 0) {
            setCharacters(chars);
            setGenerating((prev) => ({ ...prev, [id]: false }));
            clearInterval(poll);
            toast.success("Character image ready!");
          }
        } catch {}
      }, 2000);
      setTimeout(() => {
        clearInterval(poll);
        setGenerating((prev) => ({ ...prev, [id]: false }));
      }, 120000);
    } catch (err) {
      setGenerating((prev) => ({ ...prev, [id]: false }));
      toast.error(
        "Failed to generate: " + (err?.toString?.() || "Unknown error"),
      );
    }
  };

  const handleVary = async (id) => {
    setGenerating((prev) => ({ ...prev, [id]: true }));
    try {
      await api.varyCharacterImage(id);
      toast.success("Character variation queued");
      const poll = setInterval(async () => {
        try {
          const chars = await api.listCharacters(activeSongId || null);
          const updated = chars.find((c) => c.id === id);
          if (
            updated &&
            updated.image_variants?.length >
              (characters.find((c) => c.id === id)?.image_variants?.length || 0)
          ) {
            setCharacters(chars);
            setGenerating((prev) => ({ ...prev, [id]: false }));
            clearInterval(poll);
            toast.success("New variation ready!");
          }
        } catch {}
      }, 2000);
      setTimeout(() => {
        clearInterval(poll);
        setGenerating((prev) => ({ ...prev, [id]: false }));
      }, 120000);
    } catch (err) {
      setGenerating((prev) => ({ ...prev, [id]: false }));
      toast.error("Failed to vary: " + (err?.toString?.() || "Unknown error"));
    }
  };

  const handleSelectVariant = async (id, idx) => {
    try {
      await api.selectCharacterVariant(id, idx);
      load();
      toast.success("Avatar selected");
    } catch (err) {
      toast.error("Failed to select variant");
    }
  };

  const handleDiscardVariant = async (id, idx) => {
    try {
      await api.discardCharacterVariant(id, idx);
      load();
      toast.success("Variant discarded");
    } catch (err) {
      toast.error("Failed to discard variant");
    }
  };

  const handleDiscardAll = async (id) => {
    try {
      await api.discardAllCharacterVariants(id);
      load();
      toast.success("All variants discarded");
    } catch (err) {
      toast.error("Failed to discard variants");
    }
  };

  const handlePropose = async () => {
    if (!activeSongId) return toast.error("Select a song first");
    setProposing(true);
    try {
      const result = await api.proposeCharacters(activeSongId);
      if (result?.ok) {
        toast.success(`Proposed ${result.count} characters from lyrics`);
        load();
      } else {
        toast.error(result?.error || "Proposal failed");
      }
    } catch (err) {
      toast.error(
        "Failed to propose characters: " +
          (err?.toString?.() || "Unknown error"),
      );
    } finally {
      setProposing(false);
    }
  };

  const getSelectedImage = (char) => {
    const variants = char.image_variants || [];
    const idx = char.selected_variant || 0;
    if (variants.length > 0 && idx < variants.length) return variants[idx];
    if (char.image_url) return char.image_url;
    return null;
  };

  const readySongs = songs.filter((s) => s.lyrics);

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">
        step {getStepForPath("/characters")}
      </div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Characters</h1>
        <div className="flex gap-2 flex-wrap">
          <Button
            size="sm"
            variant="secondary"
            onClick={handlePropose}
            disabled={proposing || !activeSongId}
          >
            {proposing ? (
              <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
            ) : (
              <Wand2 className="w-4 h-4 mr-2" />
            )}
            {proposing ? "Analyzing..." : "Auto-propose from lyrics"}
          </Button>
          <Button size="sm" onClick={() => setCreateOpen(true)}>
            <Plus className="w-4 h-4 mr-2" />
            New Character
          </Button>
        </div>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">
        Manage characters for consistent image generation. Characters are
        automatically referenced in image prompts for contextually coherent
        storytelling.
      </p>

      {/* Song selector */}
      <Card className="p-5 mb-6">
        <div className="flex items-center gap-3 flex-wrap">
          <div className="flex-1 min-w-[200px]">
            <Select value={activeSongId || ""} onValueChange={selectSong}>
              <SelectTrigger data-testid="characters-song-select">
                <SelectValue
                  placeholder={
                    readySongs.length
                      ? "Select song for context"
                      : "No songs with lyrics available"
                  }
                />
              </SelectTrigger>
              <SelectContent>
                {readySongs.map((s) => (
                  <SelectItem key={s.id} value={s.id}>
                    {s.title}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          {activeSongId && (
            <Badge variant="outline" className="text-[10px]">
              Characters will be linked to this song
            </Badge>
          )}
        </div>
      </Card>

      {/* View toggle */}
      <Tabs value={viewMode} onValueChange={setViewMode} className="mb-4">
        <TabsList>
          <TabsTrigger value="grid">
            <ImageIcon className="w-4 h-4 mr-1" />
            Grid
          </TabsTrigger>
          <TabsTrigger value="list">
            <User className="w-4 h-4 mr-1" />
            List
          </TabsTrigger>
        </TabsList>
      </Tabs>

      {/* Character grid/list */}
      {loading && characters.length === 0 ? (
        <div className="flex items-center justify-center py-20 text-muted-foreground">
          <Loader2 className="w-5 h-5 animate-spin mr-2" />
          Loading characters...
        </div>
      ) : characters.length === 0 ? (
        <Card className="p-10 text-center text-muted-foreground border-dashed">
          <User className="w-12 h-12 mx-auto mb-3 opacity-40" />
          <div className="text-lg font-medium mb-1">No characters yet</div>
          <div className="text-sm mb-4">
            Create characters manually or auto-propose them from song lyrics.
          </div>
          <div className="flex gap-2 justify-center">
            <Button
              size="sm"
              variant="secondary"
              onClick={handlePropose}
              disabled={!activeSongId}
            >
              <Wand2 className="w-4 h-4 mr-2" />
              Auto-propose
            </Button>
            <Button size="sm" onClick={() => setCreateOpen(true)}>
              <Plus className="w-4 h-4 mr-2" />
              Create
            </Button>
          </div>
        </Card>
      ) : (
        <div
          className={
            viewMode === "grid"
              ? "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4"
              : "space-y-3"
          }
        >
          {characters.map((char) => {
            const img = getSelectedImage(char);
            const variants = char.image_variants || [];
            const selectedIdx = char.selected_variant || 0;
            const isGenerating = generating[char.id];

            return (
              <Card
                key={char.id}
                className={`overflow-hidden ${viewMode === "list" ? "flex gap-4" : ""}`}
              >
                {/* Avatar area */}
                <div
                  className={`relative ${viewMode === "list" ? "w-32 h-32 shrink-0" : "w-full aspect-square"} bg-muted/30 overflow-hidden`}
                >
                  {img ? (
                    <img
                      src={img}
                      alt={char.name}
                      className="w-full h-full object-cover"
                    />
                  ) : isGenerating ? (
                    <div className="absolute inset-0 flex items-center justify-center text-xs text-muted-foreground bg-muted/50">
                      <Loader2 className="w-5 h-5 animate-spin mr-2" />
                      generating...
                    </div>
                  ) : (
                    <div className="absolute inset-0 flex items-center justify-center">
                      <User className="w-12 h-12 text-muted-foreground/40" />
                    </div>
                  )}

                  {/* Variant navigation */}
                  {variants.length > 1 && (
                    <div className="absolute bottom-1 left-1 right-1 flex items-center justify-between gap-1">
                      <button
                        onClick={() =>
                          handleSelectVariant(
                            char.id,
                            Math.max(0, selectedIdx - 1),
                          )
                        }
                        className="bg-black/60 text-white p-1 rounded text-[10px] hover:bg-black/80"
                        disabled={selectedIdx <= 0}
                      >
                        <ChevronLeft className="w-3 h-3" />
                      </button>
                      <span className="bg-black/60 text-white px-2 py-0.5 rounded text-[10px]">
                        {selectedIdx + 1}/{variants.length}
                      </span>
                      <button
                        onClick={() =>
                          handleSelectVariant(
                            char.id,
                            Math.min(variants.length - 1, selectedIdx + 1),
                          )
                        }
                        className="bg-black/60 text-white p-1 rounded text-[10px] hover:bg-black/80"
                        disabled={selectedIdx >= variants.length - 1}
                      >
                        <ChevronRight className="w-3 h-3" />
                      </button>
                    </div>
                  )}
                </div>

                {/* Info area */}
                <div className="p-4 flex-1 min-w-0">
                  <div className="flex items-start justify-between gap-2 mb-2">
                    <div className="min-w-0">
                      <div className="font-semibold truncate flex items-center gap-2">
                        <User className="w-4 h-4 text-primary shrink-0" />
                        {char.name}
                      </div>
                      {char.song_id && (
                        <div className="text-[10px] text-muted-foreground text-mono truncate">
                          linked to song
                        </div>
                      )}
                    </div>
                    <button
                      onClick={() => handleDelete(char.id)}
                      className="text-muted-foreground hover:text-destructive shrink-0"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>

                  {char.description && (
                    <div className="text-xs text-muted-foreground line-clamp-2 mb-2 italic">
                      {char.description}
                    </div>
                  )}

                  {char.image_prompt && (
                    <div className="text-[10px] text-muted-foreground/70 line-clamp-2 mb-3 font-mono">
                      {char.image_prompt}
                    </div>
                  )}

                  {/* Action buttons */}
                  <div className="flex flex-wrap gap-1.5">
                    <Button
                      size="sm"
                      variant={img ? "secondary" : "default"}
                      onClick={() => handleGenerate(char.id)}
                      disabled={isGenerating}
                      className="text-xs"
                    >
                      {isGenerating ? (
                        <Loader2 className="w-3 h-3 mr-1 animate-spin" />
                      ) : (
                        <Sparkles className="w-3 h-3 mr-1" />
                      )}
                      {img ? "Regenerate" : "Generate"}
                    </Button>
                    {img && (
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => handleVary(char.id)}
                        disabled={isGenerating}
                        className="text-xs"
                      >
                        <RefreshCw className="w-3 h-3 mr-1" />
                        Vary
                      </Button>
                    )}
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => {
                        setEditingChar(char);
                        setEditDraft({
                          name: char.name || "",
                          description: char.description || "",
                          image_prompt: char.image_prompt || "",
                        });
                      }}
                      className="text-xs"
                    >
                      Edit
                    </Button>
                    {variants.length > 0 && (
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => handleDiscardAll(char.id)}
                        className="text-xs text-destructive hover:text-destructive"
                      >
                        <X className="w-3 h-3 mr-1" />
                        Clear variants
                      </Button>
                    )}
                  </div>

                  {/* Variant thumbnails */}
                  {variants.length > 1 && (
                    <div className="mt-3 flex gap-1.5 overflow-x-auto pb-1">
                      {variants.map((v, idx) => (
                        <button
                          key={idx}
                          onClick={() => handleSelectVariant(char.id, idx)}
                          className={`relative w-10 h-10 rounded-md overflow-hidden border-2 shrink-0 transition-all ${
                            idx === selectedIdx
                              ? "border-primary ring-1 ring-primary"
                              : "border-border hover:border-muted-foreground"
                          }`}
                        >
                          <img
                            src={v}
                            alt=""
                            className="w-full h-full object-cover"
                          />
                          {idx === selectedIdx && (
                            <div className="absolute top-0 right-0 bg-primary text-primary-foreground p-0.5">
                              <Check className="w-2.5 h-2.5" />
                            </div>
                          )}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </Card>
            );
          })}
        </div>
      )}

      {/* Create dialog */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>New Character</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div>
              <div className="text-[10px] text-mono uppercase text-muted-foreground mb-1">
                Name *
              </div>
              <Input
                placeholder="Character name"
                value={createDraft.name}
                onChange={(e) =>
                  setCreateDraft({ ...createDraft, name: e.target.value })
                }
              />
            </div>
            <div>
              <div className="text-[10px] text-mono uppercase text-muted-foreground mb-1">
                Description
              </div>
              <Textarea
                placeholder="Brief character description"
                value={createDraft.description}
                onChange={(e) =>
                  setCreateDraft({
                    ...createDraft,
                    description: e.target.value,
                  })
                }
                rows={2}
              />
            </div>
            <div>
              <div className="text-[10px] text-mono uppercase text-muted-foreground mb-1">
                Image Prompt
              </div>
              <Textarea
                placeholder="Midjourney-style prompt for character portrait"
                value={createDraft.image_prompt}
                onChange={(e) =>
                  setCreateDraft({
                    ...createDraft,
                    image_prompt: e.target.value,
                  })
                }
                rows={3}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setCreateOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleCreate} disabled={!createDraft.name.trim()}>
              Create
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Edit dialog */}
      <Dialog
        open={!!editingChar}
        onOpenChange={(v) => !v && setEditingChar(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Edit Character</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div>
              <div className="text-[10px] text-mono uppercase text-muted-foreground mb-1">
                Name
              </div>
              <Input
                value={editDraft.name}
                onChange={(e) =>
                  setEditDraft({ ...editDraft, name: e.target.value })
                }
              />
            </div>
            <div>
              <div className="text-[10px] text-mono uppercase text-muted-foreground mb-1">
                Description
              </div>
              <Textarea
                value={editDraft.description}
                onChange={(e) =>
                  setEditDraft({ ...editDraft, description: e.target.value })
                }
                rows={2}
              />
            </div>
            <div>
              <div className="text-[10px] text-mono uppercase text-muted-foreground mb-1">
                Image Prompt
              </div>
              <Textarea
                value={editDraft.image_prompt}
                onChange={(e) =>
                  setEditDraft({ ...editDraft, image_prompt: e.target.value })
                }
                rows={3}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setEditingChar(null)}>
              Cancel
            </Button>
            <Button onClick={handleUpdate}>Save</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
