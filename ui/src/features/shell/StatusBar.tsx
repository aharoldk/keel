import { Columns2, GitBranch, Rows2, TerminalSquare } from "lucide-react";
import { cn } from "@/utils";
import { useKeel } from "@/state/store";

export function StatusBar() {
  const workspace = useKeel((s) => s.workspace);
  const git = useKeel((s) => s.git);
  const activePath = useKeel((s) => s.activePath);
  const tabs = useKeel((s) => s.tabs);
  const version = useKeel((s) => s.version);
  const consoleOpen = useKeel((s) => s.consoleOpen);
  const setConsoleOpen = useKeel((s) => s.setConsoleOpen);
  const splitDirection = useKeel((s) => s.splitDirection);
  const setSplitDirection = useKeel((s) => s.setSplitDirection);

  const activeTab = tabs.find((t) => t.path === activePath);
  const dirty = activeTab ? JSON.stringify(activeTab.doc) !== JSON.stringify(activeTab.saved) : false;
  const branch = git?.hasRepo ? (git.branch ?? "detached") : null;
  const logCount =
    (activeTab?.result?.scriptLogs.length ?? 0) +
    (activeTab?.result?.scriptError ? 1 : 0);

  return (
    <div className="h-6 shrink-0 border-t border-line-0 bg-bg-1 flex items-center px-3 gap-3 text-[10px] text-fg-2 select-none">
      <span
        className="truncate max-w-48"
        title={workspace ? workspace.root : undefined}
      >
        {workspace?.name ?? "No workspace"}
      </span>
      {activePath && (
        <span className="flex items-center gap-1.5 truncate min-w-0" title={activePath}>
          <span className="font-mono truncate">{activePath}</span>
          {dirty && <span title="Unsaved changes" className="text-accent leading-none">●</span>}
        </span>
      )}

      <span className="flex-1" />

      {branch && (
        <span className="flex items-center gap-1" title="Git branch">
          <GitBranch size={10} />
          <span className="font-mono">{branch}</span>
        </span>
      )}
      <span className={cn("font-mono", !version && "opacity-50")}>
        {version ? `v${version}` : "—"}
      </span>
      <button
        type="button"
        title={
          splitDirection === "vertical"
            ? "Switch to side-by-side view"
            : "Switch to stacked view"
        }
        onClick={() =>
          setSplitDirection(splitDirection === "vertical" ? "horizontal" : "vertical")
        }
        className="flex items-center gap-1 rounded px-1 h-4 hover:text-fg-0"
      >
        {splitDirection === "vertical" ? <Columns2 size={10} /> : <Rows2 size={10} />}
        <span>{splitDirection === "vertical" ? "Side by side" : "Stacked"}</span>
      </button>
      <button
        type="button"
        title="Toggle console"
        onClick={() => setConsoleOpen(!consoleOpen)}
        className={cn(
          "flex items-center gap-1 rounded px-1 h-4",
          consoleOpen ? "bg-bg-3 text-fg-0" : "hover:text-fg-0",
        )}
      >
        <TerminalSquare size={10} />
        <span>Console</span>
        {logCount > 0 && (
          <span
            className={cn(
              activeTab?.result?.scriptError ? "text-danger" : "text-fg-2",
            )}
          >
            {logCount}
          </span>
        )}
      </button>
    </div>
  );
}

export default StatusBar;
