import { useState, useEffect, useCallback } from "react";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import { Badge } from "../components/ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Palette, Plus, Save, Trash2, Layers, Tv, Pin } from "lucide-react";
import StyleSample from "../components/StyleSample";
import { toast } from "sonner";
import StylePicker from "../components/StylePicker";

const STYLE_OPTIONS = [
  ["photoreal", "Photoreal"], ["comic", "Comic"], ["graphic_novel", "Graphic novel"],
  ["anime", "Anime"], ["oil_painting", "Oil painting"], ["watercolor", "Watercolor"],
];

const EMPTY_PARAMS = {
  comfyui_style: "photoreal",
  comfyui_prompt_prefix: "",
  comfyui_negative: "",
  comfyui_ckpt: "sd_xl_base_1.0.safetensors",
  comfyui_steps: 30,
  comfyui_cfg: 6.5,
  comfyui_width: 1024,
  comfyui_height: 1024,
  comfyui_ip_weight: 0.7,
};

// Reusable editor for a bundle of ComfyUI style params.
function ParamsEditor({ params, onChange }) {
  const set = (patch) => onChange({ ...params, ...patch });
  const num = (k) => (e) => set({ [k]: e.target.value });
  return (
    <div className="grid grid-cols-2 gap-3">
      <div className="space-y-2 col-span-2">
        <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Base style</Label>
        {/* The six originals stay in the dropdown so every stored setting still resolves to exactly
            what it did before. Everything new — and every blend — lives in the picker below, which
            is the only one of the two that can express a mix. */}
        <Select value={params.comfyui_style || "photoreal"} onValueChange={(v) => set({ comfyui_style: v })}>
          <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
          <SelectContent>{STYLE_OPTIONS.map(([v, l]) => <SelectItem key={v} value={v}>{l}</SelectItem>)}</SelectContent>
        </Select>
        <StylePicker value={params.comfyui_style} onChange={(v) => set({ comfyui_style: v })} />
      </div>
      <div className="space-y-1 col-span-2">
        <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Style nuance (prepended to prompts)</Label>
        <Input value={params.comfyui_prompt_prefix || ""} onChange={num("comfyui_prompt_prefix")} placeholder="e.g. moody rim light, textured ink" />
      </div>
      <div className="space-y-1 col-span-2">
        <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Extra negative</Label>
        <Input value={params.comfyui_negative || ""} onChange={num("comfyui_negative")} placeholder="appended to the style's negative" />
      </div>
      <div className="space-y-1 col-span-2">
        <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Checkpoint file</Label>
        <Input value={params.comfyui_ckpt || ""} onChange={num("comfyui_ckpt")} placeholder="sd_xl_base_1.0.safetensors" />
      </div>
      <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Steps</Label><Input type="number" value={params.comfyui_steps} onChange={num("comfyui_steps")} /></div>
      <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">CFG</Label><Input type="number" value={params.comfyui_cfg} onChange={num("comfyui_cfg")} /></div>
      <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Width</Label><Input type="number" value={params.comfyui_width} onChange={num("comfyui_width")} /></div>
      <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Height</Label><Input type="number" value={params.comfyui_height} onChange={num("comfyui_height")} /></div>
      <div className="space-y-1 col-span-2"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Character reference strength (0–1)</Label><Input type="number" step="0.05" value={params.comfyui_ip_weight} onChange={num("comfyui_ip_weight")} /></div>
    </div>
  );
}

