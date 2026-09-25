import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/api/client";
import type {
  AppSettings,
  EnvSummary,
  GitStatus,
  RequestDoc,
  RunOptions,
  RunnerEvent,
  RunnerItem,
  RunnerSummary,
  HttpMethod,
  SendResult,
  TreeNode,
  WorkspaceInfo,
} from "@/api/types";

export type SidebarPanel = "collections" | "environments" | "flow" | "history" | "git";

/** Panel shown in the main content area instead of the request editor. */
export type ContentPanel = { kind: "flow" };

export type EditorTab =
  | { kind: "request"; path: string }
  | { kind: "environment"; fileName: string }
  | { kind: "flow"; fileName: string | null };

export function envTabKey(fileName: string): string {
  return `env:${fileName}`;
}

export const FLOW_TAB_KEY = "flow";

export function tabKey(tab: EditorTab): string {
  if (tab.kind === "request") return tab.path;
  if (tab.kind === "environment") return envTabKey(tab.fileName);
  return FLOW_TAB_KEY;
}

export interface FlowRunStep {
  id: string;
  path: string;
  name: string;
  method: HttpMethod;
  stopOnFailure: boolean;
}

export interface FlowRunStepResult {
  status: "running" | "ok" | "error";
  result?: SendResult;
  error?: string;
}

export interface FlowRunState {
  name: string;
  running: boolean;
  steps: FlowRunStep[];
  results: Record<string, FlowRunStepResult>;
}

export interface Tab {
  path: string;
  doc: RequestDoc;
  saved: RequestDoc;
  loading: boolean;
  result: SendResult | null;
  error: string | null;
}

export interface Toast {
  id: number;
  message: string;
  kind: "error" | "success" | "info";
}

interface KeelState {
  autoSaveTimer: ReturnType<typeof setTimeout> | null;
  ready: boolean;
  version: string;
  workspace: WorkspaceInfo | null;
  tree: TreeNode[];
  envs: EnvSummary[];
  activeEnv: string | null;
  git: GitStatus | null;
  settings: AppSettings;
  tabs: Tab[];
  activePath: string | null;
  editorTabs: EditorTab[];
  activeEditor: string | null;
  sidebarPanel: SidebarPanel;
  sidebarOpen: boolean;
  contentPanel: ContentPanel | null;
  flowRun: FlowRunState | null;
  splitDirection: "vertical" | "horizontal";
  paletteOpen: boolean;
  settingsOpen: boolean;
  consoleOpen: boolean;
  aiOpen: boolean;
  codegenFor: string | null;
  toasts: Toast[];
  runnerOpen: boolean;
  runnerRunId: string | null;
  runnerFolder: string | null;
  runnerItems: RunnerItem[];
  runnerSummary: RunnerSummary | null;
  /** True while run_folder is in flight and the run id is not known yet. */
  runnerStarting: boolean;
  /** Bumped after a send so an open environment editor reloads current values. */
  envValuesRevision: number;

  init: () => Promise<void>;
  setSidebarPanel: (p: SidebarPanel) => void;
  setSidebarOpen: (open: boolean) => void;
  setContentPanel: (p: ContentPanel | null) => void;
  setFlowRun: (run: FlowRunState | null) => void;
  setSplitDirection: (d: "vertical" | "horizontal") => void;
  setPaletteOpen: (open: boolean) => void;
  setSettingsOpen: (open: boolean) => void;
  setConsoleOpen: (open: boolean) => void;
  setAiOpen: (open: boolean) => void;
  toast: (message: string, kind?: Toast["kind"]) => void;
  dismissToast: (id: number) => void;

  openWorkspace: (path: string) => Promise<WorkspaceInfo | null>;
  createWorkspace: (path: string, name: string) => Promise<void>;
  importPostman: (sourcePath: string, folder: string) => Promise<string[]>;
  closeWorkspace: () => Promise<void>;
  refreshAll: () => Promise<void>;
  refreshTree: () => Promise<void>;
  refreshGit: () => Promise<void>;
  loadEnvs: () => Promise<void>;
  selectEnv: (fileName: string | null) => void;

