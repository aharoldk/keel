import { useEffect, useRef, useState } from "react";
import { ChevronDown, FolderOpen, FolderPlus } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button, Modal } from "@/components/ui";
import { cn } from "@/utils";
import { tabIsDirty, useKeel } from "@/state/store";
import { CreateWorkspaceModal } from "@/features/onboarding/CreateWorkspaceModal";

function folderName(path: string): string {
  return path.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? path;
}

export function WorkspaceMenu() {
  const workspace = useKeel((s) => s.workspace);
  const recents = useKeel((s) => s.settings.recentWorkspaces);
  const dirtyCount = useKeel((s) => s.tabs.filter(tabIsDirty).length);
  const openWorkspace = useKeel((s) => s.openWorkspace);
  const saveAllTabs = useKeel((s) => s.saveAllTabs);
  const toast = useKeel((s) => s.toast);

  const [menuOpen, setMenuOpen] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [pending, setPending] = useState<(() => void) | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onOpen = () => setMenuOpen(true);
    window.addEventListener("keel:open-menu", onOpen);
    return () => window.removeEventListener("keel:open-menu", onOpen);
  }, []);

  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setMenuOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenuOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [menuOpen]);

  if (!workspace) return null;

  const openPath = async (path: string) => {
    if (path === workspace.root) return;
    try {
      await openWorkspace(path);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  // Switching projects drops all open tabs, so dirty tabs need a decision first
  const guard = (action: () => void) => {
    setMenuOpen(false);
    if (useKeel.getState().tabs.some(tabIsDirty)) setPending(() => action);
    else action();
  };

  const chooseOpen = () =>
    guard(async () => {
      const picked = await openDialog({ directory: true });
      if (picked) await openPath(picked);
    });

  const confirmSave = async () => {
    const action = pending;
    const ok = await saveAllTabs();
    if (!ok) return; // failed save already toasted — keep the modal open
    setPending(null);
    action?.();
  };

  const confirmDiscard = () => {
    const action = pending;
    setPending(null);
    action?.();
  };

  const recentList = (recents ?? []).filter((p) => p !== workspace.root).slice(0, 8);

  const itemCls =
    "w-full h-8 px-3 flex items-center gap-2 text-left text-xs text-fg-0 hover:bg-bg-hover focus-visible:ring-1 ring-accent/60 outline-none";

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        title={workspace.root}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onClick={() => setMenuOpen(!menuOpen)}
        className={cn(
          "flex items-center gap-1.5 h-7 px-2 rounded bg-bg-2 border border-line-0 text-xs text-fg-1 max-w-56",
          "hover:border-line-1 hover:text-fg-0 focus-visible:ring-1 ring-accent/60 outline-none",
        )}
      >
        <FolderOpen size={12} className="shrink-0 text-fg-2" />
        <span className="truncate">{workspace.name}</span>
        <ChevronDown size={12} className="shrink-0 text-fg-2" />
      </button>

      {menuOpen && (
        <div className="absolute left-0 top-full mt-1 z-40 w-64 rounded-md border border-line-0 bg-bg-1 shadow-xl py-1">
          <button type="button" className={itemCls} onClick={() => guard(() => setCreateOpen(true))}>
            <FolderPlus size={13} className="shrink-0 text-fg-2" />
            New Project…
          </button>
          <button type="button" className={itemCls} onClick={chooseOpen}>
            <FolderOpen size={13} className="shrink-0 text-fg-2" />
            Open Project…
          </button>
          {recentList.length > 0 && (
            <>
              <div className="mx-3 my-1 border-t border-line-0" />
              <div className="px-3 pt-1 pb-0.5 text-[10px] font-semibold uppercase tracking-wider text-fg-2">
                Recent projects
              </div>
              {recentList.map((p) => (
                <button
                  key={p}
                  type="button"
                  title={p}
                  className={itemCls}
                  onClick={() => guard(() => void openPath(p))}
                >
                  <FolderOpen size={13} className="shrink-0 text-fg-2" />
                  <span className="truncate">{folderName(p)}</span>
                  <span className="ml-auto shrink-0 max-w-[55%] truncate font-mono text-[10px] text-fg-2">
                    {p}
                  </span>
                </button>
              ))}
            </>
          )}
        </div>
      )}

      <CreateWorkspaceModal open={createOpen} onClose={() => setCreateOpen(false)} />

      <Modal
        open={pending !== null}
        onClose={() => setPending(null)}
        title="Unsaved changes"
        width="max-w-md"
      >
        <div className="flex flex-col gap-3">
          <p className="text-xs text-fg-1">
            You have {dirtyCount} unsaved {dirtyCount === 1 ? "request" : "requests"} in{" "}
            <span className="font-semibold text-fg-0">{workspace.name}</span>. Switching
            projects will discard any unsaved changes.
          </p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => setPending(null)}>
              Cancel
            </Button>
            <Button type="button" variant="danger" onClick={confirmDiscard}>
              Don't Save
            </Button>
            <Button type="button" variant="primary" onClick={confirmSave}>
              Save All &amp; Continue
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}

export default WorkspaceMenu;
