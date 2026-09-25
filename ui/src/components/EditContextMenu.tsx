import { useEffect, useRef, useState } from "react";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { cn } from "@/utils";

/**
 * Commands backing the right-click edit menu. `canUndo`/`canRedo`/
 * `hasSelection`/`editable` are snapshots taken when the menu opens — they
 * only gate disabled state, so staleness between open and click is harmless.
 */
export interface EditCommands {
  /** False for read-only viewers — hides Undo/Redo/Cut/Paste. */
  editable?: boolean;
  hasSelection?: boolean;
  canUndo?: boolean;
  canRedo?: boolean;
  undo?: () => void;
  redo?: () => void;
  cut?: () => void;
  copy?: () => void;
  paste?: () => void;
  selectAll?: () => void;
}

const IS_MAC = navigator.platform.toUpperCase().includes("MAC");

const SHORTCUT = {
  undo: IS_MAC ? "⌘Z" : "Ctrl+Z",
  redo: IS_MAC ? "⇧⌘Z" : "Ctrl+Y",
  cut: IS_MAC ? "⌘X" : "Ctrl+X",
  copy: IS_MAC ? "⌘C" : "Ctrl+C",
  paste: IS_MAC ? "⌘V" : "Ctrl+V",
  selectAll: IS_MAC ? "⌘A" : "Ctrl+A",
};

interface MenuState {
  x: number;
  y: number;
  commands: EditCommands;
}

interface Item {
  label: string;
  shortcut?: string;
  disabled?: boolean;
  run?: () => void;
  separator?: boolean;
}

function buildItems(c: EditCommands): Item[] {
  const items: Item[] = [];
  const editable = c.editable !== false;
  if (editable) {
    items.push(
      { label: "Undo", shortcut: SHORTCUT.undo, disabled: !c.canUndo, run: c.undo },
      { label: "Redo", shortcut: SHORTCUT.redo, disabled: !c.canRedo, run: c.redo },
      { label: "", separator: true },
      { label: "Cut", shortcut: SHORTCUT.cut, disabled: !c.hasSelection, run: c.cut },
    );
  }
  items.push({ label: "Copy", shortcut: SHORTCUT.copy, disabled: !c.hasSelection, run: c.copy });
  if (editable) items.push({ label: "Paste", shortcut: SHORTCUT.paste, run: c.paste });
  items.push(
    { label: "", separator: true },
    { label: "Select All", shortcut: SHORTCUT.selectAll, run: c.selectAll },
  );
  return items;
}

function EditMenu({ menu, onClose }: { menu: MenuState; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current?.contains(e.target as Node)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);

  const items = buildItems(menu.commands);
  // Clamp so the menu never opens past the viewport edge.
  const width = 192;
  const height = items.length * 26 + 8;
  const left = Math.max(0, Math.min(menu.x, window.innerWidth - width - 4));
  const top = Math.max(0, Math.min(menu.y, window.innerHeight - height - 4));

  return (
    <div
      ref={ref}
      className="fixed z-50 min-w-48 rounded border border-line-0 bg-bg-2 shadow-lg py-1 text-xs"
      style={{ left, top }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item, i) =>
        item.separator ? (
          <div key={i} className="my-1 h-px bg-line-0" />
        ) : (
          <button
            key={i}
            type="button"
            disabled={item.disabled}
            onMouseDown={(e) => {
              // Keep focus in the editor so the command sees the selection.
              e.preventDefault();
              e.stopPropagation();
            }}
            onClick={() => {
              onClose();
              item.run?.();
            }}
            className={cn(
              "flex w-full items-center justify-between gap-6 px-3 py-1.5 text-left",
              "text-fg-1 hover:bg-bg-hover hover:text-fg-0 disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-fg-1",
            )}
          >
            <span>{item.label}</span>
            {item.shortcut && <span className="text-[10px] text-fg-2">{item.shortcut}</span>}
          </button>
        ),
      )}
    </div>
  );
}

/**
 * Right-click edit menu for a text editor. Returns a handler to attach to the
 * editor (or its wrapper) and the menu element to render next to it.
 * `getCommands` is called when the menu opens, so state like selection and
 * undo depth is read at the moment it matters.
 */
export function useEditContextMenu(getCommands: () => EditCommands) {
  const [menu, setMenu] = useState<MenuState | null>(null);
  const onContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setMenu({ x: e.clientX, y: e.clientY, commands: getCommands() });
  };
  const editMenu = menu ? <EditMenu menu={menu} onClose={() => setMenu(null)} /> : null;
  return { onContextMenu, editMenu };
}

export interface NativeUndoable {
  undo: () => void;
  redo: () => void;
  canUndo: () => boolean;
  canRedo: () => boolean;
}

/**
 * Builds {@link EditCommands} for a plain `<input>`/`<textarea>`. Edits are
 * applied through the native value setter + an `input` event so React's
 * controlled onChange (and the undo history in useUndoableInput) records them.
 */
export function nativeEditCommands(
  el: HTMLInputElement | HTMLTextAreaElement | null,
  undoable: NativeUndoable,
): EditCommands {
  if (!el) return { editable: false };
  const start = el.selectionStart ?? 0;
  const end = el.selectionEnd ?? start;
  const editable = !el.disabled && !el.readOnly;

  const applyEdit = (text: string) => {
    const proto =
      el instanceof HTMLTextAreaElement
        ? window.HTMLTextAreaElement.prototype
        : window.HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
    setter?.call(el, el.value.slice(0, start) + text + el.value.slice(end));
    el.dispatchEvent(new Event("input", { bubbles: true }));
    requestAnimationFrame(() => {
      el.focus();
      const caret = start + text.length;
      el.setSelectionRange(caret, caret);
    });
  };

  const copySelection = () => {
    if (start !== end) void writeText(el.value.slice(start, end)).catch(() => {});
  };

  return {
    editable,
    hasSelection: start !== end,
    canUndo: undoable.canUndo(),
    canRedo: undoable.canRedo(),
    undo: undoable.undo,
    redo: undoable.redo,
    cut: () => {
      copySelection();
      applyEdit("");
    },
    copy: copySelection,
    paste: () => {
      void readText()
        .then((t) => {
          if (t) applyEdit(t);
        })
        .catch(() => {});
    },
    selectAll: () => {
      el.focus();
      el.setSelectionRange(0, el.value.length);
    },
  };
}
