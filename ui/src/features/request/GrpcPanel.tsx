import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/api/client";
import type { GrpcEvent, ProtoFile, RequestDoc } from "@/api/types";
import { Button, TextInput } from "@/components/ui";
import CodeEditor from "./CodeEditor";

interface Props {
  doc: RequestDoc;
  setDoc: (d: RequestDoc) => void;
  toast: (msg: string, kind?: "error" | "success" | "info") => void;
  onResult: (text: string) => void;
}

export default function GrpcPanel({ doc, setDoc, toast, onResult }: Props) {
  const [parsed, setParsed] = useState<ProtoFile | null>(null);
  const [body, setBody] = useState("{\n  \n}");
  const [busy, setBusy] = useState(false);
  const [streaming, setStreaming] = useState(false);
  const [frames, setFrames] = useState<string[]>([]);
  const spec = doc.grpc ?? {};
  const sessionId = `${doc.name}:${spec.service ?? ""}:${spec.method ?? ""}`;

  const service = useMemo(() => {
    if (!parsed || !spec.service) return undefined;
    return parsed.services.find((s) => fullName(parsed.package, s.name) === spec.service || s.name === spec.service);
  }, [parsed, spec.service]);
  const method = service?.methods.find((m) => m.name === spec.method);

  const setSpec = (patch: Partial<NonNullable<RequestDoc["grpc"]>>) =>
    setDoc({ ...doc, protocol: "grpc", grpc: { ...spec, ...patch } });

  const parse = async () => {
    const proto = spec.proto ?? "";
    if (!proto.trim()) {
      setParsed(null);
      return;
    }
    try {
      const file = await api.grpcParseProto(proto);
      setParsed(file);
      const first = file.services[0];
      if (first && !spec.service) {
        setSpec({
          service: fullName(file.package, first.name),
          method: first.methods[0]?.name ?? "",
        });
      }
      toast("Proto parsed", "success");
    } catch (e) {
      setParsed(null);
      toast(String(e), "error");
    }
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<GrpcEvent>("grpc://message", (event) => {
      if (event.payload.sessionId !== sessionId) return;
      const kind = event.payload.kind;
      if (kind === "end") setStreaming(false);
      const line =
        kind === "message"
          ? event.payload.bodyText ?? ""
          : kind === "trailers"
            ? `trailers status ${event.payload.grpcStatus ?? "?"} ${event.payload.grpcMessage ?? ""}`.trim()
            : kind === "error"
              ? `error ${event.payload.bodyText ?? ""}`
              : "end";
      setFrames((cur) => [...cur, line].slice(-200));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
      void api.grpcClose(sessionId);
    };
  }, [sessionId]);

  const stop = () => {
    void api.grpcClose(sessionId);
    setStreaming(false);
  };

  const call = async () => {
    if (!spec.service || !spec.method) {
      toast("Pick a service and method", "error");
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
    const input = parsed?.messages.find((m) => m.name === method?.input);
    const args = {
      url: doc.request.url.trim(),
      service: spec.service,
      method: spec.method,
      body: json,
      fields: input?.fields ?? [],
      messages: parsed?.messages ?? [],
      headers: doc.request.headers ?? [],
    };
    if (method?.serverStreaming) {
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
      const status = result.grpcStatus === 0 ? "OK" : `status ${result.grpcStatus}`;
      onResult(`${status}  ${Math.round(result.timeMs)} ms\n${result.grpcMessage}\n${result.bodyText}`);
      if (result.grpcStatus !== 0) toast(result.grpcMessage || status, "error");
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="h-full min-h-0 flex flex-col gap-2 p-2">
      <div className="flex items-center gap-2">
        <span className="text-xs font-semibold text-fg-1 w-14">Proto</span>
        <span className="text-[10px] text-fg-2">proto3. Unary and server streaming. Client streaming stays unsupported.</span>
        <Button variant="ghost" className="h-7 ml-auto" onClick={() => void parse()}>
          Parse
        </Button>
      </div>
      <div className="h-36 shrink-0 bg-bg-1">
        <CodeEditor
          value={spec.proto ?? ""}
          onChange={(v) => setSpec({ proto: v })}
          language="text"
          appearance="editor"
          lineNumbers
          height="100%"
          placeholder={'syntax = "proto3";\nservice Users {\n  rpc Get (User) returns (User);\n}'}
        />
      </div>
      <div className="flex items-center gap-2">
        <TextInput
          className="flex-1 font-mono"
          placeholder="package.Service"
          value={spec.service ?? ""}
          onChange={(e) => setSpec({ service: e.target.value })}
          list="keel-grpc-services"
        />
        <TextInput
          className="flex-1 font-mono"
          placeholder="Method"
          value={spec.method ?? ""}
          onChange={(e) => setSpec({ method: e.target.value })}
          list="keel-grpc-methods"
        />
        {streaming ? (
          <Button variant="danger" className="h-7" onClick={stop}>
            Stop
          </Button>
        ) : (
          <Button variant="primary" className="h-7" disabled={busy} onClick={() => void call()}>
            {busy ? "Calling…" : "Call"}
          </Button>
        )}
      </div>
      <datalist id="keel-grpc-services">
        {(parsed?.services ?? []).map((s) => (
          <option key={s.name} value={fullName(parsed?.package ?? "", s.name)} />
        ))}
      </datalist>
      <datalist id="keel-grpc-methods">
        {(service?.methods ?? []).map((m) => (
          <option key={m.name} value={m.name} />
        ))}
      </datalist>
      <div className="text-[10px] text-fg-2">
        {method ? `${method.input} → ${method.output}` : "JSON body, encoded with the proto field numbers"}
      </div>
      {frames.length > 0 && (
        <pre className="max-h-32 shrink-0 overflow-auto whitespace-pre-wrap break-words rounded bg-bg-0 p-1.5 font-mono text-[11px] text-fg-1">
          {frames.join("\n")}
        </pre>
      )}
      <div className="flex-1 min-h-24 bg-bg-1">
        <CodeEditor
          value={body}
          onChange={setBody}
          language="json"
          appearance="editor"
          lineNumbers
          height="100%"
        />
      </div>
    </div>
  );
}

function fullName(pkg: string, name: string): string {
  return pkg ? `${pkg}.${name}` : name;
}
