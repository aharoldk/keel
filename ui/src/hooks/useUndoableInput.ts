import { useEffect, useRef } from "react";

interface HistoryEntry {
  value: string;
  caret: number;
}

/** Edits closer together than this merge into a single undo step. */
const COALESCE_MS = 500;
const MAX_ENTRIES = 100;

/**
 * Undo/redo stacks for a controlled text input. Controlled inputs lose the
 * browser's native undo history whenever the value is set programmatically
 * (autocomplete pick, switching tabs, store updates), so Ctrl/Cmd+Z and
 * Ctrl+Shift+Z / Ctrl+Y are handled here instead.
 */
export function useUndoableInput(
  value: string,
  onChange: (value: string) => void,
  inputRef: React.RefObject<HTMLInputElement | HTMLTextAreaElement | null>,
) {
  const past = useRef<HistoryEntry[]>([]);
  const future = useRef<HistoryEntry[]>([]);
  /** Last value produced by this input — anything else is an external replace. */
  const emitted = useRef(value);
  /** True while an undo/redo-applied change flows back through onChange. */
  const applying = useRef(false);
  const lastEditAt = useRef(0);

  // Value replaced from outside the input (e.g. another request opened):
  // drop the stacks — undoing into a different document makes no sense.
  useEffect(() => {
    if (value !== emitted.current) {
      past.current = [];
      future.current = [];
      emitted.current = value;
    }
  }, [value]);

  const caret = () => inputRef.current?.selectionStart ?? value.length;

  /**
   * Records an edit coming from the input's own onChange. Does not forward
   * the value — the consumer's onChange already runs for the same event.
   */
  const change = (next: string) => {
    if (applying.current) return;
    const now = Date.now();
    if (now - lastEditAt.current > COALESCE_MS || past.current.length === 0) {
      past.current.push({ value, caret: caret() });
      if (past.current.length > MAX_ENTRIES) past.current.shift();
    }
    lastEditAt.current = now;
    future.current = [];
    emitted.current = next;
  };

  const jump = (entry: HistoryEntry) => {
    applying.current = true;
    emitted.current = entry.value;
    onChange(entry.value);
    lastEditAt.current = 0;
    requestAnimationFrame(() => {
      applying.current = false;
      const input = inputRef.current;
      input?.focus();
      input?.setSelectionRange(entry.caret, entry.caret);
    });
  };

  const undo = () => {
    const entry = past.current.pop();
    if (!entry) return;
    future.current.push({ value, caret: caret() });
    jump(entry);
  };

  const redo = () => {
    const entry = future.current.pop();
    if (!entry) return;
    past.current.push({ value, caret: caret() });
    jump(entry);
  };

  /** Returns true when the key was an undo/redo combo (and was handled). */
  const onKeyDown = (e: React.KeyboardEvent<HTMLElement>): boolean => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return false;
    const k = e.key.toLowerCase();
    if (k === "z" && !e.shiftKey) {
      e.preventDefault();
      undo();
      return true;
    }
    if ((k === "z" && e.shiftKey) || k === "y") {
      e.preventDefault();
      redo();
      return true;
    }
    return false;
  };

  const canUndo = () => past.current.length > 0;
  const canRedo = () => future.current.length > 0;

  return { change, onKeyDown, undo, redo, canUndo, canRedo };
}

export default useUndoableInput;
