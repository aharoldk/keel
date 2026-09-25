import type { RequestDoc } from "@/api/types";
import CodeEditor from "@/features/request/CodeEditor";

interface DocsTabProps {
  doc: RequestDoc;
  setDoc: (d: RequestDoc) => void;
}

export default function DocsTab({ doc, setDoc }: DocsTabProps) {
  return (
    <div className="h-full min-h-0 flex flex-col">
      <div className="flex items-baseline gap-2 p-2 shrink-0">
        <span className="text-xs font-semibold text-fg-1">Description</span>
        <span className="text-[10px] text-fg-2">Markdown, shown in tooltips</span>
      </div>
      <div className="flex-1 min-h-0 bg-bg-1">
        <CodeEditor
          value={doc.description ?? ""}
          onChange={(v) => setDoc({ ...doc, description: v })}
          language="text"
          appearance="editor"
          lineNumbers
          height="100%"
          placeholder="Describe this request…"
        />
      </div>
    </div>
  );
}

export { DocsTab };
