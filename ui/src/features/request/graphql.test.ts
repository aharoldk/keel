import { describe, expect, it } from "vitest";
import { buildQuery, namedType, typeString } from "./graphql";

describe("graphql query builder", () => {
  it("renders nested fields and args", () => {
    const query = buildQuery("query", [
      {
        name: "user",
        args: [{ name: "id", value: '"1"' }],
        children: [
          { name: "id", args: [], children: [] },
          { name: "name", args: [], children: [] },
        ],
      },
    ]);
    expect(query).toContain('user(id: "1")');
    expect(query).toContain("name");
    expect(query.startsWith("query {")).toBe(true);
  });

  it("unwraps type wrappers", () => {
    const type = {
      kind: "NON_NULL",
      ofType: { kind: "LIST", ofType: { kind: "SCALAR", name: "String" } },
    };
    expect(typeString(type)).toBe("[String]!");
    expect(namedType(type)).toBe("String");
  });
});
