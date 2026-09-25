import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronRight,
  FileDown,
  FileUp,
  Folder,
  FolderOpen,
  FolderPlus,
  MoreHorizontal,
  Play,
  Plus,
  RefreshCw,
  Search,
  Shield,
  SlidersHorizontal,
  X,
  Zap,
} from "lucide-react";
import { open as openDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api } from "@/api/client";
import type { HttpMethod, TreeNode } from "@/api/types";
import { Button, EmptyState, IconButton, Modal, TextInput } from "@/components/ui";
import { cn, methodVar } from "@/utils";
import { useKeel } from "@/state/store";
import { CollectionSettingsModal } from "./CollectionSettingsModal";
import { FolderSettingsModal } from "./FolderSettingsModal";

function MethodBadge({ method }: { method?: HttpMethod }) {
  return (
    <span
      className="w-10 shrink-0 text-center font-mono text-[10px] font-bold"
      style={{ color: methodVar(method ?? "GET") }}
    >
      {method}
    </span>
  );
}

interface MenuState {
  x: number;
  y: number;
  node: TreeNode;
}

interface CreateModal {
  kind: "request" | "folder";
  parent: string;
}

export function CollectionTree() {
  const tree = useKeel((s) => s.tree);
  const activePath = useKeel((s) => s.activePath);
  const openRequest = useKeel((s) => s.openRequest);
  const createRequest = useKeel((s) => s.createRequest);
  const createFolder = useKeel((s) => s.createFolder);
  const renameRequest = useKeel((s) => s.renameRequest);
  const duplicateRequest = useKeel((s) => s.duplicateRequest);
  const deleteNode = useKeel((s) => s.deleteNode);
  const moveNode = useKeel((s) => s.moveNode);
  const reorderNode = useKeel((s) => s.reorderNode);
  const refreshTree = useKeel((s) => s.refreshTree);
  const toast = useKeel((s) => s.toast);
  const startRun = useKeel((s) => s.startRun);
  const openCodegen = useKeel((s) => s.openCodegen);

  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [createModal, setCreateModal] = useState<CreateModal | null>(null);
  const [newName, setNewName] = useState("");
  const [renaming, setRenaming] = useState<{ path: string; name: string; original: string } | null>(null);
  const renamingRef = useRef(false);
  const [importOpen, setImportOpen] = useState(false);
  const [importFolder, setImportFolder] = useState("");
  const [importSource, setImportSource] = useState("");
  const [importBusy, setImportBusy] = useState(false);
  const [importDrag, setImportDrag] = useState(false);
  const [addMenu, setAddMenu] = useState(false);
  const addMenuRef = useRef<HTMLDivElement>(null);
  const [headerMenu, setHeaderMenu] = useState(false);
  const headerMenuRef = useRef<HTMLDivElement>(null);
  const importFileRef = useRef<HTMLInputElement>(null);
  const importFolderRef = useRef(importFolder);
  importFolderRef.current = importFolder;
  const [collectionSettingsOpen, setCollectionSettingsOpen] = useState(false);
  const [folderSettingsFor, setFolderSettingsFor] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [dropBefore, setDropBefore] = useState(true);
  const [dropInto, setDropInto] = useState(false);
  const dragPath = useRef<string | null>(null);

  const folders = useMemo(() => {
    const out: TreeNode[] = [];
    const walk = (nodes: TreeNode[]) =>
      nodes.forEach((n) => {
        if (n.kind !== "request") {
          out.push(n);
          if (n.children) walk(n.children);
        }
      });
    walk(tree);
    return out;
  }, [tree]);

  const filter = query.trim().toLowerCase();

  /** True when a request matches the active filter. */
  const requestMatches = useCallback(
    (node: TreeNode) => {
      if (!filter) return true;
      const haystack = `${node.name} ${node.path} ${node.method ?? ""} ${node.url ?? ""}`.toLowerCase();
      return filter.split(/\s+/).every((term) => haystack.includes(term));
    },
    [filter],
  );

  /** Tree pruned to matching requests; folders stay when they contain matches. */
  const visibleTree = useMemo(() => {
    if (!filter) return tree;
    const prune = (nodes: TreeNode[]): TreeNode[] => {
      const out: TreeNode[] = [];
      for (const n of nodes) {
        if (n.kind === "request") {
          if (requestMatches(n)) out.push(n);
        } else {
          const children = prune(n.children ?? []);
          if (children.length > 0) out.push({ ...n, children });
        }
      }
      return out;
    };
    return prune(tree);
  }, [tree, filter, requestMatches]);

  useEffect(() => {
    if (!addMenu) return;
    const onDown = (e: MouseEvent) => {
      if (!addMenuRef.current?.contains(e.target as Node)) setAddMenu(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setAddMenu(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [addMenu]);

  useEffect(() => {
    if (!headerMenu) return;
    const onDown = (e: MouseEvent) => {
      if (!headerMenuRef.current?.contains(e.target as Node)) setHeaderMenu(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setHeaderMenu(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [headerMenu]);

  const reportImport = (res: { count: number; skipped: number; warnings: string[] }) => {
    let msg = `${res.count} imported, ${res.skipped} skipped`;
    if (res.warnings.length > 0) msg += ` — ${res.warnings.join("; ")}`;
    toast(msg, res.warnings.length > 0 ? "info" : "success");
    setImportOpen(false);
    setImportSource("");
  };

  const importPaths = async (paths: string[]) => {
    if (paths.length === 0) return;
    const zips = paths.filter((path) => path.toLowerCase().endsWith(".zip"));
    const rest = paths.filter((path) => !path.toLowerCase().endsWith(".zip"));
    if (zips.length > 0 && rest.length > 0) {
      toast("Import a ZIP on its own, not mixed with other files.", "error");
      return;
    }
    setImportBusy(true);
    try {
      let files = 0;
      let skipped = 0;
      const warnings: string[] = [];
      const folder = importFolderRef.current;
      if (zips.length > 0) {
        for (const path of zips) {
          const res = await api.importZip(path, null, folder);
          files += res.files.length;
          skipped += res.skipped;
          warnings.push(...res.warnings);
        }
      } else {
        for (const path of rest) {
          const text = await api.readTextFile(path);
          const res = await api.importSource(text, folder);
          files += res.files.length;
          skipped += res.skipped;
          warnings.push(...res.warnings);
        }
      }
      await refreshTree();
      reportImport({ count: files, skipped, warnings });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setImportBusy(false);
    }
  };

  const importDropped = async (files: File[]) => {
    if (files.length === 0) return;
    const zips = files.filter((file) => file.name.toLowerCase().endsWith(".zip"));
    const rest = files.filter((file) => !file.name.toLowerCase().endsWith(".zip"));
    if (zips.length > 0 && rest.length > 0) {
      toast("Import a ZIP on its own, not mixed with other files.", "error");
      return;
    }
    setImportBusy(true);
    try {
      if (zips.length > 0) {
        let filesN = 0;
        let skipped = 0;
        const warnings: string[] = [];
        for (const zip of zips) {
          const dataBase64 = bytesToBase64(new Uint8Array(await zip.arrayBuffer()));
          const res = await api.importZip(null, dataBase64, importFolderRef.current);
          filesN += res.files.length;
          skipped += res.skipped;
          warnings.push(...res.warnings);
        }
        await refreshTree();
        reportImport({ count: filesN, skipped, warnings });
        return;
      }
      const texts = await readBrowserFiles(rest);
      let filesN = 0;
      let skipped = 0;
      const warnings: string[] = [];
      for (const text of texts) {
        const res = await api.importSource(text, importFolderRef.current);
        filesN += res.files.length;
        skipped += res.skipped;
        warnings.push(...res.warnings);
      }
      await refreshTree();
      reportImport({ count: filesN, skipped, warnings });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setImportBusy(false);
    }
  };

  const submitImportSource = async () => {
    const raw = importSource.trim();
    if (!raw) return;
    setImportBusy(true);
    try {
      if (isGitRepositoryUrl(raw)) {
        const parent = await openDialog({
          directory: true,
          title: "Choose where to clone the repository",
        });
        if (!parent) return;
        const root = await api.importGit(raw, parent);
        await useKeel.getState().openWorkspace(root);
        toast("Repository imported", "success");
        setImportOpen(false);
        setImportSource("");
        return;
      }
      if (/^curl\s/i.test(raw)) {
        const path = await api.importCurl(raw, importFolder);
        await refreshTree();
        toast(`Imported ${path}`, "success");
        setImportOpen(false);
        setImportSource("");
        return;
      }
      if (isHttpUrl(raw)) {
        const text = await api.fetchUrl(raw);
        const res = await api.importSource(text, importFolder);
        await refreshTree();
        reportImport({ count: res.files.length, skipped: res.skipped, warnings: res.warnings });
        return;
      }
      const res = await api.importSource(raw, importFolder);
      await refreshTree();
      reportImport({ count: res.files.length, skipped: res.skipped, warnings: res.warnings });
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setImportBusy(false);
    }
  };

  useEffect(() => {
    if (!importOpen) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (cancelled) return;
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") setImportDrag(true);
        else if (payload.type === "leave") setImportDrag(false);
        else if (payload.type === "drop") {
          setImportDrag(false);
          void importPaths(payload.paths);
        }
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [importOpen]);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("resize", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  const toggleFolder = (path: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const openMenu = (e: React.MouseEvent, node: TreeNode) => {
    e.preventDefault();
    e.stopPropagation();
    setMenu({
      x: Math.min(e.clientX, window.innerWidth - 180),
      y: Math.min(e.clientY, window.innerHeight - 220),
      node,
    });
  };

  const exportRequest = async (node: TreeNode) => {
    const dest = await saveFileDialog({
      title: "Export request",
      defaultPath: `${node.name.replace(/[^\w.-]+/g, "-").toLowerCase() || "request"}.yaml`,
      filters: [{ name: "Request (YAML)", extensions: ["yaml"] }],
    });
    if (!dest) return;
    try {
      const yaml = await api.exportRequest(node.path);
      await api.saveResponse(dest, btoa(unescape(encodeURIComponent(yaml))));
      toast("Request exported", "success");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const exportCollection = async (folder: string, label: string) => {
    const dest = await saveFileDialog({
      title: folder ? "Export folder" : "Export collection",
      defaultPath: `${label.replace(/[^\w.-]+/g, "-").toLowerCase() || "collection"}.zip`,
      filters: [{ name: "ZIP", extensions: ["zip"] }],
    });
    if (!dest) return;
    try {
      const dataBase64 = await api.exportCollection(folder);
      await api.saveResponse(dest, dataBase64);
      toast(folder ? "Folder exported" : "Collection exported", "success");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const copyPath = async (path: string) => {
    try {
      await navigator.clipboard.writeText(path);
      toast("Path copied", "success");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const submitCreate = async () => {
    if (!createModal || !newName.trim()) return;
    const { kind, parent } = createModal;
    setCreateModal(null);
    setNewName("");
    if (kind === "request") await createRequest(parent, newName.trim());
    else await createFolder(parent, newName.trim());
    if (parent) setCollapsed((prev) => {
      const next = new Set(prev);
      next.delete(parent);
      return next;
    });
  };

  const startRename = useCallback((node: TreeNode) => {
    renamingRef.current = true;
    setRenaming({ path: node.path, name: node.name, original: node.name });
    // expand ancestors so the inline input is visible
    setCollapsed((prev) => {
      const next = new Set(prev);
      const parts = node.path.split("/");
      for (let i = 1; i < parts.length; i++) next.delete(parts.slice(0, i).join("/"));
      return next;
    });
  }, []);

  const finishRename = (commit: boolean) => {
    if (!renamingRef.current) return;
    renamingRef.current = false;
    const r = renaming;
    setRenaming(null);
    if (commit && r && r.name.trim() && r.name.trim() !== r.original) {
      void renameRequest(r.path, r.name.trim());
    }
  };

  // F2 shortcut (dispatched globally) starts an inline rename of the active request
  useEffect(() => {
    const onStartRename = (e: Event) => {
      const path = (e as CustomEvent<string>).detail;
      const find = (nodes: TreeNode[]): TreeNode | null => {
        for (const n of nodes) {
          if (n.path === path) return n;
          const found = n.children ? find(n.children) : null;
          if (found) return found;
        }
        return null;
      };
      const node = find(useKeel.getState().tree);
      if (node && node.kind === "request") startRename(node);
    };
    window.addEventListener("keel:start-rename", onStartRename);
    return () => window.removeEventListener("keel:start-rename", onStartRename);
  }, [startRename]);

  const parentOf = (path: string) => {
    const i = path.lastIndexOf("/");
    return i === -1 ? "" : path.slice(0, i);
  };

  const canDropOn = (src: string, dest: string) =>
    src !== dest && !dest.startsWith(`${src}/`) && parentOf(src) !== dest;

  const dropPlace = (
    e: React.DragEvent,
    src: string | null,
    node: TreeNode,
    isFolder: boolean,
  ): { before: boolean; into: boolean } | null => {
    if (!src || src === node.path || node.path.startsWith(`${src}/`)) return null;
    const rect = e.currentTarget.getBoundingClientRect();
    const height = rect.height || 28;
    const clientY = e.clientY || (e.nativeEvent as MouseEvent).clientY || 0;
    const y = (clientY - rect.top) / height;
    const into = isFolder && y > 0.25 && y < 0.75;
    if (into && !canDropOn(src, node.path)) return null;
    return { before: y < 0.5, into };
  };

  const renderNode = (node: TreeNode, depth: number): React.ReactNode => {
    const isFolder = node.kind !== "request";
    const open = filter ? true : !collapsed.has(node.path);
    const selected = activePath === node.path;
    const isDrop = dropTarget === node.path;
    return (
      <div key={node.path}>
        <div
          role="treeitem"
          title={node.path}
          draggable={renaming?.path !== node.path}
          style={{ paddingLeft: 8 + depth * 12 }}
          onDragStart={(e) => {
            e.stopPropagation();
            dragPath.current = node.path;
            e.dataTransfer.effectAllowed = "move";
            e.dataTransfer.setData("text/plain", node.path);
          }}
          onDragEnd={() => {
            dragPath.current = null;
            setDropTarget(null);
          }}
          onDragOver={(e) => {
            const src = dragPath.current;
            const place = dropPlace(e, src, node, isFolder);
            if (!place) return;
            e.preventDefault();
            e.stopPropagation();
            e.dataTransfer.dropEffect = "move";
            if (dropTarget !== node.path || dropBefore !== place.before || dropInto !== place.into) {
              setDropTarget(node.path);
              setDropBefore(place.before);
              setDropInto(place.into);
            }
          }}
          onDragLeave={() => {
            if (dropTarget === node.path) setDropTarget(null);
          }}
          onDrop={(e) => {
            e.preventDefault();
            e.stopPropagation();
            const src = e.dataTransfer.getData("text/plain") || dragPath.current;
            dragPath.current = null;
            setDropTarget(null);
            const place = dropPlace(e, src, node, isFolder);
            if (!place || !src) return;
            if (place.into) {
              setCollapsed((prev) => {
                const next = new Set(prev);
                next.delete(node.path);
                return next;
              });
              void moveNode(src, node.path);
            } else {
              void reorderNode(src, node.path, place.before);
            }
          }}
          onClick={() => (isFolder ? toggleFolder(node.path) : openRequest(node.path))}
          onContextMenu={(e) => openMenu(e, node)}
          className={cn(
            "h-7 pr-2 flex items-center gap-1.5 rounded text-xs cursor-pointer select-none",
            selected ? "bg-accent-soft text-fg-0" : "text-fg-1 hover:bg-bg-hover",
            isDrop && dropInto && "ring-1 ring-line-focus bg-accent-soft",
            isDrop && !dropInto && dropBefore && "border-t-2 border-t-accent",
            isDrop && !dropInto && !dropBefore && "border-b-2 border-b-accent",
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
              {node.meta?.hasAuth && (
                <Shield size={10} className="shrink-0 text-[9px] text-fg-2" />
              )}
              {node.meta?.hasScripts && (
                <Zap size={10} className="shrink-0 text-[9px] text-fg-2" />
              )}
            </>
          ) : (
            <>
              <span className="w-1 shrink-0" />
              <MethodBadge method={node.method} />
              {renaming?.path === node.path ? (
                <input
                  autoFocus
                  className="min-w-0 flex-1 rounded bg-bg-2 border border-line-focus px-1 text-xs text-fg-0 outline-none"
                  value={renaming.name}
                  onChange={(e) => setRenaming((r) => (r ? { ...r, name: e.target.value } : r))}
                  onClick={(e) => e.stopPropagation()}
                  onFocus={(e) => e.currentTarget.select()}
                  onKeyDown={(e) => {
                    e.stopPropagation();
                    if (e.key === "Enter") finishRename(true);
                    else if (e.key === "Escape") finishRename(false);
                  }}
                  onBlur={() => finishRename(true)}
                />
              ) : (
                <span className="truncate">{node.name}</span>
              )}
            </>
          )}
        </div>
        {isFolder && open && node.children?.map((child) => renderNode(child, depth + 1))}
      </div>
    );
  };

  const runMenu = (fn: (node: TreeNode) => void | Promise<void>) => {
    const node = menu?.node;
    setMenu(null);
    if (node) void fn(node);
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-9 shrink-0 border-b border-line-0 flex items-center px-2.5 gap-1 text-xs font-semibold uppercase tracking-wider text-fg-2">
        <span className="flex-1">Collections</span>
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
              <HeaderMenuItem
                icon={<Plus size={13} />}
                label="Add request"
                onClick={() => {
                  setAddMenu(false);
                  setNewName("");
                  setCreateModal({ kind: "request", parent: "" });
                }}
              />
              <HeaderMenuItem
                icon={<FolderPlus size={13} />}
                label="Add folder"
                onClick={() => {
                  setAddMenu(false);
                  setNewName("");
                  setCreateModal({ kind: "folder", parent: "" });
                }}
              />
              <HeaderMenuItem
                icon={<FileDown size={13} />}
                label="Import request"
                onClick={() => {
                  setAddMenu(false);
                  setImportOpen(true);
                }}
              />
            </div>
          )}
        </div>
        <IconButton
          title="Run collection"
          aria-label="Run collection"
          className="h-6 w-6"
          onClick={() => void startRun("", { recursive: true })}
        >
          <Play size={13} />
        </IconButton>
        <div ref={headerMenuRef} className="relative">
          <IconButton
            title="More"
            aria-haspopup="menu"
            aria-expanded={headerMenu}
            className="h-6 w-6"
            onClick={() => setHeaderMenu((open) => !open)}
          >
            <MoreHorizontal size={13} />
          </IconButton>
          {headerMenu && (
            <div className="absolute right-0 top-full mt-1 z-40 w-48 rounded-md border border-line-0 bg-bg-1 shadow-xl py-1">
              <HeaderMenuItem
                icon={<SlidersHorizontal size={13} />}
                label="Collection settings…"
                onClick={() => {
                  setHeaderMenu(false);
                  setCollectionSettingsOpen(true);
                }}
              />
              <HeaderMenuItem
                icon={<FileUp size={13} />}
                label="Export collection"
                onClick={() => {
                  setHeaderMenu(false);
                  void exportCollection("", "collection");
                }}
              />
              <HeaderMenuItem
                icon={<RefreshCw size={13} />}
                label="Refresh"
                onClick={() => {
                  setHeaderMenu(false);
                  void refreshTree();
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
            placeholder="Search requests..."
            aria-label="Search requests"
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

      <div
        className="flex-1 min-h-0 overflow-y-auto p-1"
        onDragOver={(e) => {
          if (!dragPath.current) return;
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
        }}
        onDrop={(e) => {
          const src = e.dataTransfer.getData("text/plain") || dragPath.current;
          dragPath.current = null;
          setDropTarget(null);
          if (!src || parentOf(src) === "") return;
          e.preventDefault();
          void moveNode(src, "");
        }}
      >
        {tree.length === 0 ? (
          <EmptyState
            icon={<FolderOpen size={24} />}
            title="Collection is empty"
            hint="Create your first request"
            action={
              <Button
                variant="default"
                className="mt-1"
                onClick={() => {
                  setNewName("");
                  setCreateModal({ kind: "request", parent: "" });
                }}
              >
                New request
              </Button>
            }
          />
        ) : filter && visibleTree.length === 0 ? (
          <div className="flex h-full items-center justify-center p-6 text-center">
            <p className="text-xs text-fg-2">No matches for {query}</p>
          </div>
        ) : (
          visibleTree.map((n) => renderNode(n, 0))
        )}
      </div>

      {menu && (
        <div
          className="fixed z-50 min-w-40 rounded border border-line-0 bg-bg-2 shadow-lg py-1 text-xs"
          style={{ left: menu.x, top: menu.y }}
          onContextMenu={(e) => e.preventDefault()}
        >
          {menu.node.kind === "request" ? (
            <>
              <MenuItem label="Open" onClick={() => runMenu((n) => openRequest(n.path))} />
              <MenuItem label="Rename" onClick={() => runMenu((n) => startRename(n))} />
              <MenuItem label="Duplicate" onClick={() => runMenu((n) => duplicateRequest(n.path))} />
              <MenuItem
                label="Generate code…"
                onClick={() => runMenu((n) => openCodegen(n.path))}
              />
              <MenuItem label="Export…" onClick={() => runMenu((n) => exportRequest(n))} />
              <MenuItem label="Copy path" onClick={() => runMenu((n) => copyPath(n.path))} />
              <MenuItem label="Delete" danger onClick={() => runMenu((n) => deleteNode(n.path))} />
            </>
          ) : (
            <>
              <MenuItem
                label="New request…"
                onClick={() =>
                  runMenu((n) => {
                    setNewName("");
                    setCreateModal({ kind: "request", parent: n.path });
                  })
                }
              />
              <MenuItem
                label="New folder…"
                onClick={() =>
                  runMenu((n) => {
                    setNewName("");
                    setCreateModal({ kind: "folder", parent: n.path });
                  })
                }
              />
              {menu.node.kind === "folder" && (
                <>
                  <MenuItem
                    label="Run folder"
                    onClick={() =>
                      runMenu((n) => startRun(n.path, { recursive: true }))
                    }
                  />
                  <MenuItem
                    label="Folder settings…"
                    onClick={() => runMenu((n) => setFolderSettingsFor(n.path))}
                  />
                  <MenuItem
                    label="Export…"
                    onClick={() => runMenu((n) => exportCollection(n.path, n.name))}
                  />
                  <MenuItem label="Delete" danger onClick={() => runMenu((n) => deleteNode(n.path))} />
                </>
              )}
            </>
          )}
        </div>
      )}

      <Modal
        open={createModal !== null}
        onClose={() => setCreateModal(null)}
        title={createModal?.kind === "folder" ? "New folder" : "New request"}
        width="max-w-sm"
      >
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            submitCreate();
          }}
        >
          <TextInput
            autoFocus
            placeholder="Name"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
          />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => setCreateModal(null)}>
              Cancel
            </Button>
            <Button type="submit" variant="primary" disabled={!newName.trim()}>
              Create
            </Button>
          </div>
        </form>
      </Modal>

      <Modal
        open={importOpen}
        onClose={() => {
          if (!importBusy) setImportOpen(false);
        }}
        title="Import"
        width="max-w-lg"
      >
        <div className="flex flex-col gap-3">
          <div
            onDragEnter={(e) => {
              e.preventDefault();
              setImportDrag(true);
            }}
            onDragOver={(e) => {
              e.preventDefault();
              setImportDrag(true);
            }}
            onDragLeave={() => setImportDrag(false)}
            onDrop={(e) => {
              e.preventDefault();
              setImportDrag(false);
              const files = Array.from(e.dataTransfer.files);
              if (files.length === 0) return;
              void importDropped(files);
            }}
            className={cn(
              "rounded-md border border-dashed px-4 py-8 flex flex-col items-center text-center gap-1.5 transition-colors",
              importDrag ? "border-accent bg-accent-soft" : "border-line-0",
            )}
          >
            <FileDown size={20} className="text-fg-2" />
            <p className="text-xs text-fg-1">
              Drop file(s) to import or{" "}
              <button
                type="button"
                className="underline text-accent"
                onClick={() => importFileRef.current?.click()}
              >
                choose file(s)
              </button>
            </p>
            <p className="text-[11px] text-fg-2 max-w-sm">
              Supports OpenCollection, Postman, Insomnia, OpenAPI 3.x / Swagger 2.0, WSDL, and ZIP formats
            </p>
            <input
              ref={importFileRef}
              type="file"
              multiple
              className="hidden"
              accept=".json,.yaml,.yml,.wsdl,.zip,.xml,application/json,application/yaml,text/xml"
              onChange={(e) => {
                const list = e.target.files;
                if (!list || list.length === 0) return;
                const files = Array.from(list);
                e.target.value = "";
                void importDropped(files);
              }}
            />
          </div>
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void submitImportSource();
            }}
          >
            <TextInput
              className="flex-1"
              placeholder="Git repo, spec URL, or a curl command"
              value={importSource}
              disabled={importBusy}
              onChange={(e) => setImportSource(e.target.value)}
            />
            <Button type="submit" variant="primary" disabled={importBusy || !importSource.trim()}>
              {importBusy ? "Importing…" : "Import"}
            </Button>
          </form>
          {folders.length > 0 && (
            <div className="flex items-center gap-2">
              <span className="text-xs text-fg-2 shrink-0">Import into</span>
              <select
                className="flex-1 h-7 rounded bg-bg-2 border border-line-0 px-1.5 text-xs text-fg-0 outline-none focus:border-line-focus cursor-pointer"
                value={importFolder}
                onChange={(e) => setImportFolder(e.target.value)}
              >
                <option value="">Collection root</option>
                {folders.map((f) => (
                  <option key={f.path} value={f.path}>
                    {f.path}
                  </option>
                ))}
              </select>
            </div>
          )}
        </div>
      </Modal>

      <CollectionSettingsModal
        open={collectionSettingsOpen}
        onClose={() => setCollectionSettingsOpen(false)}
      />
      {folderSettingsFor !== null && (
        <FolderSettingsModal
          folderPath={folderSettingsFor}
          onClose={() => setFolderSettingsFor(null)}
        />
      )}
    </div>
  );
}

function isHttpUrl(value: string) {
  return /^https?:\/\//i.test(value.trim());
}

function isGitRepositoryUrl(raw: string) {
  const url = raw.trim();
  if (!url || url.includes(" ")) return false;
  if (url.startsWith("git@")) {
    const rest = url.slice(4);
    const split = rest.indexOf(":");
    if (split < 0) return false;
    const host = rest.slice(0, split);
    const path = rest.slice(split + 1);
    return host.includes(".") && path.split("/").filter(Boolean).length >= 2;
  }
  const match = url.match(/^(?:https?|ssh|git):\/\/([^/]+)\/(.+)$/i);
  if (!match) return false;
  const segments = match[2]
    .replace(/\/$/, "")
    .replace(/\.git$/, "")
    .split("/")
    .filter((part) => part && !part.includes("?"));
  if (segments.length < 2) return false;
  const last = segments[segments.length - 1];
  return !(last.includes(".") && !url.endsWith(".git"));
}

function bytesToBase64(bytes: Uint8Array) {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

async function readBrowserFiles(files: File[]) {
  const texts: string[] = [];
  for (const file of files) {
    texts.push(await file.text());
  }
  return texts;
}

function HeaderMenuItem({
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

function MenuItem({
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
        // preventDefault keeps the browser from moving focus to the button,
        // which would instantly blur (and dismiss) the inline rename input
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

export default CollectionTree;
