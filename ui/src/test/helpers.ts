import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { vi } from "vitest";
import { useKeel } from "@/state/store";

export const invokeMock = vi.mocked(invoke);
export const openDialogMock = vi.mocked(openDialog);

export const defaultResponses: Record<string, unknown> = {
  workspace_info: null,
  workspace_load_tree: [],
  env_list: [],
  env_read: { schemaVersion: "1", name: "local", variables: {}, secrets: {} },
  env_values_read: {},
  secret_list: [],
  history_list: [],
  history_clear: null,
  history_pins: [],
  git_status: { hasRepo: false, branch: null, entries: [], remoteUrl: null, ahead: null, behind: null },
  git_log: [],
  git_diff_file: "",
  settings_get: {
    theme: "dark",
    requestTimeoutSec: 30,
    followRedirects: true,
    saveOnSend: false,
    editorFontSize: 13,
    lastWorkspace: null,
  },
  get_app_version: "0.1.0",
  ai_status: { configured: false },
};

export function installDefaultResponses() {
  invokeMock.mockImplementation(((cmd: string) =>
    Promise.resolve(
      structuredClone(defaultResponses[cmd] ?? null),
    )) as unknown as typeof invoke);
}

export function resetStore() {
  invokeMock.mockClear();
  openDialogMock.mockClear();
  useKeel.setState({
    ready: true,
    version: "",
    workspace: null,
    tree: [],
    envs: [],
    activeEnv: null,
    git: null,
    tabs: [],
    activePath: null,
    editorTabs: [],
    activeEditor: null,
    contentPanel: null,
    flowRun: null,
    sidebarPanel: "collections",
    sidebarOpen: true,
    paletteOpen: false,
    settingsOpen: false,
    aiOpen: false,
    toasts: [],
    settings: {
      theme: "dark",
      requestTimeoutSec: 30,
      followRedirects: true,
      saveOnSend: false,
      editorFontSize: 13,
      lastWorkspace: null,
    },
  });
  localStorage.clear();
}
