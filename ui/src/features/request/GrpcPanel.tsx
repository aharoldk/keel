import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/api/client";
import type { GrpcEvent, GrpcResult, ProtoFile, RequestDoc, SendResult } from "@/api/types";
import { Button } from "@/components/ui";
import SuggestionInput from "@/components/SuggestionInput";
import { cn } from "@/utils";
import CodeEditor from "./CodeEditor";
import { formatJsonKeepingTags } from "./formatJson";

interface Props {
  doc: RequestDoc;
  setDoc: (d: RequestDoc) => void;
  toast: (msg: string, kind?: "error" | "success" | "info") => void;
  /** Emitted for unary calls and when a stream ends — shown in the response pane. */
  onResult: (result: SendResult) => void;
}

/** Debounce window for auto-parsing the proto while the user types. */
const PARSE_DEBOUNCE_MS = 400;

export default function GrpcPanel({ doc, setDoc, toast, onResult }: Props) {
  const [parsed, setParsed] = useState<ProtoFile | null>(null);
  const [parseError, setParseError] = useState<string | null>(null);
  // The body lives in the request doc (type: json) so it saves with the
  // request — like every other editor in keel. The gRPC encoder consumes
  // the parsed JSON at call time; nothing else reads this body.
  const body = doc.request.body?.type === "json" ? doc.request.body.content : "";
  const setBody = (content: string) =>
    setDoc({
      ...doc,
      protocol: "grpc",
      request: { ...doc.request, body: { type: "json", content } },
    });
  const [busy, setBusy] = useState(false);
  const [streaming, setStreaming] = useState(false);
  const [frames, setFrames] = useState<string[]>([]);
  const [protoOpen, setProtoOpen] = useState(false);
  // Proto section height in px — drag its top edge to resize. The editor
  // itself scrolls (CodeMirror), like every other code area in keel.
  const [protoHeight, setProtoHeight] = useState(208);
  const protoDrag = useRef<{ startY: number; startH: number } | null>(null);
  const spec = doc.grpc ?? {};
  const sessionId = `${doc.name}:${spec.service ?? ""}:${spec.method ?? ""}`;

  const service = useMemo(() => {
    if (!parsed || !spec.service) return undefined;
    return parsed.services.find((s) => fullName(parsed.package, s.name) === spec.service || s.name === spec.service);
  }, [parsed, spec.service]);
  const method = service?.methods.find((m) => m.name === spec.method);
  const serviceOptions = useMemo(
    () => (parsed?.services ?? []).map((s) => fullName(parsed?.package ?? "", s.name)),
    [parsed],
  );
  const methodOptions = useMemo(() => (service?.methods ?? []).map((m) => m.name), [service]);

  const setSpec = (patch: Partial<NonNullable<RequestDoc["grpc"]>>) =>
    setDoc({ ...doc, protocol: "grpc", grpc: { ...spec, ...patch } });

  const parse = useCallback(
    async (proto: string, quiet = false) => {
      if (!proto.trim()) {
        setParsed(null);
        setParseError(null);
        return;
      }
      try {
        const file = await api.grpcParseProto(proto);
        setParsed(file);
        setParseError(null);
        const first = file.services[0];
        if (first && !spec.service) {
          setSpec({
            service: fullName(file.package, first.name),
            method: first.methods[0]?.name ?? "",
          });
        }
        if (!quiet) toast("Proto parsed", "success");
      } catch (e) {
        setParsed(null);
        setParseError(String(e));
        if (!quiet) toast(String(e), "error");
      }
    },
    // spec.service / setSpec intentionally excluded: parse() only reads them
    // for the initial service/method pick, which must not re-trigger parses.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [toast],
  );

  // Auto-parse: on mount and whenever the proto text changes, debounced.
  // Keeps the field list (needed to encode the body) in sync without the
  // manual Parse click — an unparsed proto silently drops the body. Stays
  // quiet; the Proto header line shows the status and the Parse button
  // reports errors with a toast.
  const protoText = spec.proto ?? "";
  const firstRun = useRef(true);
  useEffect(() => {
    const timer = setTimeout(() => {
      void parse(protoText, true);
      firstRun.current = false;
    }, firstRun.current ? 0 : PARSE_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [protoText, parse]);

  const framesRef = useRef<string[]>([]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<GrpcEvent>("grpc://message", (event) => {
      if (event.payload.sessionId !== sessionId) return;
      const kind = event.payload.kind;
      const line =
        kind === "message"
          ? event.payload.bodyText ?? ""
          : kind === "trailers"
            ? `trailers status ${event.payload.grpcStatus ?? "?"} ${event.payload.grpcMessage ?? ""}`.trim()
            : kind === "error"
              ? `error ${event.payload.bodyText ?? ""}`
              : "end";
      framesRef.current = [...framesRef.current, line].slice(-200);
      setFrames(framesRef.current);
      if (kind === "end") {
        setStreaming(false);
        onResult({
          requestId: "",
          status: 200,
          statusText: "stream ended",
          ok: event.payload.grpcStatus === 0,
          timeMs: 0,
          sizeBytes: new TextEncoder().encode(framesRef.current.join("\n")).length,
          headers: [],
          cookies: [],
          contentType: "application/grpc",
          bodyText: framesRef.current.join("\n"),
          bodyBase64: null,
          truncated: false,
          error: null,
          variablesUsed: [],
          missingVariables: [],
          secretsUsed: [],
          testResults: [],
          scriptLogs: [],
          scriptError: null,
          timeline: [],
        });
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
      void api.grpcClose(sessionId);
    };
  }, [sessionId, onResult]);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!protoDrag.current) return;
      const { startY, startH } = protoDrag.current;
      // The proto section sits at the bottom — dragging its top edge up
      // grows it, down shrinks it.
      setProtoHeight(Math.max(80, Math.min(560, startH + (startY - e.clientY))));
    };
    const onUp = () => {
      protoDrag.current = null;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  const stop = () => {
    void api.grpcClose(sessionId);
    setStreaming(false);
  };

  const beautify = () => {
    try {
      setBody(formatJsonKeepingTags(body));
    } catch (e) {
      toast(`Invalid JSON: ${String(e)}`, "error");
    }
  };

  const call = async () => {
    if (!spec.service || !spec.method) {
      toast("Pick a service and method", "error");
      return;
    }
    if (!parsed) {
      toast("Proto is not parsed — check the Proto section below", "error");
      return;
    }
    if (method?.clientStreaming) {
      toast("Client streaming is not supported — unary and server streaming only", "error");
      return;
    }
    let json: unknown;
    try {
      json = body.trim() ? JSON.parse(body) : {};
    } catch (e) {
      toast(`Invalid JSON: ${String(e)}`, "error");
      return;
    }
    const input = parsed.messages.find((m) => m.name === method?.input);
    const args = {
      url: doc.request.url.trim(),
      service: spec.service,
      method: spec.method,
      body: json,
      fields: input?.fields ?? [],
      output: method?.output ?? "",
      messages: parsed.messages,
      headers: doc.request.headers ?? [],
    };
    if (method?.serverStreaming) {
      framesRef.current = [];
      setFrames([]);
      setStreaming(true);
      try {
        await api.grpcOpen({ sessionId, ...args });
      } catch (e) {
        setStreaming(false);
        toast(String(e), "error");
      }
      return;
    }
    setBusy(true);
    try {
      const result = await api.grpcCall(args);
      onResult(toSendResult(result));
      if (result.grpcStatus !== 0) toast(result.grpcMessage || `status ${result.grpcStatus}`, "error");
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="h-full min-h-0 flex flex-col">
      {/* ---- Call: service + method ---- */}
      <div className="flex items-center gap-2 p-2 shrink-0">
        <SuggestionInput
          className="flex-1"
          inputClassName="font-mono"
          placeholder="package.Service"
          value={spec.service ?? ""}
          onChange={(v) => setSpec({ service: v })}
          options={serviceOptions}
        />
        <SuggestionInput
          className="flex-1"
          inputClassName="font-mono"
          placeholder="Method"
          value={spec.method ?? ""}
          onChange={(v) => setSpec({ method: v })}
          options={methodOptions}
        />
        {streaming ? (
          <Button variant="danger" className="h-7 shrink-0" onClick={stop}>
            Stop
          </Button>
        ) : (
          <Button variant="primary" className="h-7 shrink-0" disabled={busy} onClick={() => void call()}>
            {busy ? "Calling…" : "Call"}
          </Button>
        )}
      </div>
      <div className="px-2 pb-1.5 text-[10px] font-mono text-fg-2 shrink-0">
        {method ? `${method.input} → ${method.output}` : "pick a service and method"}
      </div>

      {/* ---- Query: the request body ---- */}
      <div className="flex-1 min-h-0 flex flex-col border-t border-line-0">
        <div className="flex items-center gap-2 px-2 py-1 shrink-0">
          <span className="text-xs font-semibold text-fg-1">Body</span>
          <span className="text-[10px] text-fg-2">JSON, encoded with the proto field numbers</span>
          <Button variant="ghost" className="h-6 px-1.5 text-[10px] ml-auto" onClick={beautify}>
            Beautify
          </Button>
        </div>
        <div className="flex-1 min-h-0 bg-bg-1">
          <CodeEditor
            value={body}
            onChange={setBody}
            language="json"
            appearance="editor"
            lineNumbers
            height="100%"
            placeholder={'{\n  "id": "1"\n}'}
          />
        </div>
      </div>

      {/* ---- Stream frames ---- */}
      {frames.length > 0 && (
        <pre className="max-h-32 shrink-0 overflow-auto whitespace-pre-wrap break-words border-t border-line-0 bg-bg-0 p-1.5 font-mono text-[11px] text-fg-1">
          {frames.join("\n")}
        </pre>
      )}

      {/* ---- Proto: the schema (collapsible, drag to resize) ---- */}
      <div className="shrink-0 border-t border-line-0">
        <div
          onMouseDown={(e) => {
            e.preventDefault();
            protoDrag.current = { startY: e.clientY, startH: protoHeight };
            document.body.style.cursor = "row-resize";
            document.body.style.userSelect = "none";
            window.getSelection()?.removeAllRanges();
          }}
          className="h-1.5 cursor-row-resize group flex items-center"
          title="Drag to resize"
        >
          <div className="h-px w-full bg-line-0 group-hover:bg-accent" />
        </div>
        <div className="flex items-center gap-1.5 px-2 py-1">
          <button
            type="button"
            onClick={() => setProtoOpen((v) => !v)}
            className="flex items-center gap-1 text-xs font-semibold text-fg-1 hover:text-fg-0"
            aria-expanded={protoOpen}
          >
            {protoOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            Proto
          </button>
          <span
            className={cn(
              "text-[10px] truncate",
              parseError ? "text-danger" : parsed ? "text-fg-2" : "text-fg-2",
            )}
          >
            {parseError
              ? `parse error — ${parseError.split("\n")[0]}`
              : parsed
                ? `parsed · ${parsed.services.length} service(s) · ${parsed.messages.length} message(s)`
                : protoText.trim()
                  ? "not parsed"
                  : "empty — paste a .proto file here"}
          </span>
          <Button
            variant="ghost"
            className="h-6 px-1.5 text-[10px] ml-auto shrink-0"
            onClick={() => void parse(protoText)}
          >
            Parse
          </Button>
        </div>
        {protoOpen && (
          <div
            className="min-h-0 bg-bg-1 border-t border-line-0 overflow-hidden"
            style={{ height: protoHeight }}
          >
            <CodeEditor
              value={protoText}
              onChange={(v) => setSpec({ proto: v })}
              language="proto"
              appearance="editor"
              lineNumbers
              height="100%"
              placeholder={'syntax = "proto3";\nservice Users {\n  rpc Get (User) returns (User);\n}'}
            />
          </div>
        )}
      </div>
    </div>
  );
}

/** Maps a unary gRPC exchange onto the response-pane result shape. */
function toSendResult(r: GrpcResult): SendResult {
  const ok = r.grpcStatus === 0;
  return {
    requestId: "",
    status: r.status,
    statusText: ok ? "OK" : `gRPC status ${r.grpcStatus}`,
    ok,
    timeMs: r.timeMs,
    sizeBytes: new TextEncoder().encode(r.bodyText).length,
    headers: r.headers.map(([name, value]) => ({ name, value })),
    cookies: [],
    contentType: "application/grpc",
    bodyText: r.bodyText,
    bodyBase64: null,
    truncated: false,
    error: null,
    variablesUsed: [],
    missingVariables: [],
    secretsUsed: [],
    testResults: [],
    scriptLogs: [],
    scriptError: null,
    timeline: [],
  };
}

function fullName(pkg: string, name: string): string {
  return pkg ? `${pkg}.${name}` : name;
}