export default function StyleStudio() {
  const [presets, setPresets] = useState([]);
  const [channels, setChannels] = useState([]);

  // Preset editor state
  const [editId, setEditId] = useState(null);
  const [pName, setPName] = useState("");
  const [pPack, setPPack] = useState("Custom");
  const [pParams, setPParams] = useState(EMPTY_PARAMS);

  // Channel sticky-style state
  const [selChannel, setSelChannel] = useState("");
  const [chPreset, setChPreset] = useState("");
  const [chParams, setChParams] = useState(EMPTY_PARAMS);

  const loadPresets = useCallback(async () => {
    try { const r = await api.listStylePresets(); setPresets(r?.presets || []); }
    catch (e) { console.error(e); }
  }, []);

  useEffect(() => {
    loadPresets();
    (async () => {
      try { const r = await api.listChannels(); setChannels(r?.channels || r || []); }
      catch (e) { console.error(e); }
    })();
  }, [loadPresets]);

  const packs = [...new Set(presets.map((p) => p.pack || "Custom"))].sort();

  const startNew = () => { setEditId(null); setPName(""); setPPack("Custom"); setPParams(EMPTY_PARAMS); };
  const editPreset = (p) => { setEditId(p.id); setPName(p.name || ""); setPPack(p.pack || "Custom"); setPParams({ ...EMPTY_PARAMS, ...(p.params || {}) }); };

  const savePreset = async () => {
    if (!pName.trim()) return toast.error("Name the preset first.");
    try {
      await api.saveStylePreset({ id: editId || "", name: pName, pack: pPack || "Custom", params: pParams });
      toast.success("Preset saved.");
      startNew();
      loadPresets();
    } catch (e) { console.error(e); toast.error("Save failed."); }
  };

  const removePreset = async (p) => {
    if (p.builtin || String(p.id).startsWith("builtin-")) return toast.error("Built-in presets can't be deleted.");
    try { await api.deleteStylePreset(p.id); toast.success("Deleted."); loadPresets(); if (editId === p.id) startNew(); }
    catch (e) { toast.error(e?.toString() || "Delete failed."); }
  };

  const selectChannel = async (id) => {
    setSelChannel(id);
    try {
      const r = await api.getChannelStyle(id);
      setChPreset(r?.preset_id || "");
      setChParams({ ...EMPTY_PARAMS, ...(r?.params || {}) });
    } catch (e) { console.error(e); setChParams(EMPTY_PARAMS); setChPreset(""); }
  };

  const applyPresetToChannel = (presetId) => {
    setChPreset(presetId);
    const p = presets.find((x) => x.id === presetId);
    if (p?.params) setChParams({ ...EMPTY_PARAMS, ...p.params });
  };

  const saveChannelStyle = async () => {
    if (!selChannel) return toast.error("Pick a channel first.");
    try {
      await api.setChannelStyle(selChannel, { preset_id: chPreset || "", params: chParams });
      toast.success("Channel style pinned — it now sticks across sessions.");
    } catch (e) { console.error(e); toast.error("Save failed."); }
  };

  const selChannelObj = channels.find((c) => c.id === selChannel);

  return (
    <div className="p-6 max-w-6xl mx-auto">
      <div className="flex items-center gap-2 mb-1">
        <Palette className="w-5 h-5 text-primary" />
        <h1 className="text-xl font-semibold">Style Studio</h1>
      </div>
      <p className="text-muted-foreground mb-6 max-w-3xl">
        Build reusable <span className="font-medium">style presets</span> (grouped into packs), then pin a sticky style to each
        YouTube channel. Pinned styles persist across sessions and are applied automatically to every image
        generated for that channel's language (ComfyUI engine).
      </p>

      <div className="grid lg:grid-cols-2 gap-5">
        {/* ── Preset library ─────────────────────────── */}
        <Card className="p-5">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-2"><Layers className="w-4 h-4 text-primary" /><h2 className="font-semibold">Preset packs</h2></div>
            <Button size="sm" variant="secondary" onClick={startNew}><Plus className="w-3 h-3 mr-1" />New</Button>
          </div>

          <div className="space-y-4 max-h-[320px] overflow-auto pr-1">
            {packs.map((pack) => (
              <div key={pack}>
                <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-1.5">{pack}</div>
                <div className="space-y-1.5">
                  {presets.filter((p) => (p.pack || "Custom") === pack).map((p) => (
                    <div key={p.id} className={`flex items-center justify-between rounded-md border px-3 py-2 ${editId === p.id ? "border-primary" : "border-border/60"}`}>
                      <button className="text-left flex-1 min-w-0" onClick={() => editPreset(p)}>
                        <div className="text-sm font-medium truncate">{p.name} {p.builtin && <Badge variant="secondary" className="ml-1 text-[9px]">built-in</Badge>}</div>
                        <div className="text-xs text-muted-foreground truncate">{p.params?.comfyui_style} · {p.params?.comfyui_prompt_prefix || "no nuance"}</div>
                      </button>
                      <StyleSample kind="image" sampleKey={`style:${p.id}`} label={p.name}
                        spec={`${p.params?.comfyui_style || "photoreal"} style${p.params?.comfyui_prompt_prefix ? ", " + p.params.comfyui_prompt_prefix : ""}, a serene landscape with a lone figure at golden hour`} />
                      <button className="text-muted-foreground hover:text-red-400 p-1" title="Delete" onClick={() => removePreset(p)}><Trash2 className="w-4 h-4" /></button>
                    </div>
                  ))}
                </div>
              </div>
            ))}
            {!presets.length && <p className="text-xs text-muted-foreground">No presets yet.</p>}
          </div>

          <div className="mt-5 pt-4 border-t border-border/50">
            <div className="text-sm font-semibold mb-3">{editId ? "Edit preset" : "New preset"}</div>
            <div className="grid grid-cols-2 gap-3 mb-3">
              <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Name</Label><Input value={pName} onChange={(e) => setPName(e.target.value)} placeholder="e.g. Noir Comic" /></div>
              <div className="space-y-1"><Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Pack</Label><Input value={pPack} onChange={(e) => setPPack(e.target.value)} placeholder="e.g. Illustrated" /></div>
            </div>
            <ParamsEditor params={pParams} onChange={setPParams} />
            <div className="flex gap-2 mt-3">
              <Button size="sm" onClick={savePreset}><Save className="w-3 h-3 mr-2" />{editId ? "Update" : "Create"}</Button>
              {editId && <Button size="sm" variant="ghost" onClick={startNew}>Cancel</Button>}
            </div>
          </div>
        </Card>

        {/* ── Per-channel sticky styles ─────────────── */}
        <Card className="p-5">
          <div className="flex items-center gap-2 mb-4"><Tv className="w-4 h-4 text-primary" /><h2 className="font-semibold">Per-channel sticky style</h2></div>

          <div className="space-y-1 mb-4">
            <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Channel</Label>
            <Select value={selChannel} onValueChange={selectChannel}>
              <SelectTrigger className="w-full"><SelectValue placeholder="Select a channel…" /></SelectTrigger>
              <SelectContent>
                {channels.map((c) => <SelectItem key={c.id} value={c.id}>{c.name} {c.language ? `· ${c.language}` : ""}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>

          {selChannel ? (
            <>
              {selChannelObj?.language && (
                <p className="text-xs text-muted-foreground mb-3 flex items-center gap-1">
                  <Pin className="w-3 h-3" /> Applied automatically to images for <span className="font-medium">{selChannelObj.language}</span>-language songs.
                </p>
              )}
              <div className="space-y-1 mb-3">
                <Label className="text-[10px] uppercase tracking-widest text-muted-foreground">Start from preset</Label>
                <Select value={chPreset} onValueChange={applyPresetToChannel}>
                  <SelectTrigger className="w-full"><SelectValue placeholder="Optional — pick a preset to load its params…" /></SelectTrigger>
                  <SelectContent>
                    {presets.map((p) => <SelectItem key={p.id} value={p.id}>{p.name} · {p.pack}</SelectItem>)}
                  </SelectContent>
                </Select>
              </div>
              <ParamsEditor params={chParams} onChange={(p) => { setChParams(p); }} />
              <div className="flex gap-2 mt-3">
                <Button size="sm" onClick={saveChannelStyle}><Pin className="w-3 h-3 mr-2" />Pin to channel</Button>
              </div>
            </>
          ) : (
            <p className="text-sm text-muted-foreground">Select a channel to pin a sticky style. Tip: apply a preset, tweak the nuances, then pin.</p>
          )}
        </Card>
      </div>
    </div>
  );
}
