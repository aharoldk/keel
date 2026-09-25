import React, { useEffect, useRef } from "react";
import { X } from "lucide-react";
import { cn } from "@/utils";
import { useUndoableInput } from "@/hooks/useUndoableInput";
import { nativeEditCommands, useEditContextMenu } from "@/components/EditContextMenu";
import { useKeel } from "@/state/store";

/* ---------- Button ---------- */

type ButtonVariant = "primary" | "default" | "ghost" | "danger";

export function Button({
  variant = "default",
  className,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant }) {
  return (
    <button
      className={cn(
        "inline-flex items-center justify-center gap-1.5 rounded px-2.5 h-7 text-xs font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed select-none",
        {
          primary:
            "bg-accent text-accent-fg hover:bg-accent-strong font-semibold",
          default:
            "bg-bg-3 text-fg-0 border border-line-0 hover:bg-bg-hover",
          ghost: "text-fg-1 hover:text-fg-0 hover:bg-bg-hover",
          danger: "bg-transparent text-danger border border-danger/40 hover:bg-danger/10",
        }[variant],
        className,
      )}
      {...props}
    />
  );
}

export function IconButton({
  className,
  title,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      title={title}
      className={cn(
        "inline-flex items-center justify-center rounded h-7 w-7 text-fg-1 hover:text-fg-0 hover:bg-bg-hover transition-colors disabled:opacity-40",
        className,
      )}
      {...props}
    />
  );
}

/* ---------- Inputs ---------- */

export function TextInput({
  className,
  value,
  onChange,
  onKeyDown,
  ...props
}: React.InputHTMLAttributes<HTMLInputElement>) {
  const ref = useRef<HTMLInputElement>(null);
  const undoable = useUndoableInput(
    typeof value === "string" ? value : "",
    (v) => {
      const el = ref.current;
      if (!el) return;
      // Set via the native setter so React re-reads the DOM value and
      // forwards the dispatched event to the consumer's onChange.
      const setter = Object.getOwnPropertyDescriptor(
        window.HTMLInputElement.prototype,
        "value",
      )?.set;
      setter?.call(el, v);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    },
    ref,
  );
  const { onContextMenu, editMenu } = useEditContextMenu(() =>
    nativeEditCommands(ref.current, undoable),
  );
  return (
    <>
      <input
        {...props}
        ref={ref}
        value={value}
        onChange={(e) => {
          undoable.change(e.target.value);
          onChange?.(e);
        }}
        onKeyDown={(e) => {
          if (undoable.onKeyDown(e)) return;
          onKeyDown?.(e);
        }}
        onContextMenu={onContextMenu}
        className={cn(
          "h-7 rounded bg-bg-2 border border-line-0 px-2 text-xs text-fg-0 outline-none focus:border-line-focus placeholder:text-fg-2",
          className,
        )}
      />
      {editMenu}
    </>
  );
}

export function TextArea({
  className,
  value,
  onChange,
  onKeyDown,
  minRows = 4,
  maxHeight = 360,
  ...props
}: React.TextareaHTMLAttributes<HTMLTextAreaElement> & {
  minRows?: number;
  /** Growth cap in px — past this the textarea scrolls. */
  maxHeight?: number;
}) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const undoable = useUndoableInput(
    typeof value === "string" ? value : "",
    (v) => {
      const el = ref.current;
      if (!el) return;
      // Set via the native setter so React re-reads the DOM value and
      // forwards the dispatched event to the consumer's onChange.
      const setter = Object.getOwnPropertyDescriptor(
        window.HTMLTextAreaElement.prototype,
        "value",
      )?.set;
      setter?.call(el, v);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    },
    ref,
  );
  const { onContextMenu, editMenu } = useEditContextMenu(() =>
    nativeEditCommands(ref.current, undoable),
  );

  // Grow/shrink to fit the content whenever the value changes.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, maxHeight)}px`;
  }, [value, maxHeight]);

  return (
    <>
      <textarea
        {...props}
        ref={ref}
        value={value}
        rows={minRows}
        onChange={(e) => {
          undoable.change(e.target.value);
          onChange?.(e);
        }}
        onKeyDown={(e) => {
          if (undoable.onKeyDown(e)) return;
          onKeyDown?.(e);
        }}
        onContextMenu={onContextMenu}
        className={cn(
          "w-full resize-none overflow-y-auto rounded bg-bg-2 border border-line-0 px-2 py-1.5 text-xs font-mono text-fg-0 outline-none focus:border-line-focus placeholder:text-fg-2",
          className,
        )}
      />
      {editMenu}
    </>
  );
}

export function Select({
  className,
  children,
  ...props
}: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      className={cn(
        "h-7 rounded bg-bg-2 border border-line-0 px-1.5 text-xs text-fg-0 outline-none focus:border-line-focus cursor-pointer",
        className,
      )}
      {...props}
    >
      {children}
    </select>
  );
}

/* ---------- Modal ---------- */

export function Modal({
  open,
  onClose,
  title,
  width = "max-w-lg",
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  width?: string;
  children: React.ReactNode;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 pt-[12vh]"
      onMouseDown={onClose}
    >
      <div
        className={cn(
          "w-full rounded-md border border-line-0 bg-bg-1 shadow-xl flex flex-col max-h-[75vh]",
          width,
        )}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 h-10 border-b border-line-0 shrink-0">
          <span className="text-sm font-semibold text-fg-0">{title}</span>
          <IconButton onClick={onClose} title="Close">
            <X size={14} />
          </IconButton>
        </div>
        <div className="p-4 overflow-y-auto">{children}</div>
      </div>
    </div>
  );
}

/* ---------- Toasts ---------- */

export function ToastHost() {
  const toasts = useKeel((s) => s.toasts);
  const dismiss = useKeel((s) => s.dismissToast);
  return (
    <div className="fixed bottom-9 left-1/2 -translate-x-1/2 z-[60] flex flex-col gap-1.5 items-center pointer-events-none">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={cn(
            "pointer-events-auto flex items-center gap-2 rounded border px-3 py-1.5 text-xs shadow-md bg-bg-2",
            t.kind === "error" && "border-danger/50 text-danger",
            t.kind === "success" && "border-ok/50 text-ok",
            t.kind === "info" && "border-line-0 text-fg-0",
          )}
          onClick={() => dismiss(t.id)}
        >
          {t.message}
        </div>
      ))}
    </div>
  );
}

/* ---------- Misc ---------- */

export function EmptyState({
  icon,
  title,
  hint,
  action,
}: {
  icon?: React.ReactNode;
  title: string;
  hint?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 h-full text-center p-6">
      {icon && <div className="text-fg-2">{icon}</div>}
      <div className="text-sm text-fg-1">{title}</div>
      {hint && <div className="text-xs text-fg-2 max-w-xs">{hint}</div>}
      {action}
    </div>
  );
}

export function Spinner({ size = 14 }: { size?: number }) {
  return (
    <span
      className="inline-block animate-spin rounded-full border-2 border-fg-2 border-t-accent"
      style={{ width: size, height: size }}
    />
  );
}

export function Divider({ vertical }: { vertical?: boolean }) {
  return vertical ? (
    <span className="w-px self-stretch bg-line-0" />
  ) : (
    <span className="h-px w-full bg-line-0" />
  );
}
