import { useEffect, useMemo, useRef, useState } from "react";
import { cn } from "@/utils";
import { useUndoableInput } from "@/hooks/useUndoableInput";
import { nativeEditCommands, useEditContextMenu } from "@/components/EditContextMenu";
import type { VariableSuggestion } from "@/features/request/variables";

const SOURCE_LABEL: Record<VariableSuggestion["source"], string> = {
  env: "env",
  secret: "secret",
  collection: "collection",
  folder: "folder",
};

const SOURCE_COLOR: Record<VariableSuggestion["source"], string> = {
  env: "var(--ok)",
  secret: "var(--warn)",
  collection: "var(--accent)",
  folder: "var(--info)",
};

/** Color for a `{{name}}` that matches nothing in the current scope. */
const UNKNOWN_COLOR = "var(--danger)";

interface VariableInputProps
  extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange"> {
  value: string;
  onChange: (value: string) => void;
  variables?: VariableSuggestion[];
  inputClassName?: string;
}

interface Segment {
  text: string;
  /** Present when the segment is a `{{variable}}` run. */
  source?: VariableSuggestion["source"] | "unknown";
}

/**
 * Splits a value into plain text and `{{variable}}` runs so each variable
 * can be painted in its source color (unknown names use the danger color).
 * Unclosed `{{` stays plain — it may still be mid-edit.
 */
function highlightSegments(
  value: string,
  variables: VariableSuggestion[] | undefined,
): Segment[] {
  if (!value.includes("{{")) return [{ text: value }];
  const lookup = new Map((variables ?? []).map((v) => [v.name, v.source]));
  const segments: Segment[] = [];
  const re = /\{\{([^{}]*)\}\}/g;
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = re.exec(value))) {
    if (match.index > last) segments.push({ text: value.slice(last, match.index) });
    const source = lookup.get(match[1].trim());
    segments.push({ text: match[0], source: source ?? "unknown" });
    last = match.index + match[0].length;
  }
  if (last < value.length) segments.push({ text: value.slice(last) });
  return segments;
}

/**
 * Single-line input with `{{variable}}` suggestions: typing `{{` opens a
 * filtered dropdown; ↑/↓/Enter pick, Escape dismisses. Variables in the
 * text are colored by source via a highlight layer under a transparent-text
 * input (an <input> cannot color parts of its own value).
 */
