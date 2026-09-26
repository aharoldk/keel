import { useEffect, useState } from "react";
import { CheckCheck, FolderOpen } from "lucide-react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import type { AppSettings, ShortcutAction } from "@/api/types";
import { Button, IconButton, Modal, Select, TextInput } from "@/components/ui";
import {
  SHORTCUT_DEFS,
  SHORTCUT_GROUPS,
  comboFor,
  eventToCombo,
  formatCombo,
  shortcutConflict,
} from "@/shortcuts";
import { cn } from "@/utils";
import { useKeel } from "@/state/store";
import { CookiesSection } from "./CookiesSection";

type SettingsTab = "general" | "editor" | "network" | "shortcuts" | "ai" | "workspace";

const TABS: { id: SettingsTab; label: string }[] = [
  { id: "general", label: "General" },
  { id: "editor", label: "Editor" },
  { id: "network", label: "Network" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "ai", label: "AI" },
  { id: "workspace", label: "Workspace" },
];

function Checkbox({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="flex items-center gap-2 text-xs text-fg-1 hover:text-fg-0 focus-visible:ring-1 ring-accent/60 rounded outline-none"
    >
      <span
        className={cn(
          "h-4 w-4 rounded border flex items-center justify-center shrink-0",
          checked ? "border-accent bg-accent" : "border-line-0 bg-bg-2",
        )}
      >
        {checked && <CheckCheck size={11} strokeWidth={2.5} className="text-accent-fg" />}
      </span>
      {label}
    </button>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <span className="text-xs text-fg-1 shrink-0">{label}</span>
      {children}
    </div>
  );
}

