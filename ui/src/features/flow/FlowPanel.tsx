import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDown,
  ChevronRight,
  FileDown,
  Folder,
  FolderOpen,
  FolderPlus,
  Loader2,
  Play,
  Plus,
  Search,
  Trash2,
  Workflow,
  X,
} from "lucide-react";
import { open as openDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { api } from "@/api/client";
import {
  flowStepPath,
  type FlowDoc,
  type FlowTreeNode,
  type HttpMethod,
  type SendResult,
  type TreeNode,
} from "@/api/types";
import { SplitPane } from "@/components/SplitPane";
import { Button, EmptyState, IconButton, Modal, Select, TextInput } from "@/components/ui";
import ResponseViewer from "@/features/response/ResponseViewer";
import { cn, formatMs, methodVar, statusClass } from "@/utils";
import { useKeel } from "@/state/store";

interface Step {
  id: string;
  path: string;
  name: string;
  method: HttpMethod;
  /** False = keep going when this step fails. Defaults to true. */
  stopOnFailure: boolean;
}

interface StepResult {
  status: "running" | "ok" | "error";
  result?: SendResult;
  error?: string;
}

interface EditorState {
  fileName: string | null;
  folder: string;
  name: string;
  steps: Step[];
}

function requestOptions(tree: TreeNode[]) {
  const out: { path: string; name: string; method: HttpMethod; label: string }[] = [];
  const walk = (nodes: TreeNode[], prefix: string) => {
    for (const n of nodes) {
      if (n.kind === "request") {
        out.push({
          path: n.path,
          name: n.name,
          method: n.method ?? "GET",
          label: prefix ? `${prefix} / ${n.name}` : n.name,
        });
      }
      if (n.children) walk(n.children, prefix ? `${prefix} / ${n.name}` : n.name);
    }
  };
  walk(tree, "");
  return out;
}

function lookup(requests: ReturnType<typeof requestOptions>, step: import("@/api/types").FlowStep): Step {
  const path = flowStepPath(step);
  const req = requests.find((r) => r.path === path);
  const stops = typeof step === "string" ? true : step.onFailure !== "continue";
  return {
    id: crypto.randomUUID(),
    path,
    name: req?.name ?? path,
    method: req?.method ?? "GET",
    stopOnFailure: stops,
  };
}

function folderOptions(nodes: FlowTreeNode[], prefix = ""): { path: string; label: string }[] {
  const out: { path: string; label: string }[] = [];
  for (const n of nodes) {
    if (n.kind !== "folder") continue;
    const label = prefix ? `${prefix} / ${n.name}` : n.name;
    out.push({ path: n.path, label });
    if (n.children) out.push(...folderOptions(n.children, label));
  }
  return out;
}

function flowDocOf(editor: EditorState): FlowDoc {
  return {
    schemaVersion: "1",
    name: editor.name.trim() || "Untitled flow",
    kind: "flow",
    steps: editor.steps.map((s) =>
      s.stopOnFailure ? s.path : { path: s.path, onFailure: "continue" },
    ),
  };
}

function parentFolder(path: string) {
  const i = path.lastIndexOf("/");
  return i === -1 ? "" : path.slice(0, i);
}

function canDropOn(src: string, dest: string) {
  return src !== dest && !dest.startsWith(`${src}/`) && parentFolder(src) !== dest;
}

function placeAt(
  src: string | null,
  path: string,
  isFolder: boolean,
  el: HTMLElement,
  clientY: number,
): { before: boolean; into: boolean } | null {
  if (!src || src === path || path.startsWith(`${src}/`)) return null;
  const rect = el.getBoundingClientRect();
  const height = rect.height || 28;
  const y = (clientY - rect.top) / height;
  const into = isFolder && y > 0.25 && y < 0.75;
  if (into && !canDropOn(src, path)) return null;
  return { before: y < 0.5, into };
}

export function FlowPanel() {
  const tree = useKeel((s) => s.tree);
  const workspace = useKeel((s) => s.workspace);
  const activeEnv = useKeel((s) => s.activeEnv);
  const toast = useKeel((s) => s.toast);
  const requests = useMemo(() => requestOptions(tree), [tree]);

  const [nodes, setNodes] = useState<FlowTreeNode[]>([]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [pick, setPick] = useState("");
  const [running, setRunning] = useState(false);
  const [saving, setSaving] = useState(false);
  const [addMenu, setAddMenu] = useState(false);
  const addMenuRef = useRef<HTMLDivElement>(null);
  const [createFolder, setCreateFolder] = useState<string | null>(null);
  const [folderName, setFolderName] = useState("");
  const [menu, setMenu] = useState<{ x: number; y: number; node: FlowTreeNode } | null>(null);
  const [runningPath, setRunningPath] = useState<string | null>(null);
  const [draggingPath, setDraggingPath] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [dropBefore, setDropBefore] = useState(true);
  const [dropInto, setDropInto] = useState(false);
  const dragRef = useRef<{
    path: string;
    startX: number;
    startY: number;
    moved: boolean;
  } | null>(null);
  const dropRef = useRef<{ path: string; before: boolean; into: boolean } | null>(null);
  const suppressClick = useRef(false);

  const folders = useMemo(() => folderOptions(nodes), [nodes]);

  const refresh = () => {
    api.flowTree().then(setNodes).catch((e) => toast(String(e), "error"));
  };

  useEffect(() => {
    if (!workspace) return;
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspace?.root]);

  useEffect(() => {
    if (!addMenu && !menu) return;
    const onDown = (e: MouseEvent) => {
      if (addMenu && !addMenuRef.current?.contains(e.target as Node)) setAddMenu(false);
      setMenu(null);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setAddMenu(false);
        setMenu(null);
      }
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [addMenu, menu]);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      const drag = dragRef.current;
      if (!drag) return;
      if (!drag.moved) {
        if (Math.hypot(e.clientX - drag.startX, e.clientY - drag.startY) < 4) return;
        drag.moved = true;
        setDraggingPath(drag.path);
      }
      let under: Element | null = null;
      try {
        under = document.elementFromPoint(e.clientX, e.clientY);
      } catch {
        under = null;
      }
      const hit = under?.closest("[data-flow-path]");
      if (!(hit instanceof HTMLElement)) {
        dropRef.current = null;
        setDropTarget(null);
        return;
      }
      const path = hit.dataset.flowPath;
      if (!path) return;
      const place = placeAt(drag.path, path, hit.dataset.flowKind === "folder", hit, e.clientY);
      if (!place) {
        dropRef.current = null;
        setDropTarget(null);
        return;
      }
      dropRef.current = { path, before: place.before, into: place.into };
      setDropTarget(path);
      setDropBefore(place.before);
      setDropInto(place.into);
    };
    const finish = () => {
      const drag = dragRef.current;
      if (!drag) return;
      dragRef.current = null;
      const dest = dropRef.current;
      dropRef.current = null;
      if (!drag.moved) {
        setDraggingPath(null);
        setDropTarget(null);
        return;
      }
      suppressClick.current = true;
      window.setTimeout(() => {
        suppressClick.current = false;
      }, 0);
      const src = drag.path;
      setDraggingPath(null);
      setDropTarget(null);
      if (!dest || dest.path === src || dest.path.startsWith(`${src}/`)) return;
      const fail = (err: unknown) => useKeel.getState().toast(String(err), "error");
      const done = (moved: string) => {
        if (moved !== src) {
          setSelected((s) => (s === src ? moved : s));
          setEditor((ed) =>
            ed?.fileName === src ? { ...ed, fileName: moved, folder: parentFolder(moved) } : ed,
          );
        }
        api.flowTree().then(setNodes).catch(fail);
      };
      if (dest.into) {
        setCollapsed((prev) => {
          const next = new Set(prev);
          next.delete(dest.path);
          return next;
        });
        api.flowMove(src, dest.path).then(done).catch(fail);
      } else {
        api.flowReorder(src, dest.path, dest.before).then(done).catch(fail);
      }
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", finish);
    return () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", finish);
    };
  }, []);

  const openEditor = async (file: string) => {
    try {
      const doc = await api.flowRead(file);
      const slash = file.lastIndexOf("/");
      setSelected(file);
      setPick("");
      setEditor({
        fileName: file,
        folder: slash === -1 ? "" : file.slice(0, slash),
        name: doc.name,
        steps: doc.steps.map((p) => lookup(requests, p)),
      });
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const startCreate = (folder: string) => {
    setPick("");
    setEditor({ fileName: null, folder, name: "", steps: [] });
  };

  const save = async () => {
    if (!editor) return;
    const doc = flowDocOf(editor);
    setSaving(true);
    try {
      const saved = await api.flowSave(editor.fileName, doc, editor.fileName ? null : editor.folder);
      setEditor((ed) => (ed ? { ...ed, fileName: saved, name: doc.name } : ed));
      setSelected(saved);
      refresh();
      toast("Flow saved", "success");
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setSaving(false);
    }
  };

  const importFlow = async (folder: string) => {
    try {
      const picked = await openDialog({
        multiple: false,
        title: "Import flow",
        filters: [{ name: "Flow (YAML)", extensions: ["yaml", "yml"] }],
      });
      if (!picked || Array.isArray(picked)) return;
      const saved = await api.flowImport(picked, folder);
      refresh();
      await openEditor(saved);
      toast("Flow imported", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const writeExport = async (doc: FlowDoc) => {
    const dest = await saveFileDialog({
      title: "Export flow",
      defaultPath: `${doc.name.replace(/[^\w.-]+/g, "-").toLowerCase() || "flow"}.yaml`,
      filters: [{ name: "Flow (YAML)", extensions: ["yaml"] }],
    });
    if (!dest) return;
    const yaml = await api.flowToYaml(doc);
    await api.saveResponse(dest, btoa(unescape(encodeURIComponent(yaml))));
    toast("Flow exported", "success");
  };

  const exportFlow = async () => {
    if (!editor) return;
    try {
      await writeExport(flowDocOf(editor));
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const exportNode = async (path: string) => {
    try {
      await writeExport(await api.flowRead(path));
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const duplicateNode = async (path: string) => {
    try {
      const copy = await api.flowDuplicate(path);
      setSelected(copy);
      refresh();
      toast("Flow duplicated", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const removeNode = async (path: string) => {
    try {
      await api.flowDelete(path);
      if (selected === path || editor?.fileName === path) {
        setSelected(null);
        setEditor(null);
      }
      refresh();
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const submitFolder = async () => {
    const parent = createFolder;
    const name = folderName.trim();
    if (parent === null || !name) return;
    setCreateFolder(null);
    setFolderName("");
    try {
      const created = await api.flowMkdir(parent, name);
      setCollapsed((prev) => {
        const next = new Set(prev);
        if (parent) next.delete(parent);
        next.delete(created);
        return next;
      });
      refresh();
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const addStep = () => {
    const req = requests.find((r) => r.path === pick) ?? requests[0];
    if (!req || !editor) return;
    setEditor({
      ...editor,
      steps: [
        ...editor.steps,
        { id: crypto.randomUUID(), path: req.path, name: req.name, method: req.method, stopOnFailure: true },
      ],
    });
  };

  const runSteps = async (title: string, steps: Step[], path: string | null) => {
    if (steps.length === 0 || running) return;
    let live: Record<string, StepResult> = {};
    const publish = (runningNow: boolean, results: Record<string, StepResult>) =>
      useKeel.getState().setFlowRun({ name: title, running: runningNow, steps, results });
    useKeel.getState().openFlow();
    setRunning(true);
    setRunningPath(path);
    publish(true, live);
    for (const step of steps) {
      live = { ...live, [step.id]: { status: "running" } };
      publish(true, live);
      try {
        const result = await api.sendRequest(step.path, activeEnv);
        useKeel.setState((st) => ({ envValuesRevision: st.envValuesRevision + 1 }));
        const testsFailed = result.testResults.some((t) => !t.passed);
        const failed = Boolean(result.error) || !result.ok || testsFailed;
        live = {
          ...live,
          [step.id]: {
            status: failed ? "error" : "ok",
            result,
            error: result.error ?? undefined,
          },
        };
        publish(true, live);
        if (failed && step.stopOnFailure) {
          toast(`${step.name}: ${result.error ?? "failed"}`, "error");
          break;
        }
      } catch (e) {
        const error = String(e);
        live = { ...live, [step.id]: { status: "error", error } };
        publish(true, live);
        toast(`${step.name}: ${error}`, "error");
        if (step.stopOnFailure) break;
      }
    }
    setRunning(false);
    setRunningPath(null);
    publish(false, live);
  };

  const run = () => {
    if (!editor) return;
    void runSteps(editor.name.trim() || "Untitled flow", editor.steps, editor.fileName);
  };

  const runFlow = async (path: string, fallbackName: string) => {
    if (running) return;
    try {
      const doc = await api.flowRead(path);
      const steps = doc.steps.map((p) => lookup(requests, p));
      if (steps.length === 0) {
        toast("Flow has no steps", "error");
        return;
      }
      await runSteps(doc.name || fallbackName, steps, path);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const runMenu = (fn: (node: FlowTreeNode) => void | Promise<void>) => {
    const node = menu?.node;
    setMenu(null);
    if (node) void fn(node);
  };

  const filter = query.trim().toLowerCase();
  const visible = useMemo(() => {
    if (!filter) return nodes;
    const prune = (list: FlowTreeNode[]): FlowTreeNode[] => {
      const out: FlowTreeNode[] = [];
      for (const n of list) {
        if (n.kind === "flow") {
          if (n.name.toLowerCase().includes(filter) || n.path.toLowerCase().includes(filter)) out.push(n);
        } else {
          const children = prune(n.children ?? []);
          if (children.length > 0 || n.name.toLowerCase().includes(filter)) {
            out.push({ ...n, children });
          }
        }
      }
      return out;
    };
    return prune(nodes);
  }, [nodes, filter]);

  const renderNode = (node: FlowTreeNode, depth: number): React.ReactNode => {
    const isFolder = node.kind === "folder";
    const open = filter ? true : !collapsed.has(node.path);
    const active = selected === node.path;
    const isDrop = dropTarget === node.path;
    return (
      <div key={node.path}>
        <div
          role="treeitem"
          title={node.path}
          data-flow-path={node.path}
          data-flow-kind={node.kind}
          style={{ paddingLeft: 8 + depth * 12 }}
          onMouseDown={(e) => {
            if (e.button !== 0) return;
            dragRef.current = {
              path: node.path,
              startX: e.clientX,
              startY: e.clientY,
              moved: false,
            };
          }}
          onClick={() => {
            if (suppressClick.current) {
              suppressClick.current = false;
              return;
            }
            if (isFolder) {
              setCollapsed((prev) => {
                const next = new Set(prev);
                if (next.has(node.path)) next.delete(node.path);
                else next.add(node.path);
                return next;
              });
            } else {
              void openEditor(node.path);
            }
          }}
          onContextMenu={(e) => {
            e.preventDefault();
            setMenu({ x: e.clientX, y: e.clientY, node });
          }}
          className={cn(
            "h-7 pr-2 flex items-center gap-1.5 rounded text-xs cursor-pointer select-none",
            active ? "bg-accent-soft text-fg-0" : "text-fg-1 hover:bg-bg-hover",
            isDrop && dropInto && "ring-1 ring-line-focus bg-accent-soft",
            isDrop && !dropInto && dropBefore && "border-t-2 border-t-accent",
            isDrop && !dropInto && !dropBefore && "border-b-2 border-b-accent",
            draggingPath === node.path && "opacity-50",
          )}
        >
          {isFolder ? (
            <>
              <ChevronRight
                size={12}
                className={cn("shrink-0 text-fg-2 transition-transform", open && "rotate-90")}
              />
              {open ? (
                <FolderOpen size={13} className="shrink-0 text-fg-2" />
              ) : (
                <Folder size={13} className="shrink-0 text-fg-2" />
              )}
              <span className="truncate">{node.name}</span>
            </>
          ) : (
            <>
              <span className="w-3 shrink-0" />
              <Workflow size={13} className="shrink-0 text-fg-2" />
              <span className="min-w-0 flex-1 truncate">{node.name}</span>
              <IconButton
                title="Run flow"
                aria-label={`Run ${node.name}`}
                className={cn(
                  "h-5 w-5 shrink-0",
                  runningPath === node.path ? "text-accent" : "text-fg-2",
                )}
                disabled={running}
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation();
                  void runFlow(node.path, node.name);
                }}
              >
                {runningPath === node.path ? (
                  <Loader2 size={11} className="animate-spin" />
                ) : (
                  <Play size={11} />
                )}
              </IconButton>
            </>
          )}
        </div>
        {isFolder && open && node.children?.map((child) => renderNode(child, depth + 1))}
      </div>
    );
  };

  const empty = nodes.length === 0;

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-9 shrink-0 border-b border-line-0 flex items-center px-2.5 gap-1 text-xs font-semibold uppercase tracking-wider text-fg-2">
        <span className="flex-1">Flow</span>
        <div ref={addMenuRef} className="relative">
          <IconButton
            title="Add"
            aria-label="Add"
            aria-haspopup="menu"
            aria-expanded={addMenu}
            className="h-6 w-6"
            onClick={() => setAddMenu((open) => !open)}
          >
            <Plus size={13} />
          </IconButton>
          {addMenu && (
            <div className="absolute right-0 top-full mt-1 z-40 w-44 rounded-md border border-line-0 bg-bg-1 shadow-xl py-1">
              <MenuButton
                icon={<Plus size={13} />}
                label="Add flow"
                onClick={() => {
                  setAddMenu(false);
                  startCreate("");
                }}
              />
              <MenuButton
                icon={<FolderPlus size={13} />}
                label="Add folder"
                onClick={() => {
                  setAddMenu(false);
                  setFolderName("");
                  setCreateFolder("");
                }}
              />
              <MenuButton
                icon={<FileDown size={13} />}
                label="Import flow"
                onClick={() => {
                  setAddMenu(false);
                  void importFlow("");
                }}
              />
            </div>
          )}
        </div>
      </div>

      <div className="shrink-0 border-b border-line-0 px-2 py-1.5">
        <div className="flex items-center gap-1.5 rounded border border-line-0 bg-bg-2 px-2">
          <Search size={12} className="shrink-0 text-fg-2" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search flows..."
            aria-label="Search flows"
            className="h-6 min-w-0 flex-1 bg-transparent text-xs text-fg-0 outline-none placeholder:text-fg-2"
          />
          {query && (
            <button
              type="button"
              aria-label="Clear search"
              className="shrink-0 text-fg-2 hover:text-fg-0"
              onClick={() => setQuery("")}
            >
              <X size={12} />
            </button>
          )}
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto p-1">
        {empty ? (
          <EmptyState
            icon={<Workflow size={24} />}
            title="No flows"
            hint="Create a flow or a folder to organize them."
            action={
              <Button variant="default" className="mt-1" onClick={() => startCreate("")}>
                New flow
              </Button>
            }
          />
        ) : filter && visible.length === 0 ? (
          <div className="flex h-full items-center justify-center p-6 text-center">
            <p className="text-xs text-fg-2">No matches for {query}</p>
          </div>
        ) : (
          visible.map((n) => renderNode(n, 0))
        )}
      </div>

      {menu && (
        <div
          className="fixed z-50 min-w-40 rounded border border-line-0 bg-bg-2 shadow-lg py-1 text-xs"
          style={{ left: menu.x, top: menu.y }}
          onMouseDown={(e) => e.stopPropagation()}
          onContextMenu={(e) => e.preventDefault()}
        >
          {menu.node.kind === "folder" ? (
            <>
              <ContextItem label="New flow…" onClick={() => runMenu((n) => startCreate(n.path))} />
              <ContextItem
                label="New folder…"
                onClick={() =>
                  runMenu((n) => {
                    setFolderName("");
                    setCreateFolder(n.path);
                  })
                }
              />
              <ContextItem
                label="Import flow…"
                onClick={() => runMenu((n) => importFlow(n.path))}
              />
              <ContextItem label="Delete" danger onClick={() => runMenu((n) => removeNode(n.path))} />
            </>
          ) : (
            <>
              <ContextItem label="Run" onClick={() => runMenu((n) => runFlow(n.path, n.name))} />
              <ContextItem label="Edit" onClick={() => runMenu((n) => openEditor(n.path))} />
              <ContextItem label="Duplicate" onClick={() => runMenu((n) => duplicateNode(n.path))} />
              <ContextItem label="Export…" onClick={() => runMenu((n) => exportNode(n.path))} />
              <ContextItem label="Delete" danger onClick={() => runMenu((n) => removeNode(n.path))} />
            </>
          )}
        </div>
      )}

      <Modal
        open={editor !== null}
        onClose={() => {
          if (!running) setEditor(null);
        }}
        title={editor?.fileName ? "Edit flow" : "New flow"}
        width="max-w-lg"
      >
        {editor && (
          <div className="flex flex-col gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-[11px] uppercase tracking-wider text-fg-2">Name</span>
              <TextInput
                autoFocus
                value={editor.name}
                placeholder="Flow name"
                onChange={(e) => setEditor({ ...editor, name: e.target.value })}
              />
            </label>
            {!editor.fileName && (
              <label className="flex flex-col gap-1">
                <span className="text-[11px] uppercase tracking-wider text-fg-2">Folder</span>
                <Select
                  value={editor.folder}
                  onChange={(e) => setEditor({ ...editor, folder: e.target.value })}
                >
                  <option value="">flows</option>
                  {folders.map((f) => (
                    <option key={f.path} value={f.path}>
                      {f.label}
                    </option>
                  ))}
                </Select>
              </label>
            )}
            <div className="flex items-center gap-1.5">
              <Select
                className="flex-1 min-w-0"
                value={pick}
                onChange={(e) => setPick(e.target.value)}
                disabled={requests.length === 0 || running}
              >
                {requests.length === 0 ? (
                  <option value="">No requests</option>
                ) : (
                  <>
                    <option value="">Select request…</option>
                    {requests.map((r) => (
                      <option key={r.path} value={r.path}>
                        {r.method} {r.label}
                      </option>
                    ))}
                  </>
                )}
              </Select>
              <IconButton title="Add step" className="h-7 w-7" disabled={requests.length === 0 || running} onClick={addStep}>
                <Plus size={14} />
              </IconButton>
            </div>
            <div className="max-h-64 overflow-y-auto rounded border border-line-0">
              {editor.steps.length === 0 ? (
                <p className="px-3 py-6 text-center text-xs text-fg-2">
                  Add requests in order. Each step sees variables set by the one before it.
                </p>
              ) : (
                editor.steps.map((step, i) => (
                  <div key={step.id}>
                    {i > 0 && (
                      <div className="flex justify-center text-fg-2">
                        <ArrowDown size={12} />
                      </div>
                    )}
                    <div className="flex items-center gap-1 h-8 px-2">
                      <span className="w-3 shrink-0 text-center font-mono text-[10px] text-fg-2">{i + 1}</span>
                      <span
                        className="w-10 shrink-0 font-mono text-[10px] font-bold"
                        style={{ color: methodVar(step.method) }}
                      >
                        {step.method}
                      </span>
                      <span className="flex-1 truncate text-xs text-fg-0">{step.name}</span>
                      <button
                        type="button"
                        title={step.stopOnFailure ? "Stop on failure" : "Continue on failure"}
                        disabled={running}
                        onClick={() =>
                          setEditor({
                            ...editor,
                            steps: editor.steps.map((s) =>
                              s.id === step.id ? { ...s, stopOnFailure: !s.stopOnFailure } : s,
                            ),
                          })
                        }
                        className={cn(
                          "shrink-0 font-mono text-[10px]",
                          step.stopOnFailure ? "text-fg-2" : "text-fg-1",
                        )}
                      >
                        {step.stopOnFailure ? "stop" : "continue"}
                      </button>
                      <IconButton
                        title="Remove"
                        className="h-5 w-5"
                        disabled={running}
                        onClick={() =>
                          setEditor({ ...editor, steps: editor.steps.filter((s) => s.id !== step.id) })
                        }
                      >
                        <Trash2 size={11} />
                      </IconButton>
                    </div>
                  </div>
                ))
              )}
            </div>
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-1">
                {editor.fileName && (
                  <Button
                    type="button"
                    variant="danger"
                    disabled={running}
                    onClick={() => void removeNode(editor.fileName!)}
                  >
                    Delete
                  </Button>
                )}
                <Button type="button" variant="ghost" disabled={running} onClick={() => void exportFlow()}>
                  Export
                </Button>
              </div>
              <div className="flex items-center gap-2">
                <Button type="button" variant="ghost" disabled={running} onClick={() => setEditor(null)}>
                  Close
                </Button>
                <Button
                  type="button"
                  variant="default"
                  disabled={editor.steps.length === 0 || running}
                  onClick={() => void run()}
                >
                  <Play size={11} />
                  Run
                </Button>
                <Button type="button" variant="primary" disabled={saving || running} onClick={() => void save()}>
                  {editor.fileName ? "Save" : "Create"}
                </Button>
              </div>
            </div>
          </div>
        )}
      </Modal>

      <Modal
        open={createFolder !== null}
        onClose={() => setCreateFolder(null)}
        title="New folder"
        width="max-w-sm"
      >
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            void submitFolder();
          }}
        >
          <TextInput
            autoFocus
            placeholder="Name"
            value={folderName}
            onChange={(e) => setFolderName(e.target.value)}
          />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => setCreateFolder(null)}>
              Cancel
            </Button>
            <Button type="submit" variant="primary" disabled={!folderName.trim()}>
              Create
            </Button>
          </div>
        </form>
      </Modal>
    </div>
  );
}

function MenuButton({
  icon,
  label,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="w-full h-8 px-3 flex items-center gap-2 text-left text-xs text-fg-1 hover:bg-bg-hover hover:text-fg-0"
    >
      <span className="shrink-0 text-fg-2">{icon}</span>
      {label}
    </button>
  );
}

function ContextItem({
  label,
  danger,
  onClick,
}: {
  label: string;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onMouseDown={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onClick();
      }}
      className={cn(
        "block w-full px-3 py-1.5 text-left hover:bg-bg-hover",
        danger ? "text-danger" : "text-fg-1 hover:text-fg-0",
      )}
    >
      {label}
    </button>
  );
}

export function FlowRun() {
  const run = useKeel((s) => s.flowRun);
  const [openId, setOpenId] = useState<string | null>(null);

  useEffect(() => {
    if (!run) return;
    const last = [...run.steps].reverse().find((s) => run.results[s.id]);
    if (last) setOpenId(last.id);
  }, [run]);

  if (!run) return null;

  const selected = run.steps.find((s) => s.id === openId);
  const selectedResult = selected ? run.results[selected.id] : undefined;

  return (
    <div className="flex-1 min-h-0 flex flex-col bg-bg-0">
      <SplitPane
        className="flex-1 min-h-0"
        direction="horizontal"
        initial={360}
        min={220}
        max={720}
        top={
          <div className="h-full overflow-y-auto py-3">
            {run.steps.map((step, i) => {
              const res = run.results[step.id];
              const open = openId === step.id;
              return (
                <div key={step.id}>
                  {i > 0 && (
                    <div className="flex justify-center text-fg-2">
                      <ArrowDown size={12} />
                    </div>
                  )}
                  <button
                    type="button"
                    disabled={!res}
                    onClick={() => setOpenId(step.id)}
                    className={cn(
                      "mx-3 flex w-[calc(100%-1.5rem)] items-center gap-2 h-9 px-2.5 text-left rounded border",
                      open ? "border-accent bg-accent-soft" : "border-line-0 bg-bg-1",
                    )}
                  >
                    <span className="w-4 shrink-0 text-center font-mono text-[10px] text-fg-2">{i + 1}</span>
                    <span
                      className="w-10 shrink-0 font-mono text-[10px] font-bold"
                      style={{ color: methodVar(step.method) }}
                    >
                      {step.method}
                    </span>
                    <span className="flex-1 truncate text-xs text-fg-0">{step.name}</span>
                    {res?.result?.status != null && (
                      <span className={cn("font-mono text-[11px]", statusClass(res.result.status))}>
                        {res.result.status}
                      </span>
                    )}
                    {res?.result && (
                      <span className="font-mono text-[10px] text-fg-2">{formatMs(res.result.timeMs)}</span>
                    )}
                    {res?.status === "running" && <span className="text-[10px] text-fg-2">…</span>}
                    {res?.status === "error" && res.result == null && (
                      <span className="text-[10px] text-danger">error</span>
                    )}
                  </button>
                </div>
              );
            })}
          </div>
        }
        bottom={
          <ResponseViewer
            result={selectedResult?.result ?? null}
            error={selectedResult?.error && !selectedResult.result ? selectedResult.error : null}
            loading={selectedResult?.status === "running"}
            emptyHint="Select a step to see its response"
          />
        }
      />
    </div>
  );
}

export default FlowPanel;
