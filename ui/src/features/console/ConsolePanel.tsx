import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import { useKeel } from "@/state/store";
import { cn } from "@/utils";

const MIN_HEIGHT = 80;
const DEFAULT_HEIGHT = 176;

function readHeight() {
  const n = Number(localStorage.getItem("keel.consoleHeight"));
  return Number.isFinite(n) && n >= MIN_HEIGHT ? n : DEFAULT_HEIGHT;
}

export function ConsolePanel() {
  const tab = useKeel((s) => s.tabs.find((t) => t.path === s.activePath));
  const setConsoleOpen = useKeel((s) => s.setConsoleOpen);
  const [height, setHeight] = useState(readHeight);
  const dragging = useRef(false);
  const startY = useRef(0);
  const startH = useRef(0);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      const max = Math.max(MIN_HEIGHT, window.innerHeight - 120);
      const next = Math.max(
        MIN_HEIGHT,
        Math.min(max, startH.current + (startY.current - e.clientY)),
      );
      setHeight(next);
    };
    const onUp = () => {
      if (!dragging.current) return;
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      setHeight((h) => {
        localStorage.setItem("keel.consoleHeight", String(Math.round(h)));
        return h;
      });
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  const result = tab?.result ?? null;
  const logs = result?.scriptLogs ?? [];
  const scriptError = result?.scriptError ?? null;

  return (
    <div
      style={{ height }}
      className="relative shrink-0 border-t border-line-0 bg-bg-1 flex flex-col min-h-0"
    >
      <div
        title="Drag to resize"
        onMouseDown={(e) => {
          e.preventDefault();
          dragging.current = true;
          startY.current = e.clientY;
          startH.current = height;
          document.body.style.cursor = "row-resize";
          document.body.style.userSelect = "none";
        }}
        className={cn(
          "absolute -top-1 left-0 right-0 h-2 cursor-row-resize z-10",
          "hover:bg-line-focus/60",
        )}
      />
      <div className="h-7 shrink-0 border-b border-line-0 flex items-center gap-2 px-3 text-[10px] text-fg-2 select-none">
        <span className="font-semibold text-fg-1 uppercase tracking-wider">
          Console
        </span>
        {tab && (
          <span className="font-mono truncate" title={tab.path}>
            {tab.path}
          </span>
        )}
        <button
          type="button"
          title="Close console"
          onClick={() => setConsoleOpen(false)}
          className="ml-auto flex items-center justify-center h-5 w-5 rounded text-fg-2 hover:text-fg-0 hover:bg-bg-3"
        >
          <X size={11} />
        </button>
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto p-2 flex flex-col gap-1">
        {logs.length === 0 && !scriptError && (
          <div className="text-xs text-fg-2">
            No console output — use console.log() in scripts
          </div>
        )}
        {logs.map((line, i) => (
          <div
            key={i}
            className="flex gap-2 text-xs border-b border-line-0/50 pb-1"
          >
            <span className="font-mono text-fg-2 shrink-0 select-none">
              {i + 1}
            </span>
            <span className="font-mono text-fg-0 whitespace-pre-wrap break-all min-w-0">
              {line}
            </span>
          </div>
        ))}
        {scriptError && (
          <div className="text-xs text-danger font-mono break-words">
            {scriptError}
          </div>
        )}
      </div>
    </div>
  );
}

export default ConsolePanel;
