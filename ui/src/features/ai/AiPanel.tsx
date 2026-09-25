import { Sparkles, X } from "lucide-react";
import { IconButton } from "@/components/ui";
import { useKeel } from "@/state/store";

export function AiPanel() {
  const setAiOpen = useKeel((s) => s.setAiOpen);

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
      <div className="flex-1 flex items-center justify-center p-6 text-center">
        <p className="text-xs text-fg-2">AI is coming in the next release.</p>
      </div>
    </aside>
  );
}
