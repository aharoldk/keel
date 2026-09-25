import type { SendResult } from "@/api/types";

interface PreviewPaneProps {
  result: SendResult;
}

export default function PreviewPane({ result }: PreviewPaneProps) {
  const ct = (result.contentType ?? "").toLowerCase();
  const mime = ct.split(";")[0].trim();

  if (mime.startsWith("image/")) {
    return (
      <div className="flex-1 min-h-0 flex flex-col items-center justify-center overflow-auto p-2">
        <img
          src={`data:${mime};base64,${result.bodyBase64 ?? ""}`}
          alt="Response preview"
          className="max-h-full object-contain"
        />
      </div>
    );
  }

  if (mime === "application/pdf") {
    return (
      <div className="flex-1 min-h-0 flex flex-col">
        <object
          data={`data:application/pdf;base64,${result.bodyBase64 ?? ""}`}
          type="application/pdf"
          className="w-full h-full min-h-[300px]"
        />
      </div>
    );
  }

  if (mime === "text/html") {
    return (
      <div className="flex-1 min-h-0 flex flex-col">
        <iframe
          sandbox=""
          srcDoc={result.bodyText ?? ""}
          className="w-full h-full bg-white"
          title="preview"
        />
      </div>
    );
  }

  return (
    <div className="flex-1 min-h-0 flex items-center justify-center text-xs text-fg-2">
      No preview available for this content type
    </div>
  );
}

export function isPreviewable(contentType: string | null): boolean {
  const mime = (contentType ?? "").toLowerCase().split(";")[0].trim();
  return mime === "text/html" || mime.startsWith("image/") || mime === "application/pdf";
}

export { PreviewPane };
