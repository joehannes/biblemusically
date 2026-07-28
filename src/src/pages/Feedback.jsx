import { useState } from "react";
import { api } from "../lib/api";
import { sessionErrors } from "../lib/errorReporting";
import { track } from "../lib/analytics";
import { Card } from "../components/ui/card";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Textarea } from "../components/ui/textarea";
import { MessageSquare, Bug, Lightbulb, Heart, HelpCircle, CircleDollarSign, Send, Share2, Copy } from "lucide-react";
import { toast } from "sonner";
import { openUrl } from "@tauri-apps/plugin-opener";

// ─────────────────────────────────────────────────────────────────────────────
// Telling the author something.
//
// The templates exist because a blank box gets nothing. "It broke" is not actionable and neither is
// silence; three prompts that ask what happened, what was expected, and what came instead turn a
// shrug into a fixable report — and they take the writing burden off somebody who is already
// annoyed.
//
// Nothing here is sent without pressing send. Errors go on their own (see lib/errorReporting.js)
// because a crash nobody reports is a crash nobody fixes, but a person's own words are theirs.
// ─────────────────────────────────────────────────────────────────────────────

const KINDS = [
  {
    id: "bug", label: "Something is broken", icon: Bug,
    template: "What I was doing:\n\nWhat I expected:\n\nWhat happened instead:\n",
    hint: "The three lines below are worth more than a paragraph — they are what makes it reproducible.",
  },
  {
    id: "idea", label: "An idea", icon: Lightbulb,
    template: "What I am trying to do:\n\nWhat would make it easier:\n",
    hint: "The problem is more useful than the solution. If you say what you are trying to do, the answer might already exist.",
  },
  {
    id: "confusion", label: "I could not work something out", icon: HelpCircle,
    template: "Where I got stuck:\n\nWhat I thought would happen:\n",
    hint: "This is the most valuable kind. Somewhere the app is confident and wrong about what is obvious.",
  },
  {
    id: "praise", label: "Something worked well", icon: Heart,
    template: "What I made:\n\nWhat helped:\n",
    hint: "Genuinely useful: knowing what to protect is as important as knowing what to fix.",
  },
  {
    id: "money", label: "About the price or the licence", icon: CircleDollarSign,
    template: "",
    hint: "Money questions get answered first.",
  },
];

const SHARE_TEXT =
  "I've been making scripture-to-song videos with Studio Lightkid — lyrics, music, art, video and " +
  "publishing in one place.";

export default function Feedback() {
  const [kind, setKind] = useState("bug");
  const [title, setTitle] = useState("");
  const [message, setMessage] = useState(KINDS[0].template);
  const [sending, setSending] = useState(false);
  const [sent, setSent] = useState(false);
  const errors = sessionErrors();

  const chosen = KINDS.find((k) => k.id === kind) || KINDS[0];

  const pick = (k) => {
    setKind(k.id);
    // Only overwrite a template the user has not typed into — losing somebody's words to a
    // mis-click is the one thing a form must never do.
    const untouched = KINDS.some((other) => other.template === message) || !message.trim();
    if (untouched) setMessage(k.template);
    track(`feedback.kind.${k.id}`);
  };

  const send = async () => {
    if (!message.trim()) { toast.error("There is nothing written yet."); return; }
    setSending(true);
    try {
      await api.sendReport({
        kind, title: title.trim() || chosen.label, message: message.trim(),
        // Attach this session's most frequent error, if there is one. It is usually the thing they
        // are writing about, and it saves them describing a stack trace.
        fingerprint: errors.sort((a, b) => b.count - a.count)[0]?.fingerprint || "",
      });
      track(`feedback.sent.${kind}`);
      setSent(true);
      toast.success("Sent. I read all of these.");
    } catch (err) {
      toast.error(`That could not be sent: ${err}`, { duration: 12000 });
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="p-8 max-w-2xl mx-auto space-y-4 fade-in">
      <div>
        <h1 className="text-xl font-semibold flex items-center gap-2">
          <MessageSquare className="w-5 h-5 text-primary" />Tell me something
        </h1>
        <p className="text-sm text-muted-foreground">
          One person makes this. Everything here is read, and nothing is sent until you press send.
        </p>
      </div>

      <div className="flex flex-wrap gap-2">
        {KINDS.map((k) => (
          <Button key={k.id} size="sm" variant={kind === k.id ? "default" : "secondary"}
                  data-testid={`feedback-kind-${k.id}`} onClick={() => pick(k)}>
            <k.icon className="w-3.5 h-3.5 mr-1.5" />{k.label}
          </Button>
        ))}
      </div>

      <Card className="p-4 space-y-3">
        <p className="text-xs text-muted-foreground">{chosen.hint}</p>
        <Input value={title} onChange={(e) => setTitle(e.target.value)}
               data-testid="feedback-title" placeholder="One line, if you can" />
        <Textarea value={message} onChange={(e) => setMessage(e.target.value)}
                  data-testid="feedback-message" rows={10} className="font-mono text-xs" />
        <div className="flex items-center gap-2 flex-wrap">
          <Button size="sm" onClick={send} disabled={sending} data-testid="feedback-send">
            <Send className="w-3.5 h-3.5 mr-1.5" />{sending ? "Sending…" : "Send"}
          </Button>
          {sent && <Badge variant="outline" className="text-[10px]">sent — thank you</Badge>}
        </div>
        {errors.length > 0 && (
          <p className="text-[11px] text-muted-foreground">
            {errors.length} problem{errors.length === 1 ? " has" : "s have"} already been reported
            automatically this session, so you do not need to describe {errors.length === 1 ? "it" : "them"} —
            only what you were doing.
          </p>
        )}
      </Card>

      {/* ── Telling somebody else ─────────────────────────────────────────── */}
      <Card className="p-4 space-y-2">
        <h2 className="font-medium flex items-center gap-2">
          <Share2 className="w-4 h-4 text-primary" />Tell somebody else
        </h2>
        <p className="text-xs text-muted-foreground">
          Nothing is posted for you and nothing is connected to a social account here. This copies
          the words, and you decide where they go.
        </p>
        <div className="flex flex-wrap gap-2">
          {[
            ["X", `https://twitter.com/intent/tweet?text=${encodeURIComponent(SHARE_TEXT)}`],
            ["Facebook", "https://www.facebook.com/sharer/sharer.php?u=https%3A%2F%2Fstudio-lightkid.johannes-neugschwentner.workers.dev%2F"],
            ["Reddit", `https://www.reddit.com/submit?title=${encodeURIComponent("Studio Lightkid")}&text=${encodeURIComponent(SHARE_TEXT)}`],
          ].map(([label, url]) => (
            <Button key={label} size="sm" variant="secondary"
                    onClick={() => {
                      track(`share.${label.toLowerCase()}`);
                      // Not window.open — it is a silent no-op inside an Android WebView.
                      openUrl(url).catch(() => { try { window.open(url, "_blank", "noopener"); } catch { /* ignore */ } });
                    }}>
              {label}
            </Button>
          ))}
          <Button size="sm" variant="ghost" onClick={() => {
            navigator.clipboard.writeText(SHARE_TEXT);
            track("share.copy");
            toast.success("Copied.");
          }}>
            <Copy className="w-3.5 h-3.5 mr-1.5" />Copy the words
          </Button>
        </div>
      </Card>
    </div>
  );
}
