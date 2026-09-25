import type { ShortcutAction } from "@/api/types";
import { useKeel } from "@/state/store";
import { comboFor, matchesCombo, SHORTCUT_DEFS } from "@/shortcuts";
import { isEditableTarget } from "@/utils";

const SECTION_TABS: Partial<Record<ShortcutAction, string>> = {
  tabParams: "params",
  tabHeaders: "headers",
  tabAuth: "auth",
  tabBody: "body",
  tabScripts: "scripts",
  tabTests: "tests",
  tabDocs: "docs",
};

/** Run a command. Request-section commands are events the editor listens for. */
export function runCommand(id: ShortcutAction) {
  const g = useKeel.getState();
  const path = g.activePath;
  const section = SECTION_TABS[id];
  if (section) {
    window.dispatchEvent(new CustomEvent("keel:editor-tab", { detail: section }));
    return;
  }
  switch (id) {
    case "save":
      void g.saveActive();
      break;
    case "saveAll":
      void g.saveAllTabs();
      break;
    case "send":
      void g.sendActive();
      break;
    case "duplicate":
      if (path) void g.duplicateRequest(path);
      break;
    case "rename":
      if (path) window.dispatchEvent(new CustomEvent("keel:start-rename", { detail: path }));
      break;
    case "closeTab":
      if (path) window.dispatchEvent(new CustomEvent("keel:close-tab", { detail: path }));
      break;
    case "closeAllTabs":
      window.dispatchEvent(new CustomEvent("keel:close-all-tabs"));
      break;
    case "nextTab":
    case "prevTab": {
      const tabs = g.tabs;
      const idx = tabs.findIndex((t) => t.path === path);
      if (idx < 0 || tabs.length < 2) break;
      const delta = id === "nextTab" ? 1 : -1;
      const next = tabs[(idx + delta + tabs.length) % tabs.length];
      g.setActiveTab(next.path);
      break;
    }
    case "newRequest":
      void g.createRequest("", "New Request");
      break;
    case "codegen":
      if (path) g.openCodegen(path);
      break;
    case "copyCurl":
      window.dispatchEvent(new CustomEvent("keel:copy-curl"));
      break;
    case "palette":
      g.setPaletteOpen(!g.paletteOpen);
      break;
    case "settings":
      g.setSettingsOpen(true);
      break;
    case "sidebar":
      g.setSidebarOpen(!g.sidebarOpen);
      break;
    case "menu":
      window.dispatchEvent(new CustomEvent("keel:open-menu"));
      break;
    case "console":
      g.setConsoleOpen(!g.consoleOpen);
      break;
    case "splitView":
      g.setSplitDirection(g.splitDirection === "vertical" ? "horizontal" : "vertical");
      break;
    case "panelCollections":
      g.setSidebarPanel("collections");
      break;
    case "panelEnvironments":
      g.setSidebarPanel("environments");
      break;
    case "panelHistory":
      g.setSidebarPanel("history");
      break;
    case "panelGit":
      g.setSidebarPanel("git");
      break;
  }
}

export function handleGlobalKeydown(e: KeyboardEvent) {
  if (isEditableTarget(e.target)) return;
  const shortcuts = useKeel.getState().settings.shortcuts;
  for (const def of SHORTCUT_DEFS) {
    if (!matchesCombo(e, comboFor(def.id, shortcuts))) continue;
    e.preventDefault();
    runCommand(def.id);
    return;
  }
}
