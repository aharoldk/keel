import { useEffect, useRef, useState } from "react";
import { FolderTree, GitBranch, Globe2, History, PanelLeftClose, PanelLeftOpen, Workflow } from "lucide-react";
import { comboFor, formatCombo } from "@/shortcuts";
import { cn } from "@/utils";
import { useKeel, type SidebarPanel } from "@/state/store";
import { CollectionTree } from "@/features/collections/CollectionTree";
import { EnvPanel } from "@/features/environments/EnvPanel";
import { GitPanel } from "@/features/git/GitPanel";
import { HistoryPanel } from "@/features/history/HistoryPanel";
import { FlowPanel } from "@/features/flow/FlowPanel";
const PANELS: { id: SidebarPanel; label: string; icon: typeof FolderTree }[] = [
  { id: "collections", label: "Collections", icon: FolderTree },
  { id: "environments", label: "Environments", icon: Globe2 },
  { id: "flow", label: "Flow", icon: Workflow },
  { id: "history", label: "History", icon: History },
  { id: "git", label: "Git", icon: GitBranch },
];

const MIN_WIDTH = 160;
const MAX_WIDTH = 560;
const DEFAULT_WIDTH = 240;

function readWidth() {
  const n = Number(localStorage.getItem("keel.sidebarWidth"));
  return Number.isFinite(n) && n >= MIN_WIDTH && n <= MAX_WIDTH ? n : DEFAULT_WIDTH;
}

export function Sidebar() {
  const sidebarOpen = useKeel((s) => s.sidebarOpen);
  const sidebarPanel = useKeel((s) => s.sidebarPanel);
  const setSidebarPanel = useKeel((s) => s.setSidebarPanel);
  const setSidebarOpen = useKeel((s) => s.setSidebarOpen);
  const shortcuts = useKeel((s) => s.settings.shortcuts);
  const sidebarHint = formatCombo(comboFor("sidebar", shortcuts));
  const [width, setWidth] = useState(readWidth);
  const dragging = useRef(false);
  const startX = useRef(0);
  const startW = useRef(0);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      const next = Math.max(
        MIN_WIDTH,
        Math.min(MAX_WIDTH, startW.current + (e.clientX - startX.current)),
      );
      setWidth(next);
    };
    const onUp = () => {
      if (!dragging.current) return;
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      setWidth((w) => {
        localStorage.setItem("keel.sidebarWidth", String(Math.round(w)));
        return w;
      });
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  return (
    <div className="flex min-h-0">
      <div className="w-10 shrink-0 bg-bg-1 border-r border-line-0 flex flex-col items-center py-2 gap-1">
        {PANELS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            title={label}
            aria-label={label}
            onClick={() => setSidebarPanel(id)}
            className={cn(
              "h-8 w-8 rounded flex items-center justify-center transition-colors focus-visible:ring-1 ring-accent/60 outline-none",
              sidebarPanel === id
                ? "bg-accent-soft text-accent"
                : "text-fg-2 hover:text-fg-0 hover:bg-bg-hover",
            )}
          >
            <Icon size={15} />
          </button>
        ))}
        <div className="flex-1" />
        <button
          type="button"
          title={sidebarOpen ? `Minimize sidebar (${sidebarHint})` : `Expand sidebar (${sidebarHint})`}
          aria-label={sidebarOpen ? "Minimize sidebar" : "Expand sidebar"}
          onClick={() => setSidebarOpen(!sidebarOpen)}
          className="h-8 w-8 rounded flex items-center justify-center transition-colors focus-visible:ring-1 ring-accent/60 outline-none text-fg-2 hover:text-fg-0 hover:bg-bg-hover"
        >
          {sidebarOpen ? <PanelLeftClose size={15} /> : <PanelLeftOpen size={15} />}
        </button>
      </div>
      {sidebarOpen && (
        <div
          style={{ width }}
          className="relative shrink-0 border-r border-line-0 bg-bg-1 flex flex-col min-h-0"
        >
          {sidebarPanel === "collections" && <CollectionTree />}
          {sidebarPanel === "environments" && <EnvPanel />}
          {sidebarPanel === "flow" && <FlowPanel />}
          {sidebarPanel === "history" && <HistoryPanel />}
          {sidebarPanel === "git" && <GitPanel />}
          <div
            data-testid="sidebar-drag-handle"
            title="Drag to resize"
            onMouseDown={(e) => {
              e.preventDefault();
              dragging.current = true;
              startX.current = e.clientX;
              startW.current = width;
              document.body.style.cursor = "col-resize";
              document.body.style.userSelect = "none";
            }}
            className="absolute top-0 -right-1 h-full w-2 cursor-col-resize z-10 hover:bg-line-focus/60"
          />
        </div>
      )}
    </div>
  );
}

export default Sidebar;
