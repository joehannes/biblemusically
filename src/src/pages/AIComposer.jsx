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
import { 
  Bot, Plus, X, Save, Upload, Download, Sparkles, FlaskConical, 
  ArrowRight, Wand2, Trash2, Copy, Keyboard, Eye, HelpCircle, 
  Film, Image, Settings, Music, ChevronDown, Check, Info
} from "lucide-react";
import { toast } from "sonner";
import { useNavigate } from "react-router-dom";
import { useAutoSave, AutoSaveChip } from "../lib/hooks";

const STYLE_PACKS = {
  "Biblical Concept": ["cinematic biblical concept art", "sacred celestial symbolism", "radiant divine light", "visionary spiritual realism", "majestic ancient atmosphere"],
  "Orthodox Gilding": ["ethereal orthodox iconography", "gold leaf halos", "byzantine flat perspective", "sacred geometry", "ancient mosaic textures"],
  "Trash Polka": ["trash polka textures", "ink splatter", "monochrome with red accents", "tattoo aesthetics"],
  "Renaissance Oil": ["renaissance oil painting", "thick impasto texture", "rembrandt sfumato lighting", "dramatic chiaroscuro", "classical composition"],
  "Art Nouveau": ["art nouveau", "alphonse mucha style", "ornate floral borders", "soft organic pastels"],
  "Cyberpunk Sacred": ["neon-lit cathedral", "cyberpunk holiness", "translucent glowing stained glass", "techno-baroque style"],
  "Parchment Watercolor": ["ancient parchment paper background", "delicate bleeding watercolor textures", "soft wash colors", "minimalist ink outlines"],
  "Synthwave Psalter": ["retro synthwave psalm aesthetic", "glowing neon vectors", "dramatic retro-futurist sunsets", "cyber-grid landscape"],
  "Ancient Gothic": ["gothic cathedral architecture", "dark romanticism", "intricate stone masonry", "moody chiaroscuro", "mysterious shadows", "ancient dramatic atmosphere"],
  "Divine Gold & Lapis": ["lapis lazuli blue and gold leaf accents", "sacred divine majesty", "illuminated manuscript style", "heavenly clouds", "ultra-detailed filigree"],
  "Digital Glitch Psalm": ["sacred digital glitch art", "holographic glowing scripture", "cybernetic spirituality", "vaporwave color palette", "futuristic retro-grid"],
  "Sovereign Charcoal & Ink": ["dramatic charcoal sketch", "stark ink wash", "expressive hand-drawn linework", "deep rich blacks", "high contrast spiritual emotionalism"],
  "Cosmic Ethereal": ["cosmic nebulas forming holy shapes", "glittering stellar dust", "ethereal light beams", "infinite celestial scale", "dreamy vibrant space art"],
  "Messianic Joyful Praise": ["joyful messianic celebration", "golden sunlight spilling over Jerusalem hills", "dancing dynamic figures", "uplifting energetic warm palette", "flowing ribbons of light"]
};

const THEME_PRESETS = {
  "Messianic Joy": {
    global: "sacred, messianic joy, celebratory, light of the world",
    per_language: { "English": "joyful celebration", "Hebrew": "messianic praise, ancient shofar call" },
    per_channel: { "DnB": "energetic synthesis of light", "Folk": "acoustic warmth and heritage" }
  },
  "Sovereign Majesty": {
    global: "sovereign majesty, divine authority, celestial throne, awe-inspiring",
    per_language: { "English": "grand and regal", "German": "solemn and powerful majesty" },
    per_channel: { "EDM": "epic brass synths, huge scale", "Classical": "orchestral triumph" }
  },
  "Chamber Ethereal": {
    global: "ethereal quiet, soft chamber reverb, mystical whispers, gentle dawn",
    per_language: { "English": "peaceful restoration", "Spanish": "suave y místico" },
    per_channel: { "Chill": "ambient space pads, quiet reflection", "Folk": "delicate acoustic fingerpicking" }
  },
  "Apocalyptic Warning": {
    global: "dramatic skies, split clouds, trumpet sound, majestic awe, ultimate truth",
    per_language: { "English": "bold and prophetic warning", "Russian": "глубокий и пророческий" },
    per_channel: { "DnB": "heavy dark basslines", "Industrial": "metallic drums and alarms" }
  }
};

