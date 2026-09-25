import { useEffect, useMemo, useState } from "react";
import { ArrowDown, ChevronDown, ChevronRight, Play, Plus, Save, Trash2, Upload, Workflow, X } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "@/api/client";
import { flowStepPath, type FlowDoc, type HttpMethod, type SendResult, type TreeNode } from "@/api/types";
import { Button, EmptyState, IconButton, Select, TextInput } from "@/components/ui";
import { cn, formatMs, methodVar, statusClass } from "@/utils";
import { useKeel } from "@/state/store";

interface Step {
  id: string;
  path: string;
  name: string;
  method: HttpMethod;
  /** False = keep going when this step fails. Defaults to true. */
  stopOnFailure: boolean;
}

interface StepResult {
  status: "running" | "ok" | "error";
  result?: SendResult;
  error?: string;
}

function requestOptions(tree: TreeNode[]) {
  const out: { path: string; name: string; method: HttpMethod; label: string }[] = [];
  const walk = (nodes: TreeNode[], prefix: string) => {
    for (const n of nodes) {
      if (n.kind === "request") {
        out.push({
          path: n.path,
          name: n.name,
          method: n.method ?? "GET",
          label: prefix ? `${prefix} / ${n.name}` : n.name,
        });
      }
      if (n.children) walk(n.children, prefix ? `${prefix} / ${n.name}` : n.name);
    }
  };
  walk(tree, "");
  return out;
}

function StepBody({ result }: { result: StepResult }) {
  if (result.status === "running") {
    return <div className="px-2 py-1.5 text-[11px] text-fg-2">Sending…</div>;
  }
  if (result.error && !result.result) {
    return <div className="px-2 py-1.5 text-[11px] text-danger break-words">{result.error}</div>;
  }
  const r = result.result;
  if (!r) return null;
  return (
    <div className="px-2 py-1.5 flex flex-col gap-1.5 text-[11px]">
      {r.error && <div className="text-danger break-words">{r.error}</div>}
      {r.scriptError && <div className="text-warn break-words">{r.scriptError}</div>}
      {r.bodyText && (
        <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-words font-mono text-fg-1 bg-bg-0 rounded p-1.5">
          {r.bodyText}
        </pre>
      )}
    </div>
  );
}

function lookup(requests: ReturnType<typeof requestOptions>, step: import("@/api/types").FlowStep): Step {
  const path = flowStepPath(step);
  const req = requests.find((r) => r.path === path);
  const stops = typeof step === "string" ? true : step.onFailure !== "continue";
  return {
    id: crypto.randomUUID(),
    path,
    name: req?.name ?? path,
    method: req?.method ?? "GET",
    stopOnFailure: stops,
  };
}

