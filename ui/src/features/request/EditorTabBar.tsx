import { useEffect, useRef, useState, type ReactNode } from "react";
import { MoreHorizontal } from "lucide-react";
import { cn } from "@/utils";
import { useOverflowTabs } from "@/components/useOverflowTabs";

export interface EditorTabItem {
  id: string;
  label: string;
  badge?: number;
  icon?: ReactNode;
}

interface EditorTabBarProps {
  tabs: EditorTabItem[];
  active: string;
  onChange: (id: string) => void;
  /** Horizontal space reserved for the overflow button, in px. */
  overflowButtonWidth?: number;
}

/**
 * Tab strip for the request editor (Params / Headers / Auth / ...). Tabs that
 * do not fit move into a "..." dropdown; see `useOverflowTabs` for the
 * measurement behaviour.
 */
export default function EditorTabBar({
  tabs,
  active,
  onChange,
  overflowButtonWidth = 36,
}: EditorTabBarProps) {
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
            {t.icon}
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
              {t.icon}
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
                      {t.icon}
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
