import { useEffect, useState } from "react";
import { Check, Copy } from "lucide-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { api } from "@/api/client";
import { Button, Modal, Select, Spinner } from "@/components/ui";
import { pathBasename } from "@/utils";
import { useKeel } from "@/state/store";

const TARGETS: { value: string; label: string }[] = [
  { value: "curl", label: "Shell — cURL" },
  { value: "fetch", label: "JavaScript — fetch" },
  { value: "axios", label: "JavaScript — axios" },
  { value: "python-requests", label: "Python — requests" },
  { value: "httpie", label: "HTTPie" },
  { value: "native-node", label: "Node — native http" },
  { value: "go-http", label: "Go — net/http" },
  { value: "java-okhttp", label: "Java — OkHttp" },
];

export function CodegenModal() {
  const codegenFor = useKeel((s) => s.codegenFor);
  const closeCodegen = useKeel((s) => s.closeCodegen);
  const toast = useKeel((s) => s.toast);

  const [target, setTarget] = useState("curl");
  const [code, setCode] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!codegenFor) return;
    let cancelled = false;
    setBusy(true);
    setCode(null);
    setCopied(false);
    api
      .generateCode(codegenFor, target, null)
      .then((snippet) => {
        if (cancelled) return;
        setCode(snippet);
        setBusy(false);
      })
      .catch((e) => {
        if (cancelled) return;
        setBusy(false);
        setCode(null);
        toast(String(e), "error");
      });
    return () => {
      cancelled = true;
    };
  }, [codegenFor, target, toast]);

  const copy = async () => {
    if (code == null) return;
    try {
      await writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      toast(String(e), "error");
    }
  };

  return (
    <Modal
      open={codegenFor != null}
      onClose={closeCodegen}
      title={codegenFor ? `Generate code — ${pathBasename(codegenFor)}` : "Generate code"}
      width="max-w-2xl"
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs text-fg-1 shrink-0">Target</span>
          <Select
            className="flex-1"
            value={target}
            onChange={(e) => setTarget(e.target.value)}
          >
            {TARGETS.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </Select>
          <Button className="shrink-0" onClick={copy} disabled={code == null}>
            {copied ? <Check size={13} /> : <Copy size={13} />}
            Copy
          </Button>
        </div>

        {busy ? (
          <div className="flex items-center justify-center py-10">
            <Spinner size={16} />
          </div>
        ) : code != null ? (
          <pre className="font-mono text-[11px] p-3 overflow-auto max-h-[50vh] bg-bg-2 rounded border border-line-0 text-fg-0 whitespace-pre">
            {code}
          </pre>
        ) : (
          <div className="rounded border border-line-0 bg-bg-2 p-3 text-xs text-fg-2">
            Nothing to show — fix the errors and try again.
          </div>
        )}
      </div>
    </Modal>
  );
}

export default CodegenModal;
