import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Badge } from "./ui/badge";
import { Feather } from "lucide-react";

// ─────────────────────────────────────────────────────────────────────────────
// Whose voice a piece is written in.
//
// Named for what it picks rather than for the word "voice", because `VoicePicker` already means
// something else in this app — the assistant's speaking voice — and two components called the same
// thing is how one of them quietly gets overwritten.
//
// Two axes, because they are two questions. A **tradition** is a body of technique with a place and
// a history — the King James cadence, Andalusian deep song, pansori, skaz — and it is the frame
// everything sits inside. The **dials** are the surface of the prose itself: how the sentences go,
// how much is said in images, things or ideas, how raised the voice is. A person can take a
// tradition and still ask for shorter sentences.
//
// Traditions are filtered to the language being written in, and the language's own come before the
// ones that work anywhere — somebody writing in Korean should meet pansori before "plain speech".
//
// The exemplars are shown because they are how a person recognises what they are choosing. They are
// named as where a technique can be heard, never as somebody to impersonate: the backend's prompt
// says so explicitly, and the unit is always the tradition rather than the writer.
// ─────────────────────────────────────────────────────────────────────────────

const KIND_LABEL = { prose: "prose", verse: "verse", oratory: "spoken", story: "storytelling" };

export default function AuthorialVoice({ language, value = {}, onChange, compact = false }) {
  const [cat, setCat] = useState({ traditions: [], dials: [] });

  useEffect(() => {
    api.authorialCatalogue(language || null)
      .then((r) => r && setCat(r))
      .catch(() => setCat({ traditions: [], dials: [] }));
  }, [language]);

  const set = (key, id) => onChange?.({ ...value, [key]: value[key] === id ? "" : id });
  const chosen = cat.traditions.find((t) => t.id === value.tradition);

  if (!cat.traditions.length) return null;

  return (
    <div className="space-y-4">
      <div className="space-y-1.5">
        <div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">
          The tradition
        </div>
        <div className="text-[11px] text-muted-foreground leading-snug">
          A body of technique with a place and a history — not an author to imitate. The ones written
          in this language come first.
        </div>
        <div className={`grid gap-2 ${compact ? "sm:grid-cols-2" : "sm:grid-cols-2 lg:grid-cols-3"}`}>
          {cat.traditions.map((t) => {
            const active = value.tradition === t.id;
            return (
              <button key={t.id} data-testid={`voice-tradition-${t.id}`} onClick={() => set("tradition", t.id)}
                      className={`text-left rounded-md border p-2 transition-colors ${
                        active ? "border-primary bg-primary/5" : "border-border hover:border-primary/50"}`}>
                <div className="flex items-center gap-1.5 flex-wrap">
                  <span className="text-xs font-medium">{t.label}</span>
                  <Badge variant="outline" className="text-[9px] font-normal">
                    {KIND_LABEL[t.kind] || t.kind}
                  </Badge>
                </div>
                <div className="text-[10px] text-muted-foreground leading-snug">{t.hint}</div>
                <div className="text-[9px] text-muted-foreground/70 mt-0.5">{t.region}</div>
              </button>
            );
          })}
        </div>
        {chosen?.exemplars?.length > 0 && (
          <p className="text-[11px] text-muted-foreground">
            <span>Where this is heard: </span>
            <span className="text-foreground">{chosen.exemplars.join(", ")}</span>
            <span>. The technique is what travels — nobody's voice is imitated and no name reaches the page.</span>
          </p>
        )}
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        {cat.dials.map((d) => (
          <div key={d.id} className="space-y-1">
            <div className="text-[10px] text-mono uppercase tracking-widest text-muted-foreground">
              {d.label}
            </div>
            <div className="flex flex-wrap gap-1">
              {d.options.map((o) => (
                <button key={o.id} title={o.hint} data-testid={`voice-${d.id}-${o.id}`}
                        onClick={() => set(d.id, o.id)}
                        className={`text-[11px] rounded border px-1.5 py-0.5 transition-colors ${
                          value[d.id] === o.id ? "border-primary bg-primary/10 text-primary"
                                               : "border-border text-muted-foreground hover:border-primary/40"}`}>
                  {o.label}
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>

      {!compact && (
        <p className="text-[11px] text-muted-foreground flex items-start gap-1.5">
          <Feather className="w-3 h-3 mt-0.5 shrink-0 text-primary" />
          <span>
            Leave any of it unset and nothing is said about it — an unchosen dial is silence rather
            than a default the model has to argue with.
          </span>
        </p>
      )}
    </div>
  );
}
