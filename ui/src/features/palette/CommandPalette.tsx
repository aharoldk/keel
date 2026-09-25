import { useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { HttpMethod, ShortcutAction, TreeNode } from "@/api/types";
import { runCommand } from "@/commands";
import { comboFor, formatCombo, SHORTCUT_DEFS } from "@/shortcuts";
import { cn, fuzzyScore, methodVar } from "@/utils";
import { useKeel } from "@/state/store";

interface RequestItem {
  kind: "request";
  label: string;
  hint: string;
  method?: HttpMethod;
  path: string;
}

interface CommandItem {
  kind: "command";
  label: string;
  hint?: string;
  run: () => void | Promise<void>;
}

type PaletteItem = RequestItem | CommandItem;

function flattenRequests(nodes: TreeNode[]): TreeNode[] {
  return nodes.flatMap((n) =>
    n.kind === "request" ? [n] : n.children ? flattenRequests(n.children) : [],
  );
}

export function CommandPalette() {
  const paletteOpen = useKeel((s) => s.paletteOpen);
  const setPaletteOpen = useKeel((s) => s.setPaletteOpen);
  const tree = useKeel((s) => s.tree);
  const envs = useKeel((s) => s.envs);
  const shortcuts = useKeel((s) => s.settings.shortcuts);

  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  const close = () => {
    setPaletteOpen(false);
    setQuery("");
    setActive(0);
  };

  const runItem = (item: PaletteItem | undefined) => {
    if (!item) return;
    close();
    if (item.kind === "request") void useKeel.getState().openRequest(item.path);
    else void item.run();
  };

  const requests: RequestItem[] = useMemo(
    () =>
      flattenRequests(tree).map((n) => ({
        kind: "request" as const,
        label: n.name,
        hint: n.path,
        method: n.method,
        path: n.path,
      })),
    [tree],
  );

  const commands: CommandItem[] = useMemo(() => {
    const g = () => useKeel.getState();
    const bound: CommandItem[] = SHORTCUT_DEFS.map((def) => ({
      kind: "command" as const,
      label: def.name,
      hint: formatCombo(comboFor(def.id, shortcuts)),
      run: () => runCommand(def.id as ShortcutAction),
    }));
    const base: CommandItem[] = [
      ...bound,
      {
        kind: "command",
        label: "Import Postman collection or environment…",
        run: async () => {
          const picked = await openDialog({
            multiple: false,
            title: "Select Postman export (collection or environment)",
            filters: [{ name: "Postman (JSON)", extensions: ["json"] }],
          });
          if (picked) await g().importPostman(picked, "");
        },
      },
      { kind: "command", label: "Close workspace", run: () => void g().closeWorkspace() },
      {
        kind: "command",
        label: "Open workspace folder…",
        run: async () => {
          const picked = await openDialog({ directory: true });
          if (picked) {
            try {
              await g().openWorkspace(picked);
            } catch (e) {
              g().toast(String(e), "error");
            }
          }
        },
      },
    ];
    const envCmds: CommandItem[] = envs.map((env) => ({
      kind: "command" as const,
      label: `Environment: ${env.name}`,
      run: () => g().selectEnv(env.fileName),
    }));
    return [...base, ...envCmds, { kind: "command", label: "No environment", run: () => g().selectEnv(null) }];
  }, [envs, shortcuts]);

  const q = query.trim();

  const requestMatches = useMemo(() => {
    const scored = requests
      .map((r) => ({ r, score: fuzzyScore(q, `${r.label} ${r.path}`) }))
      .filter((x) => x.score >= 0);
    scored.sort((a, b) => b.score - a.score);
    return scored.map((x) => x.r);
  }, [requests, q]);

  const commandMatches = useMemo(() => {
    const scored = commands
      .map((c) => ({ c, score: fuzzyScore(q, c.label) }))
      .filter((x) => x.score >= 0);
    scored.sort((a, b) => b.score - a.score);
    return scored.map((x) => x.c);
  }, [commands, q]);

  const shownRequests = requestMatches.slice(0, 50);
  const shownCommands = commandMatches.slice(0, Math.max(0, 50 - shownRequests.length));
  const total = shownRequests.length + shownCommands.length;

  useEffect(() => {
    if (paletteOpen) {
      setQuery("");
      setActive(0);
    }
  }, [paletteOpen]);

  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(`[data-idx="${active}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [active, query, paletteOpen]);

  if (!paletteOpen) return null;

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(i + 1, Math.max(0, total - 1)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const idx = active;
      const item: PaletteItem | undefined = idx < shownRequests.length
        ? shownRequests[idx]
        : shownCommands[idx - shownRequests.length];
      runItem(item);
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  };

  const requestRow = (r: RequestItem, idx: number) => (
    <button
      key={r.path}
      type="button"
      data-idx={idx}
      onMouseDown={(e) => {
        e.preventDefault();
        runItem(r);
      }}
      className={cn(
        "w-full h-8 px-3 flex items-center gap-2.5 text-left",
        idx === active ? "bg-accent-soft" : "hover:bg-bg-hover",
      )}
    >
      <span
        className="w-10 shrink-0 text-center font-mono text-[10px] font-bold"
        style={{ color: methodVar(r.method ?? "GET") }}
      >
        {r.method}
      </span>
      <span className="text-xs text-fg-0 truncate">{r.label}</span>
      <span className="ml-auto shrink-0 max-w-[50%] truncate text-[10px] text-fg-2">{r.hint}</span>
    </button>
  );

  const commandRow = (c: CommandItem, idx: number) => (
    <button
      key={c.label}
      type="button"
      data-idx={idx}
      onMouseDown={(e) => {
        e.preventDefault();
        runItem(c);
      }}
      className={cn(
        "w-full h-8 px-3 flex items-center gap-2.5 text-left",
        idx === active ? "bg-accent-soft" : "hover:bg-bg-hover",
      )}
    >
      <span className="w-10 shrink-0" />
      <span className="text-xs text-fg-0 truncate">{c.label}</span>
      {c.hint && (
        <span className="ml-auto shrink-0 font-mono text-[10px] text-fg-2">{c.hint}</span>
      )}
    </button>
  );

  const bothGroups = shownRequests.length > 0 && shownCommands.length > 0;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/40"
      onClick={close}
      onContextMenu={(e) => e.preventDefault()}
    >
      <div
        className="fixed top-[12vh] left-1/2 -translate-x-1/2 w-[560px] max-w-[90vw] bg-bg-1 border border-line-0 rounded-md shadow-xl overflow-hidden"
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => e.stopPropagation()}
      >
        <input
          autoFocus
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
          placeholder="Search requests and commands…"
          className="w-full h-10 px-3 text-sm text-fg-0 bg-bg-2 outline-none border-b border-line-0 placeholder:text-fg-2"
        />
        <div ref={listRef} className="max-h-[50vh] overflow-y-auto py-1">
          {total === 0 && (
            <p className="px-3 py-6 text-center text-xs text-fg-2">No matches</p>
          )}
          {bothGroups && (
            <div className="px-3 pt-1.5 pb-0.5 text-[10px] font-semibold uppercase tracking-wider text-fg-2">
              Requests
            </div>
          )}
          {shownRequests.map((r, i) => requestRow(r, i))}
          {bothGroups && (
            <div className="px-3 pt-1.5 pb-0.5 text-[10px] font-semibold uppercase tracking-wider text-fg-2">
              Commands
            </div>
          )}
          {shownCommands.map((c, i) => commandRow(c, shownRequests.length + i))}
        </div>
      </div>
    </div>
  );
}

export default CommandPalette;
