import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/api/client";
import type { RequestDoc, WsEvent } from "@/api/types";
import { Button, TextInput } from "@/components/ui";

interface Props {
  path: string;
  doc: RequestDoc;
  setDoc: (d: RequestDoc) => void;
  toast: (msg: string, kind?: "error" | "success" | "info") => void;
}

interface Frame {
  dir: "in" | "out" | "sys";
  text: string;
}

export default function WsPanel({ path, doc, setDoc, toast }: Props) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState("");
  const [frames, setFrames] = useState<Frame[]>([]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<WsEvent>("ws://message", (event) => {
      if (event.payload.sessionId !== path) return;
      const kind = event.payload.kind;
      if (kind === "open") setOpen(true);
      if (kind === "close" || kind === "error") setOpen(false);
      const text =
        kind === "message"
          ? event.payload.data ?? ""
          : kind === "error"
            ? event.payload.data ?? "error"
            : kind === "open"
              ? `connected${event.payload.data ? ` (${event.payload.data})` : ""}`
              : "closed";
      const dir: Frame["dir"] = kind === "message" ? "in" : "sys";
      setFrames((cur) => [...cur, { dir, text }].slice(-200));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
      void api.wsClose(path);
    };
  }, [path]);

  const connect = async () => {
    try {
      const protocols = (doc.websocket?.protocols ?? "")
        .split(",")
        .map((p) => p.trim())
        .filter(Boolean);
      await api.wsConnect(path, doc.request.url.trim(), doc.request.headers ?? [], protocols);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const send = async () => {
    if (!draft) return;
    try {
      await api.wsSend(path, draft);
      setFrames((cur) => [...cur, { dir: "out", text: draft }]);
      setDraft("");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  return (
    <div className="h-full min-h-0 flex flex-col">
      <div className="flex items-center gap-2 p-2 shrink-0">
        <TextInput
          className="w-44 font-mono"
          placeholder="protocol"
          value={doc.websocket?.protocols ?? ""}
          onChange={(e) => setDoc({ ...doc, websocket: { protocols: e.target.value } })}
        />
        {open ? (
          <Button variant="danger" className="h-7" onClick={() => void api.wsClose(path)}>
            Disconnect
          </Button>
        ) : (
          <Button variant="primary" className="h-7" onClick={() => void connect()}>
            Connect
          </Button>
        )}
        <span className="text-[10px] text-fg-2">{open ? "open" : "closed"}</span>
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto px-2 font-mono text-xs flex flex-col gap-1">
        {frames.map((frame, i) => (
          <div
            key={i}
            className={
              frame.dir === "out" ? "text-accent" : frame.dir === "sys" ? "text-fg-2" : "text-fg-0"
            }
          >
            {frame.dir === "out" ? "→ " : frame.dir === "in" ? "← " : ""}
            {frame.text}
          </div>
        ))}
      </div>
      <div className="flex items-center gap-2 p-2 shrink-0">
        <TextInput
          className="flex-1 font-mono"
          placeholder="Message"
          value={draft}
          disabled={!open}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void send();
          }}
        />
        <Button className="h-7" disabled={!open} onClick={() => void send()}>
          Send
        </Button>
      </div>
    </div>
  );
}
