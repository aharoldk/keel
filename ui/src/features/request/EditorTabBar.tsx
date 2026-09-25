import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { MoreHorizontal } from "lucide-react";
import { cn } from "@/utils";

export interface EditorTabItem {
  id: string;
  label: string;
  badge?: number;
}

interface EditorTabBarProps {
  tabs: EditorTabItem[];
  active: string;
  onChange: (id: string) => void;
  /** Horizontal space reserved for the overflow button, in px. */
  overflowButtonWidth?: number;
}

/**
 * Tab strip for the request editor (Params / Headers / Auth / ...). When the
 * pane is too narrow to fit every tab, the ones that don't fit move into a
 * "..." dropdown so labels are never clipped. The active tab always stays on
 * screen: if it would overflow it takes the slot of the last fitting tab.
 *
 * Tab widths come from an invisible measurement row that mirrors the visible
 * one, so hiding a tab never changes what we measure for the others.
 */
export default function EditorTabBar({
  tabs,
  active,
  onChange,
  overflowButtonWidth = 36,
}: EditorTabBarProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const measureRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const menuRef = useRef<HTMLDivElement>(null);
  const [visibleCount, setVisibleCount] = useState(tabs.length);
  const [forcedActive, setForcedActive] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);

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

  // Close the overflow menu on Escape, blur, or a click outside of it.
  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (menuRef.current?.contains(e.target as Node)) return;
      setMenuOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenuOpen(false);
    };
    const onBlur = () => setMenuOpen(false);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", onBlur);
    };
  }, [menuOpen]);

  const isVisible = (i: number) => i < visibleCount || (forcedActive && i === activeIndex);

  return (
    <div ref={containerRef} className="relative h-8 border-b border-line-0 shrink-0">
      {/* Invisible measurement row: keeps natural tab widths regardless of what is shown. */}
      <div className="absolute inset-x-0 flex items-stretch px-1 invisible pointer-events-none select-none">
        {tabs.map((t, i) => (
          <button
            key={t.id}
            type="button"
            tabIndex={-1}
            ref={(el) => {
              measureRefs.current[i] = el;
            }}
            className="text-xs px-3 flex items-center gap-1.5 whitespace-nowrap shrink-0"
          >
            {t.label}
            {t.badge != null && t.badge > 0 && <span className="text-[9px]">{t.badge}</span>}
          </button>
        ))}
      </div>

      <div className="h-full flex items-stretch px-1">
        {tabs.map((t, i) => {
          if (!isVisible(i)) return null;
          const isActive = t.id === active;
          return (
            <button
              key={t.id}
              type="button"
              title={t.label}
              onClick={() => onChange(t.id)}
              className={cn(
                "text-xs px-3 flex items-center gap-1.5 border-b-2 whitespace-nowrap shrink-0",
                isActive
                  ? "border-accent text-fg-0"
                  : "border-transparent text-fg-2 hover:text-fg-0",
              )}
            >
              {t.label}
              {t.badge != null && t.badge > 0 && (
                <span className="text-[9px] text-fg-2">{t.badge}</span>
              )}
            </button>
          );
        })}

        {overflowing && (
          <div ref={menuRef} className="relative shrink-0">
            <button
              type="button"
              title="More tabs"
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              onClick={() => setMenuOpen((open) => !open)}
              className={cn(
                "h-full w-9 flex items-center justify-center border-b-2 border-transparent text-fg-2 hover:text-fg-0",
                menuOpen && "text-fg-0",
              )}
            >
              <MoreHorizontal size={14} />
            </button>
            {menuOpen && (
              <div className="absolute right-0 top-full z-40 min-w-32 rounded-md border border-line-0 bg-bg-1 shadow-xl py-1">
                {tabs.map((t, i) => {
                  if (isVisible(i)) return null;
                  const isActive = t.id === active;
                  return (
                    <button
                      key={t.id}
                      type="button"
                      title={t.label}
                      onClick={() => {
                        setMenuOpen(false);
                        onChange(t.id);
                      }}
                      className={cn(
                        "flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs whitespace-nowrap",
                        isActive
                          ? "text-fg-0 font-semibold"
                          : "text-fg-1 hover:bg-bg-hover hover:text-fg-0",
                      )}
                    >
                      {t.label}
                      {t.badge != null && t.badge > 0 && (
                        <span className="text-[9px] text-fg-2">{t.badge}</span>
                      )}
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
