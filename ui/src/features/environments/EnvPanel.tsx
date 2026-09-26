import { useEffect, useRef, useState, type ReactNode } from "react";
import { Copy, FileCode2, FilePlus2, Lock, Pencil, Plus, Trash2, Upload, X } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "@/api/client";
import { emptyEnvDoc, type EnvDoc, type EnvSummary } from "@/api/types";
import { Button, EmptyState, IconButton, Modal, Spinner, TextInput } from "@/components/ui";
import { cn } from "@/utils";
import { envTabKey, useKeel } from "@/state/store";

export function EnvPanel() {
  const envs = useKeel((s) => s.envs);
  const loadEnvs = useKeel((s) => s.loadEnvs);
  const importPostman = useKeel((s) => s.importPostman);
  const toast = useKeel((s) => s.toast);

  const openEnvironment = useKeel((s) => s.openEnvironment);
  const closeEditor = useKeel((s) => s.closeEditor);
  const [addOpen, setAddOpen] = useState(false);
  const addRef = useRef<HTMLDivElement>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [newName, setNewName] = useState("");
  const [newDesc, setNewDesc] = useState("");
  const [deleting, setDeleting] = useState<EnvSummary | null>(null);
  const [menu, setMenu] = useState<{ env: EnvSummary; x: number; y: number } | null>(null);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("resize", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  useEffect(() => {
    if (!addOpen) return;
    const onDown = (e: MouseEvent) => {
      if (!addRef.current?.contains(e.target as Node)) setAddOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setAddOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [addOpen]);

  const openEditor = (env: EnvSummary) => openEnvironment(env.fileName);

  const createEnv = async () => {
    if (!newName.trim()) return;
    try {
      await api.envSave(null, { ...emptyEnvDoc(newName.trim()), description: newDesc.trim() || undefined });
      await loadEnvs();
      toast(`Environment “${newName.trim()}” created`, "success");
      setCreateOpen(false);
      setNewName("");
      setNewDesc("");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const deleteEnv = async () => {
    if (!deleting) return;
    const target = deleting;
    setDeleting(null);
    try {
      await api.envDelete(target.fileName);
      closeEditor(envTabKey(target.fileName));
      await loadEnvs();
      toast(`Environment “${target.name}” deleted`, "success");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const duplicateEnv = async (env: EnvSummary) => {
    try {
      const doc = await api.envRead(env.fileName);
      await api.envSave(null, { ...doc, name: `${doc.name} copy` });
      await loadEnvs();
      toast(`Environment “${doc.name}” duplicated`, "success");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const importEnvironment = async () => {
    try {
      const picked = await openDialog({
        multiple: false,
        title: "Select Postman environment export",
        filters: [{ name: "Postman environment (JSON)", extensions: ["json"] }],
      });
      if (!picked) return;
      const files = await importPostman(picked, "");
      await loadEnvs();
      const imported = files.find((f) => f.startsWith("environments/"));
      if (imported) {
        const fileName = imported.slice("environments/".length);
        const importedEnv = useKeel.getState().envs.find((e) => e.fileName === fileName);
        if (importedEnv) openEditor(importedEnv);
      }
    } catch (err) {
      toast(String(err), "error");
    }
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-9 shrink-0 border-b border-line-0 flex items-center px-2.5 gap-2 text-xs font-semibold uppercase tracking-wider text-fg-2">
        <span className="flex-1">Environments</span>
        <div ref={addRef} className="relative">
          <IconButton
            title="Add"
            aria-haspopup="menu"
            aria-expanded={addOpen}
            className="h-6 w-6"
            onClick={() => setAddOpen((o) => !o)}
          >
            <Plus size={13} />
          </IconButton>
          {addOpen && (
            <div className="absolute right-0 top-full mt-1 z-40 w-48 rounded-md border border-line-0 bg-bg-1 shadow-xl py-1">
              <button
                type="button"
                className="w-full h-8 px-3 flex items-center gap-2 text-left text-xs text-fg-0 hover:bg-bg-hover"
                onClick={() => {
                  setAddOpen(false);
                  setNewName("");
                  setNewDesc("");
                  setCreateOpen(true);
                }}
              >
                <FilePlus2 size={13} className="shrink-0 text-fg-2" />
                Add environment
              </button>
              <button
                type="button"
                className="w-full h-8 px-3 flex items-center gap-2 text-left text-xs text-fg-0 hover:bg-bg-hover"
                onClick={() => {
                  setAddOpen(false);
                  void importEnvironment();
                }}
              >
                <Upload size={13} className="shrink-0 text-fg-2" />
                Import
              </button>
            </div>
          )}
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto p-1">
        {envs.length === 0 ? (
          <EmptyState
            icon={<FileCode2 size={24} />}
            title="No environments"
            hint="Use + to add an environment or import a Postman export"
          />
        ) : (
          envs.map((env) => (
            <div
              key={env.fileName}
              title={`${env.fileName} — click to edit`}
              onClick={() => openEditor(env)}
              onContextMenu={(e) => {
                e.preventDefault();
                setMenu({ env, x: e.clientX, y: e.clientY });
              }}
              className="h-7 px-2 flex items-center gap-1.5 rounded text-xs cursor-pointer select-none text-fg-1 hover:bg-bg-hover"
            >
              <FileCode2 size={13} className="shrink-0 text-fg-2" />
              <span className="truncate">{env.name}</span>
              <span className="ml-auto shrink-0 text-[10px] text-fg-2">
                {env.variableCount} vars · {env.secretCount} secrets
              </span>
            </div>
          ))
        )}
      </div>

      {menu && (
        <div
          className="fixed z-50 min-w-36 rounded border border-line-0 bg-bg-2 shadow-lg py-1 text-xs"
          style={{ left: menu.x, top: menu.y }}
          onContextMenu={(e) => e.preventDefault()}
        >
          <EnvMenuItem
            label="Duplicate"
            icon={<Copy size={12} />}
            onClick={() => {
              const env = menu.env;
              setMenu(null);
              void duplicateEnv(env);
            }}
          />
          <EnvMenuItem
            label="Edit"
            icon={<Pencil size={12} />}
            onClick={() => {
              const env = menu.env;
              setMenu(null);
              openEditor(env);
            }}
          />
          <EnvMenuItem
            label="Delete"
            danger
            icon={<Trash2 size={12} />}
            onClick={() => {
              const env = menu.env;
              setMenu(null);
              setDeleting(env);
            }}
          />
        </div>
      )}

      <Modal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        title="New environment"
        width="max-w-sm"
      >
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            createEnv();
          }}
        >
          <TextInput
            autoFocus
            placeholder="Name"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
          />
          <TextInput
            placeholder="Description (optional)"
            value={newDesc}
            onChange={(e) => setNewDesc(e.target.value)}
          />
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => setCreateOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" variant="primary" disabled={!newName.trim()}>
              Create
            </Button>
          </div>
        </form>
      </Modal>

      <Modal
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        title="Delete environment"
        width="max-w-sm"
      >
        <p className="text-xs text-fg-1">
          Delete environment “{deleting?.name}”? This cannot be undone.
        </p>
        <div className="flex justify-end gap-2 mt-4">
          <Button variant="ghost" onClick={() => setDeleting(null)}>
            Cancel
          </Button>
          <Button variant="danger" onClick={deleteEnv}>
            Delete
          </Button>
        </div>
      </Modal>

    </div>
  );
}

export function EnvironmentEditor({ fileName }: { fileName: string }) {
  const env = useKeel((s) => s.envs.find((e) => e.fileName === fileName)) ?? {
    fileName,
    name: fileName.replace(/\.ya?ml$/, ""),
    variableCount: 0,
    secretCount: 0,
  };
  const closeEditor = useKeel((s) => s.closeEditor);
  const loadEnvs = useKeel((s) => s.loadEnvs);
  const toast = useKeel((s) => s.toast);
  const envValuesRevision = useKeel((s) => s.envValuesRevision);

  const [loading, setLoading] = useState(true);
  const [name, setName] = useState(env.name);
  const [description, setDescription] = useState("");
  const [vars, setVars] = useState<{ name: string; value: string }[]>([]);
  const [currentValues, setCurrentValues] = useState<Record<string, string>>({});
  const [secrets, setSecrets] = useState<{ name: string; defaultValue: string }[]>([]);
  const [keychain, setKeychain] = useState<Set<string>>(new Set());
  const [valueFor, setValueFor] = useState<string | null>(null);
  const [secretValue, setSecretValue] = useState("");
  const [newSecret, setNewSecret] = useState("");
  const [envName, setEnvName] = useState(env.name);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [doc, present, currents] = await Promise.all([
          api.envRead(env.fileName),
          api.secretList(env.name),
          api.envValuesRead(env.fileName),
        ]);
        if (!alive) return;
        setName(doc.name);
        setEnvName(doc.name);
        setDescription(doc.description ?? "");
        setVars(Object.entries(doc.variables ?? {}).map(([k, v]) => ({ name: k, value: v })));
        setCurrentValues(currents ?? {});
        setSecrets(
          Object.entries(doc.secrets ?? {}).map(([name, defaultValue]) => ({ name, defaultValue })),
        );
        setKeychain(new Set(present));
      } catch (err) {
        if (!alive) return;
        const missing = /no such file|not found|os error 2/i.test(String(err));
        if (!missing) toast(String(err), "error");
        closeEditor(`env:${env.fileName}`);
      } finally {
        if (alive) setLoading(false);
      }
    })();
    return () => {
      alive = false;
    };
  }, [env.fileName, env.name]);

  useEffect(() => {
    if (envValuesRevision === 0) return;
    let alive = true;
    api
      .envValuesRead(env.fileName)
      .then((currents) => {
        if (alive) setCurrentValues(currents ?? {});
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [env.fileName, envValuesRevision]);

  const refreshKeychain = async () => {
    try {
      setKeychain(new Set(await api.secretList(envName)));
    } catch {
      setKeychain(new Set());
    }
  };

  // Persist a variable's current value immediately (local, never committed).
  // An empty value clears the override so the default value is used again.
  const persistCurrentValue = async (name: string, value: string) => {
    const key = name.trim();
    if (!key) return;
    try {
      if (value) {
        await api.envValueSet(env.fileName, key, value);
      } else {
        await api.envValueDelete(env.fileName, key);
      }
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const setCurrentValueLocal = (name: string, value: string) =>
    setCurrentValues((c) => {
      const next = { ...c };
      if (value) next[name] = value;
      else delete next[name];
      return next;
    });

  const saveSecretValue = async (secret: string) => {
    if (!secretValue) return;
    try {
      await api.secretSet(envName, secret, secretValue);
      setValueFor(null);
      setSecretValue("");
      await refreshKeychain();
      toast(`Secret “${secret}” saved to keychain`, "success");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const deleteSecretValue = async (secret: string) => {
    try {
      await api.secretDelete(envName, secret);
      await refreshKeychain();
      toast(`Secret “${secret}” removed from keychain`, "success");
    } catch (err) {
      toast(String(err), "error");
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      const doc: EnvDoc = {
        schemaVersion: "1",
        name: name.trim() || env.name,
        description: description.trim() || undefined,
        variables: Object.fromEntries(
          vars.filter((v) => v.name.trim()).map((v) => [v.name.trim(), v.value]),
        ),
        secrets: Object.fromEntries(
          secrets.filter((s) => s.name.trim()).map((s) => [s.name.trim(), s.defaultValue]),
        ),
      };
      await api.envSave(env.fileName, doc);
      await loadEnvs();
      toast("Environment saved", "success");
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col bg-bg-0">
      <div className="h-9 shrink-0 border-b border-line-0 flex items-center justify-end px-3">
        <Button variant="primary" disabled={saving || loading} onClick={save}>
          Save
        </Button>
      </div>
      {loading ? (
        <div className="flex-1 flex items-center justify-center">
          <Spinner size={18} />
        </div>
      ) : (
        <div className="flex-1 min-h-0 overflow-y-auto p-4 flex flex-col gap-4">
          <div className="grid grid-cols-2 gap-3">
            <label className="flex flex-col gap-1">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-2">
                Name
              </span>
              <TextInput value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-2">
                Description
              </span>
              <TextInput value={description} onChange={(e) => setDescription(e.target.value)} />
            </label>
          </div>

          <section className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-2">
                Variables
              </span>
              <Button
                variant="ghost"
                className="h-6 px-1.5"
                onClick={() => setVars((v) => [...v, { name: "", value: "" }])}
              >
                <Plus size={12} />
                Add variable
              </Button>
            </div>
            {vars.length > 0 && (
              <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-fg-2">
                <span className="flex-1">Name</span>
                <span className="flex-1">Default value</span>
                <span className="flex-1">Current value</span>
                <span className="w-6 shrink-0" />
              </div>
            )}
            {vars.length === 0 ? (
              <p className="text-xs text-fg-2">No variables</p>
            ) : (
              vars.map((row, i) => (
                <div key={i} className="flex items-center gap-1.5">
                  <TextInput
                    className="flex-1"
                    placeholder="name"
                    value={row.name}
                    onChange={(e) =>
                      setVars((v) => v.map((r, j) => (j === i ? { ...r, name: e.target.value } : r)))
                    }
                  />
                  <TextInput
                    className="flex-1 font-mono"
                    placeholder="default value"
                    title="Default value — committed to Git"
                    value={row.value}
                    onChange={(e) =>
                      setVars((v) => v.map((r, j) => (j === i ? { ...r, value: e.target.value } : r)))
                    }
                  />
                  <TextInput
                    className="flex-1 font-mono"
                    placeholder="current value"
                    title="Current value — local override (never committed); clear it to fall back to the default"
                    disabled={!row.name.trim()}
                    value={currentValues[row.name] ?? ""}
                    onChange={(e) => setCurrentValueLocal(row.name, e.target.value)}
                    onBlur={(e) => persistCurrentValue(row.name, e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        persistCurrentValue(row.name, e.currentTarget.value);
                      }
                    }}
                  />
                  <IconButton
                    title="Remove variable"
                    className="h-6 w-6 shrink-0"
                    onClick={() => setVars((v) => v.filter((_, j) => j !== i))}
                  >
                    <X size={12} />
                  </IconButton>
                </div>
              ))
            )}
          </section>

          <section className="flex flex-col gap-1.5">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-fg-2">
                Secrets
              </span>
              <span className="text-[10px] text-fg-2">
                current value (OS keychain) wins, else the default value
              </span>
            </div>
            {secrets.length > 0 && (
              <div className="flex items-center gap-1.5 pl-5 text-[10px] uppercase tracking-wider text-fg-2">
                <span className="w-32 shrink-0">Name</span>
                <span className="flex-1">Default value</span>
                <span className="shrink-0">Current value</span>
              </div>
            )}
            {secrets.length === 0 && <p className="text-xs text-fg-2">No secrets</p>}
            {secrets.map((secret) => {
              const present = keychain.has(secret.name);
              return (
                <div key={secret.name} className="flex flex-col gap-1">
                  <div className="flex items-center gap-1.5">
                    <Lock size={12} className="shrink-0 text-fg-2" />
                    <span className="font-mono text-xs text-fg-0 w-32 shrink-0 truncate">
                      {secret.name}
                    </span>
                    <TextInput
                      className="flex-1 font-mono"
                      placeholder="default value (fallback)"
                      title="Default value (committed to Git) — used when no current value is set"
                      value={secret.defaultValue}
                      onChange={(e) =>
                        setSecrets((s) =>
                          s.map((x) =>
                            x.name === secret.name ? { ...x, defaultValue: e.target.value } : x,
                          ),
                        )
                      }
                    />
                    <span
                      title={
                        present
                          ? "Current value stored in keychain (wins over default)"
                          : "No current value in keychain — default value will be used"
                      }
                      className={cn("h-1.5 w-1.5 rounded-full shrink-0", present ? "bg-ok" : "bg-fg-2")}
                    />
                    <Button
                      variant="ghost"
                      onClick={() => {
                        setValueFor(secret.name);
                        setSecretValue("");
                      }}
                    >
                      {present ? "Replace value" : "Set value…"}
                    </Button>
                    {present && (
                      <IconButton
                        title="Delete from keychain"
                        className="h-6 w-6 shrink-0 hover:text-danger"
                        onClick={() => deleteSecretValue(secret.name)}
                      >
                        <Trash2 size={11} />
                      </IconButton>
                    )}
                    <IconButton
                      title="Remove from environment"
                      className="h-6 w-6 shrink-0"
                      onClick={() => setSecrets((s) => s.filter((x) => x.name !== secret.name))}
                    >
                      <X size={12} />
                    </IconButton>
                  </div>
                  {valueFor === secret.name && (
                    <div className="flex items-center gap-1.5 pl-5">
                      <TextInput
                        autoFocus
                        type="password"
                        className="flex-1 font-mono"
                        placeholder="Current value (replaces the keychain value)"
                        value={secretValue}
                        onChange={(e) => setSecretValue(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            saveSecretValue(secret.name);
                          }
                        }}
                      />
                      <Button
                        variant="primary"
                        disabled={!secretValue}
                        onClick={() => saveSecretValue(secret.name)}
                      >
                        Save
                      </Button>
                      <IconButton
                        title="Cancel"
                        className="h-6 w-6 shrink-0"
                        onClick={() => {
                          setValueFor(null);
                          setSecretValue("");
                        }}
                      >
                        <X size={12} />
                      </IconButton>
                    </div>
                  )}
                </div>
              );
            })}
            <div className="flex items-center gap-1.5">
              <TextInput
                className="flex-1"
                placeholder="New secret name"
                value={newSecret}
                onChange={(e) => setNewSecret(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    const n = newSecret.trim();
                    if (n && !secrets.some((s) => s.name === n)) {
                      setSecrets((s) => [...s, { name: n, defaultValue: "" }]);
                    }
                    setNewSecret("");
                  }
                }}
              />
              <Button
                disabled={!newSecret.trim()}
                onClick={() => {
                  const n = newSecret.trim();
                  if (n && !secrets.some((s) => s.name === n)) {
                    setSecrets((s) => [...s, { name: n, defaultValue: "" }]);
                  }
                  setNewSecret("");
                }}
              >
                <Plus size={12} />
                Add secret
              </Button>
            </div>
          </section>

        </div>
      )}
    </div>
  );
}

function EnvMenuItem({
  label,
  danger,
  icon,
  onClick,
}: {
  label: string;
  danger?: boolean;
  icon: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className={cn(
        "w-full px-3 py-1.5 flex items-center gap-2 text-left hover:bg-bg-hover",
        danger ? "text-danger" : "text-fg-1 hover:text-fg-0",
      )}
    >
      <span className={cn("shrink-0", danger ? "text-danger" : "text-fg-2")}>{icon}</span>
      {label}
    </button>
  );
}

export default EnvPanel;
