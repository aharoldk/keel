import { useState } from "react";
import {
  AlertTriangle,
  Check,
  Clock,
  Copy,
  Database,
  Download,
  Lock,
  Send,
  X,
} from "lucide-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { useKeel } from "@/state/store";
import type { SendResult } from "@/api/types";
import { api } from "@/api/client";
import { Button, EmptyState, IconButton, Spinner } from "@/components/ui";
import { cn, formatBytes, formatMs, statusClass } from "@/utils";
import CodeEditor from "@/features/request/CodeEditor";
import PreviewPane, { isPreviewable } from "@/features/response/PreviewPane";
import TimelinePane from "@/features/response/TimelinePane";

type ResponseTabId = "body" | "headers" | "cookies" | "tests" | "preview" | "timeline";

function toBase64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let bin = "";
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
  return btoa(bin);
}

function shortJson(v: unknown): string {
  try {
    const s = JSON.stringify(v);
    if (s === undefined) return "null";
    return s.length > 60 ? `${s.slice(0, 60)}...` : s;
  } catch {
    return String(v);
  }
}

export default function ResponseViewer() {
  const tab = useKeel((s) => s.tabs.find((t) => t.path === s.activePath));
  const toast = useKeel((s) => s.toast);

  const [respTab, setRespTab] = useState<ResponseTabId>("body");
  const [pretty, setPretty] = useState(true);

  if (!tab || (!tab.result && !tab.error && !tab.loading)) {
    return (
      <div className="h-full min-h-0">
        <EmptyState
          icon={<Send size={24} />}
          title="No response yet"
          hint="Send the request with Ctrl+Enter"
        />
      </div>
    );
  }

  if (tab.loading) {
    return (
      <div className="h-full min-h-0 flex flex-col items-center justify-center gap-2">
        <Spinner size={18} />
        <span className="text-xs text-fg-2">Sending…</span>
      </div>
    );
  }

  if (tab.error) {
    return (
      <div className="h-full min-h-0 overflow-y-auto p-2">
        <div className="border border-danger/40 bg-danger/5 rounded p-3 text-xs text-danger font-mono break-words">
          {tab.error}
        </div>
      </div>
    );
  }

  const result = tab.result as SendResult;

  const previewable = isPreviewable(result.contentType);
  const activeRespTab: ResponseTabId =
    respTab === "preview" && !previewable ? "body" : respTab;
  const responseTabs: [ResponseTabId, string][] = [
    ["body", "Body"],
    ...(previewable ? [["preview", "Preview"] as [ResponseTabId, string]] : []),
    ["headers", "Headers"],
    ["cookies", "Cookies"],
    ["timeline", "Timeline"],
    ["tests", "Tests"],
  ];

  const download = async () => {
    const path = await saveFileDialog({ title: "Save response" });
    if (!path) return;
    const dataBase64 =
      result.bodyBase64 ??
      (result.bodyText != null ? toBase64(result.bodyText) : "");
    try {
      await api.saveResponse(path, dataBase64);
      toast("Response saved", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const copyBody = async () => {
    if (result.bodyText == null) return;
    try {
      await writeText(result.bodyText);
      toast("Response body copied", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  return (
    <div className="h-full min-h-0 flex flex-col">
      {/* meta header */}
      <div className="h-9 border-b border-line-0 flex items-center gap-2 px-2.5 shrink-0">
        <span className={cn("font-mono font-bold text-xs", statusClass(result.status))}>
          {result.status ?? "—"}
        </span>
        <span className="text-xs text-fg-2 truncate max-w-40">
          {result.statusText}
        </span>
        <span className="flex items-center gap-1 text-xs text-fg-2" title="Duration">
          <Clock size={11} />
          {formatMs(result.timeMs)}
        </span>
        <span className="flex items-center gap-1 text-xs text-fg-2" title="Size">
          <Database size={11} />
          {formatBytes(result.sizeBytes)}
        </span>
        {result.truncated && (
          <span className="text-[10px] text-warn border border-warn/40 rounded px-1 py-px">
            truncated
          </span>
        )}
        {result.missingVariables.length > 0 && (
          <span
            className="flex items-center gap-1 text-xs text-warn"
            title={`missing: ${result.missingVariables.join(", ")}`}
          >
            <AlertTriangle size={11} />
            missing: {result.missingVariables.slice(0, 3).join(", ")}
            {result.missingVariables.length > 3 ? ", …" : ""}
          </span>
        )}
        {result.secretsUsed.length > 0 && (
          <span
            className="flex items-center gap-1 text-xs text-accent"
            title={`secrets used: ${result.secretsUsed.join(", ")}`}
          >
            <Lock size={11} />
          </span>
        )}
        {result.authUsed && (
          <span
            className="flex items-center gap-1 text-[10px] font-mono text-fg-1 border border-line-0 rounded px-1 py-px"
            title={`auth used: ${result.authUsed}`}
          >
            <Lock size={10} />
            {result.authUsed}
          </span>
        )}

        <div className="ml-auto self-stretch flex items-stretch gap-0.5">
          {responseTabs.map(([id, label]) => {
            const pass = result.testResults.filter((r) => r.passed).length;
            const total = result.testResults.length;
            const badge =
              id === "tests" && total > 0 ? (
                <span
                  className={cn(
                    "text-[9px]",
                    pass === total ? "text-ok" : "text-danger",
                  )}
                >
                  {pass}/{total}
                </span>
              ) : null;
            return (
              <button
                key={id}
                type="button"
                onClick={() => setRespTab(id)}
                className={cn(
                  "text-xs px-2 flex items-center gap-1 border-b-2 -mb-px transition-colors",
                  activeRespTab === id
                    ? "border-accent text-fg-0 font-medium"
                    : "border-transparent text-fg-2 hover:text-fg-0",
                )}
              >
                {label}
                {badge}
              </button>
            );
          })}
        </div>
      </div>

      {/* content */}
      {activeRespTab === "body" && (
        <BodyPane
          result={result}
          pretty={pretty}
          setPretty={setPretty}
          onCopy={() => void copyBody()}
          onDownload={() => void download()}
        />
      )}
      {activeRespTab === "preview" && <PreviewPane result={result} />}
      {activeRespTab === "timeline" && (
        <TimelinePane timeline={result.timeline ?? []} />
      )}
      {activeRespTab === "headers" && (
        <div className="flex-1 min-h-0 overflow-y-auto p-2 flex flex-col gap-1">
          {result.headers.map((h, i) => (
            <div key={i} className="flex gap-2 text-xs">
              <span className="font-mono text-fg-1 w-56 shrink-0 truncate">
                {h.name}
              </span>
              <span className="font-mono break-all min-w-0">{h.value}</span>
            </div>
          ))}
          {result.headers.length === 0 && (
            <div className="text-xs text-fg-2">No headers</div>
          )}
        </div>
      )}
      {activeRespTab === "cookies" && (
        <div className="flex-1 min-h-0 overflow-y-auto p-2 flex flex-col gap-1">
          {result.cookies.length === 0 && (
            <div className="text-xs text-fg-2">No cookies</div>
          )}
          {result.cookies.map((c, i) => (
            <div key={i} className="flex gap-2 text-xs">
              <span className="font-mono text-fg-1 w-56 shrink-0 truncate">
                {c.name}
              </span>
              <span className="font-mono break-all min-w-0">{c.value}</span>
            </div>
          ))}
        </div>
      )}
      {activeRespTab === "tests" && (
        <div className="flex-1 min-h-0 overflow-y-auto p-2 flex flex-col gap-1">
          {result.testResults.map((r, i) => (
            <div key={i} className="flex items-start gap-2 text-xs">
              {r.passed ? (
                <Check size={12} className="text-ok shrink-0 mt-0.5" />
              ) : (
                <X size={12} className="text-danger shrink-0 mt-0.5" />
              )}
              <span className="font-mono text-fg-0">{r.expect}</span>
              {r.matcher && (
                <span className="font-mono text-[10px] px-1 py-px rounded bg-bg-3 text-fg-1">
                  {r.matcher}
                </span>
              )}
              <span className="font-mono text-fg-2 truncate">
                {shortJson(r.expected)} &rarr; {shortJson(r.actual)}
              </span>
              {r.message && <span className="text-danger">{r.message}</span>}
            </div>
          ))}
          {result.testResults.length === 0 && (
            <div className="text-xs text-fg-2">No tests defined</div>
          )}
          {result.scriptError && (
            <div className="mt-2 text-xs text-danger font-mono break-words">
              {result.scriptError}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/* ---------- Body pane ---------- */

function BodyPane({
  result,
  pretty,
  setPretty,
  onCopy,
  onDownload,
}: {
  result: SendResult;
  pretty: boolean;
  setPretty: (p: boolean) => void;
  onCopy: () => void;
  onDownload: () => void;
}) {
  const bodyText = result.bodyText;

  let parsedJson: string | null = null;
  let jsonFailed = false;
  if (bodyText != null && bodyText.trim() !== "") {
    try {
      parsedJson = JSON.stringify(JSON.parse(bodyText), null, 2);
    } catch {
      jsonFailed = (result.contentType ?? "").includes("json");
    }
  }

  const showJson = pretty && parsedJson != null;
  const showJsonRaw = pretty && parsedJson == null && jsonFailed;
  const isText = bodyText != null;

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-8 border-b border-line-0 flex items-center gap-1 px-2 shrink-0">
        <IconButton
          className="h-6 w-6"
          title="Copy body"
          disabled={bodyText == null}
          onClick={onCopy}
        >
          <Copy size={12} />
        </IconButton>
        <IconButton className="h-6 w-6" title="Download body" onClick={onDownload}>
          <Download size={12} />
        </IconButton>
        <div className="ml-2 flex items-center rounded border border-line-0 overflow-hidden">
          {(["pretty", "raw"] as const).map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => setPretty(m === "pretty")}
              className={cn(
                "text-[10px] px-2 h-6 capitalize",
                (m === "pretty") === pretty
                  ? "bg-bg-3 text-fg-0"
                  : "text-fg-2 hover:text-fg-0",
              )}
            >
              {m}
            </button>
          ))}
        </div>
      </div>

      {isText ? (
        pretty ? (
          <div className="flex-1 min-h-0 overflow-hidden bg-bg-1">
            <CodeEditor
              value={showJson ? (parsedJson ?? "") : (bodyText ?? "")}
              language={showJson || showJsonRaw ? "json" : "text"}
              readOnly
              lineNumbers
              appearance="editor"
              height="100%"
            />
          </div>
        ) : (
          <pre className="flex-1 min-h-0 overflow-auto font-mono text-xs whitespace-pre-wrap break-all p-2 m-0">
            {bodyText}
          </pre>
        )
      ) : (
        <div className="flex-1 min-h-0 flex flex-col items-center justify-center gap-2">
          <div className="text-xs text-fg-2">
            Binary content · {formatBytes(result.sizeBytes)}
          </div>
          <Button variant="ghost" className="h-7" onClick={onDownload}>
            <Download size={12} />
            Download
          </Button>
        </div>
      )}
    </div>
  );
}

export { ResponseViewer };
