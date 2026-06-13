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
    const obj = { id, label: newLabel || "Custom preset", projectOverrides: {}, defaultName: "New Project", resetDefaults: false, resetChannels: false };
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
              <div key={p.id} className="flex items-center justify-between p-2 border rounded">
                <div className="flex-1 pr-2">
                  <div className="font-semibold">{p.label}</div>
                  <div className="text-xs text-muted-foreground">id: {p.id}</div>
                </div>
                <div className="flex items-center gap-2">
                  <Input placeholder="Default name" value={p.defaultName||""} onChange={e=>update(p.id,{defaultName:e.target.value})} className="w-48" />
                  <Button variant="outline" onClick={()=>remove(p.id)}><Trash2 className="w-4 h-4" /></Button>
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