export function FlowPanel() {
  const tree = useKeel((s) => s.tree);
  const workspace = useKeel((s) => s.workspace);
  const activeEnv = useKeel((s) => s.activeEnv);
  const toast = useKeel((s) => s.toast);
  const requests = useMemo(() => requestOptions(tree), [tree]);

  const [flows, setFlows] = useState<string[]>([]);
  const [fileName, setFileName] = useState<string | null>(null);
  const [name, setName] = useState("Untitled flow");
  const [steps, setSteps] = useState<Step[]>([]);
  const [pick, setPick] = useState("");
  const [running, setRunning] = useState(false);
  const [saving, setSaving] = useState(false);

  const refresh = () => {
    api.flowList().then(setFlows).catch((e) => toast(String(e), "error"));
  };

  useEffect(() => {
    if (!workspace) return;
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspace?.root]);

  const load = async (file: string) => {
    try {
      const doc = await api.flowRead(file);
      setFileName(file);
      setName(doc.name);
      setSteps(doc.steps.map((p) => lookup(requests, p)));
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const save = async () => {
    const doc: FlowDoc = {
      schemaVersion: "1",
      name: name.trim() || "Untitled flow",
      kind: "flow",
      steps: steps.map((s) =>
        s.stopOnFailure ? s.path : { path: s.path, onFailure: "continue" },
      ),
    };
    setSaving(true);
    try {
      const saved = await api.flowSave(fileName, doc);
      setFileName(saved);
      setName(doc.name);
      refresh();
      toast("Flow saved", "success");
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setSaving(false);
    }
  };

  const importFlow = async () => {
    try {
      const picked = await openDialog({
        multiple: false,
        title: "Import flow",
        filters: [{ name: "Flow (YAML)", extensions: ["yaml", "yml"] }],
      });
      if (!picked || Array.isArray(picked)) return;
      const saved = await api.flowImport(picked);
      refresh();
      await load(saved);
      toast("Flow imported", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const removeFlow = async () => {
    if (!fileName) return;
    try {
      await api.flowDelete(fileName);
      setFileName(null);
      setName("Untitled flow");
      setSteps([]);
      refresh();
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const add = () => {
    const req = requests.find((r) => r.path === pick) ?? requests[0];
    if (!req) return;
    setSteps((s) => [
      ...s,
      { id: crypto.randomUUID(), path: req.path, name: req.name, method: req.method, stopOnFailure: true },
    ]);
  };

  const run = async () => {
    if (steps.length === 0 || running) return;
    const title = name.trim() || "Untitled flow";
    let live: Record<string, StepResult> = {};
    const publish = (runningNow: boolean, results: Record<string, StepResult>) =>
      useKeel.getState().setFlowRun({ name: title, running: runningNow, steps, results });
    useKeel.getState().openFlow();
    setRunning(true);
    publish(true, live);
    for (const step of steps) {
      live = { ...live, [step.id]: { status: "running" } };
      publish(true, live);
      try {
        const result = await api.sendRequest(step.path, activeEnv);
        useKeel.setState((st) => ({ envValuesRevision: st.envValuesRevision + 1 }));
        const testsFailed = result.testResults.some((t) => !t.passed);
        const failed = Boolean(result.error) || !result.ok || testsFailed;
        live = {
          ...live,
          [step.id]: {
            status: failed ? "error" : "ok",
            result,
            error: result.error ?? undefined,
          },
        };
        publish(true, live);
        if (failed && step.stopOnFailure) {
          toast(`${step.name}: ${result.error ?? "failed"}`, "error");
          break;
        }
      } catch (e) {
        const error = String(e);
        live = { ...live, [step.id]: { status: "error", error } };
        publish(true, live);
        toast(`${step.name}: ${error}`, "error");
        if (step.stopOnFailure) break;
      }
    }
    setRunning(false);
    publish(false, live);
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col bg-bg-0">
      <div className="h-9 shrink-0 border-b border-line-0 flex items-center px-2.5 gap-2 text-xs font-semibold uppercase tracking-wider text-fg-2">
        <Workflow size={14} className="shrink-0" />
        <span className="flex-1">Flow</span>
        <IconButton title="Import flow" className="h-6 w-6" disabled={running} onClick={() => void importFlow()}>
          <Upload size={13} />
        </IconButton>
        <IconButton title="Save flow" className="h-6 w-6" disabled={saving || running} onClick={() => void save()}>
          <Save size={13} />
        </IconButton>
        <Button
          variant="primary"
          className="h-6 px-2"
          disabled={steps.length === 0 || running}
          onClick={() => void run()}
        >
          <Play size={11} />
          Run
        </Button>
      </div>

      <div className="shrink-0 border-b border-line-0 p-2 flex flex-col gap-1.5">
        <div className="flex items-center gap-1.5">
          <Select
            className="flex-1 min-w-0"
            value={fileName ?? ""}
            onChange={(e) => {
              const file = e.target.value;
              if (!file) {
                setFileName(null);
                setName("Untitled flow");
                setSteps([]);
                return;
              }
              void load(file);
            }}
          >
            <option value="">New flow</option>
            {flows.map((f) => (
              <option key={f} value={f}>
                {f.replace(/\.yaml$/, "")}
              </option>
            ))}
          </Select>
          {fileName && (
            <IconButton title="Delete flow" className="h-7 w-7" disabled={running} onClick={() => void removeFlow()}>
              <Trash2 size={13} />
            </IconButton>
          )}
        </div>
        <TextInput value={name} onChange={(e) => setName(e.target.value)} placeholder="Flow name" />
      </div>

      <div className="shrink-0 border-b border-line-0 p-2 flex items-center gap-1.5">
        <Select
          className="flex-1 min-w-0"
          value={pick}
          onChange={(e) => setPick(e.target.value)}
          disabled={requests.length === 0}
        >
          {requests.length === 0 ? (
            <option value="">No requests</option>
          ) : (
            <>
              <option value="">Select request…</option>
              {requests.map((r) => (
                <option key={r.path} value={r.path}>
                  {r.method} {r.label}
                </option>
              ))}
            </>
          )}
        </Select>
        <IconButton title="Add step" className="h-7 w-7" disabled={requests.length === 0} onClick={add}>
          <Plus size={14} />
        </IconButton>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto">
        {steps.length === 0 ? (
          <EmptyState
            icon={<Workflow size={24} />}
            title="No steps"
            hint="Add requests in order. Each step sees variables set by the one before it."
          />
        ) : (
          steps.map((step, i) => (
              <div key={step.id}>
                {i > 0 && (
                  <div className="flex justify-center text-fg-2">
                    <ArrowDown size={12} />
                  </div>
                )}
                <div className="mx-1.5 rounded border border-line-0 bg-bg-0">
                  <div className="flex items-center gap-1 h-7 px-1.5">
                    <span className="w-3 shrink-0 text-center font-mono text-[10px] text-fg-2">{i + 1}</span>
                    <span
                      className="w-10 shrink-0 font-mono text-[10px] font-bold"
                      style={{ color: methodVar(step.method) }}
                    >
                      {step.method}
                    </span>
                    <span className="flex-1 truncate text-xs text-fg-0">{step.name}</span>
                    <button
                      type="button"
                      title={
                        step.stopOnFailure
                          ? "Stop the flow if this step fails"
                          : "Continue the flow if this step fails"
                      }
                      className={cn(
                        "shrink-0 rounded px-1 text-[10px]",
                        step.stopOnFailure ? "text-danger" : "text-fg-2",
                      )}
                      onClick={() =>
                        setSteps((s) =>
                          s.map((x) =>
                            x.id === step.id ? { ...x, stopOnFailure: !x.stopOnFailure } : x,
                          ),
                        )
                      }
                    >
                      {step.stopOnFailure ? "stop" : "continue"}
                    </button>
                    <IconButton
                      title="Remove"
                      className="h-5 w-5"
                      disabled={running}
                      onClick={() => setSteps((s) => s.filter((x) => x.id !== step.id))}
                    >
                      {running ? <X size={11} /> : <Trash2 size={11} />}
                    </IconButton>
                  </div>
                </div>
              </div>
            ))
        )}
      </div>
    </div>
  );
}

export function FlowRun() {
  const run = useKeel((s) => s.flowRun);
  const [openId, setOpenId] = useState<string | null>(null);

  if (!run) return null;

  return (
    <div className="flex-1 min-h-0 flex flex-col bg-bg-0">
      <div className="h-9 shrink-0 border-b border-line-0 flex items-center px-3 gap-2 text-xs font-semibold uppercase tracking-wider text-fg-2">
        <Workflow size={14} className="shrink-0" />
        <span className="flex-1 truncate normal-case tracking-normal text-fg-0">{run.name}</span>
        {run.running && <span className="normal-case tracking-normal text-fg-2">Running…</span>}
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto max-w-3xl w-full py-2">
        {run.steps.map((step, i) => {
          const res = run.results[step.id];
          const open = openId === step.id && res != null;
          return (
            <div key={step.id}>
              {i > 0 && (
                <div className="flex justify-center text-fg-2">
                  <ArrowDown size={12} />
                </div>
              )}
              <div className="mx-3 rounded border border-line-0 bg-bg-1">
                <button
                  type="button"
                  className="flex w-full items-center gap-1.5 h-8 px-2 text-left"
                  disabled={!res}
                  onClick={() => setOpenId(open ? null : step.id)}
                >
                  {res ? (
                    open ? (
                      <ChevronDown size={12} className="shrink-0 text-fg-2" />
                    ) : (
                      <ChevronRight size={12} className="shrink-0 text-fg-2" />
                    )
                  ) : (
                    <span className="w-3 shrink-0 text-center font-mono text-[10px] text-fg-2">{i + 1}</span>
                  )}
                  <span
                    className="w-10 shrink-0 font-mono text-[10px] font-bold"
                    style={{ color: methodVar(step.method) }}
                  >
                    {step.method}
                  </span>
                  <span className="flex-1 truncate text-xs text-fg-0">{step.name}</span>
                  {res?.result?.status != null && (
                    <span className={cn("font-mono text-[11px]", statusClass(res.result.status))}>
                      {res.result.status}
                    </span>
                  )}
                  {res?.result && (
                    <span className="font-mono text-[10px] text-fg-2">{formatMs(res.result.timeMs)}</span>
                  )}
                  {res?.status === "running" && <span className="text-[10px] text-fg-2">…</span>}
                  {res?.status === "error" && res.result == null && (
                    <span className="text-[10px] text-danger">error</span>
                  )}
                </button>
                {open && res && (
                  <div className="border-t border-line-0">
                    <StepBody result={res} />
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export default FlowPanel;
