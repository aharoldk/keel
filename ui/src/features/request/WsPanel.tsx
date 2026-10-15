import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import {
  ArrowDownLeft,
  ArrowDownToLine,
  ArrowUpRight,
  Check,
  Copy,
  CornerDownLeft,
  Radio,
  Search,
  Send,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/api/client";
import type { Auth, KV, RequestDoc, WsEvent } from "@/api/types";
import { Button, TextInput } from "@/components/ui";
import { SplitPane } from "@/components/SplitPane";
import { cn } from "@/utils";
import { formatJsonKeepingTags } from "@/features/request/formatJson";
import EditorTabBar, { type EditorTabItem } from "@/features/request/EditorTabBar";
import KVEditor from "@/features/request/KVEditor";
import PathParamsRows from "@/features/request/PathParamsRows";
import AuthEditor from "@/features/collections/AuthEditor";
import DocsTab from "@/features/request/DocsTab";
import { useKeel } from "@/state/store";
import type { VariableSuggestion } from "@/features/request/variables";

interface Props {
  path: string;
  doc: RequestDoc;
  setDoc: (d: RequestDoc) => void;
  toast: (msg: string, kind?: "error" | "success" | "info") => void;
  variables?: VariableSuggestion[];
  splitDirection?: "horizontal" | "vertical";
}

export interface Frame {
  id: string;
  dir: "in" | "out" | "sys";
  text: string;
  timestamp: string;
  opcode?: string;
  sizeBytes?: number;
}

export type WsTabId = "message" | "params" | "headers" | "auth" | "docs";

interface WsSessionState {
  wasConnected: boolean;
  frames: Frame[];
  msgFormat?: "json" | "text" | "binary";
  activeTab?: WsTabId;
}

const wsSessionStore = new Map<string, WsSessionState>();

/** Resets all remembered WebSocket session states. Useful for tests. */
export function clearWsSessionStore() {
  wsSessionStore.clear();
}

let frameCounter = 0;
function nextFrameId(): string {
  frameCounter += 1;
  return `ws-frame-${Date.now()}-${frameCounter}`;
}

function formatTime(d: Date): string {
  const pad = (n: number, z = 2) => String(n).padStart(z, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDuration(totalSec: number): string {
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

function tryFormatJson(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return null;
  try {
    const parsed = JSON.parse(trimmed);
    return JSON.stringify(parsed, null, 2);
  } catch {
    return null;
  }
}

function buildWsUrl(rawUrl: string, params?: KV[], variables?: VariableSuggestion[]): string {
  let url = rawUrl.trim();
  if (variables && variables.length > 0) {
    url = url.replace(/\{\{\s*([\w.-]+)\s*\}\}/g, (match, key: string) => {
      const found = variables.find((v) => v.name === key);
      return found?.value !== undefined ? found.value : match;
    });
  }
  const enabled = (params ?? []).filter((p) => p.enabled && p.name.trim().length > 0);
  if (enabled.length === 0) return url;
  try {
    const u = new URL(url);
    for (const p of enabled) {
      let val = p.value;
      if (variables && variables.length > 0) {
        val = val.replace(/\{\{\s*([\w.-]+)\s*\}\}/g, (match, key: string) => {
          const found = variables.find((v) => v.name === key);
          return found?.value !== undefined ? found.value : match;
        });
      }
      u.searchParams.append(p.name.trim(), val);
    }
    return u.toString();
  } catch {
    return url;
  }
}

function buildWsHeaders(headers?: KV[], auth?: Auth, variables?: VariableSuggestion[]): KV[] {
  const list = (headers ?? []).map((h) => {
    let value = h.value;
    if (variables && variables.length > 0) {
      value = value.replace(/\{\{\s*([\w.-]+)\s*\}\}/g, (match, key: string) => {
        const found = variables.find((v) => v.name === key);
        return found?.value !== undefined ? found.value : match;
      });
    }
    return { ...h, value };
  });

  if (auth && auth.type !== "none") {
    const hasAuth = list.some((h) => h.name.toLowerCase() === "authorization");
    if (!hasAuth) {
      if (auth.type === "bearer" && auth.token) {
        list.push({ name: "Authorization", value: `Bearer ${auth.token}`, enabled: true });
      } else if (auth.type === "basic" && (auth.username || auth.password)) {
        try {
          const creds = btoa(`${auth.username ?? ""}:${auth.password ?? ""}`);
          list.push({ name: "Authorization", value: `Basic ${creds}`, enabled: true });
        } catch {
          // ignore encoding failure
        }
      } else if (auth.type === "apikey" && auth.in !== "query" && auth.key && auth.value) {
        list.push({ name: auth.key, value: auth.value, enabled: true });
      }
    }
  }
  return list;
}

const PRESETS = [
  { label: "Ping", text: '{"type":"ping"}' },
  { label: "Subscribe", text: '{"action":"subscribe","topic":"events"}' },
  { label: "Echo", text: '{"message":"hello world"}' },
];

export default function WsPanel({
  path,
  doc,
  setDoc,
  toast,
  variables,
  splitDirection = "horizontal",
}: Props) {
  const initialSession = wsSessionStore.get(path);
  const [open, setOpen] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [activeTab, setActiveTab] = useState<WsTabId>(initialSession?.activeTab ?? "message");
  const [draft, setDraft] = useState(
    doc.request.body && "content" in doc.request.body
      ? doc.request.body.content
      : "",
  );
  const [msgFormat, setMsgFormat] = useState<"json" | "text" | "binary">(
    initialSession?.msgFormat ?? "json",
  );
  const [frames, setFrames] = useState<Frame[]>(initialSession?.frames ?? []);
  const [filterText, setFilterText] = useState("");
  const [filterDir, setFilterDir] = useState<"all" | "in" | "out">("all");
  const [autoScroll, setAutoScroll] = useState(true);
  const [elapsedSec, setElapsedSec] = useState(0);
  const [expandedFrameIds, setExpandedFrameIds] = useState<Set<string>>(new Set());
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const streamEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const docRef = useRef(doc);
  docRef.current = doc;
  const variablesRef = useRef(variables);
  variablesRef.current = variables;
  const framesRef = useRef(frames);
  framesRef.current = frames;
  const openRef = useRef(open);
  openRef.current = open;
  const connectingRef = useRef(connecting);
  connectingRef.current = connecting;
  const autoReconnectedRef = useRef(false);

  // Connection timer
  useEffect(() => {
    useKeel.getState().setWsConnected(path, open);
    if (!open) {
      setElapsedSec(0);
      return;
    }
    const start = Date.now();
    const timer = setInterval(() => {
      setElapsedSec(Math.floor((Date.now() - start) / 1000));
    }, 1000);
    return () => clearInterval(timer);
  }, [path, open]);

  // Auto-scroll when new frames arrive
  useEffect(() => {
    if (autoScroll) {
      streamEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [frames, autoScroll]);

  // Persist session state into module cache so switching tabs preserves messages & state
  useEffect(() => {
    const session = wsSessionStore.get(path) ?? { wasConnected: false, frames: [] };
    session.frames = frames;
    session.msgFormat = msgFormat;
    session.activeTab = activeTab;
    wsSessionStore.set(path, session);
  }, [path, frames, msgFormat, activeTab]);

  const connect = useCallback(async () => {
    if (connectingRef.current || openRef.current) return;
    connectingRef.current = true;
    setConnecting(true);
    try {
      const currentDoc = docRef.current;
      const currentVars = variablesRef.current;
      const protocols = (currentDoc.websocket?.protocols ?? "")
        .split(",")
        .map((p) => p.trim())
        .filter(Boolean);
      const url = buildWsUrl(currentDoc.request.url, currentDoc.request.params, currentVars);
      const headers = buildWsHeaders(currentDoc.request.headers, currentDoc.auth, currentVars);
      await api.wsConnect(path, url, headers, protocols);
      const session = wsSessionStore.get(path) ?? { wasConnected: false, frames: [] };
      session.wasConnected = true;
      wsSessionStore.set(path, session);
    } catch (e) {
      connectingRef.current = false;
      setConnecting(false);
      toast(String(e), "error");
    }
  }, [path, toast]);

  const disconnect = useCallback(async () => {
    connectingRef.current = false;
    openRef.current = false;
    setOpen(false);
    setConnecting(false);
    useKeel.getState().setWsConnected(path, false);
    // User explicitly disconnected -> do not remember to reconnect
    const session = wsSessionStore.get(path);
    if (session) {
      session.wasConnected = false;
    }
    try {
      await api.wsClose(path);
    } catch (e) {
      toast(String(e), "error");
    }
  }, [path, toast]);

  // Auto-reconnect if session was connected before tab switch
  useEffect(() => {
    if (autoReconnectedRef.current) return;
    autoReconnectedRef.current = true;
    const session = wsSessionStore.get(path);
    if (session?.wasConnected && !openRef.current && !connectingRef.current) {
      void connect();
    }
  }, [path, connect]);

  // WebSocket event listener
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<WsEvent>("ws://message", (event) => {
      if (disposed || event.payload.sessionId !== path) return;
      const kind = event.payload.kind;
      if (kind === "open") {
        openRef.current = true;
        connectingRef.current = false;
        setOpen(true);
        setConnecting(false);
        const session = wsSessionStore.get(path) ?? { wasConnected: false, frames: [] };
        session.wasConnected = true;
        wsSessionStore.set(path, session);
      }
      if (kind === "close" || kind === "error") {
        openRef.current = false;
        connectingRef.current = false;
        setOpen(false);
        setConnecting(false);
      }
      const text =
        kind === "message"
          ? event.payload.data ?? ""
          : kind === "error"
            ? event.payload.data ?? "error"
            : kind === "open"
              ? `connected${event.payload.data ? ` (${event.payload.data})` : ""}`
              : "closed";
      const dir: Frame["dir"] = kind === "message" ? "in" : "sys";
      const opcode = event.payload.opcode ?? (kind === "message" ? "text" : kind);
      const sizeBytes =
        kind === "message" && event.payload.data
          ? new TextEncoder().encode(event.payload.data).length
          : undefined;

      setFrames((cur) =>
        [
          ...cur,
          {
            id: nextFrameId(),
            dir,
            text,
            timestamp: formatTime(new Date()),
            opcode,
            sizeBytes,
          },
        ].slice(-300),
      );
    }).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlisten = fn;
      }
    });
    return () => {
      disposed = true;
      unlisten?.();
      // On unmount (tab switch / close), preserve session state and close backend socket
      const session = wsSessionStore.get(path);
      if (session) {
        session.frames = framesRef.current;
        if (openRef.current) {
          session.wasConnected = true;
        }
      }
      void api.wsClose(path);
    };
  }, [path]);

  const send = async () => {
    if (!draft) return;
    try {
      const isBinary = msgFormat === "binary";
      if (isBinary) {
        await api.wsSend(path, draft, true);
      } else {
        await api.wsSend(path, draft);
      }
      setFrames((cur) => [
        ...cur,
        {
          id: nextFrameId(),
          dir: "out",
          text: draft,
          timestamp: formatTime(new Date()),
          opcode: isBinary ? "binary" : "text",
          sizeBytes: new TextEncoder().encode(draft).length,
        },
      ]);
      setDraft("");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const beautifyDraft = () => {
    if (!draft.trim()) return;
    try {
      setDraft(formatJsonKeepingTags(draft));
    } catch {
      try {
        setDraft(JSON.stringify(JSON.parse(draft), null, 2));
      } catch (e) {
        toast(`Invalid JSON: ${String(e)}`, "error");
      }
    }
  };

  const copyPayload = async (text: string, id: string) => {
    try {
      await writeText(text);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 1200);
      toast("Copied to clipboard", "success");
    } catch {
      try {
        await navigator.clipboard.writeText(text);
        setCopiedId(id);
        setTimeout(() => setCopiedId(null), 1200);
        toast("Copied to clipboard", "success");
      } catch (e) {
        toast(String(e), "error");
      }
    }
  };

  const loadDraftIntoComposer = (text: string) => {
    setDraft(text);
    setActiveTab("message");
    toast("Loaded into composer", "info");
    textareaRef.current?.focus();
  };

  const toggleFrameExpand = (id: string) => {
    setExpandedFrameIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Tab") {
      e.preventDefault();
      const target = e.currentTarget;
      const start = target.selectionStart;
      const end = target.selectionEnd;
      const next = draft.substring(0, start) + "  " + draft.substring(end);
      setDraft(next);
      requestAnimationFrame(() => {
        target.selectionStart = target.selectionEnd = start + 2;
      });
      return;
    }
    if (e.key === "Enter" && (!e.shiftKey || e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void send();
    }
  };

  const inCount = useMemo(() => frames.filter((f) => f.dir === "in").length, [frames]);
  const outCount = useMemo(() => frames.filter((f) => f.dir === "out").length, [frames]);

  const filteredFrames = useMemo(() => {
    return frames.filter((f) => {
      if (filterDir === "in" && f.dir !== "in") return false;
      if (filterDir === "out" && f.dir !== "out") return false;
      if (filterText.trim()) {
        const q = filterText.toLowerCase();
        const matchText = f.text.toLowerCase().includes(q);
        const matchOp = f.opcode ? f.opcode.toLowerCase().includes(q) : false;
        return matchText || matchOp;
      }
      return true;
    });
  }, [frames, filterDir, filterText]);

  const editorTabs: EditorTabItem[] = useMemo(
    () => [
      {
        id: "message",
        label: "Message",
        icon: open ? (
          <span className="h-2 w-2 rounded-full bg-emerald-500 animate-pulse shrink-0" />
        ) : undefined,
      },
      { id: "params", label: "Params", badge: (doc.request.params ?? []).length },
      { id: "headers", label: "Headers", badge: (doc.request.headers ?? []).length },
      { id: "auth", label: "Auth" },
      { id: "docs", label: "Docs" },
    ],
    [doc.request.params, doc.request.headers, open],
  );

  return (
    <div className="h-full min-h-0 flex flex-col bg-bg-0">
      {/* Connection Header Bar */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-line-0 bg-bg-1 shrink-0">
        <div className="flex items-center gap-1.5 shrink-0">
          <span
            className={cn(
              "h-2 w-2 rounded-full shrink-0 transition-colors",
              open ? "bg-emerald-500 animate-pulse" : "bg-fg-3",
            )}
          />
          <span className="text-[11px] font-mono font-medium text-fg-2">
            {open ? "open" : "closed"}
          </span>
          {open && (
            <span className="text-[10px] font-mono text-fg-3 bg-bg-2 px-1.5 py-0.5 rounded ml-1">
              {formatDuration(elapsedSec)}
            </span>
          )}
        </div>

        {frames.length > 0 && (
          <div className="hidden sm:flex items-center gap-1 text-[10px] font-mono text-fg-3 shrink-0">
            <span>↑ {outCount}</span>
            <span>·</span>
            <span>↓ {inCount}</span>
          </div>
        )}

        <div className="flex-1" />

        <div className="flex items-center gap-2 shrink-0">
          <span className="text-[11px] text-fg-2 font-mono hidden md:inline">Protocols:</span>
          <TextInput
            className="w-40 font-mono text-xs"
            placeholder="protocol"
            title="Comma-separated WebSocket subprotocols"
            value={doc.websocket?.protocols ?? ""}
            onChange={(e) =>
              setDoc({ ...doc, websocket: { ...doc.websocket, protocols: e.target.value } })
            }
          />
          {open ? (
            <Button
              variant="danger"
              className="h-7 px-3 text-xs font-semibold shrink-0"
              onClick={() => void disconnect()}
            >
              Disconnect
            </Button>
          ) : (
            <Button
              variant="primary"
              className="h-7 px-3 text-xs font-semibold shrink-0"
              disabled={connecting}
              onClick={() => void connect()}
            >
              {connecting ? "Connecting…" : "Connect"}
            </Button>
          )}
        </div>
      </div>

      {/* Main Split Area */}
      <SplitPane
        direction={splitDirection}
        initial={splitDirection === "horizontal" ? 450 : 280}
        min={180}
        max={splitDirection === "horizontal" ? 850 : 550}
        className="flex-1 min-h-0"
        top={
          <div className="h-full flex flex-col min-h-0">
            <EditorTabBar
              tabs={editorTabs}
              active={activeTab}
              onChange={(id) => setActiveTab(id as WsTabId)}
            />

            <div className="flex-1 min-h-0 overflow-y-auto">
              {activeTab === "message" && (
                <div className="h-full flex flex-col p-2 gap-2 min-h-0">
                  <div className="flex items-center gap-2 shrink-0">
                    <div className="flex items-center rounded border border-line-0 p-0.5 bg-bg-1 text-[11px] font-mono">
                      {(["json", "text", "binary"] as const).map((fmt) => (
                        <button
                          key={fmt}
                          type="button"
                          onClick={() => setMsgFormat(fmt)}
                          className={cn(
                            "px-2 py-0.5 rounded transition-colors uppercase font-semibold text-[10px]",
                            msgFormat === fmt
                              ? "bg-bg-3 text-fg-0 shadow-sm"
                              : "text-fg-2 hover:text-fg-1",
                          )}
                        >
                          {fmt}
                        </button>
                      ))}
                    </div>

                    <Button
                      variant="ghost"
                      className="h-6 px-1.5 text-[11px] text-fg-2 hover:text-fg-0 gap-1"
                      onClick={beautifyDraft}
                      title="Format JSON"
                    >
                      <Sparkles size={11} />
                      Beautify
                    </Button>

                    <div className="flex items-center gap-1 ml-auto">
                      <span className="text-[10px] text-fg-3 mr-1 hidden sm:inline">Presets:</span>
                      {PRESETS.map((p) => (
                        <button
                          key={p.label}
                          type="button"
                          className="text-[10px] px-1.5 py-0.5 rounded bg-bg-2 hover:bg-bg-3 text-fg-2 hover:text-fg-0 transition-colors font-mono"
                          onClick={() => {
                            setDraft(p.text);
                            setMsgFormat("json");
                          }}
                        >
                          {p.label}
                        </button>
                      ))}
                    </div>
                  </div>

                  <div className="flex-1 min-h-[140px] flex flex-col relative">
                    <textarea
                      ref={textareaRef}
                      className="flex-1 w-full min-h-[120px] p-2.5 font-mono text-xs bg-bg-1 border border-line-0 rounded resize-none focus:outline-none focus:border-line-focus text-fg-0 placeholder:text-fg-3 disabled:opacity-40 disabled:bg-bg-0"
                      placeholder="Message"
                      value={draft}
                      disabled={!open}
                      onChange={(e) => setDraft(e.target.value)}
                      onKeyDown={handleKeyDown}
                    />
                  </div>

                  <div className="flex items-center justify-between gap-2 shrink-0 pt-1">
                    <span className="text-[10px] text-fg-3 font-mono">
                      Enter to send · Shift+Enter for newline
                    </span>
                    <Button
                      variant="primary"
                      className="h-7 px-4 font-semibold shrink-0 gap-1.5"
                      disabled={!open}
                      onClick={() => void send()}
                    >
                      <Send size={12} />
                      Send
                    </Button>
                  </div>
                </div>
              )}

              {activeTab === "params" && (
                <div className="p-2 flex flex-col gap-3">
                  <div className="text-[11px] text-fg-2">
                    Handshake Query Parameters appended to WebSocket URL:
                  </div>
                  <KVEditor
                    rows={doc.request.params}
                    onChange={(rows: KV[]) =>
                      setDoc({ ...doc, request: { ...doc.request, params: rows } })
                    }
                    namePlaceholder="Param"
                    valuePlaceholder="Value"
                    variables={variables}
                  />
                  <PathParamsRows
                    url={doc.request.url}
                    rows={doc.request.pathParams}
                    onChange={(rows: KV[]) =>
                      setDoc({ ...doc, request: { ...doc.request, pathParams: rows } })
                    }
                    variables={variables}
                  />
                </div>
              )}

              {activeTab === "headers" && (
                <div className="p-2 flex flex-col gap-2">
                  <div className="text-[11px] text-fg-2">
                    Handshake HTTP Headers sent during connection upgrade:
                  </div>
                  <KVEditor
                    rows={doc.request.headers}
                    onChange={(rows: KV[]) =>
                      setDoc({ ...doc, request: { ...doc.request, headers: rows } })
                    }
                    namePlaceholder="Header"
                    valuePlaceholder="Value"
                    variables={variables}
                  />
                </div>
              )}

              {activeTab === "auth" && (
                <div className="p-2">
                  <AuthEditor
                    auth={doc.auth ?? { type: "none" }}
                    onChange={(auth: Auth) => setDoc({ ...doc, auth })}
                    variables={variables}
                  />
                </div>
              )}

              {activeTab === "docs" && <DocsTab doc={doc} setDoc={setDoc} />}
            </div>
          </div>
        }
        bottom={
          <div className="h-full flex flex-col min-h-0 bg-bg-0">
            {/* Messages Stream Toolbar */}
            <div className="flex items-center gap-2 px-3 py-1.5 border-b border-line-0 bg-bg-1 shrink-0">
              <span className="text-xs font-semibold text-fg-1 shrink-0 flex items-center gap-1.5">
                <Radio size={12} className={open ? "text-emerald-400" : "text-fg-3"} />
                Messages
                <span className="text-[10px] text-fg-3 font-normal font-mono">
                  ({filteredFrames.length})
                </span>
              </span>

              <div className="flex-1 max-w-xs relative">
                <Search size={11} className="absolute left-2 top-2 text-fg-3 pointer-events-none" />
                <TextInput
                  className="h-6 pl-6 pr-5 text-[11px] font-mono w-full"
                  placeholder="Filter messages..."
                  value={filterText}
                  onChange={(e) => setFilterText(e.target.value)}
                />
                {filterText && (
                  <button
                    type="button"
                    className="absolute right-1.5 top-1.5 text-fg-3 hover:text-fg-1"
                    onClick={() => setFilterText("")}
                  >
                    <X size={10} />
                  </button>
                )}
              </div>

              <div className="flex items-center rounded border border-line-0 p-0.5 bg-bg-2 text-[10px] font-mono shrink-0">
                {(["all", "in", "out"] as const).map((dir) => (
                  <button
                    key={dir}
                    type="button"
                    onClick={() => setFilterDir(dir)}
                    className={cn(
                      "px-1.5 py-0.5 rounded transition-colors uppercase font-semibold",
                      filterDir === dir
                        ? "bg-bg-0 text-fg-0 shadow-sm"
                        : "text-fg-2 hover:text-fg-0",
                    )}
                  >
                    {dir === "all" ? "All" : dir === "in" ? `In (${inCount})` : `Out (${outCount})`}
                  </button>
                ))}
              </div>

              <div className="flex items-center gap-1 ml-auto shrink-0">
                <button
                  type="button"
                  title={autoScroll ? "Auto-scroll enabled" : "Auto-scroll paused"}
                  onClick={() => setAutoScroll((v) => !v)}
                  className={cn(
                    "p-1 rounded transition-colors",
                    autoScroll
                      ? "text-accent bg-accent/10 hover:bg-accent/20"
                      : "text-fg-3 hover:text-fg-1 bg-bg-2",
                  )}
                >
                  <ArrowDownToLine size={12} />
                </button>
                <button
                  type="button"
                  title="Clear messages"
                  onClick={() => setFrames([])}
                  className="p-1 rounded text-fg-3 hover:text-danger hover:bg-bg-2 transition-colors"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            </div>

            {/* Messages Stream List */}
            <div className="flex-1 min-h-0 overflow-y-auto p-2 font-mono text-xs flex flex-col gap-1.5">
              {filteredFrames.length === 0 ? (
                <div className="flex-1 flex flex-col items-center justify-center p-6 text-center text-fg-3">
                  <Radio size={24} className="mb-2 opacity-30" />
                  <p className="text-xs font-sans text-fg-2 mb-1">No messages</p>
                  <p className="text-[11px] font-sans text-fg-3 max-w-xs">
                    {frames.length > 0
                      ? "No messages match your current filter."
                      : "Connect to the WebSocket server to view real-time messages and events."}
                  </p>
                </div>
              ) : (
                filteredFrames.map((frame) => {
                  const formattedJson = tryFormatJson(frame.text);
                  const isExpanded = expandedFrameIds.has(frame.id);

                  return (
                    <div
                      key={frame.id}
                      className={cn(
                        "group flex flex-col p-2 rounded border transition-colors select-text",
                        frame.dir === "out"
                          ? "bg-accent/5 border-accent/20 hover:border-accent/40"
                          : frame.dir === "in"
                            ? "bg-bg-1 border-line-0 hover:border-line-1"
                            : "bg-bg-0 text-fg-2 border-line-0/50",
                      )}
                    >
                      <div className="flex items-center gap-2 shrink-0">
                        <span
                          className={cn(
                            "text-[9px] px-1 py-0.5 rounded font-bold uppercase shrink-0",
                            frame.dir === "out"
                              ? "bg-accent text-accent-fg"
                              : frame.dir === "in"
                                ? "bg-emerald-500/15 text-emerald-400 border border-emerald-500/20"
                                : "bg-bg-2 text-fg-2",
                          )}
                        >
                          {frame.dir === "out"
                            ? "OUT"
                            : frame.dir === "in"
                              ? "IN"
                              : frame.opcode?.toUpperCase() ?? "SYS"}
                        </span>

                        {frame.dir !== "sys" && frame.opcode && (
                          <span className="text-[10px] text-fg-3 uppercase font-semibold">
                            {frame.opcode}
                          </span>
                        )}

                        {frame.sizeBytes !== undefined && (
                          <span className="text-[10px] text-fg-3">
                            {formatBytes(frame.sizeBytes)}
                          </span>
                        )}

                        <span className="text-[10px] text-fg-3 ml-auto">{frame.timestamp}</span>

                        <div className="opacity-0 group-hover:opacity-100 flex items-center gap-1 transition-opacity">
                          {formattedJson && (
                            <button
                              type="button"
                              title={isExpanded ? "Collapse formatted JSON" : "Format JSON"}
                              className="p-1 hover:text-fg-0 text-fg-2 rounded hover:bg-bg-2 text-[10px] font-sans"
                              onClick={() => toggleFrameExpand(frame.id)}
                            >
                              {isExpanded ? "Raw" : "JSON"}
                            </button>
                          )}
                          <button
                            type="button"
                            title="Copy message"
                            className="p-1 hover:text-fg-0 text-fg-2 rounded hover:bg-bg-2"
                            onClick={() => void copyPayload(frame.text, frame.id)}
                          >
                            {copiedId === frame.id ? (
                              <Check size={11} className="text-emerald-400" />
                            ) : (
                              <Copy size={11} />
                            )}
                          </button>
                          {frame.dir !== "sys" && (
                            <button
                              type="button"
                              title="Use in composer"
                              className="p-1 hover:text-fg-0 text-fg-2 rounded hover:bg-bg-2"
                              onClick={() => loadDraftIntoComposer(frame.text)}
                            >
                              <CornerDownLeft size={11} />
                            </button>
                          )}
                        </div>
                      </div>

                      {/* Main frame line — retains exact prefix for test assertions */}
                      <div
                        className={cn(
                          "mt-1 whitespace-pre-wrap break-all",
                          frame.dir === "out"
                            ? "text-accent"
                            : frame.dir === "sys"
                              ? "text-fg-2"
                              : "text-fg-0",
                        )}
                      >
                        {frame.dir === "out" ? "→ " : frame.dir === "in" ? "← " : ""}
                        {frame.text}
                      </div>

                      {/* Pretty-printed JSON block if expanded */}
                      {isExpanded && formattedJson && (
                        <pre className="mt-1.5 p-2 bg-bg-2 rounded text-[11px] overflow-x-auto text-fg-1 border border-line-0 leading-relaxed">
                          {formattedJson}
                        </pre>
                      )}
                    </div>
                  );
                })
              )}
              <div ref={streamEndRef} />
            </div>
          </div>
        }
      />
    </div>
  );
}
