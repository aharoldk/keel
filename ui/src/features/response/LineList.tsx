import { useEffect, useRef, useState, type ReactNode } from "react";
import { cn } from "@/utils";

/** Row height in px — matches the `h-6` row class (text-xs on a 24px line box). */
const ROW_HEIGHT = 24;
/** Rows kept mounted above and below the viewport so scrolling never shows blanks. */
const OVERSCAN = 20;

/**
 * Virtualized list of text lines for the response body. Only the rows near the
 * viewport are mounted, so a multi-thousand-line body (e.g. the long-response
 * demo request) scrolls smoothly instead of choking on thousands of DOM nodes.
 *
 * Rows are a fixed height and never wrap (`whitespace-pre`), which keeps the
 * window math exact; long lines scroll horizontally instead.
 */
export default function LineList({
  lines,
  gutter,
  renderLine,
  className,
}: {
  lines: string[];
  /** Show a sticky line-number gutter. */
  gutter?: boolean;
  renderLine: (line: string) => ReactNode;
  className?: string;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(0);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const sync = () => setViewport(el.clientHeight);
    sync();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(sync);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // The browser may clamp scrollTop when the content underneath changes (pretty
  // toggle, new response), so re-read the real position whenever lines change.
  useEffect(() => {
    setScrollTop(scrollRef.current?.scrollTop ?? 0);
  }, [lines]);

  // Clamp the window start so a stale scrollTop (the browser may not have
  // clamped it yet when new lines arrive) can never leave the viewport blank.
  const first = Math.min(
    Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN),
    Math.max(0, lines.length - 1),
  );
  const last = Math.min(
    lines.length,
    Math.ceil((scrollTop + viewport) / ROW_HEIGHT) + OVERSCAN,
  );

  return (
    <div
      ref={scrollRef}
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      className={cn("min-h-0 overflow-auto font-mono text-xs leading-6 select-text", className)}
    >
      {first > 0 && <div style={{ height: first * ROW_HEIGHT }} aria-hidden />}
      {lines.slice(first, last).map((line, i) => {
        const n = first + i;
        return (
          <div key={n} className="flex h-6">
            {gutter && (
              <span className="sticky left-0 w-10 shrink-0 bg-bg-1 pr-3 text-right text-fg-2 select-none">
                {n + 1}
              </span>
            )}
            <span className="whitespace-pre pr-3 text-fg-0">{renderLine(line)}</span>
          </div>
        );
      })}
      {last < lines.length && (
        <div style={{ height: (lines.length - last) * ROW_HEIGHT }} aria-hidden />
      )}
    </div>
  );
}
