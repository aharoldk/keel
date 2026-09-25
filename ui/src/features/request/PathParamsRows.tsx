import { useEffect, useMemo } from "react";
import type { KV } from "@/api/types";
import VariableInput from "@/components/VariableInput";
import type { VariableSuggestion } from "@/features/request/variables";

export function derivePathNames(url: string): string[] {
  const q = url.indexOf("?");
  const path = q === -1 ? url : url.slice(0, q);
  const names: string[] = [];
  const re = /:([A-Za-z0-9_]+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(path)) !== null) {
    if (!names.includes(m[1])) names.push(m[1]);
  }
  return names;
}

interface PathParamsRowsProps {
  url: string;
  rows: KV[] | undefined;
  onChange: (rows: KV[]) => void;
  variables?: VariableSuggestion[];
}

export default function PathParamsRows({
  url,
  rows,
  onChange,
  variables,
}: PathParamsRowsProps) {
  const names = useMemo(() => derivePathNames(url), [url]);
  const list = rows ?? [];

  useEffect(() => {
    if (names.length === 0 || rows == null) return;
    const byName = new Map(rows.map((r) => [r.name, r]));
    const next = names.map((n) => byName.get(n) ?? { name: n, value: "" });
    if (JSON.stringify(next) !== JSON.stringify(rows)) onChange(next);
  }, [names, rows, onChange]);

  if (names.length === 0) return null;

  const valueFor = (n: string) => list.find((r) => r.name === n)?.value ?? "";

  const setValue = (n: string, v: string) => {
    const byName = new Map(list.map((r) => [r.name, r]));
    const next = names.map((name) => {
      const existing = byName.get(name);
      if (name === n) return { ...(existing ?? { name }), value: v };
      return { ...(existing ?? { name, value: "" }) };
    });
    onChange(next);
  };

  return (
    <div className="flex flex-col gap-1">
      <div className="text-xs font-semibold text-fg-1">Path</div>
      {names.map((n) => (
        <div key={n} className="flex items-center gap-1.5">
          <span
            className="h-7 inline-flex items-center rounded bg-bg-3 border border-line-0 px-2 text-xs font-mono text-fg-1 shrink-0"
            title={`Path segment in URL: :${n}`}
          >
            :{n}
          </span>
          <VariableInput
            value={valueFor(n)}
            onChange={(v) => setValue(n, v)}
            placeholder="value"
            className="flex-1 min-w-0"
            inputClassName="font-mono"
            variables={variables}
          />
        </div>
      ))}
    </div>
  );
}

export { PathParamsRows };