const SUNO_GENRES = {
  "Melodic/Liquid DnB": "melodic liquid drum and bass, female angelic vocals, ethereal pads, uplifting, 174 bpm",
  "Drill & Bass Rapid": "drill and bass, aggressive rapid breakbeats, heavy sub-bass, experimental electronic, 165 bpm",
  "Bavarian Electro Swing": "electro swing, vintage brass horn section, accordion chords, bavarian alpine folk rhythms, upbeat swing beats, 125 bpm",
  "Latino Messianic DnB": "messianic worship, liquid drum and bass, spanish acoustic guitar pluck, latino groove, uplifting female vocals, 172 bpm",
  "Ancient Jewish Fusion": "ancient jewish melody, sacred shofar, harp, messianic worship, atmospheric synth pad, driving hand drum percussion, solemn",
  "Bavarian Mountain Chill": "bavarian alpine mountain yodeling sample, chill edm, deep house groove, zither, warm bass, atmospheric chillout, 115 bpm",
  "Tropical Chill Lounge": "tropical house, soft plucks, chill edm, steel drum accents, gentle saxophone, emotional melody, 110 bpm",
  "Emo Rap Reflection": "emo rap, melancholic acoustic guitar trap loop, emotional raw male vocals, deep 808 bass, dark spiritual reflection",
  "Messianic Jewish Praise": "messianic jewish worship, joyful klezmer clarinet, driving hora beat, acoustic guitar, celebratory, 130 bpm",
  "Ancient Hebrew Chants": "ancient hebrew chanting, liturgical solo male cantor, resonant temple reverb, cinematic drone pads, mystical, solemn",
  // New presets
  "Heavenly Electro Swing Revival": "Christian EDM, Electro Swing, Playful Worship, Vintage Brass, Swing House, Happy Gospel Energy, Dunkelbunt Inspired, Joyful Basslines, Organic Samples, Piano Jazz Harmony, Faith Celebration, Male/Female Christian Vocals, Retro Soul Christian Energy, Bright Festival Feeling, Positive Spiritual Emotion, Lighthearted Worship, Warm Analog Feeling",
  "Liquid Rivers Of Grace": "Liquid Drum And Bass, Christian Atmospheric EDM, Melodic DnB, Emotional Worship, Deep Spiritual Atmosphere, Ethereal Choir Layers, Beautiful Pads, Uplifting Female Vocals, Soulful Christian Expression, Warm Sub Bass, Dreamlike Synth Textures, Hopeful Emotional Crescendo, Prayerful Energy, Heavenly Soundscape, Gentle Yet Energetic",
  "Alpine Kingdom Celebration": "Christian Folk EDM, Austrian Volksmusik Influence, Bavarian Joy, Ziehharmonika, Alpine Accordion, Playful Christian Dance, Light Folk Basslines, Organic Acoustic Energy, Happy Worship Atmosphere, European Folk Charm, Positive Male Vocals, Traditional Meets Future, Joyful Faith Celebration, Sunny Mountain Village Feeling",
  "Experimental Wilderness Worship": "Experimental Christian Electronica, Organic Percussion, David Crowder Inspired, Unexpected Rhythms, Atmospheric Worship Layers, Gospel Electronics, Textural Sound Design, Creative Faith Music, Dynamic Arrangement, Soulful Christian Male Vocals, Earthy Acoustic Instruments, Spiritual Exploration, Innovative Worship Feeling, Imperfect Organic Beauty",
  "Velvet Gospel Soul": "Retro Soul Gospel, Amy Winehouse Inspired Clean Christian Female Vocals, Neo Soul Worship, Christian Jazz Soul, Vintage Warmth, Emotional Honesty, Smooth Horn Arrangements, Gospel Emotion, Light Groove, Clean Retro Texture, Spiritual Longing, Warm Vintage Microphone Feeling, Soulful Yet Hopeful Atmosphere",
  "Sacred Complexity": "Melodic Drill And Bass, Intelligent Christian Electronica, Complex Rhythmic Structures, Hyper Detailed Percussion, Spiritual Wonder, Emotional Atmosphere, Intricate Beat Programming, Bright Energy, Experimental Yet Beautiful, Atmospheric Worship, Detailed Sound Design, Futuristic Faith Energy, Emotional Momentum",
  "Festival Of Heaven": "Future Bass Christian, Joyful EDM, Bright Chord Stacks, Emotional Worship Drops, Vocal Chops, Uplifting Christian Atmosphere, Euphoric Festival Feeling, Heavenly Synth Leads, Youthful Energy, Catchy Faith Emotion, Positive Emotional Release, Joyful Worship Celebration, Modern Inspirational Sound",
  "The King's Caravan": "Epic Christian Folk Electronic, Medieval Atmosphere, Fantasy Worship, Choir Layers, Sacred Adventure Feeling, Organic Percussion, Biblical Journey Energy, European Folk Textures, Heroic Yet Hopeful, Kingdom Feeling, Ancient Meets Modern, Spiritual Pilgrimage Atmosphere, Cinematic Worship",
  "Childlike Kingdom Joy": "Playful Christian Electronic, Cartoon Energy, Fun Worship Songs, Bright Synths, Smiling Atmosphere, Positive Lyrics, Childlike Wonder, Family Friendly Christian Energy, Joyful Choirs, Cheerful Rhythms, Lighthearted Faith Celebration, Uplifting Mood, Colorful Happiness",
  "Prophets In The Desert": "Middle Eastern Christian EDM, Mystical Atmosphere, Desert Percussion, Ancient Spiritual Feeling, Emotional Pads, Ethereal Vocals, Prophetic Energy, Biblical Atmosphere, Spiritual Tension And Hope, Cinematic Faith Journey, Sacred Mystery, Atmospheric Worship",
  "Midnight Prayer Frequencies": "LoFi Christian Drum And Bass, Warm Tape Texture, Relaxed Worship, Gentle Liquid DnB, Piano Atmosphere, Late Night Prayer Feeling, Soft Choir Layers, Soulful Christian Vocals, Peaceful Emotional Energy, Intimate Faith, Comforting Soundscape, Healing Atmosphere, Calm Spiritual Reflection",
  "Color Gospel Joy": "Christian Pop EDM, Bright Modern Production, Energetic Hooks, Radio Friendly Worship, Happy Dance Energy, Youthful Christian Atmosphere, Catchy Chorus, Positive Faith Message, Female/Male Harmony, Uplifting Emotion, Viral Friendly Sound, Modern Worship Excitement, Bright Emotional Energy"
};

const MIDJOURNEY_IMAGE_STYLES = {
  "Divine Conceptualism": "concept art, spiritual symbolism, metaphorical christian imagery, layered meaning, meaningful visual integration, sacred geometry, emotional storytelling, divine atmosphere, symbolic realism, heavenly lighting, interconnected spiritual ideas, elegant complexity, visual theology, ultra detailed, meaningful composition, transcendent beauty",
  "Chaos Into Redemption": "trash polka christian art, black ink textures, selective red accents, realism mixed with abstraction, fragmented symbolism, emotional contrast, scripture inspired imagery, symbolic chaos becoming divine order, dramatic composition, layered storytelling, distressed textures, meaningful visual collision, spiritual transformation",
  "The Kingdom Illustrated": "christian comic universe, graphic novel aesthetic, cinematic framing, dynamic storytelling, heroic biblical atmosphere, expressive characters, hopeful emotional tone, stylized realism, vibrant spiritual emotion, modern comic rendering, divine adventure feeling, uplifting visual storytelling",
  "The Sacred Epic": "biblical fantasy concept art, cinematic spiritual landscapes, heavenly atmosphere, divine scale, majestic biblical scenery, sacred realism, epic environmental storytelling, painterly concept design, spiritual grandeur, emotional depth, holy atmosphere, ancient world immersion, divine cinematic lighting",
  "Joyful Gospel Pop": "christian pop art, vibrant sacred colors, uplifting symbolism, joyful faith energy, stylized composition, playful spirituality, modern gospel visuals, bright emotional storytelling, contemporary christian design language, graphic symbolism, hopeful atmosphere, expressive color explosion",
  "The Hidden Meaning": "spiritual realism, metaphorical christian storytelling, hidden symbolism, layered visual meaning, interconnected ideas, philosophical spirituality, emotional atmosphere, subtle divine references, elegant composition, meaningful complexity, poetic realism, visual metaphor integration",
  "Windows Of Heaven": "modern stained glass aesthetic, luminous sacred colors, glowing divine light, christian symbolism, heavenly atmosphere, holy visual harmony, radiant composition, transcendent beauty, spiritual illumination, neo sacred design, cathedral inspired elegance, luminous storytelling",
  "Dreams Of Eternity": "christian surrealism, dreamlike spiritual atmosphere, floating symbolism, poetic visual theology, heavenly clouds, divine imagination, soft mystical textures, luminous spirituality, surreal biblical metaphor, emotional dreamscape, peaceful transcendence, spiritual wonder",
  "Ancient Gospel Manuscript": "vintage christian illustration, historical biblical artwork, engraved textures, warm sacred atmosphere, painterly realism, nostalgic spiritual mood, ancient storytelling aesthetic, parchment inspired textures, detailed linework, timeless christian imagery, classic biblical feeling",
  "The Joyful Kingdom": "stylized christian cartoon world, family friendly atmosphere, playful biblical storytelling, cheerful emotional tone, colorful spiritual landscapes, expressive characters, uplifting energy, modern animation feeling, joyful worship atmosphere, warm positivity, childlike wonder",
  "The New Jerusalem Protocol": "christian science fiction concept art, heavenly technology, sacred futurism, cosmic spirituality, celestial symbolism, divine futuristic architecture, biblical sci fi atmosphere, meaningful cosmic storytelling, holy universe aesthetic, epic scale, transcendent future vision",
  "The Alpine Gospel": "alpine christian folklore, austrian german visual influence, mountain spirituality, pastoral sacred realism, joyful countryside atmosphere, ziehharmonika folk feeling, wholesome biblical energy, warm sunlight, sacred european village mood, joyful rustic christian imagery, peaceful mountain kingdom"
};

