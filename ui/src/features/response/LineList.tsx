import { useEffect, useRef, useState, type ReactNode } from "react";
import { cn } from "@/utils";

/** Row height in px — matches the `h-6` row class (text-xs on a 24px line box). */
const ROW_HEIGHT = 24;
/** Rows kept mounted above and below the viewport so scrolling never shows blanks. */
const OVERSCAN = 20;
/** Approximate monospace character width in pixels for font-mono text-xs (12px). */
const CHAR_WIDTH = 7.25;

/**
 * Virtualized list of text lines for the response body. Only the rows near the
 * viewport are mounted, so a multi-thousand-line body (e.g. the long-response
 * demo request) scrolls smoothly instead of choking on thousands of DOM nodes.
 *
 * When soft wrap is disabled, rows are a fixed height and never wrap (`whitespace-pre`),
 * which keeps the window math exact; long lines scroll horizontally instead.
 * When soft wrap is enabled, lines wrap within the viewport, their heights are measured
 * dynamically, and unrendered lines are estimated to keep scrolling smooth.
 */
export default function LineList({
  lines,
  gutter,
  wrap = false,
  renderLine,
  className,
}: {
  lines: string[];
  /** Show a sticky line-number gutter. */
  gutter?: boolean;
  /** Soft wrap long lines instead of horizontal scrolling. */
  wrap?: boolean;
  renderLine: (line: string) => ReactNode;
  className?: string;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(0);
  const [viewportWidth, setViewportWidth] = useState(0);
  const measuredHeights = useRef<Map<number, number>>(new Map());

  // Clear measured heights whenever content or wrap mode changes.
  useEffect(() => {
    measuredHeights.current.clear();
  }, [lines, wrap]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const sync = () => {
      setViewport(el.clientHeight);
      setViewportWidth((prev) => {
        if (Math.abs(prev - el.clientWidth) > 5) {
          measuredHeights.current.clear();
        }
        return el.clientWidth;
      });
    };
    sync();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(sync);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // The browser may clamp scrollTop when the content underneath changes (pretty
  // toggle, wrap toggle, new response), so re-read the real position whenever lines change.
  useEffect(() => {
    setScrollTop(scrollRef.current?.scrollTop ?? 0);
  }, [lines, wrap]);

  // Available text width excluding gutter and padding.
  const gutterWidth = gutter ? 40 : 0;
  const paddingRight = 12;
  const contentWidth = Math.max(0, viewportWidth - gutterWidth - paddingRight);
  const charsPerLine =
    contentWidth > 0 ? Math.max(10, Math.floor(contentWidth / CHAR_WIDTH)) : 80;

  const getLineHeight = (idx: number): number => {
    if (!wrap) return ROW_HEIGHT;
    const measured = measuredHeights.current.get(idx);
    if (measured !== undefined && measured > 0) return measured;
    const line = lines[idx];
    if (!line) return ROW_HEIGHT;
    const count = Math.max(1, Math.ceil(line.length / charsPerLine));
    return count * ROW_HEIGHT;
  };

  let first = 0;
  let last = lines.length;
  let topSpacer = 0;
  let bottomSpacer = 0;

  if (!wrap) {
    first = Math.min(
      Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN),
      Math.max(0, lines.length - 1),
    );
    last = Math.min(
      lines.length,
      Math.ceil((scrollTop + viewport) / ROW_HEIGHT) + OVERSCAN,
    );
    topSpacer = first * ROW_HEIGHT;
    bottomSpacer = (lines.length - last) * ROW_HEIGHT;
  } else {
    let acc = 0;
    let foundFirst = false;
    let targetFirst = 0;
    let targetLast = lines.length;

    const heights = new Float32Array(lines.length);
    for (let i = 0; i < lines.length; i++) {
      const h = getLineHeight(i);
      heights[i] = h;
      if (!foundFirst && acc + h > scrollTop) {
        targetFirst = Math.max(0, i - OVERSCAN);
        foundFirst = true;
      }
      acc += h;
      if (
        foundFirst &&
        i >= targetFirst &&
        acc > scrollTop + viewport + OVERSCAN * ROW_HEIGHT &&
        targetLast === lines.length
      ) {
        targetLast = Math.min(lines.length, i + 1);
      }
    }

    if (!foundFirst && lines.length > 0) {
      targetFirst = Math.max(0, lines.length - 1 - OVERSCAN);
      targetLast = lines.length;
    }

    first = targetFirst;
    last = Math.max(first, targetLast);

    for (let i = 0; i < first; i++) {
      topSpacer += heights[i];
    }
    for (let i = last; i < lines.length; i++) {
      bottomSpacer += heights[i];
    }
  }

  return (
    <div
      ref={scrollRef}
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      className={cn(
        "min-h-0 overflow-auto font-mono text-xs leading-6 select-text",
        wrap && "overflow-x-hidden",
        className,
      )}
    >
      {topSpacer > 0 && <div style={{ height: topSpacer }} aria-hidden />}
      {lines.slice(first, last).map((line, i) => {
        const n = first + i;
        return (
          <div
            key={n}
            ref={
              wrap
                ? (el) => {
                    if (el) {
                      const h = el.offsetHeight;
                      if (h > 0) measuredHeights.current.set(n, h);
                    }
                  }
                : undefined
            }
            className={cn("flex", wrap ? "min-h-6 items-start" : "h-6")}
          >
            {gutter && (
              <span className="sticky left-0 min-w-10 shrink-0 bg-bg-1 pr-3 text-right text-fg-2 select-none">
                {n + 1}
              </span>
            )}
            <span
              className={cn(
                "pr-3 text-fg-0",
                !gutter && "pl-2.5",
                wrap ? "min-w-0 flex-1 whitespace-pre-wrap break-all" : "whitespace-pre",
              )}
            >
              {renderLine(line)}
            </span>
          </div>
        );
      })}
      {bottomSpacer > 0 && (
        <div style={{ height: bottomSpacer }} aria-hidden />
      )}
    </div>
  );
}
