import { useEffect, useState } from "react";
import { Plus, X } from "lucide-react";
import { api } from "@/api/client";
import type { CollectionDoc, EnvSummary, KV } from "@/api/types";
import { Button, Divider, IconButton, Modal, Select, Spinner, TextInput } from "@/components/ui";
import CodeEditor from "@/features/request/CodeEditor";
import KVEditor from "@/features/request/KVEditor";
import {
  gatherVariableSuggestions,
  type VariableSuggestion,
} from "@/features/request/variables";
import { useKeel } from "@/state/store";
import { AuthEditor } from "./AuthEditor";

interface Props {
  open: boolean;
  onClose: () => void;
}

interface VarRow {
  key: string;
  value: string;
}

function MapRows({
  rows,
  onChange,
  keyPlaceholder = "name",
  valuePlaceholder = "{{value}}",
}: {
  rows: VarRow[];
  onChange: (rows: VarRow[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
}) {
  const setAt = (i: number, patch: Partial<VarRow>) =>
    onChange(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));

  return (
    <div className="flex flex-col gap-1">
      {rows.map((r, i) => (
        <div key={i} className="flex items-center gap-1.5">
          <TextInput
            value={r.key}
            placeholder={keyPlaceholder}
            onChange={(e) => setAt(i, { key: e.target.value })}
            className="flex-1 min-w-0"
          />
          <TextInput
            value={r.value}
            placeholder={valuePlaceholder}
            onChange={(e) => setAt(i, { value: e.target.value })}
            className="flex-1 min-w-0 font-mono"
          />
          <IconButton
            className="h-6 w-6 hover:text-danger"
            title="Remove variable"
            onClick={() => onChange(rows.filter((_, idx) => idx !== i))}
          >
            <X size={12} />
          </IconButton>
        </div>
      ))}
      <div>
        <button
          type="button"
          onClick={() => onChange([...rows, { key: "", value: "" }])}
          className="inline-flex items-center gap-1 rounded px-1.5 h-6 text-xs text-fg-2 hover:text-fg-0 hover:bg-bg-hover transition-colors"
        >
          <Plus size={12} />
          Add variable
        </button>
      </div>
    </div>
  );
}

export function CollectionSettingsModal({ open, onClose }: Props) {
  const toast = useKeel((s) => s.toast);
  const refreshTree = useKeel((s) => s.refreshTree);
  const activeEnv = useKeel((s) => s.activeEnv);
  const workspaceRoot = useKeel((s) => s.workspace?.root ?? null);

  const [draft, setDraft] = useState<CollectionDoc | null>(null);
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [varRows, setVarRows] = useState<VarRow[]>([]);
  const [headers, setHeaders] = useState<KV[]>([]);
  const [variables, setVariables] = useState<VariableSuggestion[]>([]);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    gatherVariableSuggestions(null)
      .then((v) => {
        if (alive) setVariables(v);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [open, activeEnv, workspaceRoot]);

  useEffect(() => {
    if (!open) return;
    setDraft(null);
    api
      .collectionRead()
      .then((doc) => {
        setDraft(doc);
        setVarRows(
          Object.entries(doc.variables ?? {}).map(([key, value]) => ({ key, value })),
        );
        setHeaders(doc.headers ?? []);
      })
      .catch((e) => {
        toast(String(e), "error");
        onClose();
      });
    api.envList().then(setEnvs).catch(() => setEnvs([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const patch = (p: Partial<CollectionDoc>) =>
    setDraft((d) => (d ? { ...d, ...p } : d));

  const save = async () => {
    if (!draft) return;
    const variables = Object.fromEntries(
      varRows.filter((r) => r.key.trim()).map((r) => [r.key, r.value]),
    );
    const scripts = draft.scripts;
    const hasScripts = Boolean(scripts?.preRequest?.trim() || scripts?.postResponse?.trim());
    const doc: CollectionDoc = {
      schemaVersion: draft.schemaVersion || "1",
      name: draft.name,
      ...(draft.description?.trim() ? { description: draft.description.trim() } : {}),
      ...(Object.keys(variables).length > 0 ? { variables } : {}),
      ...(draft.defaultEnvironment ? { defaultEnvironment: draft.defaultEnvironment } : {}),
      ...(draft.auth && draft.auth.type !== "none" ? { auth: draft.auth } : {}),
      ...(headers.length > 0 ? { headers } : {}),
      ...(hasScripts && scripts ? { scripts } : {}),
    };
    try {
      await api.collectionSave(doc);
      toast("Collection settings saved", "success");
      await refreshTree();
      onClose();
    } catch (e) {
      toast(String(e), "error");
    }
  };

  return (
    <Modal open={open} onClose={onClose} title="Collection settings" width="max-w-2xl">
      {!draft ? (
        <div className="flex items-center justify-center py-10">
          <Spinner size={18} />
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <span className="w-40 shrink-0 text-xs text-fg-1">Name</span>
            <TextInput
              className="flex-1 min-w-0"
              value={draft.name}
              onChange={(e) => patch({ name: e.target.value })}
            />
          </div>
          <div className="flex items-center gap-2">
            <span className="w-40 shrink-0 text-xs text-fg-1">Description</span>
            <TextInput
              className="flex-1 min-w-0"
              placeholder="What is this collection for?"
              value={draft.description ?? ""}
              onChange={(e) => patch({ description: e.target.value })}
            />
          </div>
          <div className="flex items-center gap-2">
            <span className="w-40 shrink-0 text-xs text-fg-1">Default environment</span>
            <Select
              className="flex-1 min-w-0"
              value={draft.defaultEnvironment ?? ""}
              onChange={(e) =>
                patch({ defaultEnvironment: e.target.value || undefined })
              }
            >
              <option value="">None</option>
              {envs.map((env) => (
                <option key={env.fileName} value={env.name}>
                  {env.name}
                </option>
              ))}
            </Select>
          </div>

          <Divider />
          <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-2">
            Variables (collection scope)
          </span>
          <MapRows rows={varRows} onChange={setVarRows} keyPlaceholder="variable" />

          <Divider />
          <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-2">
            Auth (inherited by requests)
          </span>
          <AuthEditor
            auth={draft.auth}
            onChange={(a) => patch({ auth: a })}
            variables={variables}
          />

          <Divider />
          <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-2">
            Headers (merged under request headers)
          </span>
          <KVEditor
            rows={headers}
            onChange={setHeaders}
            namePlaceholder="Header name"
            valuePlaceholder="{{value}}"
            variables={variables}
          />

          <Divider />
          <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-2">
            Scripts
          </span>
          <p className="text-[10px] text-fg-2">
            JavaScript. Globals: <span className="font-mono">keel</span>{" "}
            (setVar/getVar/getCollectionVar/interpolate/sleep),{" "}
            <span className="font-mono">req</span> (pre),{" "}
            <span className="font-mono">res</span> (post),{" "}
            <span className="font-mono">test(name, fn)</span>,{" "}
            <span className="font-mono">expect(v)</span>.
          </p>
          <div className="flex flex-col gap-1">
            <span className="text-xs text-fg-1">Pre-request</span>
            <div className="rounded border border-line-0 overflow-hidden">
              <CodeEditor
                value={draft.scripts?.preRequest ?? ""}
                onChange={(v) =>
                  patch({ scripts: { ...draft.scripts, preRequest: v } })
                }
                language="javascript"
                height="120px"
                lineNumbers
              />
            </div>
            <span className="text-xs text-fg-1">Post-response</span>
            <div className="rounded border border-line-0 overflow-hidden">
              <CodeEditor
                value={draft.scripts?.postResponse ?? ""}
                onChange={(v) =>
                  patch({ scripts: { ...draft.scripts, postResponse: v } })
                }
                language="javascript"
                height="120px"
                lineNumbers
              />
            </div>
          </div>

          <div className="flex justify-end gap-2 pt-1">
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button variant="primary" onClick={() => void save()}>
              Save
            </Button>
          </div>
        </div>
      )}
    </Modal>
  );
}

export default CollectionSettingsModal;