export function VariableInput({
  value,
  onChange,
  variables,
  className,
  inputClassName,
  ...props
}: VariableInputProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const highlightRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const caretRef = useRef(0);
  const undoable = useUndoableInput(value, onChange, inputRef);
  const { onContextMenu, editMenu } = useEditContextMenu(() =>
    nativeEditCommands(inputRef.current, undoable),
  );

  const options = useMemo(() => {
    if (!variables || variables.length === 0) return [];
    const q = query.toLowerCase();
    const matches = q ? variables.filter((v) => v.name.toLowerCase().includes(q)) : variables;
    return matches.slice(0, 8);
  }, [variables, query]);

  const syncQuery = (text: string, caret: number | null, end: number | null = caret) => {
    if (caret === null) return;
    // A word selection inside `{{name}}` (double-click) counts as editing that
    // name, not as a closed token.
    const selected = end !== null && end > caret ? text.slice(caret, end) : "";
    // Selecting the whole `{{name}}` is a replace, not a filter — show every
    // suggestion so Enter swaps the token even when the old name matches none.
    if (selected && /^\{\{[^{}]*\}\}$/.test(text.slice(caret - 2, end! + 2))) {
      caretRef.current = caret + selected.length;
      setQuery("");
      setIndex(0);
      setOpen(true);
      return;
    }
    const before = text.slice(0, caret);
    const match = /\{\{([^{}]*)$/.exec(before);
    if (match) {
      caretRef.current = caret;
      setQuery(match[1]);
      setIndex(0);
      setOpen(true);
    } else {
      setOpen(false);
      setQuery("");
    }
  };

  const pick = (name: string) => {
    const input = inputRef.current;
    const live = input?.selectionStart ?? null;
    // A caret of 0 usually means the input lost focus to the dropdown, not
    // that the user is at the start. Fall back to where suggestions opened.
    const caret = live && /\{\{[^{}]*$/.test(value.slice(0, live)) ? live : caretRef.current;
    const before = value.slice(0, caret);
    const match = /\{\{([^{}]*)$/.exec(before);
    if (!match) return;
    const start = caret - match[0].length;
    // Replace the whole token, including a name that continues past the caret
    // (e.g. selecting inside `{{FLOW}}` and picking another variable).
    const after = value.slice(caret).replace(/^[^{}]*\}\}?/, "");
    const insert = `{{${name}}}`;
    const next = `${value.slice(0, start)}${insert}${after}`;
    undoable.change(next);
    onChange(next);
    setOpen(false);
    requestAnimationFrame(() => {
      input?.focus();
      const pos = start + insert.length;
      input?.setSelectionRange(pos, pos);
    });
  };

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (inputRef.current?.parentElement?.contains(e.target as Node)) return;
      setOpen(false);
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [open]);

  const segments = useMemo(() => highlightSegments(value, variables), [value, variables]);

  // Keep the highlight layer aligned with the input's horizontal scroll.
  const syncScroll = () => {
    const input = inputRef.current;
    if (input && highlightRef.current) {
      highlightRef.current.style.transform = `translateX(${-input.scrollLeft}px)`;
    }
  };

  useEffect(syncScroll, [value]);

  // Font, height, and padding are shared verbatim between the input and the
  // highlight layer so the colored text lines up exactly with the caret.
  const fieldClasses = cn(
    "h-7 px-2 text-xs leading-none rounded border",
    inputClassName,
  );
  const listOpen = open && options.length > 0;

  return (
    <div className={cn("relative min-w-0", className)}>
      <div
        aria-hidden
        className={cn(
          fieldClasses,
          "absolute inset-0 flex items-center overflow-hidden border-transparent bg-bg-2 text-fg-0 pointer-events-none",
          listOpen && "rounded-b-none",
        )}
      >
        <div ref={highlightRef} className="whitespace-pre">
          {segments.map((seg, i) =>
            seg.source ? (
              <span
                key={i}
                data-var-source={seg.source}
                style={{ color: seg.source === "unknown" ? UNKNOWN_COLOR : SOURCE_COLOR[seg.source] }}
              >
                {seg.text}
              </span>
            ) : (
              <span key={i}>{seg.text}</span>
            ),
          )}
        </div>
      </div>
      <input
        ref={inputRef}
        {...props}
        className={cn(
          fieldClasses,
          "relative w-full bg-transparent border-line-0 text-transparent caret-fg-0 outline-none focus:border-line-focus placeholder:text-fg-2 [line-height:1] py-0",
          listOpen && "rounded-b-none border-line-focus",
        )}
        value={value}
        onScroll={syncScroll}
        onChange={(e) => {
          undoable.change(e.target.value);
          onChange(e.target.value);
          syncQuery(e.target.value, e.target.selectionStart);
        }}
        onClick={(e) =>
          syncQuery(value, e.currentTarget.selectionStart, e.currentTarget.selectionEnd)
        }
        onSelect={(e) =>
          syncQuery(value, e.currentTarget.selectionStart, e.currentTarget.selectionEnd)
        }
        onKeyUp={(e) => {
          if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
            syncQuery(value, e.currentTarget.selectionStart, e.currentTarget.selectionEnd);
          }
        }}
        onContextMenu={onContextMenu}
        onKeyDown={(e) => {
          if (undoable.onKeyDown(e)) {
            setOpen(false);
            return;
          }
          if (!open || options.length === 0) return;
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setIndex((i) => (i + 1) % options.length);
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setIndex((i) => (i - 1 + options.length) % options.length);
          } else if (e.key === "Enter") {
            e.preventDefault();
            pick(options[index].name);
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
      />
      {open && options.length > 0 && (
        <div className="absolute left-0 right-0 top-full z-40 rounded-b border border-line-0 border-t-0 bg-bg-3 shadow-md overflow-hidden">
          {options.map((v, i) => (
            <button
              key={v.name}
              type="button"
              onMouseDown={(e) => {
                e.preventDefault();
                pick(v.name);
              }}
              onMouseEnter={() => setIndex(i)}
              className={cn(
                "w-full h-6 px-2 flex items-center gap-2 text-xs font-mono text-left",
                i === index ? "bg-accent-soft text-fg-0" : "text-fg-1",
              )}
            >
              <span
                className="text-[9px] font-sans uppercase shrink-0 w-14"
                style={{ color: SOURCE_COLOR[v.source] }}
              >
                {SOURCE_LABEL[v.source]}
              </span>
              <span className="truncate shrink-0">{v.name}</span>
              {v.value !== undefined && v.value !== "" && (
                <span className="truncate text-fg-2" title={v.value}>
                  {v.value}
                </span>
              )}
            </button>
          ))}
        </div>
      )}
      {editMenu}
    </div>
  );
}

export default VariableInput;
