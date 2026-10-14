import { useEffect, useState } from "react";
import { FolderTree, GitBranch, Globe2, Play, type LucideIcon } from "lucide-react";
import type { ShortcutAction } from "@/api/types";
import { Button, Modal } from "@/components/ui";
import { comboFor, formatCombo } from "@/shortcuts";
import { cn } from "@/utils";
import { useKeel } from "@/state/store";

interface Step {
  icon: LucideIcon;
  title: string;
  body: string;
  /** Optional keyboard shortcut hint shown under the body. */
  combo?: ShortcutAction;
}

const STEPS: Step[] = [
  {
    icon: FolderTree,
    title: "Browse your requests",
    body: "Every request is a plain YAML file you own. Open one from the Collections tree on the left and edit it by hand or with the built-in editor.",
  },
  {
    icon: Globe2,
    title: "Switch environments",
    body: "Pick local, staging, or production from the toolbar. The same request reuses different variables and secrets per environment.",
  },
  {
    icon: Play,
    title: "Send and test",
    body: "Fire a request and write assertions in the Tests tab to catch regressions before they ship.",
    combo: "send",
  },
  {
    icon: GitBranch,
    title: "Share with Git",
    body: "Stage, commit, and push from the Git panel. Your collection lives in the repo like any other code, ready for review.",
  },
];

export function GettingStarted() {
  const open = useKeel((s) => s.gettingStartedOpen);
  const closeGettingStarted = useKeel((s) => s.closeGettingStarted);
  const shortcuts = useKeel((s) => s.settings.shortcuts);
  const [step, setStep] = useState(0);

  useEffect(() => {
    if (open) setStep(0);
  }, [open]);

  const current = STEPS[step];
  const Icon = current.icon;
  const isLast = step === STEPS.length - 1;

  const next = () => {
    if (isLast) closeGettingStarted();
    else setStep((s) => s + 1);
  };

  return (
    <Modal open={open} onClose={closeGettingStarted} title="Getting started" width="max-w-md">
      <div className="flex flex-col gap-4">
        <div className="flex items-center gap-3">
          <div className="h-9 w-9 shrink-0 rounded-md bg-accent-soft flex items-center justify-center">
            <Icon size={16} className="text-accent" />
          </div>
          <div className="flex flex-col gap-0.5 min-w-0">
            <span className="text-sm font-medium text-fg-0">{current.title}</span>
            <span className="text-[10px] text-fg-2">
              Step {step + 1} of {STEPS.length}
            </span>
          </div>
        </div>

        <p className="text-xs text-fg-1 leading-relaxed">{current.body}</p>

        {current.combo && (
          <div className="text-[10px] text-fg-2">
            Tip: press{" "}
            <kbd className="font-mono border border-line-1 rounded px-1">
              {formatCombo(comboFor(current.combo, shortcuts))}
            </kbd>{" "}
            to send the active request.
          </div>
        )}

        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-1.5" aria-hidden="true">
            {STEPS.map((s, i) => (
              <span
                key={s.title}
                className={cn(
                  "h-1.5 w-1.5 rounded-full transition-colors",
                  i <= step ? "bg-accent" : "bg-line-1",
                )}
              />
            ))}
          </div>
          <div className="flex items-center gap-2">
            <Button type="button" variant="ghost" onClick={closeGettingStarted}>
              Skip
            </Button>
            <Button type="button" variant="primary" onClick={next}>
              Continue
            </Button>
          </div>
        </div>
      </div>
    </Modal>
  );
}

export default GettingStarted;
