import { useEffect, useRef, useState } from "react";
import { useStudio } from "../lib/store";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Switch } from "../components/ui/switch";
import { Badge } from "../components/ui/badge";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Slider } from "../components/ui/slider";
import { Bot, Plus, X, Save, Upload, Download, Sparkles, FlaskConical, ArrowRight, Wand2, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useNavigate } from "react-router-dom";

const STYLE_PACKS = {
  "Biblical Concept": ["cinematic biblical concept art", "sacred celestial symbolism", "radiant divine light", "visionary spiritual realism", "majestic ancient atmosphere"],
  "Trash Polka": ["trash polka textures", "ink splatter", "monochrome with red accents", "tattoo aesthetics"],
  "Iconography": ["orthodox iconography", "gold leaf halos", "byzantine flat perspective", "sacred geometry"],
  "Art Nouveau": ["art nouveau", "alphonse mucha style", "ornate floral borders", "soft pastels"],
  "Cyberpunk Sacred": ["neon-lit cathedral", "cyberpunk holiness", "translucent stained glass", "techno-baroque"],
  "Cinematic": ["cinematic lighting", "shallow depth of field", "anamorphic lens", "color grading"],
  "Painterly": ["oil painting", "thick impasto", "rembrandt lighting", "renaissance composition"],
  "Photo Real": ["photorealistic", "8k detail", "natural lighting", "documentary photography"],
};

const DEFAULT_CFG = {
  artist: "Joehannes Lightkid",
  title_pattern: "{artist} - {book} {chapter} ({styles})",
  generate: { title: true, language: false, styles: false, lyrics: true, annotations: true, image_styles: true },
  themes: { global: "sacred, hopeful, divine light", per_language: {}, per_channel: {} },
  mj_ar: "16:9", mj_v: "8.1", mj_chaos: 0, mj_stylize: 100, mj_weird: 0,
  style_keywords: STYLE_PACKS["Biblical Concept"].slice(),
  targets: [
    { language: "English", styles: "Messianic Liquid Drum and Bass" },
    { language: "German", styles: "Messianic Alpine Electronic Folk" },
  ],
  sections: [],
};

