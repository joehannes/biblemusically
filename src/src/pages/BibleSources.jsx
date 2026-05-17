import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { Badge } from "../components/ui/badge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../components/ui/tabs";
import { BookOpen, ArrowRight, Loader2, Copy, Save, Trash2, FileText, Globe } from "lucide-react";
import { toast } from "sonner";
import { useNavigate } from "react-router-dom";
import { useAutoSave, AutoSaveChip } from "../lib/hooks";

export default function BibleSources() {
  const nav = useNavigate();
  const [tab, setTab] = useState("remote");
  const [translations, setTranslations] = useState({});
  const [books, setBooks] = useState([]);
  const [language, setLanguage] = useState("English (modern)");
  const [translation, setTranslation] = useState("web");
  const [book, setBook] = useState("john");
  const [chapter, setChapter] = useState(1);
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(false);
  // pasted
  const [pasted, setPasted] = useState([]);
  const [pasteName, setPasteName] = useState(""); const [pasteLang, setPasteLang] = useState("English");
  const [pasteBook, setPasteBook] = useState(""); const [pasteCh, setPasteCh] = useState(1);
  const [pasteTrans, setPasteTrans] = useState(""); const [pasteText, setPasteText] = useState("");
  const [editingPasted, setEditingPasted] = useState(null);

  // auto-save the "remote" pick to drafts so reload restores the user's last context
  const remoteState = { language, translation, book, chapter };
  const { status: asStatus, lastSaved, restore } = useAutoSave("bible-remote", remoteState, { delay: 500 });
  const { status: asPasteStatus, lastSaved: lpStored, restore: restorePaste } =
    useAutoSave("bible-paste-draft",
      { pasteName, pasteLang, pasteBook, pasteCh, pasteTrans, pasteText, editingPasted }, { delay: 700 });

  useEffect(() => {
    api.bibleTranslations().then(setTranslations);
    api.bibleBooks().then(setBooks);
    api.listPasted().then(setPasted);
    (async () => {
      const v = await restore();
      if (v) {
        if (v.language) setLanguage(v.language);
        if (v.translation) setTranslation(v.translation);
        if (v.book) setBook(v.book);
        if (v.chapter) setChapter(v.chapter);
      }
      const p = await restorePaste();
      if (p) {
        if (p.pasteName !== undefined) setPasteName(p.pasteName);
        if (p.pasteLang) setPasteLang(p.pasteLang);
        if (p.pasteBook !== undefined) setPasteBook(p.pasteBook);
        if (p.pasteCh) setPasteCh(p.pasteCh);
        if (p.pasteTrans !== undefined) setPasteTrans(p.pasteTrans);
        if (p.pasteText !== undefined) setPasteText(p.pasteText);
        if (p.editingPasted !== undefined) setEditingPasted(p.editingPasted);
      }
    })();
    // eslint-disable-next-line
  }, []);

  const fetchChapter = async () => {
    setLoading(true); setData(null);
    try {
      const d = await api.bibleChapter(translation, book, chapter);
      setData(d);
      toast.success(`Loaded ${d.reference}`);
    } catch (e) {
      toast.error(e?.response?.data?.detail || "fetch failed");
    } finally { setLoading(false); }
  };

  const sendToComposer = (payload) => {
    localStorage.setItem("studio:bible-selection", JSON.stringify(payload));
    toast.success("Sent to AI Composer");
    nav("/composer");
  };

  const sendRemote = () => {
    if (!data) return;
    sendToComposer({
      reference: data.reference, language, translation, book, chapter,
      text: data.verses.map(v => `${v.verse}. ${v.text}`).join("\n"),
    });
  };

  const sendPasted = (p) => {
    sendToComposer({
      reference: p.name || `${p.book||""} ${p.chapter||""}`.trim(),
      language: p.language || "English", translation: p.translation || "manual",
      book: p.book || "", chapter: p.chapter || 0, text: p.text,
    });
  };

  const copyText = async () => {
    if (!data) return;
    await navigator.clipboard.writeText(data.verses.map(v => `${v.verse}. ${v.text}`).join("\n"));
    toast.success("Chapter text copied");
  };

  const savePasted = async () => {
    if (!pasteName || !pasteText) return toast.error("Name and text required");
    const doc = await api.savePasted({
      id: editingPasted || undefined, name: pasteName, language: pasteLang,
      translation: pasteTrans || "manual", book: pasteBook, chapter: parseInt(pasteCh)||0, text: pasteText
    });
    toast.success(`Saved "${doc.name}"`);
    setPasteName(""); setPasteBook(""); setPasteCh(1); setPasteTrans(""); setPasteText(""); setEditingPasted(null);
    setPasted(await api.listPasted());
  };

  const editPasted = (p) => {
    setEditingPasted(p.id); setPasteName(p.name); setPasteLang(p.language);
    setPasteBook(p.book); setPasteCh(p.chapter); setPasteTrans(p.translation); setPasteText(p.text);
  };
  const delPasted = async (id) => { await api.deletePasted(id); setPasted(await api.listPasted()); };

  const trList = translations[language] || [];
  const bookObj = books.find(b => b.id === book) || books[0];
  const maxCh = bookObj?.chapters || 50;

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step 0 · source</div>
      <div className="flex items-center justify-between mb-2 flex-wrap gap-3">
        <h1 className="text-4xl sm:text-5xl font-bold">Bible Sources</h1>
        <AutoSaveChip status={tab==="remote" ? asStatus : asPasteStatus} lastSaved={tab==="remote"?lastSaved:lpStored} />
      </div>
      <p className="text-muted-foreground mb-6 max-w-2xl">Hybrid sources for monetization-safe lyrics: <span className="text-foreground">bible-api.com</span> (public-domain English including YLT/Darby/OEB) + <span className="text-foreground">helloao.org</span> (50+ languages + Greek/Hebrew/literal English). Or paste your own.</p>

      <Tabs value={tab} onValueChange={setTab} className="mb-6">
        <TabsList>
          <TabsTrigger value="remote" data-testid="bible-tab-remote"><Globe className="w-3 h-3 mr-2" />Remote</TabsTrigger>
          <TabsTrigger value="paste" data-testid="bible-tab-paste"><FileText className="w-3 h-3 mr-2" />Paste your own</TabsTrigger>
        </TabsList>

        <TabsContent value="remote" className="mt-6">
          <Card className="p-5 mb-6 grid md:grid-cols-12 gap-3 items-end">
            <div className="md:col-span-3">
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Language group</div>
              <Select value={language} onValueChange={(v)=>{setLanguage(v); const ts = translations[v]||[]; if (ts[0]) setTranslation(ts[0].id);}}>
                <SelectTrigger data-testid="bible-lang-select"><SelectValue /></SelectTrigger>
                <SelectContent>{Object.keys(translations).map(l => <SelectItem key={l} value={l}>{l}</SelectItem>)}</SelectContent>
              </Select>
            </div>
            <div className="md:col-span-4">
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Translation</div>
              <Select value={translation} onValueChange={setTranslation}>
                <SelectTrigger data-testid="bible-translation-select"><SelectValue /></SelectTrigger>
                <SelectContent>{trList.map(t => <SelectItem key={t.id} value={t.id}>{t.name}</SelectItem>)}</SelectContent>
              </Select>
            </div>
            <div className="md:col-span-2">
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Book</div>
              <Select value={book} onValueChange={(v)=>{setBook(v); setChapter(1);}}>
                <SelectTrigger data-testid="bible-book-select"><SelectValue /></SelectTrigger>
                <SelectContent>{books.map(b => <SelectItem key={b.id} value={b.id}>{b.name}</SelectItem>)}</SelectContent>
              </Select>
            </div>
            <div className="md:col-span-1">
              <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Ch</div>
              <Input data-testid="bible-chapter-input" type="number" min={1} max={maxCh} value={chapter} onChange={e=>setChapter(parseInt(e.target.value)||1)} className="text-mono" />
            </div>
            <Button data-testid="bible-fetch-btn" onClick={fetchChapter} disabled={loading} className="md:col-span-2">
              {loading ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <BookOpen className="w-4 h-4 mr-2" />}Load
            </Button>
          </Card>

          {data && (
            <Card className="p-6 fade-in">
              <div className="flex items-center justify-between mb-4 flex-wrap gap-3">
                <div>
                  <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground">{data.translation}</div>
                  <h2 className="text-2xl font-semibold">{data.reference}</h2>
                  <Badge variant="secondary" className="mt-1">{data.verses.length} verses</Badge>
                </div>
                <div className="flex gap-2">
                  <Button size="sm" variant="secondary" data-testid="bible-copy-btn" onClick={copyText}><Copy className="w-3 h-3 mr-2" />Copy</Button>
                  <Button size="sm" data-testid="bible-send-btn" onClick={sendRemote}>Send to AI Composer <ArrowRight className="w-3 h-3 ml-2" /></Button>
                </div>
              </div>
              <div className="prose prose-invert max-w-none text-base leading-relaxed font-serif" style={{fontFamily:"Georgia,serif"}}>
                {data.verses.map(v => (
                  <p key={v.verse} data-testid={`bible-verse-${v.verse}`} className="my-2">
                    <sup className="text-primary text-mono mr-2">{v.verse}</sup>{v.text}
                  </p>
                ))}
              </div>
            </Card>
          )}
          {!data && !loading && <Card className="p-10 text-center text-muted-foreground border-dashed">Pick a translation, book and chapter, then load.</Card>}
        </TabsContent>

        <TabsContent value="paste" className="mt-6">
          <Card className="p-5 mb-6">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3">{editingPasted ? "Edit pasted chapter" : "Paste a chapter"}</div>
            <div className="grid md:grid-cols-12 gap-3 mb-3">
              <Input data-testid="paste-name" placeholder="Name (e.g. John 1 — my preferred reading)" value={pasteName} onChange={e=>setPasteName(e.target.value)} className="md:col-span-5" />
              <Input data-testid="paste-lang" placeholder="Language" value={pasteLang} onChange={e=>setPasteLang(e.target.value)} className="md:col-span-2" />
              <Input data-testid="paste-translation" placeholder="Translation (e.g. NIV / my own)" value={pasteTrans} onChange={e=>setPasteTrans(e.target.value)} className="md:col-span-2" />
              <Input data-testid="paste-book" placeholder="Book" value={pasteBook} onChange={e=>setPasteBook(e.target.value)} className="md:col-span-2" />
              <Input data-testid="paste-chapter" type="number" min={0} placeholder="Ch" value={pasteCh} onChange={e=>setPasteCh(parseInt(e.target.value)||0)} className="md:col-span-1 text-mono" />
            </div>
            <Textarea data-testid="paste-text" rows={12} value={pasteText} onChange={e=>setPasteText(e.target.value)} placeholder="Paste the chapter text here — one verse per line ideally. Auto-saved as you type." className="text-sm leading-relaxed" />
            <div className="flex justify-end gap-2 mt-3">
              {editingPasted && <Button variant="ghost" onClick={()=>{setEditingPasted(null); setPasteName(""); setPasteText("");}}>Cancel edit</Button>}
              <Button data-testid="paste-save" onClick={savePasted}><Save className="w-4 h-4 mr-2" />{editingPasted ? "Update" : "Save chapter"}</Button>
            </div>
          </Card>

          <Card className="p-5">
            <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-3">Saved pasted chapters ({pasted.length})</div>
            <div className="space-y-2">
              {pasted.map(p => (
                <div key={p.id} data-testid={`pasted-${p.id}`} className="border border-border rounded-md p-3 flex items-center gap-3">
                  <div className="flex-1 min-w-0">
                    <div className="font-medium truncate">{p.name}</div>
                    <div className="text-xs text-muted-foreground truncate">{p.language} · {p.translation} · {p.book} {p.chapter}</div>
                    <div className="text-[11px] text-muted-foreground line-clamp-1 italic">{(p.text||"").slice(0,140)}</div>
                  </div>
                  <Button size="sm" variant="ghost" data-testid={`pasted-edit-${p.id}`} onClick={()=>editPasted(p)}>edit</Button>
                  <Button size="sm" data-testid={`pasted-send-${p.id}`} onClick={()=>sendPasted(p)}>Use <ArrowRight className="w-3 h-3 ml-1" /></Button>
                  <Button size="sm" variant="ghost" onClick={()=>delPasted(p.id)}><Trash2 className="w-3 h-3 text-destructive" /></Button>
                </div>
              ))}
              {!pasted.length && <div className="text-xs text-muted-foreground italic">No pasted chapters saved yet.</div>}
            </div>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