export function SettingsModal() {
  const settingsOpen = useKeel((s) => s.settingsOpen);
  const setSettingsOpen = useKeel((s) => s.setSettingsOpen);
  const workspace = useKeel((s) => s.workspace);
  const version = useKeel((s) => s.version);
  const saveSettings = useKeel((s) => s.saveSettings);

  const [tab, setTab] = useState<SettingsTab>("general");
  const [draft, setDraft] = useState<AppSettings>(useKeel.getState().settings);

  useEffect(() => {
    if (settingsOpen) {
      setDraft({ ...useKeel.getState().settings });
      setTab("general");
    }
  }, [settingsOpen]);

  const patch = (p: Partial<AppSettings>) => setDraft((d) => ({ ...d, ...p }));

  const browseCaCert = async () => {
    try {
      const picked = await openFileDialog({
        multiple: false,
        title: "Select CA certificate (PEM)",
      });
      if (typeof picked === "string") patch({ caCertPath: picked });
    } catch {
      // dialog cancelled — keep current path
    }
  };

  const setShortcut = (id: ShortcutAction, combo: string | null) => {
    const shortcuts = { ...(draft.shortcuts ?? {}) };
    if (!combo) delete shortcuts[id];
    else shortcuts[id] = combo;
    patch({ shortcuts });
  };

  const save = async () => {
    const next: AppSettings = {
      ...draft,
      requestTimeoutSec: Math.max(1, Math.round(draft.requestTimeoutSec) || 1),
      editorFontSize: Math.min(24, Math.max(8, Math.round(draft.editorFontSize) || 13)),
      proxyUrl: draft.proxyUrl?.trim() ? draft.proxyUrl.trim() : null,
      caCertPath: draft.caCertPath?.trim() ? draft.caCertPath.trim() : null,
      insecureTls: draft.insecureTls ?? false,
      sendCookies: draft.sendCookies ?? true,
      storeCookies: draft.storeCookies ?? true,
      maxRedirects: Number.isFinite(draft.maxRedirects)
        ? Math.min(50, Math.max(0, Math.round(draft.maxRedirects ?? 10)))
        : 10,
      autoSave: draft.autoSave ?? false,
      autoSaveInterval: Math.max(100, Math.round(draft.autoSaveInterval ?? 1000) || 1000),
      shortcuts: draft.shortcuts ?? {},
    };
    await saveSettings(next);
    setSettingsOpen(false);
  };

  return (
    <Modal
      open={settingsOpen}
      onClose={() => setSettingsOpen(false)}
      title="Settings"
      width="max-w-xl"
    >
      <div className="flex flex-col gap-3">
        <div role="tablist" className="flex gap-1 border-b border-line-0">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              onClick={() => setTab(t.id)}
              className={cn(
                "h-7 px-2.5 text-xs border-b-2 -mb-px",
                tab === t.id
                  ? "border-accent text-fg-0"
                  : "border-transparent text-fg-2 hover:text-fg-0",
              )}
            >
              {t.label}
            </button>
          ))}
        </div>

        {tab === "general" && (
          <div className="flex flex-col gap-3">
            <Row label="Theme">
              <Select
                className="w-28"
                value={draft.theme}
                onChange={(e) => patch({ theme: e.target.value as AppSettings["theme"] })}
              >
                <option value="dark">Dark</option>
                <option value="light">Light</option>
              </Select>
            </Row>
            <Checkbox
              checked={draft.saveOnSend}
              onChange={(v) => patch({ saveOnSend: v })}
              label="Save request before send"
            />
            <Row label="Auto save">
              <div className="flex items-center gap-2">
                <Checkbox
                  checked={draft.autoSave ?? false}
                  onChange={(v) => patch({ autoSave: v })}
                  label="Enabled"
                />
                <TextInput
                  className="w-24"
                  type="number"
                  min={100}
                  aria-label="Auto save delay (ms)"
                  disabled={!(draft.autoSave ?? false)}
                  value={draft.autoSaveInterval ?? 1000}
                  onChange={(e) => {
                    const n = Number(e.target.value);
                    patch({ autoSaveInterval: Number.isFinite(n) ? n : 1000 });
                  }}
                />
              </div>
            </Row>
          </div>
        )}

        {tab === "editor" && (
          <div className="flex flex-col gap-3">
            <Row label="Font size">
              <TextInput
                className="w-20"
                type="number"
                min={8}
                max={24}
                aria-label="Editor font size"
                value={draft.editorFontSize}
                onChange={(e) =>
                  patch({ editorFontSize: Number(e.target.value) || draft.editorFontSize })
                }
              />
            </Row>
          </div>
        )}

        {tab === "network" && (
          <div className="flex flex-col gap-3">
            <Row label="Request timeout (seconds)">
              <TextInput
                className="w-20"
                type="number"
                min={1}
                value={draft.requestTimeoutSec}
                onChange={(e) =>
                  patch({ requestTimeoutSec: Number(e.target.value) || draft.requestTimeoutSec })
                }
              />
            </Row>
            <Checkbox
              checked={draft.followRedirects}
              onChange={(v) => patch({ followRedirects: v })}
              label="Follow redirects"
            />
            <Row label="Max redirects">
              <TextInput
                className="w-20"
                type="number"
                min={0}
                max={50}
                value={draft.maxRedirects ?? 10}
                onChange={(e) => {
                  const n = Number(e.target.value);
                  patch({ maxRedirects: Number.isFinite(n) ? n : 10 });
                }}
              />
            </Row>
            <Row label="Proxy URL">
              <TextInput
                className="flex-1 min-w-0 font-mono"
                placeholder="http://127.0.0.1:7890 (empty = off)"
                value={draft.proxyUrl ?? ""}
                onChange={(e) => patch({ proxyUrl: e.target.value || null })}
              />
            </Row>
            <div className="flex flex-col gap-1">
              <Checkbox
                checked={draft.insecureTls ?? false}
                onChange={(v) => patch({ insecureTls: v })}
                label="Skip TLS verification"
              />
              {draft.insecureTls && (
                <p className="pl-6 text-[10px] text-danger">
                  Disables certificate checks — only use this against trusted
                  local/self-signed servers.
                </p>
              )}
            </div>
            <div className="flex items-center gap-1.5">
              <span className="shrink-0 text-xs text-fg-1">CA certificate</span>
              <TextInput
                className="flex-1 min-w-0 font-mono"
                placeholder="Path to a PEM bundle (empty = system roots)"
                value={draft.caCertPath ?? ""}
                onChange={(e) => patch({ caCertPath: e.target.value || null })}
              />
              <IconButton
                className="h-6 w-6"
                title="Browse for CA certificate"
                onClick={() => void browseCaCert()}
              >
                <FolderOpen size={12} />
              </IconButton>
            </div>
            <Checkbox
              checked={draft.sendCookies ?? true}
              onChange={(v) => patch({ sendCookies: v })}
              label="Send cookies"
            />
            <Checkbox
              checked={draft.storeCookies ?? true}
              onChange={(v) => patch({ storeCookies: v })}
              label="Store cookies"
            />
            <CookiesSection />
          </div>
        )}

        {tab === "shortcuts" && (
          <div className="flex flex-col gap-2">
            <div className="flex justify-end">
              <Button variant="ghost" onClick={() => patch({ shortcuts: {} })}>
                Reset
              </Button>
            </div>
            {SHORTCUT_GROUPS.map((group) => (
              <div key={group} className="flex flex-col gap-1">
                <span className="text-[10px] uppercase tracking-wider text-fg-2">{group}</span>
                {SHORTCUT_DEFS.filter((d) => d.group === group).map((def) => {
                  const combo = comboFor(def.id, draft.shortcuts);
                  const conflict = shortcutConflict(combo, def.id, draft.shortcuts);
                  return (
                    <div key={def.id} className="flex items-center justify-between gap-3">
                      <span className="text-xs text-fg-1">{def.name}</span>
                      <input
                        aria-label={`${def.name} shortcut`}
                        readOnly
                        value={formatCombo(combo)}
                        title={conflict ? `Already used by ${conflict}` : "Press a shortcut. Backspace resets."}
                        onKeyDown={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          if (e.key === "Backspace" || e.key === "Delete") {
                            setShortcut(def.id, null);
                            return;
                          }
                          const next = eventToCombo(e);
                          if (next) setShortcut(def.id, next);
                        }}
                        className={cn(
                          "h-7 w-40 rounded bg-bg-2 border px-2 text-xs text-fg-0 text-right font-mono outline-none focus:border-line-focus",
                          conflict ? "border-danger" : "border-line-0",
                        )}
                      />
                    </div>
                  );
                })}
              </div>
            ))}
          </div>
        )}

        {tab === "ai" && (
          <p className="text-xs text-fg-2">AI is coming in the next release.</p>
        )}

        {tab === "workspace" && (
          <div className="flex flex-col gap-2">
            {workspace && (
              <>
                <div className="text-xs font-semibold text-fg-0">{workspace.name}</div>
                <div className="font-mono text-[10px] break-all text-fg-2">{workspace.root}</div>
              </>
            )}
            <div className="flex items-center justify-between text-[10px] text-fg-2 pt-2">
              <span>{version ? `Keel ${version}` : "Keel"}</span>
              <span>MIT licensed</span>
            </div>
          </div>
        )}

        <div className="flex justify-end gap-2 pt-1">
          <Button variant="ghost" onClick={() => setSettingsOpen(false)}>
            Cancel
          </Button>
          <Button variant="primary" onClick={save}>
            Save
          </Button>
        </div>
      </div>
    </Modal>
  );
}

export default SettingsModal;
