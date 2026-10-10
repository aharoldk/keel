import { useState } from "react";
import { Sparkles, X } from "lucide-react";
import { api } from "@/api/client";
import type { RequestDoc } from "@/api/types";
import { Button, IconButton, Select, TextArea } from "@/components/ui";
import { useKeel } from "@/state/store";

type AiKind = "script" | "test" | "docs" | "request";

const TASKS: { id: AiKind; label: string; hint: string }[] = [
  { id: "script", label: "Script", hint: "Write a pre-request or post-response script" },
  { id: "test", label: "Tests", hint: "Write assertions for the last response" },
  { id: "docs", label: "Docs", hint: "Write markdown for this request" },
  { id: "request", label: "Request", hint: "Change the request YAML" },
];

function requestContext(doc: RequestDoc | null, kind: AiKind): string {
  if (!doc) return "";
  if (kind === "script") {
    return [doc.scripts?.preRequest, doc.scripts?.postResponse].filter(Boolean).join("\n");
  }
  if (kind === "test") return JSON.stringify(doc.tests ?? [], null, 2);
  if (kind === "docs") return doc.description ?? "";
  return JSON.stringify(doc, null, 2);
}

export function AiPanel() {
  const setAiOpen = useKeel((s) => s.setAiOpen);
  const setSettingsOpen = useKeel((s) => s.setSettingsOpen);
  const settings = useKeel((s) => s.settings);
  const tab = useKeel((s) => s.tabs.find((t) => t.path === s.activePath) ?? null);
  const updateDoc = useKeel((s) => s.updateDoc);
  const toast = useKeel((s) => s.toast);

  const [kind, setKind] = useState<AiKind>("script");
  const [prompt, setPrompt] = useState("");
  const [output, setOutput] = useState("");
  const [busy, setBusy] = useState(false);

  const enabled = (settings.aiProvider ?? "off") !== "off";
  const task = TASKS.find((t) => t.id === kind) ?? TASKS[0];

  const generate = async () => {
    if (busy || !prompt.trim()) return;
    setBusy(true);
    setOutput("");
    try {
      const text = await api.aiGenerate(kind, prompt.trim(), requestContext(tab?.doc ?? null, kind));
      setOutput(text.trim());
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    if (!output || !tab) return;
    const doc = tab.doc;
    if (kind === "request") {
      try {
        const next = await api.requestFromYaml(output);
        updateDoc(tab.path, { ...next, name: doc.name });
        toast("Request updated", "success");
      } catch (e) {
        toast(String(e), "error");
      }
      return;
    }
    if (kind === "script") {
      updateDoc(tab.path, {
        ...doc,
        scripts: { ...doc.scripts, postResponse: output },
      });
      toast("Post-response script updated", "success");
      return;
    }
    if (kind === "test") {
      try {
        const tests = JSON.parse(output);
        if (!Array.isArray(tests)) throw new Error("Tests must be a JSON array");
        updateDoc(tab.path, { ...doc, tests });
        toast("Tests updated", "success");
      } catch (e) {
        toast(String(e), "error");
      }
      return;
    }
    updateDoc(tab.path, { ...doc, description: output });
    toast("Description updated", "success");
  };

  return (
    <aside className="w-80 shrink-0 border-l border-line-0 bg-bg-1 flex flex-col min-h-0">
      <div className="h-9 shrink-0 px-2 border-b border-line-0 flex items-center gap-1.5">
        <Sparkles size={13} className="text-fg-2 shrink-0" />
        <span className="text-xs font-semibold text-fg-0">AI</span>
        <span className="flex-1" />
        <IconButton title="Close AI" aria-label="Close AI" onClick={() => setAiOpen(false)}>
          <X size={13} />
        </IconButton>
      </div>

      {!enabled ? (
        <div className="flex-1 flex flex-col items-center justify-center gap-2 p-6 text-center">
          <p className="text-xs text-fg-2">AI is off. Choose a provider and add an API key.</p>
          <Button variant="default" onClick={() => setSettingsOpen(true)}>
            Open AI settings
          </Button>
        </div>
      ) : (
        <div className="flex-1 min-h-0 flex flex-col gap-2 p-2">
          <Select
            aria-label="AI task"
            value={kind}
            onChange={(e) => {
              setKind(e.target.value as AiKind);
              setOutput("");
            }}
          >
            {TASKS.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}
              </option>
            ))}
          </Select>
          <p className="text-[10px] text-fg-2 px-0.5">
            {task.hint}
            {tab ? ` · ${tab.doc.name}` : " · no request open"}
          </p>
          <TextArea
            aria-label="AI prompt"
            minRows={3}
            maxHeight={140}
            placeholder="Describe what to generate — requests, scripts, tests, docs"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void generate();
              }
            }}
          />
          <Button
            variant="primary"
            className="w-full"
            disabled={busy || !prompt.trim()}
            onClick={() => void generate()}
          >
            <Sparkles size={13} />
            {busy ? "Generating…" : "Generate"}
          </Button>
          <pre className="flex-1 min-h-0 overflow-auto rounded bg-bg-2 border border-line-0 p-2 font-mono text-[10px] whitespace-pre-wrap text-fg-1">
            {output || (busy ? "…" : "Output appears here.")}
          </pre>
          <Button variant="default" className="w-full" disabled={!output || !tab || busy} onClick={() => void apply()}>
            Apply to request
          </Button>
        </div>
      )}
    </aside>
  );
}
