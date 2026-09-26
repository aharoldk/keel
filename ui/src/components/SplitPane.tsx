import React, { useEffect, useRef, useState } from "react";
import { cn } from "@/utils";

/** Simple draggable split. Sizes in px. Direction "vertical" stacks
 *  top/bottom (resize by height), "horizontal" places panes side by side
 *  (resize by width). */
export function SplitPane({
  top,
  bottom,
  direction = "vertical",
  initial = 340,
  min = 120,
  max = 600,
  className,
}: {
  top: React.ReactNode;
  bottom: React.ReactNode;
  direction?: "vertical" | "horizontal";
  initial?: number;
  min?: number;
  max?: number;
  className?: string;
}) {
  const [size, setSize] = useState(initial);
  const dragging = useRef(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!dragging.current || !containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const s = direction === "horizontal" ? e.clientX - rect.left : e.clientY - rect.top;
      const clamped = Math.max(min, Math.min(max, s));
      setSize(clamped);
    };
    const onUp = () => {
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [direction, min, max]);

  const horizontal = direction === "horizontal";

  return (
    <div
      ref={containerRef}
      className={cn(horizontal ? "flex flex-row min-w-0" : "flex flex-col min-h-0", className)}
    >
      <div
        style={horizontal ? { width: size } : { height: size }}
        className={cn("shrink-0 overflow-hidden", horizontal ? "min-w-0" : "min-h-0")}
      >
        {top}
      </div>
      <div
        onMouseDown={(e) => {
          e.preventDefault();
          dragging.current = true;
          document.body.style.cursor = horizontal ? "col-resize" : "row-resize";
          document.body.style.userSelect = "none";
          window.getSelection()?.removeAllRanges();
        }}
        className={cn(
          "bg-line-0 hover:bg-line-focus relative group shrink-0",
          horizontal ? "w-px cursor-col-resize" : "h-px cursor-row-resize",
        )}
      >
        <div
          className={cn(
            "absolute",
            horizontal ? "-left-1.5 -right-1.5 top-0 bottom-0" : "-top-1.5 -bottom-1.5 left-0 right-0",
          )}
        />
      </div>
      <div
        className={cn(
          "flex-1 overflow-hidden",
          horizontal ? "min-w-0" : "min-h-0",
        )}
      >
        {bottom}
      </div>
    </div>
  );
}