const LANGUAGES = ["English", "German", "Hebrew", "Spanish", "Portuguese", "French", "Italian", "Russian"];

const DEFAULT_CFG = {
  artist: "Joehannes Lightkid",
  title_pattern: "{artist} - {book} {chapter} ({styles})",
  generate: { title: true, language: false, styles: false, lyrics: true, annotations: true, image_styles: true },
  themes: { global: "sacred, hopeful, divine light", per_language: {}, per_channel: {} },
  mj_ar: "16:9", mj_v: "8.1", mj_chaos: 0, mj_stylize: 100, mj_weird: 0,
  mj_video: false,
  mj_tile: false,
  mj_quality: "1",
  mj_no: "",
  mj_seed: "",
  style_keywords: ["cinematic biblical concept art", "sacred celestial symbolism", "radiant divine light"],
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
  const [customKw, setCustomKw] = useState("");
  const importRef = useRef();
  const taRef = useRef();

  // New states for the custom theme preset CRUD
  const [customThemePresets, setCustomThemePresets] = useState(() => {
    return JSON.parse(localStorage.getItem("studio:custom-theme-presets") || "{}");
  });
  const [newPresetName, setNewPresetName] = useState("");

  // New inline editor states for Themes
  const [addLangKey, setAddLangKey] = useState("English");
  const [addLangCustom, setAddLangCustom] = useState("");
  const [addLangVal, setAddLangVal] = useState("");
  
  const [addChanKey, setAddChanKey] = useState("");
  const [addChanVal, setAddChanVal] = useState("");

  // Suno style helper expansion tracking
  const [activeSunoHelperIdx, setActiveSunoHelperIdx] = useState(null);

  const composerState = { cfg, chapterRef, chapterText, chapterLang, chapterBook, chapterNum };
  const { status: asStatus, lastSaved, restore: restoreDraft } = useAutoSave("ai-composer", composerState, { delay: 800 });

  // Load bible selection if pushed from BibleSources screen + restore auto-saved composer draft
  useEffect(() => {
    (async () => {
      let restoredCfg = null;
      const draft = await restoreDraft();
      if (draft) {
        if (draft.cfg) restoredCfg = draft.cfg;
        if (draft.chapterRef) setChapterRef(draft.chapterRef);
        if (draft.chapterText) setChapterText(draft.chapterText);
        if (draft.chapterLang) setChapterLang(draft.chapterLang);
        if (draft.chapterBook) setChapterBook(draft.chapterBook);
        if (draft.chapterNum) setChapterNum(draft.chapterNum);
      }
      const raw = localStorage.getItem("studio:bible-selection");
      if (raw) {
        try {
          const b = JSON.parse(raw);
          setChapterRef(b.reference); setChapterText(b.text); setChapterLang(b.language || "English");
          setChapterBook(b.book || "john"); setChapterNum(b.chapter || 1);
        } catch {}
        localStorage.removeItem("studio:bible-selection");
      }
      const d = await api.getComposeConfig();
      let finalCfg = { ...DEFAULT_CFG };
      if (d && Object.keys(d).length && !draft) {
        finalCfg = { ...finalCfg, ...d };
      } else if (restoredCfg) {
        finalCfg = { ...finalCfg, ...restoredCfg };
      }

      try {
        const chs = await api.listChannels();
        if (chs && chs.length > 0) {
          if (!finalCfg.targets || finalCfg.targets.length === 0 || 
              (finalCfg.targets.length === 2 && finalCfg.targets[0].styles === "Messianic Liquid Drum and Bass")) {
            finalCfg.targets = chs.map(c => ({
              language: c.language || "English",
              styles: c.styles || ""
            }));
          }
        }
      } catch (err) {
        console.error("Failed to auto-populate targets", err);
      }

      setCfg(finalCfg);
    })();
    // eslint-disable-next-line
  }, []);

  // Keyboard shortcuts listener
  useEffect(() => {
    const handleKeyDown = (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
        e.preventDefault();
        generate();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        saveCfg();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
    // eslint-disable-next-line
  }, [cfg, chapterText]);

  const saveCfg = async () => {
    try {
      await api.saveComposeConfig(cfg);
      toast.success("Saved on server");
    } catch (error) {
      console.error("Failed to save composer config", error);
      toast.error("Could not save composer config");
    }
  };

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

  const addCustomKw = () => {
    const kw = customKw.trim();
    if (!kw) return;
    const list = cfg.style_keywords || [];
    if (list.includes(kw)) {
      toast.error("Keyword already exists");
    } else {
      setCfg({ ...cfg, style_keywords: [...list, kw] });
      setCustomKw("");
      toast.success(`Added keyword: ${kw}`);
    }
  };

  const toggleKw = (k) => {
    const list = cfg.style_keywords || [];
    setCfg({ ...cfg, style_keywords: list.includes(k) ? list.filter(x=>x!==k) : [...list, k] });
  };
  const applyPack = (name) => {
    const pack = STYLE_PACKS[name] || [];
    setCfg({ ...cfg, style_keywords: Array.from(new Set([...(cfg.style_keywords||[]), ...pack])) });
  };
  const toggleSunoPreset = (targetIndex, genreName) => {
    const csvContent = SUNO_GENRES[genreName];
    if (!csvContent) return;
    const target = cfg.targets?.[targetIndex];
    if (!target) return;

    const existing = target.styles || "";
    const existingItems = existing.split(",").map(s => s.trim()).filter(Boolean);
    const packItems = csvContent.split(",").map(s => s.trim()).filter(Boolean);
    const hasAll = packItems.every(item => existingItems.includes(item));

    const nextItems = hasAll
      ? existingItems.filter(item => !packItems.includes(item))
      : Array.from(new Set([...existingItems, ...packItems]));

    updateTarget(targetIndex, "styles", nextItems.join(", "));
  };

  const renderSunoGenreButton = (targetIndex, genreName, csvContent) => {
    const target = cfg.targets?.[targetIndex] || {};
    const selected = target.styles?.split(",").map(s => s.trim()).filter(Boolean).every(item => csvContent.split(",").map(s => s.trim()).filter(Boolean).includes(item));
    const buttonClass = selected
      ? "text-left text-xs p-2 rounded border transition-all text-mono group border-primary bg-primary/10"
      : "text-left text-xs p-2 rounded border transition-all text-mono group border-border/60 hover:bg-primary/5 hover:border-primary";

    return (
      <button
        key={genreName}
        type="button"
        onClick={() => {
          toggleSunoPreset(targetIndex, genreName);
          toast.success(`${selected ? "Removed" : "Added"} genre: ${genreName}`);
        }}
        className={buttonClass}
      >
        <div className="font-semibold text-primary group-hover:text-primary/90 flex justify-between items-center">
          <span>{genreName}</span>
          {selected && <Check className="w-3 h-3 text-emerald-500" />}
        </div>
        <div className="text-[10px] text-muted-foreground leading-normal mt-0.5 truncate">{csvContent}</div>
      </button>
    );
  };
  const clearKw = () => setCfg({ ...cfg, style_keywords: [] });

  const addTarget = () => setCfg({ ...cfg, targets: [...(cfg.targets||[]), { language: "English", styles: "" }] });
  const updateTarget = (i, k, v) => { const t=[...cfg.targets]; t[i] = {...t[i], [k]: v}; setCfg({...cfg, targets: t}); };
  const removeTarget = (i) => setCfg({ ...cfg, targets: cfg.targets.filter((_,x)=>x!==i) });

  const syncTargetsFromChannels = async () => {
    try {
      const chs = await api.listChannels();
      if (chs && chs.length > 0) {
        const syncedTargets = chs.map(c => ({
          language: c.language || "English",
          styles: c.styles || ""
        }));
        setCfg(prev => ({ ...prev, targets: syncedTargets }));
        toast.success(`Synced ${syncedTargets.length} targets from channels`);
      } else {
        toast.info("No channels found in Channel Manager to sync.");
      }
    } catch (e) {
      console.error(e);
      toast.error("Failed to sync channels");
    }
  };

  // Constructed Midjourney Prompt Suffix Builder
  const mjParams = `--ar ${cfg.mj_ar} --v ${cfg.mj_v}${cfg.mj_chaos ? ` --chaos ${cfg.mj_chaos}` : ""}${cfg.mj_stylize !== 100 ? ` --stylize ${cfg.mj_stylize}` : ""}${cfg.mj_weird ? ` --weird ${cfg.mj_weird}` : ""}${cfg.mj_video ? " --video" : ""}${cfg.mj_tile ? " --tile" : ""}${cfg.mj_quality && cfg.mj_quality !== "1" ? ` --quality ${cfg.mj_quality}` : ""}${cfg.mj_no ? ` --no "${cfg.mj_no}"` : ""}${cfg.mj_seed ? ` --seed ${cfg.mj_seed}` : ""}`;

  // Theme Preset Actions
  const applyThemePreset = (presetData) => {
    setCfg(prev => ({
      ...prev,
      themes: {
        global: presetData.global || "",
        per_language: presetData.per_language || {},
        per_channel: presetData.per_channel || {}
      }
    }));
    toast.success("Applied theme preset");
  };

  const saveCustomThemePreset = () => {
    const name = newPresetName.trim();
    if (!name) return toast.error("Enter a theme preset name first");
    const next = {
      ...customThemePresets,
      [name]: {
        global: cfg.themes.global || "",
        per_language: cfg.themes.per_language || {},
        per_channel: cfg.themes.per_channel || {}
      }
    };
    setCustomThemePresets(next);
    localStorage.setItem("studio:custom-theme-presets", JSON.stringify(next));
    setNewPresetName("");
    toast.success(`Theme preset "${name}" saved locally`);
  };

  const deleteCustomThemePreset = (name) => {
    const next = { ...customThemePresets };
    delete next[name];
    setCustomThemePresets(next);
    localStorage.setItem("studio:custom-theme-presets", JSON.stringify(next));
    toast.success(`Theme preset "${name}" deleted`);
  };

  // Add/Remove helpers for GUI Theme lists
  const handleAddPerLang = () => {
    const key = addLangKey === "custom" ? addLangCustom.trim() : addLangKey;
    const val = addLangVal.trim();
    if (!key) return toast.error("Please specify a language");
    if (!val) return toast.error("Please specify a theme value");

    const updatedLangs = { ...(cfg.themes.per_language || {}), [key]: val };
    setCfg({ ...cfg, themes: { ...cfg.themes, per_language: updatedLangs } });
    
    setAddLangVal("");
    setAddLangCustom("");
    toast.success(`Added theme for ${key}`);
  };

  const handleRemovePerLang = (key) => {
    const updatedLangs = { ...(cfg.themes.per_language || {}) };
    delete updatedLangs[key];
    setCfg({ ...cfg, themes: { ...cfg.themes, per_language: updatedLangs } });
    toast.success(`Removed theme for ${key}`);
  };

  const handleAddPerChan = () => {
    const key = addChanKey.trim();
    const val = addChanVal.trim();
    if (!key) return toast.error("Please specify a channel/style name");
    if (!val) return toast.error("Please specify a theme value");

    const updatedChans = { ...(cfg.themes.per_channel || {}), [key]: val };
    setCfg({ ...cfg, themes: { ...cfg.themes, per_channel: updatedChans } });
    
    setAddChanKey("");
    setAddChanVal("");
    toast.success(`Added theme for channel ${key}`);
  };

  const handleRemovePerChan = (key) => {
    const updatedChans = { ...(cfg.themes.per_channel || {}) };
    delete updatedChans[key];
    setCfg({ ...cfg, themes: { ...cfg.themes, per_channel: updatedChans } });
    toast.success(`Removed theme for channel ${key}`);
  };

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
      if (r.error) return toast.error(r.error);
      if (r.items?.length) {
        setItems(r.items);
        toast.success(`Generated ${r.count} song variants via ${r.model}`);
      } else {
        toast.error("No lyrics were generated.");
      }
    } catch (error) {
      console.error("AI composer generation failed", error);
      toast.error("Composer failed to generate lyrics.");
    } finally {
      setBusy(false);
    }
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
    <div className="max-w-7xl mx-auto px-4 py-8 space-y-6">
      {/* HEADER SECTION */}
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">AI Composer</h1>
        <div className="flex items-center gap-3">
          <AutoSaveChip status={asStatus} lastSaved={lastSaved} />
          <Button size="sm" variant="secondary" data-testid="composer-save-cfg" onClick={saveCfg}>
            <Save className="w-3 h-3 mr-2" />
            Save config <kbd className="ml-1.5 hidden sm:inline text-[9px] font-mono opacity-70 bg-background px-1 py-0.5 rounded border border-border">Ctrl+S</kbd>
          </Button>
          <Button data-testid="composer-generate-btn" onClick={generate} disabled={busy}>
            {busy ? <FlaskConical className="w-4 h-4 mr-2 animate-pulse" /> : <Sparkles className="w-4 h-4 mr-2" />}
            Generate <kbd className="ml-1.5 hidden sm:inline text-[9px] font-mono opacity-70 bg-primary-foreground/20 px-1 py-0.5 rounded">Ctrl+Enter</kbd>
          </Button>
        </div>
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">Authors a multi-channel <span className="text-mono">lyrics.json</span> from your bible chapter + themes + section ideas. Powered by OpenRouter Qwen (free tier).</p>

      {/* COMPOSER WORKSPACE LAYOUT */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* LEFT COLUMN: CONTROLS & PARAMS */}
        <Card className="p-5 space-y-5 h-fit self-start">
          
          {/* PROFILE / PRESETS SECTION */}
          <div className="space-y-2">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Composer Config Profiles</div>
            <div className="flex gap-2">
              <Input data-testid="composer-profile-name" value={profileName} onChange={e=>setProfileName(e.target.value)} placeholder="profile name" className="h-8 text-xs" />
              <Button size="sm" className="h-8" variant="secondary" data-testid="composer-profile-save" onClick={saveProfile}><Save className="w-3.5 h-3.5" /></Button>
              <Button size="sm" className="h-8" variant="ghost" onClick={exportProfile}><Download className="w-3.5 h-3.5" /></Button>
              <Button size="sm" className="h-8" variant="ghost" onClick={()=>importRef.current?.click()}><Upload className="w-3.5 h-3.5" /></Button>
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

          {/* PARAMS FIELD TOGGLES */}
          <div className="border-t border-border/50 pt-4">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2.5">Field Generation Toggles</div>
            <div className="grid grid-cols-2 gap-2 text-xs">
              {Object.keys(cfg.generate).map(k => (
                <label key={k} className="flex items-center justify-between gap-2 bg-muted/30 p-1.5 rounded border border-border/20">
                  <span className="text-mono text-[10px]">{k}</span>
                  <Switch data-testid={`composer-toggle-${k}`} checked={cfg.generate[k]} onCheckedChange={(v)=>setCfg({...cfg, generate:{...cfg.generate,[k]:v}})} />
                </label>
              ))}
            </div>
          </div>

          {/* PREMIUM THEMES SECTION (FULLY GUI CONTROLLED) */}
          <div className="border-t border-border/50 pt-4 space-y-4">
            <div className="flex items-center justify-between">
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground flex items-center gap-1"><Settings className="w-3.5 h-3.5 text-primary" /> Image &amp; Lyrics Flavor Themes</div>
              <Badge variant="outline" className="text-[9px]">GUI Mode</Badge>
            </div>

            {/* Prepackaged Theme Presets */}
            <div>
              <div className="text-[9px] text-mono uppercase tracking-widest text-muted-foreground mb-1.5">Prepackaged Theme Presets</div>
              <div className="flex flex-wrap gap-1">
                {Object.keys(THEME_PRESETS).map(p => (
                  <button
                    key={p}
                    type="button"
                    onClick={() => applyThemePreset(THEME_PRESETS[p])}
                    className="text-[9px] font-mono px-2 py-0.5 bg-primary/10 text-primary border border-primary/20 rounded hover:bg-primary hover:text-primary-foreground transition-colors"
                  >
                    {p}
                  </button>
                ))}
              </div>
            </div>

            {/* Custom User Theme Presets CRUD */}
            <div className="bg-muted/40 p-2 rounded-lg border border-border/40 space-y-2">
              <div className="text-[9px] text-mono uppercase tracking-widest text-muted-foreground">Save Current Theme as Preset</div>
              <div className="flex gap-1.5">
                <Input 
                  placeholder="preset name..." 
                  value={newPresetName} 
                  onChange={e => setNewPresetName(e.target.value)} 
                  className="h-7 text-xs" 
                />
                <Button size="sm" onClick={saveCustomThemePreset} className="h-7 text-xs px-2.5">Save</Button>
              </div>
              
              {Object.keys(customThemePresets).length > 0 && (
                <div className="flex flex-wrap gap-1 mt-1">
                  {Object.keys(customThemePresets).map(name => (
                    <div key={name} className="flex items-center gap-1 bg-background border border-border/60 rounded px-1.5 py-0.5">
                      <button 
                        type="button"
                        onClick={() => applyThemePreset(customThemePresets[name])}
                        className="text-[9px] font-mono text-muted-foreground hover:text-primary"
                      >
                        {name}
                      </button>
                      <button onClick={() => deleteCustomThemePreset(name)} className="text-muted-foreground hover:text-destructive"><X className="w-2.5 h-2.5" /></button>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {/* 1. Global Theme */}
            <div className="space-y-1.5">
              <div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">1. Global Theme Flavor</div>
              <Input 
                data-testid="composer-theme-global" 
                placeholder="e.g. sacred, hopeful, divine light" 
                value={cfg.themes.global} 
                onChange={e=>setCfg({...cfg, themes:{...cfg.themes, global:e.target.value}})} 
                className="h-8 text-xs"
              />
            </div>

            {/* 2. Per-Language Themes */}
            <div className="space-y-2">
              <div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">2. Per-Language Themes</div>
              
              {/* Active list */}
              <div className="space-y-1 max-h-32 overflow-y-auto scroll-thin">
                {Object.entries(cfg.themes.per_language || {}).map(([lang, theme]) => (
                  <div key={lang} className="flex items-center justify-between text-xs bg-muted/65 p-1.5 rounded border border-border/30">
                    <div className="flex items-center gap-1.5">
                      <Badge variant="secondary" className="text-[9px] font-mono">{lang}</Badge>
                      <span className="text-muted-foreground font-mono text-[10px] truncate max-w-[130px]">{theme}</span>
                    </div>
                    <button type="button" onClick={() => handleRemovePerLang(lang)} className="text-muted-foreground hover:text-destructive"><X className="w-3 h-3" /></button>
                  </div>
                ))}
                {!Object.keys(cfg.themes.per_language || {}).length && (
                  <div className="text-[10px] text-muted-foreground italic">No language themes added yet</div>
                )}
              </div>

              {/* Add form */}
              <div className="grid grid-cols-12 gap-1.5 bg-muted/30 p-2 rounded border border-border/10">
                <div className="col-span-5">
                  <Select value={addLangKey} onValueChange={setAddLangKey}>
                    <SelectTrigger className="h-7 text-xs">
                      <SelectValue placeholder="Lang" />
                    </SelectTrigger>
                    <SelectContent>
                      {LANGUAGES.map(l => (
                        <SelectItem key={l} value={l} className="text-xs">{l}</SelectItem>
                      ))}
                      <SelectItem value="custom" className="text-xs">Custom...</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="col-span-5">
                  <Input 
                    placeholder="Theme flavor" 
                    value={addLangVal} 
                    onChange={e => setAddLangVal(e.target.value)}
                    className="h-7 text-xs"
                  />
                </div>
                <div className="col-span-2">
                  <Button type="button" size="sm" onClick={handleAddPerLang} className="w-full h-7 px-1 text-xs"><Plus className="w-3.5 h-3.5" /></Button>
                </div>
                {addLangKey === "custom" && (
                  <div className="col-span-12 mt-1">
                    <Input 
                      placeholder="Type custom language..." 
                      value={addLangCustom} 
                      onChange={e => setAddLangCustom(e.target.value)}
                      className="h-7 text-xs"
                    />
                  </div>
                )}
              </div>
            </div>

            {/* 3. Per-Channel Themes */}
            <div className="space-y-2">
              <div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">3. Per-Channel Themes</div>
              
              {/* Active list */}
              <div className="space-y-1 max-h-32 overflow-y-auto scroll-thin">
                {Object.entries(cfg.themes.per_channel || {}).map(([chan, theme]) => (
                  <div key={chan} className="flex items-center justify-between text-xs bg-muted/65 p-1.5 rounded border border-border/30">
                    <div className="flex items-center gap-1.5">
                      <Badge className="text-[9px] font-mono bg-primary/20 text-primary hover:bg-primary/20">{chan}</Badge>
                      <span className="text-muted-foreground font-mono text-[10px] truncate max-w-[130px]">{theme}</span>
                    </div>
                    <button type="button" onClick={() => handleRemovePerChan(chan)} className="text-muted-foreground hover:text-destructive"><X className="w-3 h-3" /></button>
                  </div>
                ))}
                {!Object.keys(cfg.themes.per_channel || {}).length && (
                  <div className="text-[10px] text-muted-foreground italic">No channel themes added yet</div>
                )}
              </div>

              {/* Add form */}
              <div className="grid grid-cols-12 gap-1.5 bg-muted/30 p-2 rounded border border-border/10">
                <div className="col-span-5">
                  <Input 
                    placeholder="Channel (e.g. DnB)" 
                    value={addChanKey} 
                    onChange={e => setAddChanKey(e.target.value)}
                    className="h-7 text-xs"
                  />
                </div>
                <div className="col-span-5">
                  <Input 
                    placeholder="Theme flavor" 
                    value={addChanVal} 
                    onChange={e => setAddChanVal(e.target.value)}
                    className="h-7 text-xs"
                  />
                </div>
                <div className="col-span-2">
                  <Button type="button" size="sm" onClick={handleAddPerChan} className="w-full h-7 px-1 text-xs"><Plus className="w-3.5 h-3.5" /></Button>
                </div>
              </div>
            </div>
          </div>

          {/* MIDJOURNEY ADVANCED IMAGE & VIDEO PARAMETERS */}
          <div className="border-t border-border/50 pt-4 space-y-4">
            <div className="flex items-center justify-between">
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground flex items-center gap-1">
                <Film className="w-3.5 h-3.5 text-primary" /> Image &amp; Video Options
              </div>
              <Badge variant="secondary" className="text-[9px]">v7 &amp; v8.1 Ready</Badge>
            </div>

            {/* Mode Toggle: Image vs Video */}
            <div className="flex items-center justify-between bg-muted/40 p-2.5 rounded-lg border border-border/40">
              <div className="space-y-0.5">
                <div className="text-xs font-semibold flex items-center gap-1.5">
                  {cfg.mj_video ? <Film className="w-3.5 h-3.5 text-primary" /> : <Image className="w-3.5 h-3.5 text-primary" />}
                  {cfg.mj_video ? "Midjourney Video Mode" : "Midjourney Image Mode"}
                </div>
                <div className="text-[9px] text-muted-foreground">Generates a video process or tileable flat canvas</div>
              </div>
              <Switch 
                checked={cfg.mj_video} 
                onCheckedChange={(val) => setCfg(prev => ({ ...prev, mj_video: val }))}
              />
            </div>

            {/* Tile Pattern switch */}
            <div className="flex items-center justify-between text-xs">
              <span className="font-mono text-[10px] text-muted-foreground">Tileable Seamless Pattern (--tile)</span>
              <Switch 
                checked={cfg.mj_tile} 
                onCheckedChange={(val) => setCfg(prev => ({ ...prev, mj_tile: val }))}
              />
            </div>

            {/* Model Version Select (Includes Latest 7 and 8.1!) */}
            <div>
              <div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground mb-1.5">Model Version</div>
              <div className="flex flex-wrap gap-1.5">
                {["8.1", "7", "6.1", "6.0", "5.2", "niji 6"].map((v) => (
                  <button
                    key={v}
                    type="button"
                    onClick={() => setCfg({ ...cfg, mj_v: v })}
                    className={`text-xs px-2 py-0.5 rounded border transition-all ${
                      cfg.mj_v === v
                        ? "bg-primary text-primary-foreground border-primary font-medium"
                        : "bg-muted hover:bg-secondary border-border text-muted-foreground"
                    }`}
                  >
                    {v === "8.1" || v === "7" ? `✨ ${v}` : v}
                  </button>
                ))}
              </div>
            </div>

            {/* Aspect Ratio Buttons */}
            <div>
              <div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground mb-1.5">Aspect Ratio</div>
              <div className="flex flex-wrap gap-1.5">
                {["16:9", "9:16", "1:1", "4:5"].map((ar) => (
                  <button
                    key={ar}
                    type="button"
                    onClick={() => setCfg({ ...cfg, mj_ar: ar })}
                    className={`text-xs px-2.5 py-1 rounded border transition-all ${
                      cfg.mj_ar === ar
                        ? "bg-primary text-primary-foreground border-primary font-medium"
                        : "bg-muted hover:bg-secondary border-border text-muted-foreground"
                    }`}
                  >
                    {ar} {ar === "16:9" ? "🖥️" : ar === "9:16" ? "📱" : ar === "1:1" ? "🔲" : "📸"}
                  </button>
                ))}
                <Input
                  className="h-7 w-20 text-xs text-mono"
                  placeholder="Custom"
                  value={cfg.mj_ar}
                  onChange={(e) => setCfg({ ...cfg, mj_ar: e.target.value })}
                />
              </div>
            </div>

            {/* Render Quality Selection */}
            <div>
              <div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground mb-1">Rendering Quality (--quality)</div>
              <Select 
                value={cfg.mj_quality || "1"} 
                onValueChange={(val) => setCfg(prev => ({ ...prev, mj_quality: val }))}
              >
                <SelectTrigger className="h-8 text-xs">
                  <SelectValue placeholder="Select quality..." />
                </SelectTrigger>
                <SelectContent className="text-xs">
                  <SelectItem value="0.25">0.25 (Fast/Rough drafts)</SelectItem>
                  <SelectItem value="0.5">0.5 (Standard speed draft)</SelectItem>
                  <SelectItem value="1">1.0 (Full standard beauty)</SelectItem>
                  <SelectItem value="2">2.0 (Double detailed beauty)</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {/* Seed and Negative Prompts inputs */}
            <div className="grid grid-cols-2 gap-2 text-xs">
              <div className="space-y-1">
                <span className="text-[9px] text-mono uppercase text-muted-foreground">Seed Value (--seed)</span>
                <Input 
                  type="number"
                  placeholder="e.g. 1948" 
                  value={cfg.mj_seed || ""}
                  onChange={e => setCfg(prev => ({ ...prev, mj_seed: e.target.value }))}
                  className="h-7 text-xs font-mono"
                />
              </div>
              <div className="space-y-1">
                <span className="text-[9px] text-mono uppercase text-muted-foreground">Negative (--no)</span>
                <Input 
                  placeholder="e.g. text, watermark" 
                  value={cfg.mj_no || ""}
                  onChange={e => setCfg(prev => ({ ...prev, mj_no: e.target.value }))}
                  className="h-7 text-xs"
                />
              </div>
            </div>

            {/* Parameters Sliders with Descriptions */}
            <div className="space-y-3.5">
              <div>
                <div className="flex justify-between items-center mb-1">
                  <span className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">Chaos: <span className="font-semibold text-primary">{cfg.mj_chaos}</span></span>
                  <span className="text-[9px] text-muted-foreground italic">Variety &amp; surprise</span>
                </div>
                <Slider value={[cfg.mj_chaos]} onValueChange={(v) => setCfg({ ...cfg, mj_chaos: v[0] })} max={100} step={1} className="py-1" />
              </div>

              <div>
                <div className="flex justify-between items-center mb-1">
                  <span className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">Stylize: <span className="font-semibold text-primary">{cfg.mj_stylize}</span></span>
                  <span className="text-[9px] text-muted-foreground italic">Artistic flare</span>
                </div>
                <Slider value={[cfg.mj_stylize]} onValueChange={(v) => setCfg({ ...cfg, mj_stylize: v[0] })} max={1000} step={10} className="py-1" />
              </div>

              <div>
                <div className="flex justify-between items-center mb-1">
                  <span className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">Weird: <span className="font-semibold text-primary">{cfg.mj_weird}</span></span>
                  <span className="text-[9px] text-muted-foreground italic">Quirky &amp; unusual</span>
                </div>
                <Slider value={[cfg.mj_weird]} onValueChange={(v) => setCfg({ ...cfg, mj_weird: v[0] })} max={3000} step={50} className="py-1" />
              </div>
            </div>

            {/* Live Prompt Suffix Preview Box */}
            <div className="bg-muted/70 rounded-lg p-2.5 border border-border/40 text-xs">
              <div className="flex items-center justify-between text-muted-foreground text-[9px] uppercase tracking-wider mb-1">
                <span className="flex items-center gap-1 font-mono"><Eye className="w-3 h-3" /> Live Midjourney Suffix</span>
                <button
                  onClick={() => {
                    navigator.clipboard.writeText(mjParams);
                    toast.success("Suffix copied to clipboard!");
                  }}
                  className="hover:text-primary transition-colors flex items-center gap-1 font-mono text-[9px]"
                >
                  <Copy className="w-3 h-3" /> Copy
                </button>
              </div>
              <div className="font-mono text-[10px] text-primary break-all leading-tight bg-background/50 p-1.5 rounded border border-border/20">
                {mjParams}
              </div>
            </div>
          </div>

          {/* STYLE KEYWORDS & STYLE PACKS */}
          <div className="border-t border-border/50 pt-4 space-y-3">
            <div className="flex items-center justify-between">
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">Style Keywords &amp; Packs</div>
              <button onClick={clearKw} className="text-[9px] font-mono text-muted-foreground hover:text-destructive flex items-center gap-0.5"><X className="w-2.5 h-2.5" /> Clear All</button>
            </div>
            
            {/* Custom Keyword Input */}
            <div className="flex gap-2">
              <Input
                placeholder="type custom keyword (e.g. 'cinematic light')"
                value={customKw}
                onChange={(e) => setCustomKw(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    addCustomKw();
                  }
                }}
                className="h-8 text-xs"
              />
              <Button size="sm" className="h-8 text-xs" onClick={addCustomKw}>Add</Button>
            </div>

            <div>
              <div className="text-[9px] text-mono uppercase tracking-widest text-muted-foreground mb-1.5">Presets Packs (click to add)</div>
              <div className="flex flex-wrap gap-1">
                {Object.keys(STYLE_PACKS).map(p => (
                  <button
                    key={p}
                    type="button"
                    data-testid={`composer-pack-${p}`}
                    onClick={() => applyPack(p)}
                    className="text-[9px] font-mono px-2 py-0.5 bg-muted rounded hover:bg-primary hover:text-primary-foreground border border-border/40 transition-colors"
                  >
                    {p}
                  </button>
                ))}
              </div>
            </div>

            <div>
              <div className="text-[9px] text-mono uppercase tracking-widest text-muted-foreground mb-1.5">Active Prompt Suffix Keywords</div>
              <div className="flex flex-wrap gap-1 max-h-36 overflow-y-auto scroll-thin">
                {(cfg.style_keywords || []).map((k, i) => (
                  <button
                    key={k + i}
                    type="button"
                    onClick={() => toggleKw(k)}
                    className="text-[10px] text-mono px-2 py-0.5 bg-primary/10 text-primary border border-primary/20 rounded flex items-center gap-1 hover:bg-destructive hover:text-destructive-foreground hover:border-destructive transition-all"
                  >
                    {k}
                    <X className="w-2.5 h-2.5" />
                  </button>
                ))}
                {!(cfg.style_keywords || []).length && (
                  <span className="text-[10px] text-muted-foreground italic">No style keywords active</span>
                )}
              </div>
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

          {/* DYNAMIC TARGETS (LANGUAGE × SUNO AI STYLE) */}
          <Card className="p-5">
            <div className="flex items-center justify-between mb-3">
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground flex items-center gap-1.5">
                <Music className="w-4 h-4 text-primary animate-pulse" /> Targets (Channel × Language × Suno AI Styles)
              </div>
              <div className="flex gap-2">
                <Button size="sm" variant="outline" onClick={syncTargetsFromChannels}>Sync from Channels</Button>
                <Button size="sm" variant="secondary" data-testid="composer-add-target" onClick={addTarget}><Plus className="w-3 h-3 mr-1" />Add Channel</Button>
              </div>
            </div>
            
            <div className="space-y-3.5">
              {(cfg.targets||[]).map((t, i) => {
                const isCustomLang = !LANGUAGES.includes(t.language);
                return (
                  <div key={i} className="border border-border/80 rounded-xl p-3.5 bg-muted/20 space-y-3 relative group">
                    <div className="absolute top-2.5 right-2.5">
                      <button onClick={()=>removeTarget(i)} className="text-muted-foreground hover:text-destructive transition-colors"><Trash2 className="w-3.5 h-3.5" /></button>
                    </div>

                    <div className="grid md:grid-cols-12 gap-3 items-end">
                      {/* Language Select Dropdown */}
                      <div className="md:col-span-3 space-y-1">
                        <span className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">Language</span>
                        <Select 
                          value={isCustomLang && t.language ? "custom" : t.language} 
                          onValueChange={(val) => {
                            if (val === "custom") {
                              updateTarget(i, "language", "");
                            } else {
                              updateTarget(i, "language", val);
                            }
                          }}
                        >
                          <SelectTrigger className="h-8 text-xs bg-background">
                            <SelectValue placeholder="Select Language" />
                          </SelectTrigger>
                          <SelectContent className="text-xs">
                            {LANGUAGES.map(lang => (
                              <SelectItem key={lang} value={lang} className="text-xs">{lang}</SelectItem>
                            ))}
                            <SelectItem value="custom" className="text-xs font-semibold">Custom (Type below)...</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>

                      {/* Suno AI Styles Input */}
                      <div className="md:col-span-7 space-y-1">
                        <div className="flex justify-between items-center">
                          <span className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground flex items-center gap-1">Suno AI Style CSV</span>
                          <button 
                            type="button" 
                            onClick={() => setActiveSunoHelperIdx(activeSunoHelperIdx === i ? null : i)}
                            className="text-[9px] text-primary font-semibold hover:underline flex items-center gap-0.5"
                          >
                            <Sparkles className="w-2.5 h-2.5" /> 
                            {activeSunoHelperIdx === i ? "Hide Genre Mixes" : "Genre Presets Helper"}
                          </button>
                        </div>
                        <Input 
                          data-testid={`composer-target-style-${i}`} 
                          value={t.styles} 
                          onChange={e=>updateTarget(i,"styles",e.target.value)} 
                          placeholder="e.g. messianic liquid drum and bass, 174 bpm" 
                          className="h-8 text-xs bg-background" 
                        />
                      </div>

                      {/* Channel title trigger display */}
                      <div className="md:col-span-2 text-center">
                        <Badge variant="outline" className="h-8 font-mono text-[9px] block text-center truncate pt-2">Channel {i+1}</Badge>
                      </div>
                    </div>

                    {/* Conditional Custom Language input */}
                    {isCustomLang && (
                      <div className="w-1/2 space-y-1">
                        <span className="text-[9px] text-mono uppercase text-muted-foreground">Type Custom Language</span>
                        <Input 
                          value={t.language}
                          onChange={e => updateTarget(i, "language", e.target.value)}
                          placeholder="e.g. Yiddish"
                          className="h-7 text-xs"
                        />
                      </div>
                    )}

                    {/* COLLAPSIBLE SUNO GENRE PRESETS HELPER */}
                    {activeSunoHelperIdx === i && (
                      <div className="bg-background border border-border rounded-lg p-3 mt-2 space-y-2.5 animate-in fade-in slide-in-from-top-1 duration-150">
                        <div className="flex justify-between items-center text-[10px] text-mono uppercase tracking-widest text-muted-foreground border-b border-border/40 pb-1.5">
                          <span className="flex items-center gap-1 font-semibold text-primary"><Music className="w-3 h-3" /> Suno Style Prepackaged Genre Mixes</span>
                          <span>Click to Apply</span>
                        </div>
                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 max-h-56 overflow-y-auto scroll-thin">
                          {Object.entries(SUNO_GENRES).map(([genreName, csvContent]) => renderSunoGenreButton(i, genreName, csvContent))}
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
              {!cfg.targets?.length && (
                <div className="text-xs text-muted-foreground text-center py-4 border border-dashed border-border rounded-lg">No targets added. Click 'Add Channel' above.</div>
              )}
            </div>
          </Card>

          {/* GENERATED RESULTS WORKSPACE */}
          <Card className="p-5">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3 flex items-center justify-between">
              <span>Generated Results ({items.length})</span>
              {items.length > 0 && <span className="text-[9px] text-emerald-500 font-mono">Ready to import</span>}
            </div>
            {items.length > 0 ? (
              <div className="space-y-4">
                <div className="space-y-2 max-h-80 overflow-y-auto scroll-thin">
                  {items.map((item, idx) => (
                    <div key={idx} className="border border-border/80 rounded-lg p-3 bg-muted/20 text-xs space-y-2">
                      <div className="flex justify-between items-center font-bold">
                        <span className="text-primary truncate max-w-[200px]">{item.title}</span>
                        <Badge variant="secondary" className="font-mono text-[9px]">{item.language}</Badge>
                      </div>
                      {item.styles && <div className="text-muted-foreground italic text-[10px]">Style: {item.styles}</div>}
                      <div className="max-h-24 overflow-y-auto bg-background/50 border border-border/30 rounded p-1.5 text-mono font-mono text-[9px] leading-relaxed scroll-thin">
                        {(item.lyrics || []).map((l, lIdx) => (
                          <div key={lIdx} className="mb-1">
                            <span className="text-primary/75 font-semibold">[{l.section}]</span> {l.lines}
                          </div>
                        ))}
                      </div>
                      {item.image_prompt && (
                        <div className="p-1.5 bg-background border border-border/30 rounded text-[9px] font-mono break-all text-muted-foreground">
                          <span className="font-semibold text-primary">Prompt:</span> {item.image_prompt}
                        </div>
                      )}
                    </div>
                  ))}
                </div>

                <div className="flex flex-wrap gap-2 justify-end">
                  <Button size="sm" variant="ghost" onClick={downloadJson}><Download className="w-3.5 h-3.5 mr-2" />Save as JSON file</Button>
                  <Button size="sm" onClick={sendToLyrics}><ArrowRight className="w-3.5 h-3.5 mr-2" />Send to Studio Lyrics Editor</Button>
                </div>
              </div>
            ) : (
              <div className="text-xs text-muted-foreground py-6 text-center border border-dashed border-border rounded-lg">
                <Bot className="w-8 h-8 mx-auto text-muted-foreground/60 mb-2" />
                No results generated yet. Load bible text and click 'Generate' above.
              </div>
            )}
          </Card>
        </div>
      </div>
    </div>
  );
}
