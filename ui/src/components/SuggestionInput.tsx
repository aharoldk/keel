import { useEffect, useMemo, useRef, useState } from "react";
import { cn } from "@/utils";
import { TextInput } from "./ui";

interface SuggestionInputProps
  extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange"> {
  value: string;
  onChange: (value: string) => void;
  /** Values offered in the dropdown, filtered case-insensitively by what's typed. */
  options: string[];
  /** Classes for the wrapper (layout); the inner input always fills it. */
  className?: string;
  inputClassName?: string;
}

/**
 * TextInput with a themed autocomplete dropdown: focus opens the filtered
 * list, ↑/↓ moves, Enter picks, Escape dismisses. Replaces native <datalist>,
 * whose popup ignores the app theme and shows white text on a white
 * background in the light theme inside the Tauri webview.
 */
export function SuggestionInput({
  value,
  onChange,
  options,
  className,
  inputClassName,
  onKeyDown,
  onFocus,
  ...props
}: SuggestionInputProps) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [index, setIndex] = useState(0);

  const filtered = useMemo(() => {
    const q = value.trim().toLowerCase();
    return q ? options.filter((o) => o.toLowerCase().includes(q)) : options;
  }, [options, value]);

  const listOpen = open && filtered.length > 0;

  const pick = (option: string) => {
    onChange(option);
    setOpen(false);
    setIndex(0);
  };

  // Clicking anywhere outside closes the dropdown — a mousedown on an option
  // stays inside the wrapper, so it never triggers this.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrapRef.current?.contains(e.target as Node)) return;
      setOpen(false);
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [open]);

  // Keep the highlighted row visible when arrowing through a long list.
  useEffect(() => {
    if (!listOpen) return;
    const row = listRef.current?.children[index] as HTMLElement | undefined;
    row?.scrollIntoView?.({ block: "nearest" });
  }, [listOpen, index]);

  return (
    <div ref={wrapRef} className={cn("relative min-w-0", className)}>
      <TextInput
        {...props}
        className={cn("w-full", listOpen && "rounded-b-none border-line-focus", inputClassName)}
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
          setOpen(true);
          setIndex(0);
        }}
        onFocus={(e) => {
          onFocus?.(e);
          setOpen(true);
        }}
        onKeyDown={(e) => {
          onKeyDown?.(e);
          if (!listOpen) return;
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setIndex((i) => (i + 1) % filtered.length);
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setIndex((i) => (i - 1 + filtered.length) % filtered.length);
          } else if (e.key === "Enter") {
            e.preventDefault();
            pick(filtered[index]);
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
      />
      {listOpen && (
        <div
          ref={listRef}
          role="listbox"
          className="absolute left-0 right-0 top-full z-40 max-h-48 overflow-y-auto rounded-b border border-line-0 border-t-0 bg-bg-3 shadow-md"
        >
          {filtered.map((option, i) => (
            <button
              key={option}
              type="button"
              role="option"
              aria-selected={i === index}
              // preventDefault keeps focus on the input so the dropdown can
              // reopen right after a pick.
              onMouseDown={(e) => {
                e.preventDefault();
                pick(option);
              }}
              onMouseEnter={() => setIndex(i)}
              className={cn(
                "w-full h-6 px-2 flex items-center truncate text-xs text-left",
                i === index ? "bg-accent-soft text-fg-0" : "text-fg-1",
              )}
            >
              {option}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export default SuggestionInput;
