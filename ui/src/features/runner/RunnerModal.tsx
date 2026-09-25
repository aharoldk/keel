import { useMemo, useState } from "react";
import {
  AlertCircle,
  CheckCircle,
  FolderOpen,
  Loader2,
  MinusCircle,
  XCircle,
} from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { HttpMethod, RunnerItem, TreeNode } from "@/api/types";
import { Button, Modal, Select, Spinner } from "@/components/ui";
import { cn, formatBytes, formatMs, pathBasename, methodVar, statusClass } from "@/utils";
import { useKeel } from "@/state/store";

function StatusIcon({ status }: { status: RunnerItem["status"] }) {
  switch (status) {
    case "running":
      return <Loader2 size={12} className="shrink-0 animate-spin text-accent" />;
    case "passed":
      return <CheckCircle size={12} className="shrink-0 text-ok" />;
    case "failed":
      return <XCircle size={12} className="shrink-0 text-danger" />;
    case "error":
      return <AlertCircle size={12} className="shrink-0 text-warn" />;
    case "skipped":
      return <MinusCircle size={12} className="shrink-0 text-fg-2" />;
  }
}

function useFolderOptions() {
  const tree = useKeel((s) => s.tree);
  return useMemo(() => {
    const out: { path: string; label: string }[] = [];
    const walk = (nodes: TreeNode[], depth: number) =>
      nodes.forEach((n) => {
        if (n.kind === "request") return;
        out.push({ path: n.kind === "collection" ? "" : n.path, label: `${"  ".repeat(depth)}${n.name}` });
        if (n.children) walk(n.children, depth + 1);
      });
    walk(tree, 0);
    return out;
  }, [tree]);
}

