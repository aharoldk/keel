import { useEffect, useState } from "react";
import { Globe2, Plus, Workflow, X } from "lucide-react";
import { tabIsDirty, tabKey, useKeel, type EditorTab, type Tab } from "@/state/store";
import { Button, IconButton, Modal, Spinner, TextInput } from "@/components/ui";
import { cn, methodVar } from "@/utils";

function TabLabel({ editor, request }: { editor: EditorTab; request?: Tab }) {
  const envs = useKeel((s) => s.envs);
  const flowName = useKeel((s) => s.flowRun?.name);
  if (editor.kind === "flow") {
    return (
      <>
        <Workflow size={12} className="shrink-0 text-fg-2" />
        <span className="truncate max-w-40">{flowName || "Flow"}</span>
      </>
    );
  }
  if (editor.kind === "environment") {
    const name = envs.find((e) => e.fileName === editor.fileName)?.name;
    return (
      <>
        <Globe2 size={12} className="shrink-0 text-fg-2" />
        <span className="truncate max-w-40">{name || editor.fileName.replace(/\.ya?ml$/, "")}</span>
      </>
    );
  }
  if (!request) return <span className="truncate max-w-40">Untitled</span>;
  return (
    <>
      <span
        className="font-mono text-[9px] font-bold"
        style={{ color: methodVar(request.doc.request.method) }}
      >
        {request.doc.request.method}
      </span>
      <span className="truncate max-w-40">{request.doc.name || "Untitled"}</span>
      {request.loading ? (
        <Spinner size={10} />
      ) : (
        tabIsDirty(request) && <span className="h-1.5 w-1.5 rounded-full bg-warn shrink-0" />
      )}
    </>
  );
}

