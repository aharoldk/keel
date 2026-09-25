import { useMemo, useState } from "react";
import { ChevronRight, Plus, X } from "lucide-react";
import { api } from "@/api/client";
import type { GqlSchema, KV, RequestDoc } from "@/api/types";
import { Button, TextInput } from "@/components/ui";
import { cn } from "@/utils";
import { buildQuery, isScalar, namedType, type BuiltField, typeString } from "./graphql";

interface Props {
  doc: RequestDoc;
  setDoc: (d: RequestDoc) => void;
  toast: (msg: string, kind?: "error" | "success" | "info") => void;
}

export default function QueryBuilder({ doc, setDoc, toast }: Props) {
  const [schema, setSchema] = useState<GqlSchema | null>(null);
  const [loading, setLoading] = useState(false);
  const [operation, setOperation] = useState<"query" | "mutation" | "subscription">(
    doc.graphql?.operation ?? "query",
  );
  const [picked, setPicked] = useState<BuiltField[]>([]);

  const rootName = schema
    ? operation === "mutation"
      ? schema.mutationType
      : operation === "subscription"
        ? schema.subscriptionType
        : schema.queryType
    : undefined;
  const root = schema?.types.find((t) => t.name === rootName);

  const load = async () => {
    const url = doc.request.url.trim();
    if (!url || url.includes("{{")) {
      toast("Set a concrete GraphQL endpoint URL first", "error");
      return;
    }
    setLoading(true);
    try {
      const next = await api.graphqlIntrospect(url, doc.request.headers ?? []);
      setSchema(next);
      setPicked([]);
      toast("Schema loaded", "success");
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setLoading(false);
    }
  };

  const apply = () => {
    const query = buildQuery(operation, picked);
    const body = doc.request.body;
    setDoc({
      ...doc,
      protocol: "graphql",
      graphql: { ...doc.graphql, operation },
      request: {
        ...doc.request,
        method: "POST",
        body: {
          type: "graphql",
          query,
          variables: body?.type === "graphql" ? body.variables : "",
        },
      },
    });
  };

  return (
    <div className="flex flex-col gap-2 p-2 border-t border-line-0">
      <div className="flex items-center gap-2">
        <span className="text-xs font-semibold text-fg-1">Query builder</span>
        <select
          className="h-7 text-xs rounded border border-line-0 bg-bg-1 px-1"
          value={operation}
          onChange={(e) => setOperation(e.target.value as typeof operation)}
        >
          <option value="query">query</option>
          <option value="mutation">mutation</option>
          <option value="subscription">subscription</option>
        </select>
        <Button variant="ghost" className="h-7" onClick={() => void load()} disabled={loading}>
          {loading ? "Loading…" : schema ? "Reload schema" : "Load schema"}
        </Button>
        <Button variant="primary" className="h-7" onClick={apply} disabled={!picked.length}>
          Use query
        </Button>
      </div>
      {schema && root && (
        <div className="grid grid-cols-2 gap-2 min-h-40">
          <div className="border border-line-0 rounded overflow-y-auto max-h-64 p-1">
            {root.fields.map((field) => (
              <button
                key={field.name}
                type="button"
                className="w-full text-left text-xs px-1.5 py-1 rounded hover:bg-bg-hover flex items-center gap-1"
                onClick={() =>
                  setPicked((cur) =>
                    cur.some((f) => f.name === field.name)
                      ? cur
                      : [...cur, { name: field.name, args: [], children: [] }],
                  )
                }
              >
                <Plus size={11} />
                <span className="font-mono">{field.name}</span>
                <span className="text-fg-2 ml-auto">{typeString(field.type)}</span>
              </button>
            ))}
          </div>
          <div className="border border-line-0 rounded overflow-y-auto max-h-64 p-1">
            {picked.length === 0 && (
              <div className="text-xs text-fg-2 p-1">Pick fields from the schema.</div>
            )}
            {picked.map((field, i) => (
              <FieldNode
                key={field.name}
                schema={schema}
                parentType={root.name}
                field={field}
                onChange={(next) =>
                  setPicked((cur) => cur.map((f, idx) => (idx === i ? next : f)))
                }
                onRemove={() => setPicked((cur) => cur.filter((_, idx) => idx !== i))}
              />
            ))}
          </div>
        </div>
      )}
      {schema && !root && (
        <div className="text-xs text-fg-2">This schema has no {operation} type.</div>
      )}
    </div>
  );
}

function FieldNode({
  schema,
  parentType,
  field,
  onChange,
  onRemove,
}: {
  schema: GqlSchema;
  parentType: string;
  field: BuiltField;
  onChange: (next: BuiltField) => void;
  onRemove: () => void;
}) {
  const [open, setOpen] = useState(true);
  const def = useMemo(() => {
    const parent = schema.types.find((t) => t.name === parentType);
    return parent?.fields.find((f) => f.name === field.name);
  }, [schema, parentType, field.name]);
  const childType = namedType(def?.type);
  const child = schema.types.find((t) => t.name === childType);
  const expandable = !!def && !isScalar(schema, def.type) && !!child?.fields.length;

  return (
    <div className="text-xs">
      <div className="flex items-center gap-1 px-1 py-0.5">
        {expandable ? (
          <button type="button" onClick={() => setOpen((v) => !v)} className="text-fg-2">
            <ChevronRight size={12} className={cn(open && "rotate-90")} />
          </button>
        ) : (
          <span className="w-3" />
        )}
        <span className="font-mono">{field.name}</span>
        <button type="button" className="ml-auto text-fg-2 hover:text-fg-0" onClick={onRemove}>
          <X size={12} />
        </button>
      </div>
      {open && def && def.args.length > 0 && (
        <div className="pl-5 flex flex-col gap-1 py-1">
          {def.args.map((arg) => (
            <label key={arg.name} className="flex items-center gap-1">
              <span className="w-16 truncate text-fg-2" title={typeString(arg.type)}>
                {arg.name}
              </span>
              <TextInput
                className="flex-1 font-mono h-6"
                placeholder={arg.defaultValue ?? typeString(arg.type)}
                value={field.args.find((a) => a.name === arg.name)?.value ?? ""}
                onChange={(e) => {
                  const args = def.args.map((a) => ({
                    name: a.name,
                    value:
                      a.name === arg.name
                        ? e.target.value
                        : (field.args.find((x) => x.name === a.name)?.value ?? ""),
                  }));
                  onChange({ ...field, args });
                }}
              />
            </label>
          ))}
        </div>
      )}
      {open && expandable && child && (
        <div className="pl-3">
          {child.fields.map((sub) => {
            const selected = field.children.find((c) => c.name === sub.name);
            if (!selected) {
              return (
                <button
                  key={sub.name}
                  type="button"
                  className="flex items-center gap-1 px-1 py-0.5 text-fg-2 hover:text-fg-0"
                  onClick={() =>
                    onChange({
                      ...field,
                      children: [...field.children, { name: sub.name, args: [], children: [] }],
                    })
                  }
                >
                  <Plus size={10} />
                  <span className="font-mono">{sub.name}</span>
                </button>
              );
            }
            return (
              <FieldNode
                key={sub.name}
                schema={schema}
                parentType={child.name}
                field={selected}
                onChange={(next) =>
                  onChange({
                    ...field,
                    children: field.children.map((c) => (c.name === sub.name ? next : c)),
                  })
                }
                onRemove={() =>
                  onChange({ ...field, children: field.children.filter((c) => c.name !== sub.name) })
                }
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

export function enabledHeaders(rows: KV[] | undefined): KV[] {
  return (rows ?? []).filter((r) => r.enabled !== false && r.name.trim());
}