function PreRunForm() {
  const folders = useFolderOptions();
  const startRun = useKeel((s) => s.startRun);
  const toast = useKeel((s) => s.toast);
  const [preFolder, setPreFolder] = useState("");
  const [delayMs, setDelayMs] = useState(0);
  const [stopOnFailure, setStopOnFailure] = useState(false);
  const [recursive, setRecursive] = useState(true);
  const [dataFile, setDataFile] = useState<string | null>(null);

  const pickDataFile = async () => {
    try {
      const picked = await openDialog({
        multiple: false,
        title: "Select data file (CSV or JSON)",
        filters: [{ name: "Data file", extensions: ["csv", "json"] }],
      });
      if (typeof picked === "string") setDataFile(picked);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <span className="w-32 shrink-0 text-xs text-fg-1">Target folder</span>
        <Select
          className="flex-1 font-mono"
          value={preFolder}
          onChange={(e) => setPreFolder(e.target.value)}
        >
          <option value="">Collection root</option>
          {folders
            .filter((f) => f.path !== "")
            .map((f) => (
              <option key={f.path} value={f.path}>
                {f.label}
              </option>
            ))}
        </Select>
      </div>
      <div className="flex items-center gap-2">
        <span className="w-32 shrink-0 text-xs text-fg-1">Data file</span>
        <span
          className="flex-1 min-w-0 truncate font-mono text-xs text-fg-2"
          title={dataFile ?? undefined}
        >
          {dataFile ?? "None (one pass)"}
        </span>
        <Button variant="ghost" className="h-7" onClick={() => void pickDataFile()}>
          <FolderOpen size={12} /> Choose
        </Button>
        {dataFile && (
          <Button variant="ghost" className="h-7" onClick={() => setDataFile(null)}>
            Clear
          </Button>
        )}
      </div>
      <div className="flex items-center gap-2">
        <span className="w-32 shrink-0 text-xs text-fg-1">Delay (ms)</span>
        <input
          type="number"
          min={0}
          value={delayMs}
          onChange={(e) => setDelayMs(Math.max(0, Number(e.target.value) || 0))}
          className="h-7 w-24 rounded bg-bg-2 border border-line-0 px-2 text-xs font-mono text-fg-0 outline-none focus:border-line-focus"
        />
      </div>
      <label className="flex items-center gap-2 pl-32 text-xs text-fg-1 cursor-pointer select-none">
        <input
          type="checkbox"
          checked={stopOnFailure}
          onChange={(e) => setStopOnFailure(e.target.checked)}
          className="h-3 w-3 accent-[var(--accent)] cursor-pointer"
        />
        Stop on failure
      </label>
      <label className="flex items-center gap-2 pl-32 text-xs text-fg-1 cursor-pointer select-none">
        <input
          type="checkbox"
          checked={recursive}
          onChange={(e) => setRecursive(e.target.checked)}
          className="h-3 w-3 accent-[var(--accent)] cursor-pointer"
        />
        Include sub-folders
      </label>
      <div className="flex justify-end">
        <Button
          variant="primary"
          onClick={() =>
            void startRun(preFolder, { delayMs, stopOnFailure, recursive, dataFile })
          }
        >
          Run
        </Button>
      </div>
    </div>
  );
}

function RunResults() {
  const items = useKeel((s) => s.runnerItems);
  const summary = useKeel((s) => s.runnerSummary);
  const runId = useKeel((s) => s.runnerRunId);
  const cancelRun = useKeel((s) => s.cancelRun);
  const setRunnerOpen = useKeel((s) => s.setRunnerOpen);
  const running = runId != null && summary == null;

  return (
    <div className="flex flex-col gap-2">
      <div className="rounded border border-line-0 overflow-hidden">
        <div className="grid grid-cols-[20px_56px_1fr_64px_64px_64px_56px] items-center gap-2 px-2 h-7 bg-bg-2 border-b border-line-0 text-[10px] font-semibold uppercase tracking-wider text-fg-2">
          <span />
          <span>Method</span>
          <span>Request</span>
          <span className="text-right">Status</span>
          <span className="text-right">Time</span>
          <span className="text-right">Size</span>
          <span className="text-right">Tests</span>
        </div>
        <div className="max-h-[50vh] overflow-y-auto">
          {items.length === 0 ? (
            <div className="flex items-center justify-center gap-2 py-8 text-xs text-fg-2">
              {running ? (
                <>
                  <Spinner size={14} /> Starting run…
                </>
              ) : (
                "No items"
              )}
            </div>
          ) : (
            items.map((item) => (
              <div
                key={`${item.path}#${item.iteration ?? 0}`}
                title={item.error ?? item.path}
                className="grid grid-cols-[20px_56px_1fr_64px_64px_64px_56px] items-center gap-2 px-2 h-7 border-b border-line-0 last:border-b-0 text-xs"
              >
                <span className="flex items-center justify-center">
                  <StatusIcon status={item.status} />
                </span>
                <span
                  className="font-mono text-[10px] font-bold truncate"
                  style={{ color: methodVar(item.method as HttpMethod) }}
                >
                  {item.method}
                </span>
                <span className="truncate text-fg-0">
                  {item.name}
                  {(item.iteration ?? 0) > 0 && (
                    <span className="ml-1.5 text-[10px] text-fg-2">row {item.iteration}</span>
                  )}
                </span>
                <span
                  className={cn(
                    "text-right font-mono text-[11px]",
                    statusClass(item.statusCode ?? null),
                  )}
                >
                  {item.statusCode ?? (item.status === "running" ? "…" : "—")}
                </span>
                <span className="text-right font-mono text-[11px] text-fg-1">
                  {item.status === "running" ? "—" : formatMs(item.timeMs)}
                </span>
                <span className="text-right font-mono text-[11px] text-fg-1">
                  {item.status === "running" ? "—" : formatBytes(item.sizeBytes)}
                </span>
                <span
                  className={cn(
                    "text-right font-mono text-[11px]",
                    item.testsTotal > 0 && item.testsPassed < item.testsTotal
                      ? "text-danger"
                      : "text-fg-1",
                  )}
                >
                  {item.testsTotal > 0
                    ? `${item.testsPassed}/${item.testsTotal}`
                    : "—"}
                </span>
              </div>
            ))
          )}
        </div>
      </div>

      {summary && (
        <div className="rounded border border-line-0 bg-bg-2 px-2.5 py-1.5 text-xs text-fg-1">
          <span className="text-ok">{summary.passed} passed</span>
          {" · "}
          <span className={summary.failed > 0 ? "text-danger" : undefined}>
            {summary.failed} failed
          </span>
          {" · "}
          <span className={summary.errored > 0 ? "text-warn" : undefined}>
            {summary.errored} errored
          </span>
          {" · "}
          <span className="text-fg-2">{summary.skipped} skipped</span>
          {" · "}
          <span className="font-mono text-fg-2">{formatMs(summary.durationMs)}</span>
        </div>
      )}

      <div className="flex justify-end gap-2">
        {running ? (
          <Button variant="danger" onClick={() => void cancelRun()}>
            Cancel
          </Button>
        ) : (
          <Button variant="default" onClick={() => setRunnerOpen(false)}>
            Close
          </Button>
        )}
      </div>
    </div>
  );
}

export function RunnerModal() {
  const runnerOpen = useKeel((s) => s.runnerOpen);
  const runnerRunId = useKeel((s) => s.runnerRunId);
  const runnerFolder = useKeel((s) => s.runnerFolder);
  const setRunnerOpen = useKeel((s) => s.setRunnerOpen);

  if (!runnerOpen) return null;
  const target = runnerRunId ? runnerFolder : "";
  const title =
    (target ?? "") === "" ? "Runner" : `Runner — ${pathBasename(target ?? "")}`;

  return (
    <Modal
      open={runnerOpen}
      onClose={() => setRunnerOpen(false)}
      title={title}
      width="max-w-3xl"
    >
      {runnerRunId == null ? <PreRunForm /> : <RunResults />}
    </Modal>
  );
}

export default RunnerModal;