export default function RequestTabsBar() {
  const tabs = useKeel((s) => s.tabs);
  const storedEditorTabs = useKeel((s) => s.editorTabs);
  const storedActiveEditor = useKeel((s) => s.activeEditor);
  const activePath = useKeel((s) => s.activePath);
  const editorTabs =
    storedEditorTabs.length > 0
      ? storedEditorTabs
      : tabs.map((t) => ({ kind: "request" as const, path: t.path }));
  const activeEditor = storedActiveEditor ?? activePath;
  const setActiveEditor = useKeel((s) => s.setActiveEditor);
  const closeEditor = useKeel((s) => s.closeEditor);
  const saveTab = useKeel((s) => s.saveTab);
  const moveTab = useKeel((s) => s.moveTab);
  const createRequest = useKeel((s) => s.createRequest);

  const [newOpen, setNewOpen] = useState(false);
  const [newName, setNewName] = useState("");
  const [dragIdx, setDragIdx] = useState<number | null>(null);
  const [overIdx, setOverIdx] = useState<number | null>(null);
  const [pendingClose, setPendingClose] = useState<string | null>(null);
  const [closeQueue, setCloseQueue] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);

  const handleCreate = async () => {
    const name = newName.trim();
    if (!name) return;
    setNewOpen(false);
    setNewName("");
    await createRequest("", name);
  };

  useEffect(() => {
    const onClose = (e: Event) => {
      const path = (e as CustomEvent<string>).detail;
      if (!path) return;
      const { tabs: open, closeEditor: close } = useKeel.getState();
      const tab = open.find((t) => t.path === path);
      if (tab && tabIsDirty(tab)) setPendingClose(path);
      else close(path);
    };
    const onCloseAll = () => {
      const { editorTabs: open, tabs: docs, closeEditor: close } = useKeel.getState();
      const dirty = docs.filter(tabIsDirty).map((t) => t.path);
      for (const tab of open) {
        const key = tabKey(tab);
        if (!dirty.includes(key)) close(key);
      }
      if (dirty.length > 0) {
        setPendingClose(dirty[0]);
        setCloseQueue(dirty.slice(1));
      }
    };
    window.addEventListener("keel:close-tab", onClose);
    window.addEventListener("keel:close-all-tabs", onCloseAll);
    return () => {
      window.removeEventListener("keel:close-tab", onClose);
      window.removeEventListener("keel:close-all-tabs", onCloseAll);
    };
  }, []);

  const requestClose = (key: string) => {
    const tab = useKeel.getState().tabs.find((t) => t.path === key);
    if (tab && tabIsDirty(tab)) setPendingClose(key);
    else closeEditor(key);
  };

  const pendingTab = pendingClose
    ? tabs.find((t) => t.path === pendingClose)
    : undefined;

  const advanceQueue = () => {
    const [next, ...rest] = closeQueue;
    setPendingClose(next ?? null);
    setCloseQueue(rest);
  };

  const confirmDiscard = () => {
    const path = pendingClose;
    if (path) closeEditor(path);
    advanceQueue();
  };

  const confirmSave = async () => {
    const path = pendingClose;
    if (!path) return;
    setSaving(true);
    const ok = await saveTab(path);
    setSaving(false);
    if (!ok) return;
    closeEditor(path);
    advanceQueue();
  };

  return (
    <div className="h-9 border-b border-line-0 bg-bg-1 flex items-stretch overflow-x-auto">
      {editorTabs.map((editor, i) => {
        const key = tabKey(editor);
        const active = key === activeEditor;
        const request = editor.kind === "request" ? tabs.find((t) => t.path === editor.path) : undefined;
        return (
          <div
            key={key}
            onClick={() => setActiveEditor(key)}
            onAuxClick={(e) => {
              if (e.button === 1) {
                e.preventDefault();
                requestClose(key);
              }
            }}
            onMouseDown={(e) => {
              if (e.button === 1) e.preventDefault();
            }}
            draggable
            onDragStart={(e) => {
              setDragIdx(i);
              e.dataTransfer.effectAllowed = "move";
              e.dataTransfer.setData("text/plain", key);
            }}
            onDragOver={(e) => {
              if (dragIdx == null) return;
              e.preventDefault();
              e.dataTransfer.dropEffect = "move";
              if (overIdx !== i) setOverIdx(i);
            }}
            onDragLeave={() => {
              if (overIdx === i) setOverIdx(null);
            }}
            onDrop={(e) => {
              e.preventDefault();
              if (dragIdx != null && dragIdx !== i) moveTab(dragIdx, i);
              setDragIdx(null);
              setOverIdx(null);
            }}
            onDragEnd={() => {
              setDragIdx(null);
              setOverIdx(null);
            }}
            className={cn(
              "group flex items-center gap-2 px-3 border-r border-line-0 text-xs shrink-0 cursor-pointer select-none",
              active
                ? "bg-bg-2 text-fg-0 border-t-2 border-t-accent"
                : "text-fg-1 hover:text-fg-0",
              dragIdx != null &&
                overIdx === i &&
                dragIdx !== i &&
                "border-l-2 border-l-accent",
              dragIdx === i && "opacity-60",
            )}
            title={key}
          >
            <TabLabel editor={editor} request={request} />
            <button
              type="button"
              title="Close tab"
              onClick={(e) => {
                e.stopPropagation();
                requestClose(key);
              }}
              className="h-4 w-4 inline-flex items-center justify-center rounded-sm text-fg-2 hover:text-danger transition-colors shrink-0"
            >
              <X size={10} />
            </button>
          </div>
        );
      })}
      <div className="ml-auto sticky right-0 bg-bg-1 flex items-center gap-1 px-2 border-l border-line-0 shrink-0">
        <IconButton title="New request" onClick={() => setNewOpen(true)}>
          <Plus size={14} />
        </IconButton>
      </div>

      <Modal
        open={newOpen}
        onClose={() => setNewOpen(false)}
        title="New request"
        width="max-w-sm"
      >
        <div className="flex flex-col gap-3">
          <TextInput
            autoFocus
            placeholder="Request name"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void handleCreate();
            }}
          />
          <div className="flex justify-end">
            <button
              type="button"
              disabled={!newName.trim()}
              onClick={() => void handleCreate()}
              className="inline-flex items-center rounded h-7 px-3 text-xs font-semibold bg-accent text-accent-fg hover:bg-accent-strong transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              Create
            </button>
          </div>
        </div>
      </Modal>

      <Modal
        open={pendingClose !== null}
        onClose={() => {
          if (saving) return;
          setPendingClose(null);
          setCloseQueue([]);
        }}
        title="Unsaved changes"
        width="max-w-md"
      >
        <div className="flex flex-col gap-3">
          <p className="text-xs text-fg-1">
            <span className="font-semibold text-fg-0">
              {pendingTab?.doc.name || "Untitled"}
            </span>{" "}
            has unsaved changes. Closing it will discard them.
          </p>
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="ghost"
              disabled={saving}
              onClick={() => {
                setPendingClose(null);
                setCloseQueue([]);
              }}
            >
              Cancel
            </Button>
            <Button type="button" variant="danger" disabled={saving} onClick={confirmDiscard}>
              Don't Save
            </Button>
            <Button type="button" variant="primary" disabled={saving} onClick={() => void confirmSave()}>
              {saving ? "Saving…" : "Save & Close"}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}

export { RequestTabsBar };
