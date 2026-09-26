import { useEffect, useRef, useState } from "react";
import { ChevronDown, Globe, LayoutDashboard, Search, Settings, Sparkles } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { comboFor, formatCombo } from "@/shortcuts";
import { cn } from "@/utils";
import { useKeel } from "@/state/store";
import { WorkspaceMenu } from "@/features/workspace/WorkspaceMenu";

export function Toolbar() {
  const workspace = useKeel((s) => s.workspace);
  const envs = useKeel((s) => s.envs);
  const activeEnv = useKeel((s) => s.activeEnv);
  const selectEnv = useKeel((s) => s.selectEnv);
  const setPaletteOpen = useKeel((s) => s.setPaletteOpen);
  const setSettingsOpen = useKeel((s) => s.setSettingsOpen);
  const aiOpen = useKeel((s) => s.aiOpen);
  const setAiOpen = useKeel((s) => s.setAiOpen);
  const setSidebarPanel = useKeel((s) => s.setSidebarPanel);
  const contentPanel = useKeel((s) => s.contentPanel);
  const setContentPanel = useKeel((s) => s.setContentPanel);
  const dashboardOpen = contentPanel?.kind === "dashboard";
  const shortcuts = useKeel((s) => s.settings.shortcuts);

  const [envOpen, setEnvOpen] = useState(false);
  const envRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!envOpen) return;
    const onDown = (e: MouseEvent) => {
      if (!envRef.current?.contains(e.target as Node)) setEnvOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setEnvOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [envOpen]);

  const active = envs.find((e) => e.fileName === activeEnv) ?? null;
  const itemCls =
    "w-full h-8 px-2.5 flex items-center gap-2 text-left text-xs hover:bg-bg-hover focus-visible:ring-1 ring-accent/60 outline-none";

  const pickEnv = (fileName: string | null) => {
    selectEnv(fileName);
    setEnvOpen(false);
  };

  return (
    <div
      data-tauri-drag-region
      onMouseDown={(e) => {
        if (e.buttons !== 1 || (e.target as HTMLElement).closest("button")) return;
        if (e.detail === 2) void getCurrentWindow().toggleMaximize();
        else void getCurrentWindow().startDragging();
      }}
      className="h-9 shrink-0 border-b border-line-0 bg-bg-1 grid grid-cols-[1fr_auto_1fr] items-center px-2.5 gap-2"
    >
      <div className="flex items-center gap-3 min-w-0">
        <div className="flex items-center gap-2 shrink-0">
          <img src="/logo.png" alt="" className="keel-logo keel-logo-on-dark h-5 w-5 rounded object-contain" />
          <img src="/logo-light.png" alt="" className="keel-logo keel-logo-on-light h-5 w-5 rounded object-contain" />
          <span className="font-semibold text-sm text-fg-0">Keel</span>
        </div>
        {workspace && (
          <>
            {!dashboardOpen && (
              <button
                type="button"
                title="Dashboard"
                aria-label="Dashboard"
                onClick={() => setContentPanel({ kind: "dashboard" })}
                className="h-7 w-7 shrink-0 rounded flex items-center justify-center text-fg-1 hover:text-fg-0 hover:bg-bg-hover focus-visible:ring-1 ring-accent/60 outline-none"
              >
                <LayoutDashboard size={14} />
              </button>
            )}
            {!dashboardOpen && <WorkspaceMenu />}
          </>
        )}
      </div>

      {!dashboardOpen && (
        <button
          type="button"
          onClick={() => setPaletteOpen(true)}
          className="w-72 h-7 rounded bg-bg-2 border border-line-0 text-fg-2 flex items-center gap-2 px-2 text-xs hover:border-line-1 hover:text-fg-1 focus-visible:ring-1 ring-accent/60 outline-none"
        >
          <Search size={12} />
          <span className="flex-1 text-left">Search</span>
          <kbd className="font-mono text-[10px] border border-line-1 rounded px-1">
            {formatCombo(comboFor("palette", shortcuts))}
          </kbd>
        </button>
      )}

      <div className={cn("flex items-center justify-end gap-1 min-w-0", dashboardOpen && "col-span-2")}>
      {!dashboardOpen && <div ref={envRef} className="relative">
        <button
          type="button"
          title="Environment"
          aria-haspopup="menu"
          aria-expanded={envOpen}
          onClick={() => setEnvOpen((o) => !o)}
          className={cn(
            "flex items-center gap-1.5 h-7 px-2 rounded bg-bg-2 border text-xs max-w-56",
            "hover:border-line-1 focus-visible:ring-1 ring-accent/60 outline-none",
            active
              ? "border-accent/50 text-fg-0"
              : "border-line-0 text-fg-1 hover:text-fg-0",
          )}
        >
          <span
            className={cn("h-1.5 w-1.5 shrink-0 rounded-full", active ? "bg-accent" : "bg-fg-2/40")}
            title={active ? "Environment active" : "No environment"}
          />
          <Globe size={12} className={cn("shrink-0", active ? "text-accent" : "text-fg-2")} />
          <span className="truncate">{active?.name ?? "No environment"}</span>
          <ChevronDown size={12} className="shrink-0 text-fg-2" />
        </button>

        {envOpen && (
          <div className="absolute right-0 top-full mt-1 z-40 w-72 rounded-md border border-line-0 bg-bg-1 shadow-xl py-1">
            <button
              type="button"
              className={cn(itemCls, "text-fg-1")}
              onClick={() => pickEnv(null)}
            >
              <Globe size={13} className="shrink-0 text-fg-2" />
              <span className="flex-1 truncate">No environment</span>
            </button>
            {envs.length > 0 && <div className="mx-2.5 my-1 border-t border-line-0" />}
            {envs.map((env) => (
              <button
                key={env.fileName}
                type="button"
                title={env.fileName}
                className={cn(itemCls, "text-fg-1")}
                onClick={() => pickEnv(env.fileName)}
              >
                <Globe size={13} className="shrink-0 text-fg-2" />
                <span className="truncate">{env.name}</span>
                <span className="ml-auto shrink-0 font-mono text-[10px] text-fg-2">
                  {env.variableCount} vars · {env.secretCount} secrets
                </span>
              </button>
            ))}
            <div className="mx-2.5 my-1 border-t border-line-0" />
            <button
              type="button"
              className={cn(itemCls, "text-fg-1")}
              onClick={() => {
                setEnvOpen(false);
                setSidebarPanel("environments");
              }}
            >
              <Settings size={13} className="shrink-0 text-fg-2" />
              Manage environments
            </button>
          </div>
        )}
      </div>}

      {!dashboardOpen && (
        <button
          type="button"
          onClick={() => setAiOpen(!aiOpen)}
          title="AI"
          aria-pressed={aiOpen}
          className={cn(
            "h-7 w-7 shrink-0 rounded flex items-center justify-center hover:bg-bg-hover focus-visible:ring-1 ring-accent/60 outline-none",
            aiOpen ? "text-accent bg-accent-soft" : "text-fg-1 hover:text-fg-0",
          )}
        >
          <Sparkles size={14} />
        </button>
      )}
      <button
        type="button"
        onClick={() => setSettingsOpen(true)}
        title="Settings"
        className="h-7 w-7 shrink-0 rounded flex items-center justify-center text-fg-1 hover:text-fg-0 hover:bg-bg-hover focus-visible:ring-1 ring-accent/60 outline-none"
      >
        <Settings size={14} />
      </button>
      <WindowControls />
      </div>
    </div>
  );
}

export function WindowControls() {
  const win = getCurrentWindow();
  const btn =
    "h-7 w-7 shrink-0 rounded flex items-center justify-center text-fg-2 hover:bg-bg-hover hover:text-fg-0 focus-visible:ring-1 ring-accent/60 outline-none";
  return (
    <div className="flex items-center ml-1">
      <button type="button" title="Minimize" className={btn} onClick={() => void win.minimize()}>
        <span className="block h-px w-2.5 bg-current" />
      </button>
      <button
        type="button"
        title="Maximize"
        className={btn}
        onClick={() => void win.toggleMaximize()}
      >
        <span className="block h-2.5 w-2.5 border border-current" />
      </button>
      <button
        type="button"
        title="Close"
        className={cn(btn, "hover:bg-danger hover:text-white")}
        onClick={() => void win.close()}
      >
        <span className="relative block h-2.5 w-2.5">
          <span className="absolute left-1/2 top-1/2 h-px w-2.5 -translate-x-1/2 -translate-y-1/2 rotate-45 bg-current" />
          <span className="absolute left-1/2 top-1/2 h-px w-2.5 -translate-x-1/2 -translate-y-1/2 -rotate-45 bg-current" />
        </span>
      </button>
    </div>
  );
}

export default Toolbar;
