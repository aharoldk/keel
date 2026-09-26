import { useEffect, useState, type ReactNode } from "react";
import {
  ArrowDown,
  ArrowUp,
  ChevronRight,
  GitBranch,
  GitCommitHorizontal,
  Minus,
  Plus,
  RefreshCw,
} from "lucide-react";
import { api } from "@/api/client";
import type { GitCommit, GitEntryStatus } from "@/api/types";
import { Button, EmptyState, IconButton, Modal, Spinner } from "@/components/ui";
import { cn, formatTime } from "@/utils";
import { useKeel } from "@/state/store";

const STATUS_STYLE: Record<GitEntryStatus, { letter: string; cls: string }> = {
  modified: { letter: "M", cls: "text-warn" },
  added: { letter: "A", cls: "text-ok" },
  deleted: { letter: "D", cls: "text-danger" },
  renamed: { letter: "R", cls: "text-info" },
  untracked: { letter: "??", cls: "text-fg-2" },
  conflicted: { letter: "C", cls: "text-danger" },
};

export function GitPanel() {
  const git = useKeel((s) => s.git);
  const gitStage = useKeel((s) => s.gitStage);
  const gitUnstage = useKeel((s) => s.gitUnstage);
  const gitCommit = useKeel((s) => s.gitCommit);
  const gitInit = useKeel((s) => s.gitInit);
  const gitCheckout = useKeel((s) => s.gitCheckout);
  const gitCreateBranch = useKeel((s) => s.gitCreateBranch);
  const gitSetRemote = useKeel((s) => s.gitSetRemote);
  const gitPull = useKeel((s) => s.gitPull);
  const gitPush = useKeel((s) => s.gitPush);
  const refreshGit = useKeel((s) => s.refreshGit);
  const openGitDiff = useKeel((s) => s.openGitDiff);
  const toast = useKeel((s) => s.toast);

  const [message, setMessage] = useState("");
  const [commits, setCommits] = useState<GitCommit[]>([]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [branches, setBranches] = useState<string[]>([]);
  const [branchOpen, setBranchOpen] = useState(false);
  const [newBranch, setNewBranch] = useState("");
  const [remoteOpen, setRemoteOpen] = useState(false);
  const [remoteUrl, setRemoteUrl] = useState("");
  const [busy, setBusy] = useState<"pull" | "push" | null>(null);

  const loadCommits = async () => {
    try {
      setCommits((await api.gitLog(10)) ?? []);
    } catch {
      setCommits([]);
    }
  };

  useEffect(() => {
    if (git?.hasRepo) {
      loadCommits();
      api
        .gitBranches()
        .then((b) => setBranches(Array.isArray(b) ? b : []))
        .catch(() => setBranches([]));
    } else {
      setCommits([]);
      setBranches([]);
    }
  }, [git]);

  if (git == null) {
    return (
      <div className="flex-1 min-h-0 flex items-center justify-center">
        <Spinner size={18} />
      </div>
    );
  }

  if (!git.hasRepo) {
    return (
      <div className="flex-1 min-h-0 flex flex-col">
        <div className="h-9 shrink-0 border-b border-line-0 flex items-center px-2.5 gap-2 text-xs font-semibold uppercase tracking-wider text-fg-2">
          <span className="flex-1">Git</span>
        </div>
        <div className="flex-1 min-h-0">
          <EmptyState
            icon={<GitBranch size={24} />}
            title="No Git repository"
            hint="Initialize one to track your collection in version control"
            action={
              <Button variant="default" className="mt-1" onClick={() => gitInit()}>
                Initialize repository
              </Button>
            }
          />
        </div>
      </div>
    );
  }

  const staged = git.entries.filter((e) => e.staged);
  const unstaged = git.entries.filter((e) => !e.staged);

  const toggleSection = (id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const openDiff = (path: string) => {
    const title = path.split("/").pop() || path;
    openGitDiff(`file:${path}`, title, null);
    void api
      .gitDiffFile(path)
      .then((text) => openGitDiff(`file:${path}`, title, text))
      .catch((err) => toast(String(err), "error"));
  };

  const openCommit = (commit: GitCommit) => {
    const title = commit.shortOid;
    openGitDiff(`commit:${commit.oid}`, title, null);
    void api
      .gitDiffCommit(commit.oid)
      .then((text) => openGitDiff(`commit:${commit.oid}`, title, text))
      .catch((err) => toast(String(err), "error"));
  };

  const doCommit = async () => {
    if (!message.trim() || staged.length === 0) return;
    const msg = message;
    setMessage("");
    await gitCommit(msg);
  };

  const resolveConflict = async (path: string, side: "ours" | "theirs") => {
    try {
      await api.gitResolve(path, side);
      await refreshGit();
      toast(`Resolved ${path} with the ${side} side`, "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const renderEntry = (stagedFlag: boolean) => (entry: NonNullable<typeof git>["entries"][number]) => {
    const st = STATUS_STYLE[entry.status];
    if (entry.status === "conflicted") {
      return (
        <div
          key={`${entry.path}-C`}
          title={`${entry.path} — conflicted; pick a side to resolve`}
          onClick={() => openDiff(entry.path)}
          className="group h-7 px-2 flex items-center gap-1.5 rounded cursor-pointer select-none hover:bg-bg-hover"
        >
          <span className={cn("w-5 shrink-0 text-center font-mono text-[10px] font-bold", st.cls)}>
            {st.letter}
          </span>
          <span className="flex-1 min-w-0 truncate text-xs text-fg-1">{entry.path}</span>
          <button
            type="button"
            title="Write the ours side and stage it"
            onClick={(e) => {
              e.stopPropagation();
              void resolveConflict(entry.path, "ours");
            }}
            className="h-5 shrink-0 inline-flex items-center justify-center rounded px-1 text-[10px] text-fg-2 hover:text-fg-0 hover:bg-bg-hover"
          >
            ours
          </button>
          <button
            type="button"
            title="Write the theirs side and stage it"
            onClick={(e) => {
              e.stopPropagation();
              void resolveConflict(entry.path, "theirs");
            }}
            className="h-5 shrink-0 inline-flex items-center justify-center rounded px-1 text-[10px] text-fg-2 hover:text-fg-0 hover:bg-bg-hover"
          >
            theirs
          </button>
        </div>
      );
    }
    return (
      <div
        key={`${entry.path}-${entry.staged}`}
        title={entry.path}
        onClick={() => openDiff(entry.path)}
        className="group h-7 px-2 flex items-center gap-1.5 rounded cursor-pointer select-none hover:bg-bg-hover"
      >
        <span
          className={cn(
            "w-5 shrink-0 text-center font-mono text-[10px] font-bold",
            st.cls,
          )}
        >
          {st.letter}
        </span>
        <span className="flex-1 min-w-0 truncate text-xs text-fg-1">{entry.path}</span>
        <button
          type="button"
          title={stagedFlag ? "Unstage" : "Stage"}
          onClick={(e) => {
            e.stopPropagation();
            if (stagedFlag) gitUnstage([entry.path]);
            else gitStage([entry.path]);
          }}
          className="h-5 w-5 shrink-0 inline-flex items-center justify-center rounded text-fg-2 hover:text-fg-0 hover:bg-bg-hover opacity-0 group-hover:opacity-100"
        >
          {stagedFlag ? <Minus size={12} /> : <Plus size={12} />}
        </button>
      </div>
    );
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-9 shrink-0 border-b border-line-0 flex items-center px-2.5 gap-2 text-xs font-semibold uppercase tracking-wider text-fg-2">
        <span className="flex-1">Git</span>
        <IconButton title="Refresh" className="h-6 w-6" onClick={() => refreshGit()}>
          <RefreshCw size={13} />
        </IconButton>
      </div>

      <div className="px-2.5 h-8 shrink-0 border-b border-line-0 flex items-center gap-1.5">
        <button
          type="button"
          title="Switch or create a branch"
          onClick={() => setBranchOpen(true)}
          className="flex items-center gap-1.5 min-w-0 hover:text-fg-0 text-fg-1"
        >
          <GitBranch size={12} className="shrink-0 text-fg-2" />
          <span className="font-mono text-xs text-fg-0 truncate" title={git.branch ?? ""}>
            {git.branch ?? "detached"}
          </span>
        </button>
        {git.ahead != null && git.behind != null && (
          <span className="shrink-0 font-mono text-[10px] text-fg-2" title="ahead / behind upstream">
            {git.ahead}↑ {git.behind}↓
          </span>
        )}
        <span className="flex-1" />
        <IconButton
          title={git.remoteUrl ? `Pull from ${git.remoteUrl}` : "Set a remote to pull"}
          className="h-6 w-6"
          disabled={!git.remoteUrl || busy !== null}
          onClick={async () => {
            setBusy("pull");
            await gitPull();
            setBusy(null);
          }}
        >
          <ArrowDown size={13} />
        </IconButton>
        <IconButton
          title={git.remoteUrl ? `Push to ${git.remoteUrl}` : "Set a remote to push"}
          className="h-6 w-6"
          disabled={!git.remoteUrl || busy !== null}
          onClick={async () => {
            setBusy("push");
            await gitPush();
            setBusy(null);
          }}
        >
          <ArrowUp size={13} />
        </IconButton>
      </div>

      <button
        type="button"
        onClick={() => {
          setRemoteUrl(git.remoteUrl ?? "");
          setRemoteOpen(true);
        }}
        title={git.remoteUrl ?? "No remote"}
        className="px-2.5 h-6 shrink-0 border-b border-line-0 flex items-center text-[10px] font-mono text-fg-2 hover:text-fg-0 truncate"
      >
        {git.remoteUrl ?? "no remote — click to add origin"}
      </button>

      <div className="px-2 py-1.5 shrink-0 flex items-center gap-1.5 border-b border-line-0">
        <Button variant="ghost" onClick={() => gitStage(null)} disabled={unstaged.length === 0}>
          Stage all
        </Button>
        <Button variant="ghost" onClick={() => gitUnstage(null)} disabled={staged.length === 0}>
          Unstage all
        </Button>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto">
        <Section
          id="staged"
          label={`Staged (${staged.length})`}
          collapsed={collapsed.has("staged")}
          onToggle={toggleSection}
        >
          {staged.length === 0 ? (
            <p className="px-2 py-1 text-xs text-fg-2">Nothing staged</p>
          ) : (
            staged.map(renderEntry(true))
          )}
        </Section>
        <Section
          id="changes"
          label={`Changes (${unstaged.length})`}
          collapsed={collapsed.has("changes")}
          onToggle={toggleSection}
        >
          {unstaged.length === 0 ? (
            <p className="px-2 py-1 text-xs text-fg-2">No changes</p>
          ) : (
            unstaged.map(renderEntry(false))
          )}
        </Section>
        <Section
          id="history"
          label="History"
          collapsed={collapsed.has("history")}
          onToggle={toggleSection}
        >
          {commits.length === 0 ? (
            <p className="px-2 py-1 text-xs text-fg-2">No commits yet</p>
          ) : (
            commits.map((c) => (
              <div
                key={c.oid}
                title={`${c.shortOid} — ${c.message}\n${c.author}`}
                onClick={() => openCommit(c)}
                className="h-7 px-2 flex items-center gap-2 select-none cursor-pointer rounded hover:bg-bg-hover"
              >
                <span className="shrink-0 font-mono text-[10px] text-accent">{c.shortOid}</span>
                <span className="flex-1 min-w-0 truncate text-xs text-fg-1">{c.message}</span>
                <span className="shrink-0 text-[10px] text-fg-2">
                  {c.author} · {formatTime(c.time)}
                </span>
              </div>
            ))
          )}
        </Section>
      </div>

      <div className="shrink-0 border-t border-line-0 p-2 flex flex-col gap-1.5">
        <textarea
          rows={3}
          placeholder="Commit message"
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              doCommit();
            }
          }}
          className="w-full h-auto rounded bg-bg-2 border border-line-0 px-2 py-1.5 text-xs text-fg-0 outline-none focus:border-line-focus placeholder:text-fg-2 resize-none"
        />
        <Button
          variant="primary"
          className="w-full"
          disabled={!message.trim() || staged.length === 0}
          onClick={doCommit}
        >
          <GitCommitHorizontal size={13} />
          Commit
        </Button>
      </div>

      <Modal
        open={branchOpen}
        onClose={() => setBranchOpen(false)}
        title="Branch"
        width="max-w-sm"
      >
        <div className="flex flex-col gap-1 max-h-60 overflow-y-auto">
          {branches.map((b) => (
            <button
              key={b}
              type="button"
              onClick={async () => {
                setBranchOpen(false);
                if (b !== git.branch) await gitCheckout(b);
              }}
              className={cn(
                "h-7 px-2 rounded text-left text-xs font-mono hover:bg-bg-hover",
                b === git.branch ? "text-accent" : "text-fg-1",
              )}
            >
              {b}
            </button>
          ))}
          {branches.length === 0 && (
            <p className="px-2 py-1 text-xs text-fg-2">No branches yet — commit first</p>
          )}
        </div>
        <form
          className="mt-2 flex gap-1.5"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!newBranch.trim()) return;
            const name = newBranch.trim();
            setNewBranch("");
            setBranchOpen(false);
            await gitCreateBranch(name);
          }}
        >
          <input
            value={newBranch}
            onChange={(e) => setNewBranch(e.target.value)}
            placeholder="new-branch"
            className="flex-1 h-7 px-2 rounded bg-bg-2 border border-line-0 text-xs font-mono text-fg-0 outline-none focus:border-line-focus"
          />
          <Button variant="primary" type="submit" disabled={!newBranch.trim()}>
            Create
          </Button>
        </form>
      </Modal>

      <Modal
        open={remoteOpen}
        onClose={() => setRemoteOpen(false)}
        title="Remote"
        width="max-w-sm"
      >
        <form
          className="flex flex-col gap-2"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!remoteUrl.trim()) return;
            setRemoteOpen(false);
            await gitSetRemote(remoteUrl.trim());
          }}
        >
          <input
            value={remoteUrl}
            onChange={(e) => setRemoteUrl(e.target.value)}
            placeholder="https://github.com/you/api.git"
            className="h-7 px-2 rounded bg-bg-2 border border-line-0 text-xs font-mono text-fg-0 outline-none focus:border-line-focus"
          />
          <Button variant="primary" type="submit" disabled={!remoteUrl.trim()}>
            Save origin
          </Button>
        </form>
      </Modal>

    </div>
  );
}

function Section({
  id,
  label,
  collapsed,
  onToggle,
  children,
}: {
  id: string;
  label: string;
  collapsed: boolean;
  onToggle: (id: string) => void;
  children: ReactNode;
}) {
  return (
    <div>
      <button
        type="button"
        onClick={() => onToggle(id)}
        className="w-full px-2 pt-2 pb-1 flex items-center gap-1 text-[10px] font-semibold uppercase tracking-wider text-fg-2 hover:text-fg-0"
      >
        <ChevronRight size={12} className={cn("shrink-0 transition-transform", !collapsed && "rotate-90")} />
        {label}
      </button>
      {!collapsed && children}
    </div>
  );
}

export default GitPanel;
