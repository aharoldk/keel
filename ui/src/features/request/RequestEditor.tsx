import { useEffect, useState } from "react";
import { Braces, ClipboardCopy, FolderOpen, Play, Save } from "lucide-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { Check, Plus, X } from "lucide-react";
import { useKeel } from "@/state/store";
import {
  Button,
  IconButton,
  Select,
  Spinner,
  TextInput,
} from "@/components/ui";
import { SplitPane } from "@/components/SplitPane";
import { comboFor, formatCombo } from "@/shortcuts";
import { cn, methodVar } from "@/utils";
import {
  HTTP_METHODS,
  type Auth,
  type Body,
  type HttpMethod,
  type KV,
  type RequestDoc,
  type RequestProtocol,
  type SendResult,
  type TestAssertion,
  type TestMatcherName,
} from "@/api/types";
import { api } from "@/api/client";
import KVEditor from "@/features/request/KVEditor";
import EditorTabBar from "@/features/request/EditorTabBar";
import CodeEditor from "@/features/request/CodeEditor";
import PathParamsRows from "@/features/request/PathParamsRows";
import DocsTab from "@/features/request/DocsTab";
import AuthEditor from "@/features/collections/AuthEditor";
import ResponseViewer from "@/features/response/ResponseViewer";
import QueryBuilder from "@/features/request/QueryBuilder";
import WsPanel from "@/features/request/WsPanel";
import GrpcPanel from "@/features/request/GrpcPanel";
import VariableInput from "@/components/VariableInput";
import {
  gatherVariableSuggestions,
  type VariableSuggestion,
} from "@/features/request/variables";

type EditorTabId =
  | "params"
  | "headers"
  | "auth"
  | "body"
  | "scripts"
  | "tests"
  | "docs";

const MATCHERS: (TestMatcherName | "")[] = [
  "",
  "toBe",
  "toNotBe",
  "toEqual",
  "toContain",
  "toMatch",
  "toBeType",
  "toHaveLength",
  "toBeGreaterThan",
  "toBeLessThan",
  "toBeTruthy",
  "toBeNull",
];

const MATCHER_KEYS: TestMatcherName[] = [
  "toBe",
  "toNotBe",
  "toEqual",
  "toContain",
  "toMatch",
  "toBeType",
  "toHaveLength",
  "toBeGreaterThan",
  "toBeLessThan",
  "toBeTruthy",
  "toBeNull",
];

function currentMatcher(a: TestAssertion): TestMatcherName | "" {
  for (const k of MATCHER_KEYS) {
    if (k in a) return k;
  }
  return "";
}