  openRequest: (path: string) => Promise<void>;
  openEnvironment: (fileName: string) => void;
  openFlow: () => void;
  closeTab: (path: string) => void;
  closeEditor: (key: string) => void;
  setActiveTab: (path: string) => void;
  setActiveEditor: (key: string) => void;
  moveTab: (from: number, to: number) => void;
  openCodegen: (path: string) => void;
  closeCodegen: () => void;
  updateDoc: (path: string, doc: RequestDoc) => void;
  saveTab: (path: string) => Promise<boolean>;
  saveAllTabs: () => Promise<boolean>;
  saveActive: () => Promise<void>;
  sendActive: () => Promise<void>;
  renameRequest: (path: string, newName: string) => Promise<string | null>;
  duplicateRequest: (path: string) => Promise<void>;
  deleteNode: (path: string) => Promise<void>;
  moveNode: (path: string, dest: string) => Promise<string | null>;
  reorderNode: (path: string, target: string, before: boolean) => Promise<string | null>;
  createRequest: (folder: string, name: string) => Promise<string | null>;
  createFolder: (parent: string, name: string) => Promise<string | null>;

  gitStage: (paths: string[] | null) => Promise<void>;
  gitUnstage: (paths: string[] | null) => Promise<void>;
  gitCommit: (message: string) => Promise<void>;
  gitInit: () => Promise<void>;
  gitCheckout: (name: string) => Promise<void>;
  gitCreateBranch: (name: string) => Promise<void>;
  gitSetRemote: (url: string) => Promise<void>;
  gitPull: () => Promise<void>;
  gitPush: () => Promise<void>;

  startRun: (folderPath: string, options?: RunOptions) => Promise<void>;
  cancelRun: () => Promise<void>;
  setRunnerOpen: (open: boolean) => void;

  saveSettings: (patch: Partial<AppSettings>) => Promise<void>;
}

let toastId = 0;

/** Events received while a run is starting (run id not known yet). */
let runnerEventBuffer: RunnerEvent[] = [];

function docEquals(a: RequestDoc | null, b: RequestDoc | null): boolean {
  if (!a || !b) return false;
  return JSON.stringify(a) === JSON.stringify(b);
}

/** True when the tab has edits not yet written to disk. */
export function tabIsDirty(t: Tab): boolean {
  return !docEquals(t.doc, t.saved);
}

type SetState = (partial: Partial<KeelState> | ((s: KeelState) => Partial<KeelState>)) => void;

function applyRunnerEvent(set: SetState, payload: RunnerEvent) {
  if (payload.kind === "item" && payload.item) {
    const item = payload.item;
    set((s) => {
      const idx = s.runnerItems.findIndex(
        (i) => i.path === item.path && (i.iteration ?? 0) === (item.iteration ?? 0),
      );
      const items = [...s.runnerItems];
      if (idx >= 0) items[idx] = item;
      else items.push(item);
      return { runnerItems: items };
    });
  } else if (payload.kind === "done") {
    set({ runnerSummary: payload.summary ?? null });
  }
}

