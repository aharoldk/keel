import { Spinner } from "@/components/ui";
import { cn } from "@/utils";

function lineClass(line: string): string {
  if (line.startsWith("+++") || line.startsWith("---")) return "text-fg-2";
  if (line.startsWith("+")) return "text-ok";
  if (line.startsWith("-")) return "text-danger";
  if (line.startsWith("@@")) return "text-info";
  return "text-fg-1";
}

export function GitDiffView({ title, text }: { title: string; text: string | null }) {
  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="h-9 shrink-0 border-b border-line-0 flex items-center px-3 text-xs font-semibold text-fg-1 truncate">
        {title}
      </div>
      {text == null ? (
        <div className="flex-1 flex items-center justify-center">
          <Spinner size={18} />
        </div>
      ) : (
        <pre className="flex-1 min-h-0 overflow-auto p-3 font-mono text-[11px] leading-5 whitespace-pre">
          {text.split("\n").map((line, i) => (
            <div key={i} className={cn(lineClass(line))}>
              {line || " "}
            </div>
          ))}
        </pre>
      )}
    </div>
  );
}
