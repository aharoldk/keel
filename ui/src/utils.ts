import { clsx, type ClassValue } from "clsx";
import type { HttpMethod } from "@/api/types";

export function cn(...inputs: ClassValue[]) {
  return clsx(inputs);
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function formatMs(ms: number): string {
  if (!Number.isFinite(ms)) return "—";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

export function formatTime(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

export function statusClass(status: number | null): string {
  if (status == null) return "text-fg-2";
  if (status < 300) return "text-ok";
  if (status < 400) return "text-info";
  if (status < 500) return "text-warn";
  return "text-danger";
}

export function methodVar(method: HttpMethod): string {
  return `var(--method-${method.toLowerCase()})`;
}

/** Simple subsequence fuzzy match. Returns a score, or -1 when no match. */
export function fuzzyScore(query: string, text: string): number {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (!q) return 0;
  let score = 0;
  let ti = 0;
  let streak = 0;
  for (let qi = 0; qi < q.length; qi++) {
    const ch = q[qi];
    const found = t.indexOf(ch, ti);
    if (found === -1) return -1;
    streak = found === ti ? streak + 1 : 0;
    score += 10 + streak * 4 + (found === 0 ? 8 : 0);
    if (found > 0 && /[^a-z0-9]/.test(t[found - 1] ?? "")) score += 4;
    ti = found + 1;
  }
  score -= Math.max(0, t.length - q.length) >> 1;
  return score;
}

export function pathBasename(path: string): string {
  const i = path.lastIndexOf("/");
  return i === -1 ? path : path.slice(i + 1);
}

export function pathDirname(path: string): string {
  const i = path.lastIndexOf("/");
  return i === -1 ? "" : path.slice(0, i);
}

/**
 * True when the event target is (or is inside) an editable element — used to
 * keep global shortcuts from firing while the user is typing.
 */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest('input, textarea, select, [contenteditable="true"], .cm-content'),
  );
}
