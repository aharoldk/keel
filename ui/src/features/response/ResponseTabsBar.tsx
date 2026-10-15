import { useEffect, useRef, useState, type ReactNode } from "react";
import { MoreHorizontal } from "lucide-react";
import { cn } from "@/utils";
import { useOverflowTabs } from "@/components/useOverflowTabs";

export interface ResponseTabItem {
  id: string;
  label: string;
  badge?: ReactNode;
}

interface ResponseTabsBarProps {
  tabs: ResponseTabItem[];
  active: string;
  onChange: (id: string) => void;
  /** Horizontal space reserved for the overflow button, in px. */
  overflowButtonWidth?: number;
}

/**
 * Tab strip for the response viewer (Body / Headers / Tests / ...). When the
 * pane is too narrow to fit every tab, the ones that don't fit move into a
 * "..." dropdown so labels are never clipped; see `useOverflowTabs` for the
 * measurement behaviour.
 */
export default function ResponseTabsBar({
  tabs,
  active,
  onChange,
  overflowButtonWidth = 36,
}: ResponseTabsBarProps) {
  const { containerRef, measureRefs, isVisible, overflowing } = useOverflowTabs(
    tabs,
    active,
    overflowButtonWidth,
  );
  const menuRef = useRef<HTMLDivElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);

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

  const tabClass = (isActive: boolean) =>
    cn(
      "text-xs px-2 flex items-center gap-1 border-b-2 -mb-px whitespace-nowrap shrink-0 transition-colors",
      isActive
        ? "border-accent text-fg-0 font-medium"
        : "border-transparent text-fg-2 hover:text-fg-0",
    );

  return (
    <div
      ref={containerRef}
      className="relative grow self-stretch flex items-stretch min-w-0"
    >
      {/* Invisible measurement row: keeps natural tab widths regardless of what is shown. */}
      <div className="absolute inset-x-0 flex items-stretch invisible pointer-events-none select-none">
        {tabs.map((t, i) => (
          <button
            key={t.id}
            type="button"
            tabIndex={-1}
            ref={(el) => {
              measureRefs.current[i] = el;
            }}
            className={tabClass(false)}
          >
            {t.label}
            {t.badge}
          </button>
        ))}
      </div>

      <div className="ml-auto h-full flex items-stretch gap-0.5">
        {tabs.map((t, i) =>
          isVisible(i) ? (
            <button
              key={t.id}
              type="button"
              title={t.label}
              onClick={() => onChange(t.id)}
              className={tabClass(t.id === active)}
            >
              {t.label}
              {t.badge}
            </button>
          ) : null,
        )}

        {overflowing && (
          <div ref={menuRef} className="relative shrink-0 flex">
            <button
              type="button"
              title="More tabs"
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              onClick={() => setMenuOpen((open) => !open)}
              className={cn(
                "h-full w-9 flex items-center justify-center border-b-2 border-transparent -mb-px text-fg-2 hover:text-fg-0",
                menuOpen && "text-fg-0",
              )}
            >
              <MoreHorizontal size={14} />
            </button>
            {menuOpen && (
              <div className="absolute right-0 top-full z-40 min-w-32 rounded-md border border-line-0 bg-bg-1 shadow-xl py-1">
                {tabs.map((t, i) =>
                  isVisible(i) ? null : (
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
                        t.id === active
                          ? "text-fg-0 font-semibold"
                          : "text-fg-1 hover:bg-bg-hover hover:text-fg-0",
                      )}
                    >
                      {t.label}
                      {t.badge}
                    </button>
                  ),
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
