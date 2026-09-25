import type { TimelineEvent } from "@/api/types";
import { cn, formatTime } from "@/utils";

const PHASE_CLASS: Record<TimelineEvent["phase"], string> = {
  prepared: "text-fg-2",
  request: "text-info",
  response: "text-ok",
  auth: "text-accent",
  redirect: "text-warn",
  error: "text-danger",
};

interface TimelinePaneProps {
  timeline: TimelineEvent[];
}

export default function TimelinePane({ timeline }: TimelinePaneProps) {
  if (timeline.length === 0) {
    return (
      <div className="flex-1 min-h-0 overflow-y-auto p-2">
        <div className="text-xs text-fg-2">No timeline events</div>
      </div>
    );
  }

  return (
    <div className="flex-1 min-h-0 overflow-y-auto p-2 flex flex-col gap-1">
      {timeline.map((ev, i) => (
        <div key={i} className="flex items-start gap-2">
          <span className="font-mono text-[10px] text-fg-2 shrink-0 mt-px tabular-nums">
            {formatTime(ev.ts)}
          </span>
          <span
            className={cn(
              "text-[9px] uppercase font-semibold px-1 py-px rounded bg-bg-3 shrink-0",
              PHASE_CLASS[ev.phase] ?? "text-fg-2",
            )}
          >
            {ev.phase}
          </span>
          <span className="font-mono text-[11px] text-fg-0 break-all min-w-0">
            {ev.message}
          </span>
        </div>
      ))}
    </div>
  );
}

export { TimelinePane };