function coerceValue(raw: string): string | number {
  return /^-?\d+(\.\d+)?$/.test(raw) ? Number(raw) : raw;
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

export default function RequestEditor() {
  const activePath = useKeel((s) => s.activePath);
  const tab = useKeel((s) => s.tabs.find((t) => t.path === s.activePath));
  const updateDoc = useKeel((s) => s.updateDoc);
  const sendActive = useKeel((s) => s.sendActive);
  const saveActive = useKeel((s) => s.saveActive);
  const toast = useKeel((s) => s.toast);
  const openCodegen = useKeel((s) => s.openCodegen);
  const activeEnv = useKeel((s) => s.activeEnv);
  const envValuesRevision = useKeel((s) => s.envValuesRevision);
  const workspaceRoot = useKeel((s) => s.workspace?.root ?? null);
  const splitDirection = useKeel((s) => s.splitDirection);
  const shortcuts = useKeel((s) => s.settings.shortcuts);
  const prevRefsRevision = useKeel((s) => s.prevRefsRevision);

  const [variables, setVariables] = useState<VariableSuggestion[]>([]);

  useEffect(() => {
    let alive = true;
    gatherVariableSuggestions(activePath)
      .then((v) => {
        if (alive) setVariables(v);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [activePath, activeEnv, workspaceRoot, envValuesRevision, prevRefsRevision]);

  const [editorTab, setEditorTab] = useState<EditorTabId>("params");
  const [grpcResult, setGrpcResult] = useState("");

  useEffect(() => {
    const onTab = (e: Event) => {
      setEditorTab((e as CustomEvent<EditorTabId>).detail);
    };
    window.addEventListener("keel:editor-tab", onTab);
    return () => window.removeEventListener("keel:editor-tab", onTab);
  }, []);

  useEffect(() => {
    if (!tab) return;
    const path = tab.path;
    const doc = tab.doc;
    const onCurl = async () => {
      try {
        const curl = await api.exportCurl(path, doc);
        await writeText(curl);
        toast("cURL copied", "success");
      } catch (e) {
        toast(String(e), "error");
      }
    };
    window.addEventListener("keel:copy-curl", onCurl);
    return () => window.removeEventListener("keel:copy-curl", onCurl);
  }, [tab, toast]);

  if (!tab || !activePath || tab.path !== activePath) return null;
  const doc = tab.doc;
  const setDoc = (next: RequestDoc) => updateDoc(tab.path, next);
  const setRequest = (patch: Partial<RequestDoc["request"]>) =>
    setDoc({ ...doc, request: { ...doc.request, ...patch } });

  const isDirty = JSON.stringify(doc) !== JSON.stringify(tab.saved);

  const copyCurl = async () => {
    try {
      const curl = await api.exportCurl(tab.path, doc);
      await writeText(curl);
      toast("cURL copied", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const protocol: RequestProtocol = doc.protocol ?? "http";
  const setProtocol = (next: RequestProtocol) => {
    if (next === "graphql") {
      setDoc({
        ...doc,
        protocol: next,
        request: {
          ...doc.request,
          method: "POST",
          body:
            doc.request.body?.type === "graphql"
              ? doc.request.body
              : { type: "graphql", query: "", variables: "" },
        },
      });
      return;
    }
    setDoc({ ...doc, protocol: next });
  };

  const editorTabs: { id: EditorTabId; label: string; badge?: number }[] = [
    { id: "params", label: "Params", badge: (doc.request.params ?? []).length },
    { id: "headers", label: "Headers", badge: (doc.request.headers ?? []).length },
    { id: "auth", label: "Auth" },
    { id: "body", label: "Body" },
    { id: "scripts", label: "Scripts" },
    { id: "tests", label: "Tests", badge: (doc.tests ?? []).length },
    { id: "docs", label: "Docs" },
  ];

  return (
    <div className="flex flex-col min-h-0 flex-1">
      {/* URL bar */}
      <div className="h-10 border-b border-line-0 flex items-center gap-2 px-2.5 shrink-0">
        <Select
          className="text-[11px] w-[92px] shrink-0"
          value={protocol}
          onChange={(e) => setProtocol(e.target.value as RequestProtocol)}
        >
          <option value="http">HTTP</option>
          <option value="graphql">GraphQL</option>
          <option value="websocket">WebSocket</option>
          <option value="grpc">gRPC</option>
        </Select>
        {protocol === "http" && (
          <Select
            className="font-mono font-semibold text-[11px] w-24 shrink-0"
            style={{ color: methodVar(doc.request.method) }}
            value={doc.request.method}
            onChange={(e) =>
              setRequest({ method: e.target.value as HttpMethod })
            }
          >
            {HTTP_METHODS.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </Select>
        )}
        <VariableInput
          className="flex-1 h-8 min-w-0"
          inputClassName="h-8 font-mono"
          placeholder={
            protocol === "websocket"
              ? "wss://host/path"
              : protocol === "grpc"
                ? "https://host:443"
                : "{{baseUrl}}/path"
          }
          value={doc.request.url}
          onChange={(url) => setRequest({ url })}
          variables={variables}
        />
        {protocol !== "websocket" && protocol !== "grpc" && (
          <Button
            variant="primary"
            className="h-8 px-4 font-semibold shrink-0"
            onClick={() => void sendActive()}
            disabled={tab.loading}
          >
            {tab.loading ? <Spinner size={12} /> : <Play size={12} />}
            Send
          </Button>
        )}
        <IconButton
          title={`Save (${formatCombo(comboFor("save", shortcuts))})`}
          onClick={() => void saveActive()}
          className="relative shrink-0"
        >
          <Save size={14} />
          {isDirty && (
            <span className="absolute top-1 right-1 h-1.5 w-1.5 rounded-full bg-warn" />
          )}
        </IconButton>
        <IconButton
          title="Generate code"
          onClick={() => openCodegen(tab.path)}
          className="shrink-0"
        >
          <Braces size={14} />
        </IconButton>
      </div>

      {protocol === "websocket" && (
        <div className="flex-1 min-h-0">
          <WsPanel path={tab.path} doc={doc} setDoc={setDoc} toast={toast} />
        </div>
      )}
      {protocol === "grpc" && (
        <div className="flex-1 min-h-0 flex flex-col">
          <GrpcPanel doc={doc} setDoc={setDoc} toast={toast} onResult={setGrpcResult} />
          {grpcResult && (
            <pre className="mx-2 mb-2 p-2 text-xs font-mono bg-bg-1 border border-line-0 rounded overflow-auto max-h-48 shrink-0">
              {grpcResult}
            </pre>
          )}
        </div>
      )}

      {/* Editor + response split */}
      {protocol !== "websocket" && protocol !== "grpc" && (
      <SplitPane
        key={splitDirection}
        className="flex-1 min-h-0"
        direction={splitDirection}
        initial={splitDirection === "horizontal" ? 480 : 320}
        min={140}
        max={splitDirection === "horizontal" ? 960 : 620}
        top={
          <div className="h-full flex flex-col">
            <EditorTabBar
              tabs={editorTabs}
              active={editorTab}
              onChange={(id) => setEditorTab(id as EditorTabId)}
            />
            <div className="flex-1 min-h-0 overflow-y-auto">
              {editorTab === "params" && (
                <div className="p-2 flex flex-col gap-3">
                  <KVEditor
                    rows={doc.request.params}
                    onChange={(rows: KV[]) => setRequest({ params: rows })}
                    namePlaceholder="Param"
                    valuePlaceholder="Value"
                    variables={variables}
                  />
                  <PathParamsRows
                    url={doc.request.url}
                    rows={doc.request.pathParams}
                    onChange={(rows: KV[]) => setRequest({ pathParams: rows })}
                    variables={variables}
                  />
                </div>
              )}
              {editorTab === "headers" && (
                <div className="p-2">
                  <KVEditor
                    rows={doc.request.headers}
                    onChange={(rows: KV[]) => setRequest({ headers: rows })}
                    namePlaceholder="Header"
                    valuePlaceholder="Value"
                    variables={variables}
                  />
                </div>
              )}
              {editorTab === "auth" && (
                <div className="p-2">
                  <AuthEditor
                    auth={doc.auth ?? { type: "none" }}
                    onChange={(auth: Auth) => setDoc({ ...doc, auth })}
                    variables={variables}
                  />
                </div>
              )}
              {editorTab === "body" && (
                <div className="h-full min-h-0 flex flex-col">
                  <BodyTab doc={doc} setDoc={setDoc} toast={toast} variables={variables} />
                  {protocol === "graphql" && doc.request.body?.type === "graphql" && (
                    <QueryBuilder doc={doc} setDoc={setDoc} toast={toast} />
                  )}
                </div>
              )}
              {editorTab === "scripts" && (
                <ScriptsTab doc={doc} setDoc={setDoc} />
              )}
              {editorTab === "tests" && (
                <TestsTab doc={doc} setDoc={setDoc} result={tab.result} />
              )}
              {editorTab === "docs" && (
                <DocsTab doc={doc} setDoc={setDoc} />
              )}
            </div>
          </div>
        }
        bottom={<ResponseViewer />}
      />
      )}
    </div>
  );
}

/* ---------- Body tab ---------- */

/**
 * Rewrites `#{…}` / `{{…}}` tags in JSON text, reporting whether each one
 * sits inside a JSON string (vs. in a value position). String state tracks
 * escapes so `\"` inside a value doesn't flip it.
 */
function mapJsonTags(text: string, f: (tag: string, inString: boolean) => string): string {
  const re = /#\{[^{}#]*\}|\{\{[^{}]*\}\}/g;
  let out = "";
  let last = 0;
  let inStr = false;
  let esc = false;
  const track = (s: string) => {
    for (const ch of s) {
      if (esc) {
        esc = false;
        continue;
      }
      if (ch === "\\" && inStr) esc = true;
      else if (ch === '"') inStr = !inStr;
    }
  };
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    const before = text.slice(last, m.index);
    out += before;
    track(before);
    const emitted = f(m[0], inStr);
    out += emitted;
    track(emitted);
    last = m.index + m[0].length;
  }
  return out + text.slice(last);
}

/** Pretty-prints JSON while preserving `#{…}` / `{{…}}` tags, bare or not. */
export function formatJsonKeepingTags(content: string): string {
  const masks: { tag: string; outside: boolean }[] = [];
  const masked = mapJsonTags(content, (tag, inString) => {
    const token = `__KEEL_TAG_${masks.length}__`;
    masks.push({ tag, outside: !inString });
    // A bare tag sits in a value position — give the token quotes so the
    // masked text parses; inside a string the quotes are already there.
    return inString ? token : `"${token}"`;
  });
  const parsed = JSON.parse(masked);
  let out = JSON.stringify(parsed, null, 2);
  for (let i = 0; i < masks.length; i++) {
    const { tag, outside } = masks[i];
    const token = `__KEEL_TAG_${i}__`;
    out = outside
      ? out.replace(`"${token}"`, tag) // drop the JSON quotes again
      : out.split(token).join(tag); // substitute within the string
  }
  return out;
}

const BODY_TYPES: { value: Body["type"]; label: string }[] = [
  { value: "none", label: "None" },
  { value: "json", label: "JSON" },
  { value: "text", label: "Text" },
  { value: "xml", label: "XML" },
  { value: "form-urlencoded", label: "Form URL-encoded" },
  { value: "multipart", label: "Multipart" },
  { value: "binary", label: "Binary" },
  { value: "graphql", label: "GraphQL" },
];

function BodyTab({
  doc,
  setDoc,
  toast,
  variables,
}: {
  doc: RequestDoc;
  setDoc: (d: RequestDoc) => void;
  toast: (msg: string, kind?: "error" | "success" | "info") => void;
  variables: VariableSuggestion[];
}) {
  const body: Body = doc.request.body ?? { type: "none" };

  const setBody = (b: Body) =>
    setDoc({ ...doc, request: { ...doc.request, body: b } });

  const changeType = (t: Body["type"]) => {
    if (t === "none") setBody({ type: "none" });
    else if (t === "json" || t === "text" || t === "xml")
      setBody({ type: t, content: "" });
    else if (t === "form-urlencoded" || t === "multipart")
      setBody({ type: t, items: [] });
    else if (t === "graphql") setBody({ type: "graphql", query: "", variables: "" });
    else setBody({ type: "binary", path: "" });
  };

  const browseBinary = async () => {
    const selected = await openFileDialog({ multiple: false });
    if (typeof selected === "string") setBody({ type: "binary", path: selected });
  };

  const formatJson = () => {
    if (body.type !== "json") return;
    try {
      setBody({ ...body, content: formatJsonKeepingTags(body.content) });
    } catch (e) {
      toast(`Invalid JSON: ${String(e)}`, "error");
    }
  };

  return (
    <div className="h-full min-h-0 flex flex-col">
      <div className="flex items-center gap-2 p-2 shrink-0">
        <Select
          value={body.type}
          onChange={(e) => changeType(e.target.value as Body["type"])}
          className="w-44"
        >
          {BODY_TYPES.map((t) => (
            <option key={t.value} value={t.value}>
              {t.label}
            </option>
          ))}
        </Select>
        {body.type === "json" && (
          <Button variant="ghost" className="h-7" onClick={formatJson}>
            Format
          </Button>
        )}
      </div>

      {(body.type === "json" || body.type === "text" || body.type === "xml") && (
        <div className="flex-1 min-h-0 flex flex-col bg-bg-1">
          <div className="shrink-0 px-2 pt-1 text-[10px] font-mono text-fg-2">
            {"#{body.accessToken} · #{header.X-Trace} · #{status} — from the previous response"}
          </div>
          <div className="flex-1 min-h-0">
            <CodeEditor
              value={body.content}
              onChange={(v) => setBody({ ...body, content: v })}
              language={body.type === "json" ? "json" : "text"}
              appearance="editor"
              lineNumbers
              height="100%"
              variables={variables}
              placeholder={
                body.type === "json" ? '{\n  "key": "#{body.accessToken}"\n}' : "Body…"
              }
            />
          </div>
        </div>
      )}

      {(body.type === "form-urlencoded" || body.type === "multipart") && (
        <div className="p-2 pt-0 flex flex-col gap-1.5">
          <KVEditor
            rows={body.items}
            onChange={(items) => setBody({ ...body, items })}
            allowFile={body.type === "multipart"}
            namePlaceholder="Name"
            valuePlaceholder="Value"
            variables={variables}
          />
          {body.type === "multipart" && (
            <div className="text-xs text-fg-2">
              File rows send the file at the given path.
            </div>
          )}
        </div>
      )}

      {body.type === "binary" && (
        <div className="flex items-center gap-1.5 px-2">
          <TextInput
            value={body.path}
            onChange={(e) => setBody({ ...body, path: e.target.value })}
            placeholder="/path/to/file"
            className="flex-1 font-mono"
          />
          <IconButton title="Browse for file" onClick={() => void browseBinary()}>
            <FolderOpen size={14} />
          </IconButton>
        </div>
      )}

      {body.type === "graphql" && (
        <div className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-2 pb-2">
          <div className="flex flex-col gap-1">
            <div className="px-2 text-xs font-semibold text-fg-1">Query</div>
            <div className="h-44 shrink-0 bg-bg-1">
              <CodeEditor
                value={body.query}
                onChange={(v) => setBody({ ...body, query: v })}
                language="text"
                appearance="editor"
                lineNumbers
                height="100%"
                variables={variables}
                placeholder="{ me { name } }"
              />
            </div>
          </div>
          <div className="flex flex-col gap-1">
            <div className="px-2 flex items-baseline gap-2">
              <span className="text-xs font-semibold text-fg-1">
                Variables (JSON)
              </span>
              <span className="text-[10px] text-fg-2">left blank for none</span>
            </div>
            <div className="h-28 shrink-0 bg-bg-1">
              <CodeEditor
                value={body.variables ?? ""}
                onChange={(v) => setBody({ ...body, variables: v })}
                language="json"
                appearance="editor"
                lineNumbers
                height="100%"
                variables={variables}
                placeholder='{ "id": 1 }'
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/* ---------- Scripts tab ---------- */

type ScriptTabId = "preRequest" | "postResponse";

const SCRIPT_TABS: { id: ScriptTabId; label: string; placeholder: string }[] = [
  { id: "preRequest", label: "Pre-request", placeholder: 'set("token", "{{accessToken}}")' },
  { id: "postResponse", label: "Post-response", placeholder: 'set("userId", json("data.id"))' },
];

function ScriptsTab({
  doc,
  setDoc,
}: {
  doc: RequestDoc;
  setDoc: (d: RequestDoc) => void;
}) {
  const scripts = doc.scripts ?? {};
  const [scriptTab, setScriptTab] = useState<ScriptTabId>("preRequest");

  const setScript = (k: ScriptTabId, v: string) =>
    setDoc({ ...doc, scripts: { ...scripts, [k]: v } });

  const active = SCRIPT_TABS.find((t) => t.id === scriptTab)!;

  return (
    <div className="h-full min-h-0 flex flex-col bg-bg-1 text-fg-0">
      {/* Tabs — clean strip, subtle active underline, no boxed tabs */}
      <div className="h-9 shrink-0 flex items-stretch px-2 border-b border-line-0">
        {SCRIPT_TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            onClick={() => setScriptTab(t.id)}
            className={cn(
              "px-3 text-xs border-b-2 -mb-px transition-colors",
              scriptTab === t.id
                ? "border-accent text-fg-0 font-medium"
                : "border-transparent text-fg-2 hover:text-fg-0",
            )}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Editor — borderless surface with line numbers */}
      <div className="flex-1 min-h-0">
        <CodeEditor
          value={scripts[scriptTab] ?? ""}
          onChange={(v) => setScript(scriptTab, v)}
          language="javascript"
          appearance="editor"
          lineNumbers
          height="100%"
          placeholder={active.placeholder}
        />
      </div>
    </div>
  );
}

/* ---------- Tests tab ---------- */

function TestsTab({
  doc,
  setDoc,
  result,
}: {
  doc: RequestDoc;
  setDoc: (d: RequestDoc) => void;
  result: SendResult | null;
}) {
  const tests = doc.tests ?? [];

  const setTests = (next: TestAssertion[]) =>
    setDoc({ ...doc, tests: next });

  const setRow = (i: number, patch: Partial<TestAssertion>) => {
    setTests(tests.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
  };

  const removeRow = (i: number) => setTests(tests.filter((_, idx) => idx !== i));

  const addTest = () => setTests([...tests, { expect: "response.status" }]);

  const setMatcher = (i: number, m: TestMatcherName | "") => {
    const row = tests[i];
    const stripped: TestAssertion = { expect: row.expect };
    const prev = currentMatcher(row);
    const prevValue = prev ? row[prev] : undefined;
    if (m) {
      if (m === "toBeTruthy" || m === "toBeNull") {
        stripped[m] = true;
      } else if (prevValue != null) {
        stripped[m] = coerceValue(String(prevValue));
      }
    }
    setTests(tests.map((r, idx) => (idx === i ? stripped : r)));
  };

  const setMatcherValue = (i: number, raw: string) => {
    const row = tests[i];
    const m = currentMatcher(row);
    if (!m) return;
    setRow(i, { [m]: coerceValue(raw) });
  };

  const testResults = result?.testResults ?? [];

  return (
    <div className="p-2 flex flex-col gap-1">
      {tests.map((row, i) => {
        const m = currentMatcher(row);
        const needsValue = m !== "" && m !== "toBeTruthy" && m !== "toBeNull";
        return (
          <div key={i} className="flex items-center gap-1.5">
            <TextInput
              value={row.expect}
              onChange={(e) => setRow(i, { expect: e.target.value })}
              placeholder="response.status"
              className="flex-1 min-w-0 font-mono"
            />
            <Select
              value={m}
              onChange={(e) => setMatcher(i, e.target.value as TestMatcherName | "")}
              className="w-36 shrink-0"
            >
              {MATCHERS.map((mo) => (
                <option key={mo || "none"} value={mo}>
                  {mo || "matcher"}
                </option>
              ))}
            </Select>
            {needsValue && (
              <TextInput
                value={m ? String(row[m] ?? "") : ""}
                onChange={(e) => setMatcherValue(i, e.target.value)}
                placeholder="expected"
                className="flex-1 min-w-0 font-mono"
              />
            )}
            <IconButton
              className="h-6 w-6 hover:text-danger"
              title="Remove test"
              onClick={() => removeRow(i)}
            >
              <X size={12} />
            </IconButton>
          </div>
        );
      })}
      <div>
        <button
          type="button"
          onClick={addTest}
            className="inline-flex items-center gap-1 rounded px-1.5 h-6 text-xs text-fg-2 hover:text-fg-0 hover:bg-bg-hover transition-colors"
        >
          <Plus size={12} />
          Add test
        </button>
      </div>

      {testResults.length > 0 && (
        <div className="mt-2 rounded border border-line-0 overflow-hidden">
          <div className="h-7 flex items-center px-2 border-b border-line-0 bg-bg-2 text-xs font-semibold text-fg-1">
            Last run
          </div>
          <div className="flex flex-col">
            {testResults.map((r, i) => (
              <div
                key={i}
                className="flex items-start gap-2 px-2 py-1.5 text-xs border-b border-line-0 last:border-b-0"
              >
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
                {r.message && (
                  <span className="text-danger">{r.message}</span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export { RequestEditor };
