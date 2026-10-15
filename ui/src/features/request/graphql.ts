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
  args: { name: string; value: string; type?: string }[];
  children: BuiltField[];
}

export interface BuiltQuery {
  /** The operation document, ready for the query editor. */
  query: string;
  /** Variables JSON for the operation; `""` when the operation has none. */
  variables: string;
}

/** Parses an argument value: JSON when it parses, otherwise raw text as a string. */
function variableValue(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

function capitalize(s: string): string {
  return s ? s[0].toUpperCase() + s.slice(1) : s;
}

/**
 * `$name` once the argument joins the variable set, otherwise the inline
 * literal. Repeated arguments share one variable when the type and value
 * match; a conflict keeps the literal so the query still sends what was
 * typed.
 */
function argRef(
  arg: { name: string; value: string; type?: string },
  variables: Map<string, { type: string; value: string }>,
): string {
  const value = arg.value.trim();
  const existing = variables.get(arg.name);
  if (!arg.type) return value;
  if (!existing) {
    variables.set(arg.name, { type: arg.type, value });
    return `$${arg.name}`;
  }
  if (existing.type === arg.type && existing.value === value) return `$${arg.name}`;
  return value;
}

/**
 * Builds an operation document from a field selection. Filled arguments
 * become GraphQL variables — `query GetUser($id: ID!) { user(id: $id) … }` —
 * together with the matching variables JSON. The operation is named when
 * variables exist (they require one) or when a name is given; otherwise the
 * anonymous shorthand is kept.
 */
export function buildQuery(
  operation: string,
  fields: BuiltField[],
  name?: string,
): BuiltQuery {
  const variables = new Map<string, { type: string; value: string }>();
  const body = render(fields, 1, variables);
  const varDefs = [...variables.entries()]
    .map(([n, v]) => `$${n}: ${v.type}`)
    .join(", ");
  const opName = (name ?? "").trim() || (variables.size ? capitalize(operation) : "");
  const header = opName ? `${operation} ${opName}${varDefs ? `(${varDefs})` : ""}` : operation;
  const query = `${header} {\n${body}}\n`;
  const varsJson = variables.size
    ? JSON.stringify(
        Object.fromEntries(
          [...variables.entries()].map(([n, v]) => [n, variableValue(v.value.trim())]),
        ),
        null,
        2,
      )
    : "";
  return { query, variables: varsJson };
}

function render(
  fields: BuiltField[],
  depth: number,
  variables: Map<string, { type: string; value: string }>,
): string {
  const pad = "  ".repeat(depth);
  return (
    fields
      .map((field) => {
        const args = field.args.filter((a) => a.value.trim());
        const argText = args.length
          ? `(${args.map((a) => `${a.name}: ${argRef(a, variables)}`).join(", ")})`
          : "";
        if (!field.children.length) return `${pad}${field.name}${argText}`;
        return `${pad}${field.name}${argText} {\n${render(field.children, depth + 1, variables)}${pad}}`;
      })
      .join("\n") + "\n"
  );
}

/**
 * Pretty-prints a GraphQL document: selection-set braces open a new indented
 * line and fields land one per line, while strings (including `"""…"""` block
 * strings) and `#` comments are copied verbatim so braces inside them never
 * affect indentation. Operation headers (`query GetUser($id: ID!) {`) stay on
 * one line, as do type conditions (`… on User`).
 */
export function formatGraphql(text: string): string {
  const lines: { text: string; depth: number }[] = [];
  let buf = "";
  let depth = 0;
  let inString = false;
  let escaped = false;
  let nextField = false;

  const flush = () => {
    const trimmed = buf.trim();
    if (trimmed) lines.push({ text: trimmed, depth });
    buf = "";
  };
  const isNameChar = (c: string) => /[_0-9A-Za-z]/.test(c);
  const nextToken = (from: number): string => {
    let j = from;
    while (j < text.length && /\s/.test(text[j])) j += 1;
    return text[j] ?? "";
  };

  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if (inString) {
      buf += ch;
      if (escaped) escaped = false;
      else if (ch === "\\") escaped = true;
      else if (ch === '"') inString = false;
      continue;
    }
    if (text.startsWith('"""', i)) {
      const end = text.indexOf('"""', i + 3);
      const stop = end === -1 ? text.length : end + 3;
      buf += text.slice(i, stop);
      i = stop - 1;
      continue;
    }
    if (ch === '"') {
      inString = true;
      buf += ch;
      continue;
    }
    if (ch === "#") {
      flush();
      let comment = "#";
      while (i + 1 < text.length && text[i + 1] !== "\n") {
        comment += text[i + 1];
        i += 1;
      }
      lines.push({ text: comment.trimEnd(), depth });
      continue;
    }
    if (ch === "{") {
      buf += ch;
      flush();
      depth += 1;
      continue;
    }
    if (ch === "}") {
      flush();
      depth = Math.max(0, depth - 1);
      buf = "}";
      flush();
      continue;
    }
    if (/\s/.test(ch)) {
      if (buf.length > 0 && !buf.endsWith(" ") && !nextField) buf += " ";
      continue;
    }
    if (isNameChar(ch)) {
      if (nextField) {
        flush();
        nextField = false;
      }
      let name = ch;
      while (i + 1 < text.length && isNameChar(text[i + 1])) {
        i += 1;
        name += text[i];
      }
      buf += name;
      // Two names back to back inside a selection set are separate fields —
      // except after `on`, which prefixes a type condition.
      if (depth >= 1 && name !== "on" && isNameChar(nextToken(i + 1))) nextField = true;
      continue;
    }
    buf += ch;
  }
  flush();

  return (
    lines.map((l) => "  ".repeat(l.depth) + l.text).join("\n") + (lines.length ? "\n" : "")
  );
}
