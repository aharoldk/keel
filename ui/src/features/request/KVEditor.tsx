import { FolderOpen, Paperclip, Plus, X } from "lucide-react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import type { KV } from "@/api/types";
import { IconButton, TextInput } from "@/components/ui";
import VariableInput from "@/components/VariableInput";
import type { VariableSuggestion } from "@/features/request/variables";

interface KVEditorProps {
  rows: KV[] | undefined;
  onChange: (rows: KV[]) => void;
  allowFile?: boolean;
  namePlaceholder?: string;
  valuePlaceholder?: string;
  variables?: VariableSuggestion[];
}

export default function KVEditor({
  rows,
  onChange,
  allowFile = false,
  namePlaceholder = "Name",
  valuePlaceholder = "Value",
  variables,
}: KVEditorProps) {
  const list: KV[] = rows ?? [];

  const setRow = (i: number, patch: Partial<KV>) => {
    onChange(list.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
  };

  const removeRow = (i: number) => {
    onChange(list.filter((_, idx) => idx !== i));
  };

  const addRow = () => {
    onChange([...list, { name: "", value: "", enabled: true }]);
  };

  const browseFile = async (i: number) => {
    const selected = await openFileDialog({ multiple: false });
    if (typeof selected === "string") {
      setRow(i, { value: selected, kind: "file" });
    }
  };

  return (
    <div className="flex flex-col gap-1">
      {list.map((row, i) => {
        const isFile = allowFile && row.kind === "file";
        return (
          <div key={i} className="flex items-center gap-1.5">
            <input
              type="checkbox"
              checked={row.enabled !== false}
              onChange={(e) => setRow(i, { enabled: e.target.checked })}
              className="h-3 w-3 shrink-0 mr-1 accent-[var(--accent)] cursor-pointer"
              title={row.enabled !== false ? "Enabled" : "Disabled"}
            />
            <TextInput
              value={row.name}
              onChange={(e) => setRow(i, { name: e.target.value })}
              placeholder={namePlaceholder}
              className="flex-1 min-w-0"
            />
            {isFile ? (
              <>
                <VariableInput
                  value={row.value}
                  onChange={(value) => setRow(i, { value })}
                  placeholder="/path/to/file"
                  className="flex-1 min-w-0"
                  inputClassName="font-mono"
                  title="File path sent as multipart file"
                  variables={variables}
                />
                <IconButton
                  className="h-6 w-6"
                  title="Browse for file"
                  onClick={() => void browseFile(i)}
                >
                  <FolderOpen size={12} />
                </IconButton>
              </>
            ) : (
              <>
                <VariableInput
                  value={row.value}
                  onChange={(value) => setRow(i, { value })}
                  placeholder={valuePlaceholder}
                  className="flex-1 min-w-0"
                  inputClassName="font-mono"
                  variables={variables}
                />
                {allowFile && (
                  <IconButton
                    className="h-6 w-6"
                    title="Switch to file row (sends the file at the given path)"
                    onClick={() => setRow(i, { kind: "file", value: "" })}
                  >
                    <Paperclip size={12} />
                  </IconButton>
                )}
              </>
            )}
            <IconButton
              className="h-6 w-6 hover:text-danger"
              title="Remove row"
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
          onClick={addRow}
          className="inline-flex items-center gap-1 rounded px-1.5 h-6 text-xs text-fg-2 hover:text-fg-0 hover:bg-bg-hover transition-colors"
        >
          <Plus size={12} />
          Add
        </button>
      </div>
    </div>
  );
}
