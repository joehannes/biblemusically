import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { Card } from "./ui/card";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Clock, AlertTriangle, ChevronDown, ChevronRight, Loader2 } from "lucide-react";

// When each channel will actually publish, shown where publishing is about to happen.
//
// A channel's publishing hour is set on the Channels page, one channel at a time. That is the right
// place to *choose* it and the wrong place to *check* it: the question "is this batch going out at a
// sensible hour for each audience?" is asked here, with the queue in front of you, about all of them
// at once.
//
// The part worth saying plainly is the gap. A channel with no time set publishes the moment the
// upload finishes — which for an audience twelve hours away is the middle of their night, and the
// first hours of a video's life are when the algorithm decides what to do with it. So channels
// without a time lead, and the rest are folded away.

const DAY_SHORT = { monday: "Mon", tuesday: "Tue", wednesday: "Wed", thursday: "Thu", friday: "Fri", saturday: "Sat", sunday: "Sun" };
const shortDays = (days) => {
  if (!Array.isArray(days) || days.length === 0) return "any day";
  if (days.length === 7) return "any day";
  return days.map((d) => DAY_SHORT[String(d).toLowerCase()] || d).join(", ");
};

export default function PublishTimesPanel() {
  const navigate = useNavigate();
  const [rows, setRows] = useState(null);
  const [missing, setMissing] = useState(null);
  const [open, setOpen] = useState(false);

  const load = useCallback(async () => {
    try {
      const [list, gaps] = await Promise.all([
        api.listPublishTimes(),
        api.channelsMissingPublishTime(),
      ]);
      setRows(list?.channels || []);
      setMissing(gaps?.missing || []);
    } catch {
      setRows([]); setMissing([]);
    }
  }, []);
  useEffect(() => { load(); }, [load]);

  if (rows === null) {
    return (
      <Card className="p-4 mb-5 flex items-center gap-2 text-xs text-muted-foreground">
        <Loader2 className="w-3.5 h-3.5 animate-spin" />Checking publishing hours…
      </Card>
    );
  }
  if (rows.length === 0) return null; // no channels yet — nothing to say

  const scheduled = rows.filter((r) => r.time);
  const gapCount = missing?.length || 0;

  return (
    <Card className="p-4 mb-5 space-y-3">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <h2 className="text-sm font-medium flex items-center gap-2">
          <Clock className="w-4 h-4 text-primary" /> Publishing hours
        </h2>
        <Button size="sm" variant="ghost" className="h-7 text-[11px]" onClick={() => navigate("/channels")}>
          Set them on Channels
        </Button>
      </div>

      {gapCount > 0 ? (
        <div className="rounded-md border border-amber-500/40 bg-amber-500/10 p-2.5 space-y-1.5">
          <div className="text-xs text-amber-200 flex items-start gap-1.5">
            <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-px text-amber-400" />
            <span>
              <b>{gapCount} channel{gapCount === 1 ? "" : "s"} publish the moment the upload finishes.</b>{" "}
              For an audience in another timezone that is usually the middle of their night, and the
              first hours are when the algorithm decides what to do with a video.
            </span>
          </div>
          <div className="flex flex-wrap gap-1.5 pl-5">
            {missing.map((m) => (
              <Badge key={m.channel_id} variant="outline" className="text-[10px]">
                {m.name || "Untitled"}
                {m.has_suggestion && <span className="ml-1 text-emerald-400">· suggestion ready</span>}
              </Badge>
            ))}
          </div>
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">
          Every channel has an hour set. Public uploads go out privately and YouTube makes them public
          at that time.
        </p>
      )}

      {scheduled.length > 0 && (
        <div>
          <button onClick={() => setOpen((v) => !v)}
                  className="text-[11px] text-muted-foreground hover:text-foreground flex items-center gap-1">
            {open ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
            {scheduled.length} channel{scheduled.length === 1 ? "" : "s"} with an hour set
          </button>
          {open && (
            <div className="mt-2 space-y-1">
              {scheduled.map((r) => (
                <div key={r.channel_id} className="flex items-center gap-2 text-xs rounded border border-border/50 px-2 py-1.5">
                  <span className="flex-1 min-w-0 truncate">{r.name || "Untitled"}</span>
                  <span className="font-mono text-muted-foreground" data-no-i18n>{r.time}</span>
                  <span className="text-muted-foreground text-[11px] hidden sm:inline">{shortDays(r.days)}</span>
                  {r.timezone && (
                    <span className="text-muted-foreground text-[11px] font-mono hidden md:inline truncate max-w-[12rem]" data-no-i18n>
                      {r.timezone}
                    </span>
                  )}
                  {/* An AI guess and a measured answer must never look alike. */}
                  <Badge variant={r.source === "chosen" ? "secondary" : "outline"} className="text-[9px] shrink-0">
                    {r.source === "chosen" ? "yours" : r.source === "analytics" ? "measured" : "AI guess"}
                  </Badge>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </Card>
  );
}