export const useKeel = create<KeelState>((set, get) => ({
  ready: false,
  version: "",
  workspace: null,
  tree: [],
  envs: [],
  activeEnv: null,
  git: null,
  settings: {
    theme: "dark",
    requestTimeoutSec: 30,
    followRedirects: true,
    saveOnSend: false,
    editorFontSize: 13,
    lastWorkspace: null,
  },
  tabs: [],
  activePath: null,
  editorTabs: [],
  activeEditor: null,
  sidebarPanel: "collections",
  sidebarOpen: true,
  contentPanel: null,
  flowRun: null,
  splitDirection:
    localStorage.getItem("keel.splitDirection") === "horizontal"
      ? "horizontal"
      : "vertical",
  paletteOpen: false,
  settingsOpen: false,
  autoSaveTimer: null,
  consoleOpen: false,
  aiOpen: false,
  codegenFor: null,
  toasts: [],
  runnerOpen: false,
  runnerRunId: null,
  runnerFolder: null,
  runnerItems: [],
  runnerSummary: null,
  envValuesRevision: 0,
  runnerStarting: false,

  async init() {
    try {
      const settings = await api.settingsGet();
      const version = await api.appVersion();
      set({ settings, version });
      document.documentElement.dataset.theme = settings.theme ?? "dark";
      const last = settings.lastWorkspace;
      if (last) {
        try {
          const ws = await api.workspaceOpen(last);
          set({ workspace: ws });
          await get().refreshAll();
          // restore last selected env if it still exists
          const savedEnv = localStorage.getItem(`keel.env.${last}`);
          if (savedEnv) {
            const envs = get().envs;
            if (envs.some((e) => e.fileName === savedEnv)) {
              set({ activeEnv: savedEnv });
            }
          }
        } catch {
          // last workspace no longer valid — start on welcome screen
        }
      }
    } catch (e) {
      get().toast(`Startup failed: ${String(e)}`, "error");
    } finally {
      set({ ready: true });
    }
    try {
      await listen("runner://update", (event) => {
        const payload = event.payload as RunnerEvent;
        if (!payload) return;
        if (payload.runId !== get().runnerRunId) {
          // The backend spawns the run before run_folder resolves, so events
          // for a just-started run can arrive before its id is known — buffer
          // them and let startRun flush once the id is recorded.
          if (get().runnerStarting) runnerEventBuffer.push(payload);
          return;
        }
        applyRunnerEvent(set, payload);
      });
      let fsTimer: number | undefined;
      await listen("workspace://changed", () => {
        window.clearTimeout(fsTimer);
        fsTimer = window.setTimeout(() => {
          get().refreshTree();
          get().refreshGit();
        }, 250);
      });
    } catch {
      // not running inside Tauri (unit tests) — events are a no-op
    }
  },

  setSidebarPanel: (p) => set({ sidebarPanel: p, sidebarOpen: true }),
  setSidebarOpen: (open) => set({ sidebarOpen: open }),
  setContentPanel: (p) => set({ contentPanel: p }),
  setFlowRun: (run) => set({ flowRun: run }),
  setSplitDirection: (d) => {
    localStorage.setItem("keel.splitDirection", d);
    set({ splitDirection: d });
  },
  setPaletteOpen: (open) => set({ paletteOpen: open }),
  setConsoleOpen: (open) => set({ consoleOpen: open }),
  setAiOpen: (open) => set({ aiOpen: open }),
  setSettingsOpen: (open) => set({ settingsOpen: open }),

  toast(message, kind = "info") {
    const id = ++toastId;
    set((s) => ({ toasts: [...s.toasts, { id, message, kind }] }));
    setTimeout(() => get().dismissToast(id), kind === "error" ? 7000 : 4000);
  },
  dismissToast(id) {
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
  },

  async openWorkspace(path) {
    const ws = await api.workspaceOpen(path);
    set({ workspace: ws, tabs: [], activePath: null, editorTabs: [], activeEditor: null, contentPanel: null });
    const recent = [path, ...(get().settings.recentWorkspaces ?? []).filter((p) => p !== path)].slice(0, 10);
    await get().saveSettings({ lastWorkspace: path, recentWorkspaces: recent });
    await get().refreshAll();
    const savedEnv = localStorage.getItem(`keel.env.${path}`);
    if (savedEnv && get().envs.some((e) => e.fileName === savedEnv)) {
      set({ activeEnv: savedEnv });
    }
    return ws;
  },

  async createWorkspace(path, name) {
    try {
      const ws = await api.workspaceInit(path, name);
      set({ workspace: ws, tabs: [], activePath: null, editorTabs: [], activeEditor: null, contentPanel: null });
      const recent = [path, ...(get().settings.recentWorkspaces ?? []).filter((p) => p !== path)].slice(0, 10);
      await get().saveSettings({ lastWorkspace: path, recentWorkspaces: recent });
      await get().refreshAll();
      get().toast(`Workspace “${ws.name}” created`, "success");
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async importPostman(sourcePath, folder) {
    try {
      const res = await api.importPostman(sourcePath, folder);
      await get().refreshAll();
      let msg = `${res.files.length} file(s) imported, ${res.skipped} skipped`;
      if (res.warnings.length > 0) msg += ` — ${res.warnings.join("; ")}`;
      get().toast(msg, res.warnings.length > 0 ? "info" : "success");
      return res.files;
    } catch (e) {
      get().toast(String(e), "error");
      return [];
    }
  },

  async closeWorkspace() {
    try {
      await api.workspaceClose();
    } finally {
      set({ workspace: null, tree: [], envs: [], activeEnv: null, git: null, tabs: [], activePath: null, editorTabs: [], activeEditor: null, contentPanel: null });
    }
  },

  async refreshAll() {
    await Promise.all([
      get().refreshTree(),
      get().loadEnvs(),
      get().refreshGit(),
    ]);
  },

  async refreshTree() {
    if (!get().workspace) return;
    try {
      const tree = await api.loadTree();
      set({ tree });
      // close tabs whose files disappeared
      const flat = new Set<string>();
      const walk = (nodes: TreeNode[]) =>
        nodes.forEach((n) => {
          if (n.kind === "request") flat.add(n.path);
          if (n.children) walk(n.children);
        });
      walk(tree);
      set((s) => {
        // keep dirty tabs even if their file is momentarily missing from the
        // tree (e.g. a git checkout touching the file) — dropping them would
        // discard unsaved edits
        const tabs = s.tabs.filter((t) => flat.has(t.path) || !docEquals(t.doc, t.saved));
        const activePath =
          s.activePath && tabs.some((t) => t.path === s.activePath)
            ? s.activePath
            : (tabs[tabs.length - 1]?.path ?? null);
        return { tabs, activePath };
      });
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async refreshGit() {
    if (!get().workspace) return;
    try {
      const git = await api.gitStatus();
      set({ git });
    } catch {
      set({ git: null });
    }
  },

  async loadEnvs() {
    if (!get().workspace) return;
    try {
      const envs = await api.envList();
      set((s) => {
        const activeEnv =
          s.activeEnv && envs.some((e) => e.fileName === s.activeEnv)
            ? s.activeEnv
            : envs.length > 0
              ? envs[0].fileName
              : null;
        return { envs, activeEnv };
      });
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  selectEnv(fileName) {
    set({ activeEnv: fileName });
    const root = get().workspace?.root;
    if (root) localStorage.setItem(`keel.env.${root}`, fileName ?? "");
  },

  async openRequest(path) {
    const existing = get().tabs.find((t) => t.path === path);
    const key = path;
    const focus = (s: KeelState): Partial<KeelState> => {
      const editorTabs = s.editorTabs.some((t) => tabKey(t) === key)
        ? s.editorTabs
        : [...s.editorTabs, { kind: "request" as const, path }];
      return { activePath: path, activeEditor: key, editorTabs, contentPanel: null };
    };
    if (existing) {
      set(focus);
      return;
    }
    try {
      const doc = await api.requestRead(path);
      set((s) => {
        // re-check inside the updater: a concurrent openRequest for the same
        // path may have added the tab while requestRead was in flight
        if (s.tabs.some((t) => t.path === path)) return focus(s);
        return {
          tabs: [...s.tabs, { path, doc, saved: doc, loading: false, result: null, error: null }],
          ...focus(s),
        };
      });
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  openEnvironment(fileName) {
    const key = envTabKey(fileName);
    set((s) => {
      const editorTabs = s.editorTabs.some((t) => tabKey(t) === key)
        ? s.editorTabs
        : [...s.editorTabs, { kind: "environment" as const, fileName }];
      return { editorTabs, activeEditor: key, contentPanel: null };
    });
  },

  openFlow() {
    set((s) => {
      const editorTabs = s.editorTabs.some((t) => t.kind === "flow")
        ? s.editorTabs
        : [...s.editorTabs, { kind: "flow" as const, fileName: null }];
      return { editorTabs, activeEditor: FLOW_TAB_KEY, contentPanel: null };
    });
  },

  closeTab(path) {
    get().closeEditor(path);
  },

  closeEditor(key) {
    set((s) => {
      const idx = s.editorTabs.findIndex((t) => tabKey(t) === key);
      const editorTabs = s.editorTabs.filter((t) => tabKey(t) !== key);
      const tabs = s.tabs.filter((t) => t.path !== key);
      let activeEditor = s.activeEditor;
      if (activeEditor === key) {
        const neighbor = editorTabs[Math.min(idx, editorTabs.length - 1)];
        activeEditor = neighbor ? tabKey(neighbor) : null;
      }
      const active = editorTabs.find((t) => tabKey(t) === activeEditor);
      const activePath = active?.kind === "request" ? active.path : null;
      return { editorTabs, tabs, activeEditor, activePath };
    });
  },

  setActiveTab(path) {
    set({ activePath: path, activeEditor: path, contentPanel: null });
  },

  setActiveEditor(key) {
    set((s) => {
      const tab = s.editorTabs.find((t) => tabKey(t) === key);
      if (!tab) return {};
      return {
        activeEditor: key,
        contentPanel: null,
        activePath: tab.kind === "request" ? tab.path : s.activePath,
      };
    });
  },

  moveTab(from, to) {
    set((s) => {
      const editorTabs = [...s.editorTabs];
      if (from < 0 || from >= editorTabs.length || to < 0 || to >= editorTabs.length) return {};
      const [moved] = editorTabs.splice(from, 1);
      editorTabs.splice(to, 0, moved);
      const order = new Map(
        editorTabs.filter((t) => t.kind === "request").map((t, i) => [t.path, i]),
      );
      const tabs = [...s.tabs].sort(
        (a, b) => (order.get(a.path) ?? 0) - (order.get(b.path) ?? 0),
      );
      return { editorTabs, tabs };
    });
  },

  openCodegen: (path) => set({ codegenFor: path }),
  closeCodegen: () => set({ codegenFor: null }),

  updateDoc(path, doc) {
    set((s) => ({
      tabs: s.tabs.map((t) => (t.path === path ? { ...t, doc } : t)),
    }));
    const { autoSave, autoSaveInterval } = get().settings;
    if (!autoSave) return;
    const pending = get().autoSaveTimer;
    if (pending) clearTimeout(pending);
    const delay = Math.max(100, autoSaveInterval ?? 1000);
    const timer = setTimeout(() => {
      set({ autoSaveTimer: null });
      void get().saveAllTabs();
    }, delay);
    set({ autoSaveTimer: timer });
  },

  async saveTab(path) {
    const tab = get().tabs.find((t) => t.path === path);
    if (!tab) return false;
    // snapshot the doc being sent; edits made while the save is in flight
    // must keep the tab dirty
    const doc = tab.doc;
    try {
      await api.requestSave(path, doc);
      set((s) => ({
        tabs: s.tabs.map((t) =>
          t.path === path ? { ...t, saved: doc } : t,
        ),
      }));
      await Promise.all([get().refreshTree(), get().refreshGit()]);
      return true;
    } catch (e) {
      get().toast(String(e), "error");
      return false;
    }
  },

  async saveAllTabs() {
    // sequential so one failure stops the batch and the tab order stays stable
    for (const tab of get().tabs) {
      if (!tabIsDirty(tab)) continue;
      const ok = await get().saveTab(tab.path);
      if (!ok) return false;
    }
    return true;
  },

  async saveActive() {
    const p = get().activePath;
    if (p) await get().saveTab(p);
  },

  async sendActive() {
    const s = get();
    const path = s.activePath;
    if (!path) return;
    const tab = s.tabs.find((t) => t.path === path);
    if (!tab || tab.loading) return;

    if (s.settings.saveOnSend && !docEquals(tab.doc, tab.saved)) {
      // abort the send when the save failed — otherwise the backend would
      // send the stale file from disk (the error is already toasted)
      const saved = await s.saveTab(path);
      if (!saved) return;
    }
    set((st) => ({
      tabs: st.tabs.map((t) =>
        t.path === path ? { ...t, loading: true, error: null } : t,
      ),
    }));
    try {
      const result = await api.sendRequest(
        path,
        s.activeEnv,
        s.settings.saveOnSend ? null : tab.doc,
      );
      set((st) => ({
        tabs: st.tabs.map((t) =>
          t.path === path ? { ...t, loading: false, result, error: null } : t,
        ),
        envValuesRevision: st.envValuesRevision + 1,
      }));
      const failed = result.error ?? result.testResults.find((r) => !r.passed);
      if (result.error) {
        get().toast(result.error, "error");
      } else if (result.testResults.some((r) => !r.passed)) {
        get().toast("Request sent — some tests failed", "info");
      } else if (result.testResults.length > 0) {
        get().toast("Request sent — all tests passed", "success");
      } else if (failed === undefined && result.status && result.status >= 400) {
        // keep silent; status is visible in the response pane
      }
    } catch (e) {
      const msg = String(e);
      set((st) => ({
        tabs: st.tabs.map((t) =>
          t.path === path ? { ...t, loading: false, error: msg } : t,
        ),
      }));
      get().toast(msg, "error");
    }
  },

  async renameRequest(path, newName) {
    try {
      const newPath = await api.requestRename(path, newName);
      set((s) => ({
        tabs: s.tabs.map((t) => (t.path === path ? { ...t, path: newPath } : t)),
        editorTabs: s.editorTabs.map((t) =>
          t.kind === "request" && t.path === path ? { ...t, path: newPath } : t,
        ),
        activePath: s.activePath === path ? newPath : s.activePath,
        activeEditor: s.activeEditor === path ? newPath : s.activeEditor,
      }));
      await Promise.all([get().refreshTree(), get().refreshGit()]);
      return newPath;
    } catch (e) {
      get().toast(String(e), "error");
      return null;
    }
  },

  async duplicateRequest(path) {
    try {
      const newPath = await api.requestDuplicate(path);
      await Promise.all([get().refreshTree(), get().refreshGit()]);
      await get().openRequest(newPath);
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async deleteNode(path) {
    try {
      await api.nodeDelete(path);
      await Promise.all([get().refreshTree(), get().refreshGit()]);
      get().toast("Deleted", "success");
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async moveNode(path, dest) {
    try {
      const newPath = await api.nodeMove(path, dest);
      if (newPath !== path) {
        const remap = (p: string) =>
          p === path || p.startsWith(`${path}/`) ? `${newPath}${p.slice(path.length)}` : p;
        set((s) => ({
          tabs: s.tabs.map((t) => (remap(t.path) === t.path ? t : { ...t, path: remap(t.path) })),
          editorTabs: s.editorTabs.map((t) =>
            t.kind === "request" && remap(t.path) !== t.path ? { ...t, path: remap(t.path) } : t,
          ),
          activePath: s.activePath ? remap(s.activePath) : s.activePath,
          activeEditor:
            s.activeEditor && !s.activeEditor.startsWith("env:") ? remap(s.activeEditor) : s.activeEditor,
        }));
        await Promise.all([get().refreshTree(), get().refreshGit()]);
      }
      return newPath;
    } catch (e) {
      get().toast(String(e), "error");
      return null;
    }
  },

  async reorderNode(path, target, before) {
    try {
      const newPath = await api.nodeReorder(path, target, before);
      if (newPath !== path) {
        const remap = (p: string) =>
          p === path || p.startsWith(`${path}/`) ? `${newPath}${p.slice(path.length)}` : p;
        set((s) => ({
          tabs: s.tabs.map((t) => (remap(t.path) === t.path ? t : { ...t, path: remap(t.path) })),
          editorTabs: s.editorTabs.map((t) =>
            t.kind === "request" && remap(t.path) !== t.path ? { ...t, path: remap(t.path) } : t,
          ),
          activePath: s.activePath ? remap(s.activePath) : s.activePath,
          activeEditor:
            s.activeEditor && !s.activeEditor.startsWith("env:") ? remap(s.activeEditor) : s.activeEditor,
        }));
      }
      await Promise.all([get().refreshTree(), get().refreshGit()]);
      return newPath;
    } catch (e) {
      get().toast(String(e), "error");
      return null;
    }
  },

  async createRequest(folder, name) {
    try {
      const newPath = await api.requestCreate(folder, name);
      await Promise.all([get().refreshTree(), get().refreshGit()]);
      await get().openRequest(newPath);
      return newPath;
    } catch (e) {
      get().toast(String(e), "error");
      return null;
    }
  },

  async createFolder(parent, name) {
    try {
      const newPath = await api.folderCreate(parent, name);
      await Promise.all([get().refreshTree(), get().refreshGit()]);
      return newPath;
    } catch (e) {
      get().toast(String(e), "error");
      return null;
    }
  },

  async gitStage(paths) {
    try {
      await api.gitStage(paths);
      await get().refreshGit();
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async gitUnstage(paths) {
    try {
      await api.gitUnstage(paths);
      await get().refreshGit();
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async gitCommit(message) {
    try {
      await api.gitCommit(message);
      await get().refreshGit();
      get().toast("Committed", "success");
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async gitInit() {
    try {
      await api.gitInit();
      await get().refreshGit();
      get().toast("Git repository initialized", "success");
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async gitCheckout(name) {
    try {
      await api.gitCheckout(name);
      await Promise.all([get().refreshTree(), get().refreshGit()]);
      get().toast(`Switched to ${name}`, "success");
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async gitCreateBranch(name) {
    try {
      await api.gitCreateBranch(name);
      await get().refreshGit();
      get().toast(`Created ${name}`, "success");
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async gitSetRemote(url) {
    try {
      await api.gitSetRemote(url);
      await get().refreshGit();
      get().toast("Remote updated", "success");
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async gitPull() {
    try {
      const out = await api.gitPull();
      await Promise.all([get().refreshTree(), get().refreshGit()]);
      get().toast(out || "Already up to date", "success");
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async gitPush() {
    try {
      const out = await api.gitPush();
      await get().refreshGit();
      get().toast(out || "Pushed", "success");
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  async startRun(folderPath, options = {}) {
    const st = get();
    if (st.runnerRunId || st.runnerStarting) return;
    set({ runnerStarting: true });
    try {
      const tab = st.tabs.find((t) => t.path === st.activePath);
      if (tab && JSON.stringify(tab.doc) !== JSON.stringify(tab.saved)) {
        await st.saveTab(tab.path);
      }
      const runId = await api.runFolder(folderPath, st.activeEnv, {
        recursive: true,
        ...options,
      });
      set({
        runnerRunId: runId,
        runnerFolder: folderPath,
        runnerOpen: true,
        runnerItems: [],
        runnerSummary: null,
        runnerStarting: false,
      });
      // flush events that arrived before the run id was known
      const buffered = runnerEventBuffer;
      runnerEventBuffer = [];
      for (const ev of buffered) {
        if (ev.runId === runId) applyRunnerEvent(set, ev);
      }
    } catch (e) {
      runnerEventBuffer = [];
      set({ runnerStarting: false });
      get().toast(String(e), "error");
    }
  },

  async cancelRun() {
    const id = get().runnerRunId;
    if (!id) return;
    try {
      await api.runCancel(id);
    } catch (e) {
      get().toast(String(e), "error");
    }
  },

  setRunnerOpen: (open) => {
    if (!open) {
      const { runnerRunId, runnerSummary } = get();
      // closing the modal mid-run must not orphan the live run — cancel it so
      // it stops emitting events and startRun is not blocked afterwards
      if (runnerRunId && !runnerSummary) {
        void api.runCancel(runnerRunId).catch(() => {});
      }
      set({
        runnerOpen: false,
        runnerRunId: null,
        runnerFolder: null,
        runnerStarting: false,
      });
    } else set({ runnerOpen: true });
  },

  async saveSettings(patch) {
    const settings = { ...get().settings, ...patch };
    set({ settings });
    document.documentElement.dataset.theme = settings.theme ?? "dark";
    try {
      await api.settingsSet(settings);
    } catch (e) {
      get().toast(String(e), "error");
    }
  },
}));
