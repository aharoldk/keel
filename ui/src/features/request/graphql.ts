import type { GqlSchema, GqlTypeRef } from "@/api/types";

export function typeString(typeRef: GqlTypeRef | undefined): string {
  if (!typeRef) return "";
  if (typeRef.kind === "NON_NULL") return `${typeString(typeRef.ofType)}!`;
  if (typeRef.kind === "LIST") return `[${typeString(typeRef.ofType)}]`;
  return typeRef.name ?? "";
}

export function namedType(typeRef: GqlTypeRef | undefined): string | undefined {
  let cur = typeRef;
  while (cur) {
    if (cur.name) return cur.name;
    cur = cur.ofType;
  }
  return undefined;
}

export function isScalar(schema: GqlSchema, typeRef: GqlTypeRef | undefined): boolean {
  const name = namedType(typeRef);
  if (!name) return true;
  const found = schema.types.find((t) => t.name === name);
  if (!found) return true;
  return found.kind === "SCALAR" || found.kind === "ENUM";
}

export interface BuiltField {
  name: string;
  args: { name: string; value: string }[];
  children: BuiltField[];
}

export function buildQuery(operation: string, fields: BuiltField[]): string {
  const body = render(fields, 1);
  return `${operation} {\n${body}}\n`;
}

function render(fields: BuiltField[], depth: number): string {
  const pad = "  ".repeat(depth);
  return fields
    .map((field) => {
      const args = field.args.filter((a) => a.value.trim());
      const argText = args.length
        ? `(${args.map((a) => `${a.name}: ${a.value.trim()}`).join(", ")})`
        : "";
      if (!field.children.length) return `${pad}${field.name}${argText}`;
      return `${pad}${field.name}${argText} {\n${render(field.children, depth + 1)}${pad}}`;
    })
    .join("\n") + "\n";
}