export default function AIComposer() {
  const nav = useNavigate();
  const { activeProjectId, refreshSongs } = useStudio();
  const [cfg, setCfg] = useState(DEFAULT_CFG);
  const [profiles, setProfiles] = useState(() => JSON.parse(localStorage.getItem("studio:composer-profiles") || "{}"));
  const [profileName, setProfileName] = useState("default");
  const [chapterRef, setChapterRef] = useState("");
  const [chapterText, setChapterText] = useState("");
  const [chapterLang, setChapterLang] = useState("English");
  const [chapterBook, setChapterBook] = useState("john");
  const [chapterNum, setChapterNum] = useState(1);
  const [busy, setBusy] = useState(false);
  const [items, setItems] = useState([]);
  const [selStart, setSelStart] = useState(0); const [selEnd, setSelEnd] = useState(0);
  const [sectionIdea, setSectionIdea] = useState("");
  const importRef = useRef();
  const taRef = useRef();

  // Load bible selection if pushed from BibleSources screen
  useEffect(() => {
    const raw = localStorage.getItem("studio:bible-selection");
    if (raw) {
      try {
        const b = JSON.parse(raw);
        setChapterRef(b.reference); setChapterText(b.text); setChapterLang(b.language || "English");
        setChapterBook(b.book || "john"); setChapterNum(b.chapter || 1);
      } catch {}
      localStorage.removeItem("studio:bible-selection");
    }
    api.getComposeConfig().then(d => { if (d && Object.keys(d).length) setCfg(c => ({ ...c, ...d })); });
  }, []);

  const saveCfg = async () => { await api.saveComposeConfig(cfg); toast.success("Saved on server"); };

  const saveProfile = () => {
    const next = { ...profiles, [profileName]: cfg };
    setProfiles(next); localStorage.setItem("studio:composer-profiles", JSON.stringify(next));
    toast.success(`Profile "${profileName}" saved locally`);
  };
  const loadProfile = (name) => { const c = profiles[name]; if (!c) return; setCfg(c); setProfileName(name); toast.success(`Loaded "${name}"`); };
  const deleteProfile = (name) => { const next = { ...profiles }; delete next[name]; setProfiles(next); localStorage.setItem("studio:composer-profiles", JSON.stringify(next)); };
  const exportProfile = () => {
    const blob = new Blob([JSON.stringify(cfg, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob); const a = document.createElement("a");
    a.href = url; a.download = `composer-${profileName}.json`; a.click(); URL.revokeObjectURL(url);
  };
  const importProfile = async (e) => {
    const f = e.target.files?.[0]; if (!f) return;
    try { const t = await f.text(); const c = JSON.parse(t); setCfg(c); toast.success("Profile imported"); }
    catch { toast.error("Invalid JSON"); }
  };

  const onTextSelect = () => {
    const ta = taRef.current; if (!ta) return;
    setSelStart(ta.selectionStart); setSelEnd(ta.selectionEnd);
  };
  const addSection = () => {
    const text = chapterText.slice(selStart, selEnd).trim();
    if (!text) return toast.error("Select some text in the chapter first");
    const next = [...(cfg.sections || []), { text, idea: sectionIdea }];
    setCfg({ ...cfg, sections: next }); setSectionIdea("");
    toast.success(`Section added (${next.length})`);
  };
  const removeSection = (idx) => setCfg({ ...cfg, sections: cfg.sections.filter((_, i) => i !== idx) });

  const toggleKw = (k) => {
    const list = cfg.style_keywords || [];
    setCfg({ ...cfg, style_keywords: list.includes(k) ? list.filter(x=>x!==k) : [...list, k] });
  };
  const applyPack = (name) => {
    const pack = STYLE_PACKS[name] || [];
    setCfg({ ...cfg, style_keywords: Array.from(new Set([...(cfg.style_keywords||[]), ...pack])) });
  };
  const clearKw = () => setCfg({ ...cfg, style_keywords: [] });

  const addTarget = () => setCfg({ ...cfg, targets: [...(cfg.targets||[]), { language: "English", styles: "" }] });
  const updateTarget = (i, k, v) => { const t=[...cfg.targets]; t[i] = {...t[i], [k]: v}; setCfg({...cfg, targets: t}); };
  const removeTarget = (i) => setCfg({ ...cfg, targets: cfg.targets.filter((_,x)=>x!==i) });

  const mjParams = `--ar ${cfg.mj_ar} --v ${cfg.mj_v}${cfg.mj_chaos?` --chaos ${cfg.mj_chaos}`:""}${cfg.mj_stylize!==100?` --stylize ${cfg.mj_stylize}`:""}${cfg.mj_weird?` --weird ${cfg.mj_weird}`:""}`;

  const generate = async () => {
    if (!chapterText.trim()) return toast.error("Load a bible chapter first (Bible Sources)");
    setBusy(true); setItems([]);
    try {
      const r = await api.composeLyrics({
        chapter_text: chapterText,
        sections: cfg.sections || [],
        targets: cfg.targets || [],
        themes: cfg.themes || {},
        mj_params: mjParams,
        style_keywords: cfg.style_keywords || [],
        generate: cfg.generate || {},
        title_pattern: cfg.title_pattern,
        artist: cfg.artist,
      });
      if (r.error) toast.error(r.error);
      if (r.items?.length) { setItems(r.items); toast.success(`Generated ${r.count} song variants via ${r.model}`); }
    } finally { setBusy(false); }
  };

  const sendToLyrics = async () => {
    if (!activeProjectId) return toast.error("Select a project first");
    if (!items.length) return toast.error("Generate first");
    const res = await api.importLyrics(activeProjectId, items);
    toast.success(`Imported ${res.created} songs`); refreshSongs(); nav("/lyrics");
  };
  const downloadJson = () => {
    const blob = new Blob([JSON.stringify(items, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob); const a = document.createElement("a");
    a.href = url; a.download = `lyrics-${Date.now()}.json`; a.click(); URL.revokeObjectURL(url);
  };

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step 1 · author</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">AI Composer</h1>
        <div className="flex gap-2">
          <Button size="sm" variant="secondary" data-testid="composer-save-cfg" onClick={saveCfg}><Save className="w-3 h-3 mr-2" />Save config</Button>
          <Button data-testid="composer-generate-btn" onClick={generate} disabled={busy}>{busy ? <FlaskConical className="w-4 h-4 mr-2 animate-pulse" /> : <Sparkles className="w-4 h-4 mr-2" />}Generate via Qwen</Button>
        </div>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">Authors a multi-channel <span className="text-mono">lyrics.json</span> from your bible chapter + themes + section ideas. Powered by OpenRouter Qwen (free tier).</p>

      <div className="grid lg:grid-cols-3 gap-6">
        {/* LEFT — config */}
        <Card className="p-5 lg:col-span-1 space-y-5">
          <div>
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2">Profiles</div>
            <div className="flex gap-2">
              <Input data-testid="composer-profile-name" value={profileName} onChange={e=>setProfileName(e.target.value)} placeholder="profile name" />
              <Button size="sm" variant="secondary" data-testid="composer-profile-save" onClick={saveProfile}><Save className="w-3 h-3" /></Button>
              <Button size="sm" variant="ghost" onClick={exportProfile}><Download className="w-3 h-3" /></Button>
              <Button size="sm" variant="ghost" onClick={()=>importRef.current?.click()}><Upload className="w-3 h-3" /></Button>
              <input ref={importRef} type="file" accept=".json" className="hidden" onChange={importProfile} />
            </div>
            <div className="flex flex-wrap gap-1 mt-2">
              {Object.keys(profiles).map(n => (
                <div key={n} className="flex items-center gap-1">
                  <button data-testid={`composer-profile-${n}`} className="text-[10px] text-mono px-2 py-0.5 bg-muted rounded hover:bg-secondary" onClick={()=>loadProfile(n)}>{n}</button>
                  <button onClick={()=>deleteProfile(n)} className="text-muted-foreground hover:text-destructive"><X className="w-3 h-3" /></button>
                </div>
              ))}
            </div>
          </div>

          <div>
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2">Field generation toggles</div>
            <div className="space-y-2 text-sm">
              {Object.keys(cfg.generate).map(k => (
                <label key={k} className="flex items-center justify-between gap-2">
                  <span className="text-mono text-xs">{k}</span>
                  <Switch data-testid={`composer-toggle-${k}`} checked={cfg.generate[k]} onCheckedChange={(v)=>setCfg({...cfg, generate:{...cfg.generate,[k]:v}})} />
                </label>
              ))}
            </div>
          </div>

          <div>
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2">Themes (salt &amp; pepper)</div>
            <Input data-testid="composer-theme-global" placeholder="global theme" value={cfg.themes.global} onChange={e=>setCfg({...cfg, themes:{...cfg.themes, global:e.target.value}})} />
            <Textarea data-testid="composer-theme-per-lang" rows={2} className="mt-2 text-mono text-xs" placeholder='{"English":"warm hopeful","German":"alpine sacred"}' value={JSON.stringify(cfg.themes.per_language||{})} onChange={e=>{try{setCfg({...cfg, themes:{...cfg.themes, per_language: JSON.parse(e.target.value||"{}")}});}catch{}}} />
            <Textarea data-testid="composer-theme-per-chan" rows={2} className="mt-2 text-mono text-xs" placeholder='{"DnB":"hard-hitting","Folk":"earthy"}' value={JSON.stringify(cfg.themes.per_channel||{})} onChange={e=>{try{setCfg({...cfg, themes:{...cfg.themes, per_channel: JSON.parse(e.target.value||"{}")}});}catch{}}} />
          </div>

          <div>
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2">Midjourney params</div>
            <div className="grid grid-cols-2 gap-2">
              <Input data-testid="composer-mj-ar" placeholder="ar" value={cfg.mj_ar} onChange={e=>setCfg({...cfg, mj_ar:e.target.value})} />
              <Input data-testid="composer-mj-v" placeholder="v" value={cfg.mj_v} onChange={e=>setCfg({...cfg, mj_v:e.target.value})} />
            </div>
            <div className="mt-2 space-y-2">
              <div><div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">Chaos {cfg.mj_chaos}</div><Slider value={[cfg.mj_chaos]} onValueChange={(v)=>setCfg({...cfg, mj_chaos:v[0]})} max={100} step={1} /></div>
              <div><div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">Stylize {cfg.mj_stylize}</div><Slider value={[cfg.mj_stylize]} onValueChange={(v)=>setCfg({...cfg, mj_stylize:v[0]})} max={1000} step={10} /></div>
              <div><div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">Weird {cfg.mj_weird}</div><Slider value={[cfg.mj_weird]} onValueChange={(v)=>setCfg({...cfg, mj_weird:v[0]})} max={3000} step={50} /></div>
            </div>
            <div className="mt-2 text-mono text-[11px] text-muted-foreground">{mjParams}</div>
          </div>

          <div>
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2 flex items-center justify-between"><span>Style keywords / packs</span><button onClick={clearKw} className="text-muted-foreground hover:text-destructive"><X className="w-3 h-3" /></button></div>
            <div className="flex flex-wrap gap-1 mb-2">
              {Object.keys(STYLE_PACKS).map(p => <button key={p} data-testid={`composer-pack-${p}`} onClick={()=>applyPack(p)} className="text-[10px] text-mono px-2 py-0.5 bg-muted rounded hover:bg-primary hover:text-primary-foreground">{p}</button>)}
            </div>
            <div className="flex flex-wrap gap-1">
              {(cfg.style_keywords||[]).map((k,i) => <button key={k+i} onClick={()=>toggleKw(k)} className="text-[10px] text-mono px-2 py-0.5 bg-primary/20 text-primary rounded">{k}<X className="w-2 h-2 inline ml-1" /></button>)}
            </div>
          </div>
        </Card>

        {/* RIGHT — chapter + sections + targets + output */}
        <div className="lg:col-span-2 space-y-5">
          <Card className="p-5">
            <div className="flex items-center justify-between mb-2">
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Bible chapter ({chapterRef || "no ref"})</div>
              <Button size="sm" variant="ghost" onClick={()=>nav("/bible")}>Pick from Bible Sources <ArrowRight className="w-3 h-3 ml-1" /></Button>
            </div>
            <Textarea data-testid="composer-chapter-textarea" ref={taRef} rows={10} value={chapterText} onChange={e=>setChapterText(e.target.value)} onSelect={onTextSelect}
              placeholder="Paste a chapter or use 'Bible Sources' to load one. Then select text below and click Add Section." className="text-sm leading-relaxed" />
            <div className="mt-3 flex gap-2 items-center">
              <Input data-testid="composer-section-idea" placeholder="image idea for this section (e.g. 'sun rising over Jordan')" value={sectionIdea} onChange={e=>setSectionIdea(e.target.value)} />
              <Button size="sm" data-testid="composer-add-section" onClick={addSection}><Plus className="w-3 h-3 mr-1" />Add</Button>
            </div>
          </Card>

          <Card className="p-5">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3">Authored sections ({(cfg.sections||[]).length})</div>
            <div className="space-y-2 max-h-48 overflow-auto scroll-thin">
              {(cfg.sections||[]).map((s, i) => (
                <div key={i} data-testid={`composer-section-${i}`} className="flex items-start gap-2 text-sm border border-border rounded p-2">
                  <div className="flex-1 min-w-0">
                    <div className="truncate font-medium">{s.text}</div>
                    {s.idea && <div className="text-xs text-primary italic">{s.idea}</div>}
                  </div>
                  <button onClick={()=>removeSection(i)} className="text-muted-foreground hover:text-destructive"><Trash2 className="w-3 h-3" /></button>
                </div>
              ))}
              {!cfg.sections?.length && <div className="text-xs text-muted-foreground">No sections yet — select text above and add.</div>}
            </div>
          </Card>

          <Card className="p-5">
            <div className="flex items-center justify-between mb-3">
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Targets (channel × language × style)</div>
              <Button size="sm" variant="secondary" data-testid="composer-add-target" onClick={addTarget}><Plus className="w-3 h-3 mr-1" />Add</Button>
            </div>
            <div className="space-y-2">
              {(cfg.targets||[]).map((t, i) => (
                <div key={i} className="grid md:grid-cols-12 gap-2 items-center">
                  <Input data-testid={`composer-target-lang-${i}`} value={t.language} onChange={e=>updateTarget(i,"language",e.target.value)} placeholder="language" className="md:col-span-3" />
                  <Input data-testid={`composer-target-style-${i}`} value={t.styles} onChange={e=>updateTarget(i,"styles",e.target.value)} placeholder="styles (e.g. Messianic DnB)" className="md:col-span-8" />
                  <button onClick={()=>removeTarget(i)} className="text-muted-foreground hover:text-destructive md:col-span-1"><Trash2 className="w-3 h-3" /></button>
                </div>
              ))}
            </div>
          </Card>

          {items.length > 0 && (
            <Card className="p-5">
              <div className="flex items-center justify-between mb-3 flex-wrap gap-2">
                <div>
                  <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Generated</div>
                  <h3 className="font-semibold">{items.length} song variants</h3>
                </div>
                <div className="flex gap-2">
                  <Button size="sm" variant="secondary" data-testid="composer-download-json" onClick={downloadJson}><Download className="w-3 h-3 mr-2" />Download JSON</Button>
                  <Button size="sm" data-testid="composer-send-lyrics" onClick={sendToLyrics}>Import to Lyrics <ArrowRight className="w-3 h-3 ml-2" /></Button>
                </div>
              </div>
              <div className="space-y-2 max-h-96 overflow-auto scroll-thin">
                {items.map((it, i) => (
                  <div key={i} data-testid={`composer-result-${i}`} className="border border-border rounded p-3 text-sm">
                    <div className="flex items-center justify-between mb-1">
                      <span className="font-medium">{it.title}</span>
                      <Badge variant="secondary">{it.language}</Badge>
                    </div>
                    <div className="text-xs italic text-muted-foreground mb-2">{it.styles}</div>
                    <div className="text-xs line-clamp-3 whitespace-pre-line">{it.lyrics}</div>
                  </div>
                ))}
              </div>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
}
