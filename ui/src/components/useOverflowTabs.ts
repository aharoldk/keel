import { useLayoutEffect, useRef, useState } from "react";

interface OverflowTab {
  id: string;
}

/**
 * Measures a row of tabs against its container width and reports which ones
 * fit; the rest are meant to move into a "..." overflow menu so labels are
 * never clipped. The active tab always stays on screen: if it would overflow
 * it takes the slot of the last fitting tab.
 *
 * Callers render two rows inside `containerRef`: an invisible measurement row
 * whose buttons get refs stored in `measureRefs`, plus the visible row. Hiding
 * a tab then never changes the widths measured for the others.
 */
export function useOverflowTabs(
  tabs: readonly OverflowTab[],
  active: string,
  overflowButtonWidth = 36,
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const measureRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [visibleCount, setVisibleCount] = useState(tabs.length);
  const [forcedActive, setForcedActive] = useState(false);

  const activeIndex = tabs.findIndex((t) => t.id === active);
  const overflowing = visibleCount < tabs.length;

  const measure = () => {
    const container = containerRef.current;
    if (!container) return;
    const style = getComputedStyle(container);
    const avail =
      container.clientWidth -
      parseFloat(style.paddingLeft || "0") -
      parseFloat(style.paddingRight || "0");
    const widths = measureRefs.current.map((el) => (el ? el.offsetWidth : 0));
    const total = widths.reduce((a, b) => a + b, 0);

    let count = tabs.length;
    let forced = false;
    if (total > avail) {
      count = 0;
      let acc = 0;
      for (let i = 0; i < tabs.length; i++) {
        if (acc + widths[i] + overflowButtonWidth > avail) break;
        acc += widths[i];
        count = i + 1;
      }
      if (activeIndex >= count) {
        forced = true;
        while (count > 0 && acc + widths[activeIndex] + overflowButtonWidth > avail) {
          count -= 1;
          acc -= widths[count];
        }
      }
    }

    setVisibleCount((prev) => (prev === count ? prev : count));
    setForcedActive((prev) => (prev === forced ? prev : forced));
  };

  const measureRef = useRef(measure);
  measureRef.current = measure;

  // Re-measure on every render: badges, the active tab and labels all change
  // what fits.
  useLayoutEffect(() => {
    measureRef.current();
  });

  // Re-measure when the pane is resized (splitter drag, window resize).
  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => measureRef.current());
    ro.observe(container);
    return () => ro.disconnect();
  }, []);

  const isVisible = (i: number) => i < visibleCount || (forcedActive && i === activeIndex);

  return { containerRef, measureRefs, isVisible, overflowing };
}
