import type { ShortcutAction, Shortcuts } from "@/api/types";

export interface ShortcutDef {
  id: ShortcutAction;
  name: string;
  group: string;
  /** Default combo, stored as `mod+key` (`mod` is Ctrl or Cmd). */
  defaultCombo: string;
}

export const SHORTCUT_DEFS: ShortcutDef[] = [
  { id: "save", name: "Save", group: "Request", defaultCombo: "mod+s" },
  { id: "saveAll", name: "Save all", group: "Request", defaultCombo: "mod+shift+s" },
  { id: "send", name: "Send request", group: "Request", defaultCombo: "mod+enter" },
  { id: "duplicate", name: "Duplicate", group: "Request", defaultCombo: "mod+d" },
  { id: "rename", name: "Rename", group: "Request", defaultCombo: "mod+shift+r" },
  { id: "newRequest", name: "New request", group: "Request", defaultCombo: "mod+n" },
  { id: "codegen", name: "Generate code", group: "Request", defaultCombo: "mod+shift+c" },
  { id: "copyCurl", name: "Copy as cURL", group: "Request", defaultCombo: "mod+shift+u" },
  { id: "closeTab", name: "Close tab", group: "Tabs", defaultCombo: "mod+w" },
  { id: "closeAllTabs", name: "Close all tabs", group: "Tabs", defaultCombo: "mod+shift+w" },
  { id: "nextTab", name: "Next tab", group: "Tabs", defaultCombo: "mod+alt+arrowright" },
  { id: "prevTab", name: "Previous tab", group: "Tabs", defaultCombo: "mod+alt+arrowleft" },
  { id: "palette", name: "Open menu", group: "View", defaultCombo: "mod+k" },
  { id: "settings", name: "Open settings", group: "View", defaultCombo: "mod+comma" },
  { id: "sidebar", name: "Minimize sidebar", group: "View", defaultCombo: "mod+b" },
  { id: "menu", name: "Open project menu", group: "View", defaultCombo: "mod+shift+p" },
  { id: "console", name: "Open console", group: "View", defaultCombo: "mod+shift+j" },
  { id: "splitView", name: "Switch stacked view", group: "View", defaultCombo: "mod+shift+backslash" },
  { id: "panelCollections", name: "Collections", group: "Sidebar", defaultCombo: "mod+1" },
  { id: "panelEnvironments", name: "Environments", group: "Sidebar", defaultCombo: "mod+2" },
  { id: "panelHistory", name: "History", group: "Sidebar", defaultCombo: "mod+3" },
  { id: "panelGit", name: "Git", group: "Sidebar", defaultCombo: "mod+4" },
  { id: "tabParams", name: "Params", group: "Request section", defaultCombo: "mod+alt+1" },
  { id: "tabHeaders", name: "Headers", group: "Request section", defaultCombo: "mod+alt+2" },
  { id: "tabAuth", name: "Auth", group: "Request section", defaultCombo: "mod+alt+3" },
  { id: "tabBody", name: "Body", group: "Request section", defaultCombo: "mod+alt+4" },
  { id: "tabScripts", name: "Scripts", group: "Request section", defaultCombo: "mod+alt+5" },
  { id: "tabTests", name: "Tests", group: "Request section", defaultCombo: "mod+alt+6" },
  { id: "tabDocs", name: "Docs", group: "Request section", defaultCombo: "mod+alt+7" },
];

export const SHORTCUT_GROUPS = [...new Set(SHORTCUT_DEFS.map((d) => d.group))];

const MODIFIER_KEYS = new Set(["control", "meta", "alt", "shift"]);

export function comboFor(id: ShortcutAction, overrides?: Shortcuts): string {
  const custom = overrides?.[id]?.trim();
  if (custom) return custom.toLowerCase();
  return SHORTCUT_DEFS.find((d) => d.id === id)?.defaultCombo ?? "";
}

/** Human label, e.g. `mod+shift+s` → `Ctrl+Shift+S` (or `⌘⇧S` on macOS). */
export function formatCombo(combo: string, mac = navigator.platform.startsWith("Mac")): string {
  return combo
    .toLowerCase()
    .split("+")
    .filter(Boolean)
    .map((part) => {
      if (part === "mod") return mac ? "⌘" : "Ctrl";
      if (part === "shift") return mac ? "⇧" : "Shift";
      if (part === "alt") return mac ? "⌥" : "Alt";
      if (part === "enter") return "Enter";
      if (part === "comma") return ",";
      if (part === "backslash") return "\\";
      if (part === "arrowleft") return "←";
      if (part === "arrowright") return "→";
      if (part === "arrowup") return "↑";
      if (part === "arrowdown") return "↓";
      if (part.length === 1) return part.toUpperCase();
      return part[0].toUpperCase() + part.slice(1);
    })
    .join(mac ? "" : "+");
}

/** Turn a keydown into a stored combo, or null when it is not a bindable chord. */
export function eventToCombo(e: { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }): string | null {
  const key = e.key.toLowerCase();
  if (MODIFIER_KEYS.has(key)) return null;
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("mod");
  if (e.altKey) parts.push("alt");
  if (e.shiftKey) parts.push("shift");
  if (parts.length === 0) return null;
  parts.push(key === " " ? "space" : key);
  return parts.join("+");
}

export function matchesCombo(
  e: { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean },
  combo: string,
): boolean {
  const pressed = eventToCombo(e);
  return pressed !== null && pressed === combo.toLowerCase();
}

/** Another action already using this combo, if any. */
export function shortcutConflict(
  combo: string,
  action: ShortcutAction,
  overrides?: Shortcuts,
): ShortcutAction | null {
  const needle = combo.toLowerCase();
  for (const def of SHORTCUT_DEFS) {
    if (def.id === action) continue;
    if (comboFor(def.id, overrides) === needle) return def.id;
  }
  return null;
}
