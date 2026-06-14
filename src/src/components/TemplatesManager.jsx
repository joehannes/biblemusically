import { useState } from "react";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Dialog, DialogContent, DialogTrigger, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "../components/ui/dialog";
import presetStore from "../lib/presetStore";
import { Trash2, Plus } from "lucide-react";

export default function TemplatesManager({ onChange }) {
  const [presets, setPresets] = useState(presetStore.loadPresets());
  const [editing, setEditing] = useState(null);
  const [newLabel, setNewLabel] = useState("");

  const persist = (next) => {
    setPresets(next);
    presetStore.savePresets(next);
    if (onChange) onChange(next);
  };

  const add = () => {
    const id = `custom-${Date.now()}`;
    const obj = {
      id,
      label: newLabel || "Custom preset",
      description: "",
      defaultName: "New Project",
      defaultTopic: "",
      defaultSchedule: "",
      projectOverrides: {},
      resetDefaults: false,
      resetChannels: false,
    };
    persist([obj, ...presets]);
    setNewLabel("");
  };

  const remove = (id) => persist(presets.filter(p=>p.id!==id));

  const update = (id, patch) => persist(presets.map(p=>p.id===id?{...p,...patch}:p));

  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="ghost">Manage templates</Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Templates</DialogTitle>
          <DialogDescription>Manage local templates/presets for new projects.</DialogDescription>
        </DialogHeader>
        <div className="space-y-3 mt-2">
          <div className="flex gap-2">
            <Input placeholder="New preset label" value={newLabel} onChange={e=>setNewLabel(e.target.value)} />
            <Button onClick={add}><Plus className="w-4 h-4" /></Button>
          </div>
          <div className="space-y-2 max-h-64 overflow-auto">
            {presets.map(p=> (
              <div key={p.id} className="space-y-2 p-3 border rounded">
              <div className="flex items-start justify-between gap-3">
                <div className="flex-1 min-w-0">
                  <div className="font-semibold truncate">{p.label}</div>
                  <div className="text-xs text-muted-foreground">id: {p.id}</div>
                </div>
                <Button variant="outline" onClick={()=>remove(p.id)}><Trash2 className="w-4 h-4" /></Button>
              </div>
              <div className="grid gap-2 sm:grid-cols-2">
                <Input placeholder="Default name" value={p.defaultName||""} onChange={e=>update(p.id,{defaultName:e.target.value})} />
                <Input placeholder="Default topic" value={p.defaultTopic||""} onChange={e=>update(p.id,{defaultTopic:e.target.value})} />
                <Input placeholder="Default schedule" value={p.defaultSchedule||""} onChange={e=>update(p.id,{defaultSchedule:e.target.value})} />
                <Input placeholder="Description" value={p.description||""} onChange={e=>update(p.id,{description:e.target.value})} />
              </div>
              <div className="grid gap-2 sm:grid-cols-2">
                <Input placeholder="Language or languages" value={(p.projectOverrides?.languages || []).join(", ")} onChange={e=>update(p.id,{ projectOverrides: { ...p.projectOverrides, languages: e.target.value.split(",").map(s=>s.trim()).filter(Boolean) } })} />
                <Input placeholder="Styles" value={(p.projectOverrides?.styles || []).join(", ")} onChange={e=>update(p.id,{ projectOverrides: { ...p.projectOverrides, styles: e.target.value.split(",").map(s=>s.trim()).filter(Boolean) } })} />
              </div>
              <div className="flex items-center gap-2">
                <label className="text-xs text-muted-foreground flex-1">Reset defaults <input type="checkbox" checked={p.resetDefaults} onChange={(e)=>update(p.id,{resetDefaults:e.target.checked})} className="ml-2" /></label>
                <label className="text-xs text-muted-foreground flex-1">Clear channels <input type="checkbox" checked={p.resetChannels} onChange={(e)=>update(p.id,{resetChannels:e.target.checked})} className="ml-2" /></label>
              </div>
            </div>
            ))}
          </div>
        </div>
        <DialogFooter>
          <Button onClick={()=>{presetStore.savePresets(presets); if (onChange) onChange(presets);}}>Close</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
