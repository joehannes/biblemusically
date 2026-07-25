import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { Button } from "./ui/button";
import { speak, stopSpeaking, voicePrefs, setVoicePrefs, voiceInputAvailable } from "../lib/voice";
import { Volume2, Loader2, Mic, Check } from "lucide-react";
import { toast } from "sonner";

// Choosing the assistant's voice, with a listen-before-you-pick button — the only honest way to pick
// a voice. Used by the first-run guide and by Settings, so the two can never drift apart.
//
// Every phrase is cached on disk by (text, voice) in the backend, so auditioning voices costs one
// request each and nothing thereafter.
export default function VoicePicker({ compact = false, onChange }) {
  const [prefs, setPrefs] = useState(() => voicePrefs());
  const [voices, setVoices] = useState([]);
  const [engines, setEngines] = useState([]);
  const [previewing, setPreviewing] = useState("");

  useEffect(() => {
    api.listAssistantVoices()
      .then((r) => { setVoices(r?.voices || []); setEngines(r?.engines || []); })
      .catch(() => { /* the picker degrades to whatever is already chosen */ });
    return () => stopSpeaking();
  }, []);

  const update = (patch) => {
    const next = setVoicePrefs(patch);
    setPrefs(next);
    onChange?.(next);
    // Mirrored into settings so the backend's own spoken output (and any later device) agrees.
    api.saveSettings({
      assistant_voice: next.voice,
      voice_engine: next.engine,
      voice_speak: next.speak,
      voice_listen: next.listen,
    }).catch(() => { /* preference still holds locally */ });
  };

  const preview = async (voice) => {
    setPreviewing(voice);
    try {
      const r = await speak(
        "Hi — I'm your studio guide. I'll walk you through today's song, one decision at a time.",
        { force: true },
      );
      if (!r?.spoken) toast.error("Couldn't play that voice — check your Gemini key in Settings → AI.");
    } finally { setPreviewing(""); }
  };

  return (
    <div className="space-y-3">
      {/* Engine */}
      <div className="space-y-1.5">
        <div className="text-[10px] uppercase tracking-widest text-muted-foreground">How should it speak?</div>
        <div className="grid sm:grid-cols-3 gap-2">
          {(engines.length ? engines : [{ id: "gemini", label: "Gemini voices" }, { id: "browser", label: "System voice" }, { id: "off", label: "Silent" }])
            .map((eng) => (
              <button key={eng.id} onClick={() => update({ engine: eng.id, speak: eng.id !== "off" })}
                className={`text-left rounded-lg border p-2.5 transition-all hover:border-primary/60 ${prefs.engine === eng.id ? "border-primary/60 bg-primary/5" : "border-border"}`}>
                <div className="text-sm font-medium flex items-center gap-1.5">
                  {eng.label}{prefs.engine === eng.id && <Check className="w-3 h-3 text-primary" />}
                </div>
                {eng.detail && <div className="text-[11px] text-muted-foreground mt-0.5 leading-snug">{eng.detail}</div>}
              </button>
            ))}
        </div>
      </div>

      {/* Voice */}
      {prefs.engine === "gemini" && (
        <div className="space-y-1.5">
          <div className="text-[10px] uppercase tracking-widest text-muted-foreground">Which voice?</div>
          <div className={`grid gap-2 ${compact ? "sm:grid-cols-2" : "sm:grid-cols-3"}`}>
            {voices.map((v) => {
              const active = prefs.voice === v.id;
              return (
                <div key={v.id}
                  className={`rounded-lg border p-2.5 ${active ? "border-primary/60 bg-primary/5" : "border-border"}`}>
                  <button onClick={() => update({ voice: v.id })} className="text-left w-full">
                    <div className="text-sm font-medium flex items-center gap-1.5">
                      {v.label}{active && <Check className="w-3 h-3 text-primary" />}
                    </div>
                    <div className="text-[11px] text-muted-foreground">{v.description}</div>
                  </button>
                  <Button size="sm" variant="ghost" className="mt-1 h-6 px-1.5 text-[10px]"
                    onClick={() => { update({ voice: v.id }); preview(v.id); }} disabled={previewing === v.id}>
                    {previewing === v.id
                      ? <><Loader2 className="w-3 h-3 mr-1 animate-spin" />playing</>
                      : <><Volume2 className="w-3 h-3 mr-1" />listen</>}
                  </Button>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Voice answers */}
      <label className="flex items-start gap-2 text-[11px] text-muted-foreground cursor-pointer">
        <input type="checkbox" checked={!!prefs.listen} onChange={(ev) => update({ listen: ev.target.checked })}
          className="mt-0.5" />
        <span>
          <b className="text-foreground">Let me answer out loud.</b> A microphone button appears next to each
          question — say "yes", "the second one", or just name the option.
          {!voiceInputAvailable() && (
            <span className="text-amber-400"> No microphone is reachable from this window yet; allowing access will enable it.</span>
          )}
        </span>
      </label>

      {prefs.listen && voiceInputAvailable() && (
        <Button size="sm" variant="secondary" onClick={async () => {
          try {
            await navigator.mediaDevices.getUserMedia({ audio: true }).then((s) => s.getTracks().forEach((t) => t.stop()));
            toast.success("Microphone access granted.");
          } catch { toast.error("Microphone access was refused — the system may need to allow it for this app."); }
        }}>
          <Mic className="w-3 h-3 mr-1.5" />Test the microphone
        </Button>
      )}
    </div>
  );
}
