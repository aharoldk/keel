import { useEffect, useMemo, useState } from "react";
import { Cookie as CookieIcon, Trash2, X } from "lucide-react";
import { api } from "@/api/client";
import type { CookieDto } from "@/api/types";
import { Button, EmptyState, IconButton, Modal } from "@/components/ui";
import { cn } from "@/utils";
import { useKeel } from "@/state/store";

export function CookiesSection() {
  const settingsOpen = useKeel((s) => s.settingsOpen);
  const toast = useKeel((s) => s.toast);

  const [cookies, setCookies] = useState<CookieDto[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  const refetch = async () => {
    try {
      setCookies((await api.cookieList()) ?? []);
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setLoaded(true);
    }
  };

  useEffect(() => {
    if (settingsOpen) void refetch();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settingsOpen]);

  const byDomain = useMemo(() => {
    const map = new Map<string, CookieDto[]>();
    for (const c of cookies) {
      const list = map.get(c.domain) ?? [];
      list.push(c);
      map.set(c.domain, list);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [cookies]);

  const remove = async (c: CookieDto) => {
    try {
      await api.cookieDelete(c.domain, c.name);
      await refetch();
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const clearAll = async () => {
    setConfirmClear(false);
    try {
      await api.cookieClear();
      await refetch();
      toast("Cookie jar cleared", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <span className="flex-1 text-[10px] font-semibold uppercase tracking-wider text-fg-2">
          Cookies ({cookies.length})
        </span>
        <Button
          variant="ghost"
          className="text-danger hover:bg-danger/10"
          disabled={cookies.length === 0}
          onClick={() => setConfirmClear(true)}
        >
          Clear all
        </Button>
      </div>

      {!loaded ? (
        <p className="text-xs text-fg-2 py-2">Loading…</p>
      ) : cookies.length === 0 ? (
        <EmptyState
          icon={<CookieIcon size={20} />}
          title="No cookies"
          hint="Responses with Set-Cookie will appear here when cookie storage is enabled."
        />
      ) : (
        <div className="flex flex-col gap-1 max-h-48 overflow-y-auto rounded border border-line-0">
          {byDomain.map(([domain, list]) => (
            <details key={domain} open className="group">
              <summary className="h-7 px-2 flex items-center gap-1.5 text-xs text-fg-1 cursor-pointer select-none bg-bg-2 border-b border-line-0 last:border-b-0">
                <span className="font-mono truncate" title={domain}>
                  {domain}
                </span>
                <span className="ml-auto text-[10px] text-fg-2">
                  {list.length}
                </span>
              </summary>
              {list.map((c) => (
                <div
                  key={`${c.domain}${c.path}${c.name}`}
                  title={c.value}
                  className="h-7 px-2 flex items-center gap-1.5 text-xs"
                >
                  <span className="shrink-0 font-mono text-fg-0">{c.name}</span>
                  <span className="text-fg-2">=</span>
                  <span
                    className={cn(
                      "flex-1 min-w-0 truncate font-mono text-fg-1",
                    )}
                  >
                    {c.value}
                  </span>
                  <span className="shrink-0 rounded bg-bg-3 px-1 text-[9px] font-mono text-fg-2">
                    {c.path}
                  </span>
                  {c.secure && (
                    <span className="shrink-0 rounded bg-bg-3 px-1 text-[9px] text-fg-2">
                      secure
                    </span>
                  )}
                  {c.httpOnly && (
                    <span className="shrink-0 rounded bg-bg-3 px-1 text-[9px] text-fg-2">
                      httponly
                    </span>
                  )}
                  <IconButton
                    className="h-5 w-5 shrink-0 hover:text-danger"
                    title="Delete cookie"
                    onClick={() => void remove(c)}
                  >
                    <Trash2 size={11} />
                  </IconButton>
                </div>
              ))}
            </details>
          ))}
        </div>
      )}

      <Modal
        open={confirmClear}
        onClose={() => setConfirmClear(false)}
        title="Clear all cookies?"
        width="max-w-sm"
      >
        <div className="flex flex-col gap-3">
          <p className="text-xs text-fg-1">
            This removes all {cookies.length} cookies from Keel's jar. It
            cannot be undone.
          </p>
          <div className="flex justify-end gap-2">
            <Button
              variant="ghost"
              onClick={() => setConfirmClear(false)}
            >
              Cancel
            </Button>
            <Button variant="danger" onClick={() => void clearAll()}>
              <X size={13} />
              Clear all
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
}

export default CookiesSection;
