import { useEffect, useMemo, useState } from "react";
import { FolderOpen, Pencil, Sparkles, Trash2 } from "lucide-react";
import { api } from "@/api/client";
import type { WorkspaceInfo } from "@/api/types";
import { Button, IconButton, Modal, TextInput } from "@/components/ui";
import { tabIsDirty, useKeel } from "@/state/store";
import { cn } from "@/utils";

export function Dashboard() {
  const workspace = useKeel((s) => s.workspace);
  const recents = useKeel((s) => s.settings.recentWorkspaces);
  const openWorkspace = useKeel((s) => s.openWorkspace);
  const removeRecent = useKeel((s) => s.removeRecent);
  const saveAllTabs = useKeel((s) => s.saveAllTabs);
  const setContentPanel = useKeel((s) => s.setContentPanel);
  const toast = useKeel((s) => s.toast);

  const [screen, setScreen] = useState<"projects" | "ai">("projects");
  const [query, setQuery] = useState("");
  const [projects, setProjects] = useState<WorkspaceInfo[]>([]);
  const [editing, setEditing] = useState<WorkspaceInfo | null>(null);
  const [editName, setEditName] = useState("");
  const [saving, setSaving] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<WorkspaceInfo | null>(null);
  const [pendingOpen, setPendingOpen] = useState<string | null>(null);

  const paths = useMemo(() => {
    const list = recents ?? [];
    if (workspace && !list.includes(workspace.root)) return [workspace.root, ...list];
    return list;
  }, [recents, workspace]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const loaded = (
        await Promise.all(
          paths.map(async (path) => {
            if (workspace?.root === path) return workspace;
            return api.workspacePeek(path);
          }),
        )
      ).filter((p): p is WorkspaceInfo => p !== null);
      if (!cancelled) setProjects(loaded);
    })();
    return () => {
      cancelled = true;
    };
  }, [paths, workspace]);

  const visible = projects.filter((p) => {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return p.name.toLowerCase().includes(q) || p.root.toLowerCase().includes(q);
  });

  const openProject = async (path: string) => {
    if (path === workspace?.root) {
      setContentPanel(null);
      return;
    }
    if (useKeel.getState().tabs.some(tabIsDirty)) {
      setPendingOpen(path);
      return;
    }
    try {
      await openWorkspace(path);
      setContentPanel(null);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const confirmOpen = async (save: boolean) => {
    const path = pendingOpen;
    if (!path) return;
    if (save) {
      const ok = await saveAllTabs();
      if (!ok) return;
    }
    setPendingOpen(null);
    try {
      await openWorkspace(path);
      setContentPanel(null);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const startEdit = (project: WorkspaceInfo) => {
    setEditing(project);
    setEditName(project.name);
  };

  const saveEdit = async () => {
    if (!editing) return;
    const next = editName.trim();
    if (!next || next === editing.name) {
      setEditing(null);
      return;
    }
    setSaving(true);
    const current = useKeel.getState().workspace?.root;
    try {
      if (current !== editing.root) await api.workspaceOpen(editing.root);
      const doc = await api.collectionRead();
      await api.collectionSave({ ...doc, name: next });
      if (current && current !== editing.root) await api.workspaceOpen(current);
      else {
        const info = await api.workspaceInfo();
        if (info) useKeel.setState({ workspace: info });
      }
      setProjects((list) =>
        list.map((p) => (p.root === editing.root ? { ...p, name: next } : p)),
      );
      toast("Project renamed", "success");
      setEditing(null);
    } catch (e) {
      if (current && current !== editing.root) {
        try {
          await api.workspaceOpen(current);
        } catch {
          // leave the restored workspace as-is; the error toast covers the failure
        }
      }
      toast(String(e), "error");
    } finally {
      setSaving(false);
    }
  };

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    const path = pendingDelete.root;
    setPendingDelete(null);
    try {
      await removeRecent(path);
      setProjects((list) => list.filter((p) => p.root !== path));
      toast("Project removed", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  return (
    <div className="flex-1 min-h-0 overflow-y-auto bg-bg-0">
      <div className="max-w-xl mx-auto w-full flex flex-col gap-4 px-6 py-8">
        <div className="flex items-center gap-1">
          <button
            type="button"
            aria-pressed={screen === "projects"}
            onClick={() => setScreen("projects")}
            className={cn(
              "h-7 px-2.5 rounded text-xs outline-none focus-visible:ring-1 ring-accent/60",
              screen === "projects" ? "bg-bg-2 text-fg-0" : "text-fg-2 hover:text-fg-0",
            )}
          >
            Projects
          </button>
          <button
            type="button"
            aria-pressed={screen === "ai"}
            onClick={() => setScreen("ai")}
            className={cn(
              "h-7 px-2.5 rounded text-xs inline-flex items-center gap-1.5 outline-none focus-visible:ring-1 ring-accent/60",
              screen === "ai" ? "bg-bg-2 text-fg-0" : "text-fg-2 hover:text-fg-0",
            )}
          >
            <Sparkles size={12} />
            AI
          </button>
        </div>
        {screen === "ai" ? (
          <div className="flex-1 flex items-center justify-center py-16 text-center">
            <p className="text-xs text-fg-2">AI is coming in the next release.</p>
          </div>
        ) : (
        <>
        <TextInput
          value={query}
          placeholder="Search projects"
          aria-label="Search projects"
          onChange={(e) => setQuery(e.target.value)}
        />
        {visible.length === 0 ? (
          <div className="text-xs text-fg-2">No projects match.</div>
        ) : (
          <ul className="flex flex-col border border-line-0 rounded-md overflow-hidden">
            {visible.map((project) => {
              const current = project.root === workspace?.root;
              return (
                <li
                  key={project.root}
                  className={cn(
                    "flex items-center gap-2 px-3 h-12 border-b border-line-0 last:border-b-0",
                    current ? "bg-accent-soft" : "bg-bg-1",
                  )}
                >
                  <button
                    type="button"
                    title={project.root}
                    onClick={() => void openProject(project.root)}
                    className="flex-1 min-w-0 flex items-center gap-2 text-left outline-none focus-visible:ring-1 ring-accent/60 rounded"
                  >
                    <FolderOpen size={14} className="shrink-0 text-fg-2" />
                    <span className="min-w-0">
                      <span className="block truncate text-xs text-fg-0">{project.name}</span>
                      <span className="block truncate font-mono text-[10px] text-fg-2">
                        {project.root}
                      </span>
                    </span>
                  </button>
                  <IconButton title="Rename project" onClick={() => startEdit(project)}>
                    <Pencil size={13} />
                  </IconButton>
                  <IconButton title="Delete project" onClick={() => setPendingDelete(project)}>
                    <Trash2 size={13} />
                  </IconButton>
                </li>
              );
            })}
          </ul>
        )}
        </>
        )}
      </div>

      <Modal open={editing !== null} onClose={() => setEditing(null)} title="Rename project" width="max-w-md">
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            void saveEdit();
          }}
        >
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-fg-2">Name</span>
            <TextInput
              value={editName}
              autoFocus
              onChange={(e) => setEditName(e.target.value)}
            />
          </label>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => setEditing(null)}>
              Cancel
            </Button>
            <Button
              type="submit"
              variant="primary"
              disabled={saving || !editName.trim() || editName.trim() === editing?.name}
            >
              Rename
            </Button>
          </div>
        </form>
      </Modal>

      <Modal
        open={pendingDelete !== null}
        onClose={() => setPendingDelete(null)}
        title="Delete project"
        width="max-w-md"
      >
        <div className="flex flex-col gap-3">
          <p className="text-xs text-fg-1">
            Remove <span className="font-semibold text-fg-0">{pendingDelete?.name}</span> from
            your projects? The files on disk are kept.
          </p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => setPendingDelete(null)}>
              Cancel
            </Button>
            <Button type="button" variant="danger" onClick={() => void confirmDelete()}>
              Delete
            </Button>
          </div>
        </div>
      </Modal>

      <Modal
        open={pendingOpen !== null}
        onClose={() => setPendingOpen(null)}
        title="Unsaved changes"
        width="max-w-md"
      >
        <div className="flex flex-col gap-3">
          <p className="text-xs text-fg-1">
            Switching projects will discard any unsaved changes.
          </p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => setPendingOpen(null)}>
              Cancel
            </Button>
            <Button type="button" variant="danger" onClick={() => void confirmOpen(false)}>
              Don't Save
            </Button>
            <Button type="button" variant="primary" onClick={() => void confirmOpen(true)}>
              Save All &amp; Continue
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}

export default Dashboard;
