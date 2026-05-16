import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Card } from "../components/ui/card";
import { Button } from "../components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../components/ui/select";
import { Input } from "../components/ui/input";
import { Badge } from "../components/ui/badge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../components/ui/tabs";
import { BookOpen, ArrowRight, Loader2, Copy } from "lucide-react";
import { toast } from "sonner";
import { useNavigate } from "react-router-dom";

export default function BibleSources() {
  const nav = useNavigate();
  const [translations, setTranslations] = useState({});
  const [books, setBooks] = useState([]);
  const [language, setLanguage] = useState("English");
  const [translation, setTranslation] = useState("web");
  const [book, setBook] = useState("john");
  const [chapter, setChapter] = useState(1);
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    api.bibleTranslations().then(setTranslations);
    api.bibleBooks().then(setBooks);
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

  const sendToComposer = () => {
    if (!data) return;
    const payload = {
      reference: data.reference,
      language,
      translation,
      book,
      chapter,
      text: data.verses.map(v => `${v.verse}. ${v.text}`).join("\n"),
    };
    localStorage.setItem("studio:bible-selection", JSON.stringify(payload));
    toast.success("Sent to AI Composer");
    nav("/composer");
  };

  const copyText = async () => {
    if (!data) return;
    await navigator.clipboard.writeText(data.verses.map(v => `${v.verse}. ${v.text}`).join("\n"));
    toast.success("Chapter text copied");
  };

  const trList = translations[language] || [];
  const bookObj = books.find(b => b.id === book) || books[0];
  const maxCh = bookObj?.chapters || 50;

  return (
    <div className="p-8 max-w-7xl mx-auto fade-in">
      <div className="text-mono text-[11px] uppercase tracking-[0.3em] text-muted-foreground mb-2">step 0 · source</div>
      <h1 className="text-4xl sm:text-5xl font-bold mb-2">Bible Sources</h1>
      <p className="text-muted-foreground mb-6 max-w-2xl">Public-domain translations only — safe for YouTube/Spotify monetization. English from <span className="text-foreground">bible-api.com</span>, others from <span className="text-foreground">wldeh/bible-api</span>.</p>

      <Card className="p-5 mb-6 grid md:grid-cols-12 gap-3 items-end">
        <div className="md:col-span-2">
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Language</div>
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
        <div className="md:col-span-3">
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Book</div>
          <Select value={book} onValueChange={(v)=>{setBook(v); setChapter(1);}}>
            <SelectTrigger data-testid="bible-book-select"><SelectValue /></SelectTrigger>
            <SelectContent>{books.map(b => <SelectItem key={b.id} value={b.id}>{b.name}</SelectItem>)}</SelectContent>
          </Select>
        </div>
        <div className="md:col-span-1">
          <div className="text-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-1">Chapter</div>
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
              <Button size="sm" data-testid="bible-send-btn" onClick={sendToComposer}>Send to AI Composer <ArrowRight className="w-3 h-3 ml-2" /></Button>
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
    </div>
  );
}
