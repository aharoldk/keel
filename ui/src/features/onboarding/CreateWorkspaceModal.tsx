import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button, Modal, TextInput } from "@/components/ui";
import { useKeel } from "@/state/store";

export function CreateWorkspaceModal({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const createWorkspace = useKeel((s) => s.createWorkspace);

  const [name, setName] = useState("My API");
  const [dir, setDir] = useState<string | null>(null);

  useEffect(() => {
    if (open) setDir(null);
  }, [open]);

  const chooseDir = async () => {
    const picked = await openDialog({ directory: true });
    if (picked) setDir(picked);
  };

  const create = async () => {
    if (!dir || !name.trim()) return;
    onClose();
    await createWorkspace(dir, name.trim());
  };

  return (
    <Modal open={open} onClose={onClose} title="Create workspace" width="max-w-md">
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          create();
        }}
      >
        <label className="flex flex-col gap-1">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-2">
            Name
          </span>
          <TextInput value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <div className="flex flex-col gap-1">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-2">
            Folder
          </span>
          <Button type="button" onClick={chooseDir}>
            Choose folder…
          </Button>
          {dir && <span className="font-mono text-[10px] break-all text-fg-2">{dir}</span>}
        </div>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={!dir || !name.trim()}>
            Create
          </Button>
        </div>
      </form>
    </Modal>
  );
}

export default CreateWorkspaceModal;
