import { useState } from "react";
import { FolderOpen, FolderPlus } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useKeel } from "@/state/store";
import { CreateWorkspaceModal } from "./CreateWorkspaceModal";

export function Welcome() {
  const version = useKeel((s) => s.version);
  const openWorkspace = useKeel((s) => s.openWorkspace);
  const toast = useKeel((s) => s.toast);

  const [createOpen, setCreateOpen] = useState(false);

  const chooseOpen = async () => {
    const picked = await openDialog({ directory: true });
    if (!picked) return;
    try {
      await openWorkspace(picked);
      if (!useKeel.getState().workspace) {
        toast("Could not open folder — use Create workspace to initialize it", "error");
      }
    } catch (err) {
      toast(`${String(err)} — use Create workspace to initialize this folder`, "error");
    }
  };

  return (
    <div className="h-full flex flex-col items-center justify-center gap-8 bg-bg-0">
      <div className="flex flex-col items-center gap-3">
        <div className="h-14 w-14 rounded-xl overflow-hidden border border-line-0">
          <img src="/logo.png" alt="" className="keel-logo keel-logo-on-dark h-full w-full object-cover" />
          <img src="/logo-light.png" alt="" className="keel-logo keel-logo-on-light h-full w-full object-cover" />
        </div>
        <div className="text-2xl font-semibold text-fg-0">Keel</div>
        <div className="text-fg-1 text-sm text-center max-w-md">
          Your API project as plain text files — browse, send, test, and commit. Fully offline.
        </div>
      </div>

      <div className="flex items-stretch gap-4">
        <button
          type="button"
          onClick={chooseOpen}
          className="w-64 text-left border border-line-0 rounded-md p-4 hover:border-accent/50 hover:bg-bg-1 cursor-pointer flex flex-col gap-1.5 focus-visible:ring-1 ring-accent/60 outline-none"
        >
          <FolderOpen size={18} className="text-accent" />
          <span className="text-sm font-medium text-fg-0">Open workspace</span>
          <span className="text-xs text-fg-2">Choose an existing Keel folder</span>
        </button>

        <button
          type="button"
          onClick={() => setCreateOpen(true)}
          className="w-64 text-left border border-line-0 rounded-md p-4 hover:border-accent/50 hover:bg-bg-1 cursor-pointer flex flex-col gap-1.5 focus-visible:ring-1 ring-accent/60 outline-none"
        >
          <FolderPlus size={18} className="text-accent" />
          <span className="text-sm font-medium text-fg-0">Create workspace</span>
          <span className="text-xs text-fg-2">Scaffold a new collection</span>
        </button>
      </div>

      <div className="flex items-center gap-1.5 text-[10px] text-fg-2">
        <span>{version ? `Keel ${version}` : "Keel"}</span>
        <span>·</span>
        <span>MIT License</span>
        <span>·</span>
        <span>Collections are plain YAML files you own</span>
      </div>

      <CreateWorkspaceModal open={createOpen} onClose={() => setCreateOpen(false)} />
    </div>
  );
}

export default Welcome;
