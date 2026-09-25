import { useEffect, useMemo, useState } from "react";
import { History, Pin, PinOff, Trash2 } from "lucide-react";
import { api } from "@/api/client";
import type { HistoryEntry, HistoryPin } from "@/api/types";
import { Button, EmptyState, IconButton, Modal } from "@/components/ui";
import { cn, formatTime, methodVar, statusClass } from "@/utils";
import { useKeel } from "@/state/store";

function pinKey(ts: string, requestPath: string | null): string {
  return `${ts}|${requestPath ?? ""}`;
}

export function HistoryPanel() {
  const openRequest = useKeel((s) => s.openRequest);
  const toast = useKeel((s) => s.toast);

  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [pins, setPins] = useState<Set<string>>(new Set());
  const [confirmClear, setConfirmClear] = useState(false);

  const refresh = async () => {
    const [list, pinList] = await Promise.all([api.historyList(50), api.historyPins()]);
    setEntries(list);
    setPins(new Set(pinList.map((p) => pinKey(p.ts, p.requestPath ?? null))));
  };

  useEffect(() => {
    let alive = true;
    refresh().catch((err) => {
      if (alive) toast(String(err), "error");
    });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const sorted = useMemo(() => {
    const isPinned = (e: HistoryEntry) => pins.has(pinKey(e.ts, e.requestPath ?? null));
    return [...entries].sort((a, b) => {
      const pa = isPinned(a) ? 1 : 0;
      const pb = isPinned(b) ? 1 : 0;
      if (pa !== pb) return pb - pa;
      return a.ts < b.ts ? 1 : -1;
    });
  }, [entries, pins]);

  const togglePin = async (entry: HistoryEntry) => {
    const key = pinKey(entry.ts, entry.requestPath ?? null);
    const pin: HistoryPin = { ts: entry.ts, requestPath: entry.requestPath ?? null };
    try {
      if (pins.has(key)) {
        await api.historyUnpin(entry.ts, entry.requestPath ?? null);
        setPins((prev) => {
          const next = new Set(prev);
          next.delete(key);
          return next;
        });
      } else {
        await api.historyPin(pin);
        setPins((prev) => new Set(prev).add(key));
      }
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const clear = async () => {
    setConfirmClear(false);
    try {
      await api.historyClear();
      await refresh();
      toast("History cleared", "success");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-9 shrink-0 border-b border-line-0 flex items-center px-2.5 gap-2 text-xs font-semibold uppercase tracking-wider text-fg-2">
        <span className="flex-1">History</span>
        <IconButton
          title="Clear history"
          className="h-6 w-6"
          disabled={entries.length === 0}
          onClick={() => setConfirmClear(true)}
        >
          <Trash2 size={13} />
        </IconButton>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto p-1">
        {entries.length === 0 ? (
          <EmptyState icon={<History size={24} />} title="No requests yet" />
        ) : (
          sorted.map((entry, i) => {
            const key = pinKey(entry.ts, entry.requestPath ?? null);
            const isPinned = pins.has(key);
            return (
              <div
                key={`${entry.ts}-${i}`}
                title={entry.url}
                onClick={() =>
                  entry.requestPath
                    ? openRequest(entry.requestPath)
                    : toast("Request file no longer exists")
                }
                className="group h-7 px-2 flex items-center gap-1.5 rounded text-xs cursor-pointer select-none text-fg-1 hover:bg-bg-hover"
              >
                <button
                  type="button"
                  title={isPinned ? "Unpin" : "Pin"}
                  aria-label={isPinned ? "Unpin" : "Pin"}
                  onClick={(e) => {
                    e.stopPropagation();
                    void togglePin(entry);
                  }}
                  className={cn(
                    "shrink-0 w-4 h-4 inline-flex items-center justify-center rounded",
                    isPinned
                      ? "text-accent"
                      : "text-fg-2 opacity-0 group-hover:opacity-100 hover:text-fg-0",
                  )}
                >
                  {isPinned ? <Pin size={11} /> : <PinOff size={11} />}
                </button>
                <span
                  className="w-10 shrink-0 text-center font-mono text-[10px] font-bold"
                  style={{ color: methodVar(entry.method) }}
                >
                  {entry.method}
                </span>
                <span className="truncate flex-1">{entry.url}</span>
                <span className={cn("shrink-0 font-mono text-[10px]", statusClass(entry.status))}>
                  {entry.status ?? "—"}
                </span>
                <span className="shrink-0 text-[10px] text-fg-2">{formatTime(entry.ts)}</span>
              </div>
            );
          })
        )}
      </div>

      <Modal
        open={confirmClear}
        onClose={() => setConfirmClear(false)}
        title="Clear history"
        width="max-w-sm"
      >
        <p className="text-xs text-fg-1">
          Delete all history entries and pins? This cannot be undone.
        </p>
        <div className="flex justify-end gap-2 mt-4">
          <Button variant="ghost" onClick={() => setConfirmClear(false)}>
            Cancel
          </Button>
          <Button variant="danger" onClick={clear}>
            Clear
          </Button>
        </div>
      </Modal>
    </div>
  );
}

export default HistoryPanel;
