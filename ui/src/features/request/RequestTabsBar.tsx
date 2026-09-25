import { useEffect, useState } from "react";
import { Plus, X } from "lucide-react";
import { tabIsDirty, useKeel } from "@/state/store";
import { Button, IconButton, Modal, Spinner, TextInput } from "@/components/ui";
import { cn, methodVar } from "@/utils";

export default function RequestTabsBar() {
  const tabs = useKeel((s) => s.tabs);
  const activePath = useKeel((s) => s.activePath);
  const setActiveTab = useKeel((s) => s.setActiveTab);
  const closeTab = useKeel((s) => s.closeTab);
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
      const { tabs: open, closeTab: close } = useKeel.getState();
      const tab = open.find((t) => t.path === path);
      if (tab && tabIsDirty(tab)) setPendingClose(path);
      else close(path);
    };
    const onCloseAll = () => {
      const { tabs: open, closeTab: close } = useKeel.getState();
      const dirty = open.filter(tabIsDirty).map((t) => t.path);
      for (const tab of open) {
        if (!tabIsDirty(tab)) close(tab.path);
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

  const requestClose = (path: string) => {
    const tab = useKeel.getState().tabs.find((t) => t.path === path);
    if (tab && tabIsDirty(tab)) setPendingClose(path);
    else closeTab(path);
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
    if (path) closeTab(path);
    advanceQueue();
  };

  const confirmSave = async () => {
    const path = pendingClose;
    if (!path) return;
    setSaving(true);
    const ok = await saveTab(path);
    setSaving(false);
    if (!ok) return;
    closeTab(path);
    advanceQueue();
  };

  return (
    <div className="h-9 border-b border-line-0 bg-bg-1 flex items-stretch overflow-x-auto">
      {tabs.map((t, i) => {
        const active = t.path === activePath;
        return (
          <div
            key={t.path}
            onClick={() => setActiveTab(t.path)}
            onAuxClick={(e) => {
              if (e.button === 1) {
                e.preventDefault();
                requestClose(t.path);
              }
            }}
            onMouseDown={(e) => {
              if (e.button === 1) e.preventDefault();
            }}
            draggable
            onDragStart={(e) => {
              setDragIdx(i);
              e.dataTransfer.effectAllowed = "move";
              e.dataTransfer.setData("text/plain", String(i));
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
            title={t.path}
          >
            <span
              className="font-mono text-[9px] font-bold"
              style={{ color: methodVar(t.doc.request.method) }}
            >
              {t.doc.request.method}
            </span>
            <span className="truncate max-w-40">{t.doc.name || "Untitled"}</span>
            {t.loading ? (
              <Spinner size={10} />
            ) : (
              tabIsDirty(t) && (
                <span className="h-1.5 w-1.5 rounded-full bg-warn shrink-0" />
              )
            )}
            <button
              type="button"
              title="Close tab"
              onClick={(e) => {
                e.stopPropagation();
                requestClose(t.path);
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
