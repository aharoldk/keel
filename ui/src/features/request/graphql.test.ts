import { describe, expect, it } from "vitest";
import { buildQuery, namedType, typeString } from "./graphql";

const leaf = (name: string) => ({ name, args: [], children: [] });

describe("graphql query builder", () => {
  it("renders nested fields with arguments as variables", () => {
    const { query, variables } = buildQuery("query", [
      {
        name: "user",
        args: [{ name: "id", value: '"1"', type: "ID!" }],
        children: [leaf("id"), leaf("name")],
      },
    ]);
    expect(query).toContain('user(id: $id)');
    expect(query).toContain("name");
    expect(query.startsWith("query Query($id: ID!) {")).toBe(true);
    expect(JSON.parse(variables)).toEqual({ id: "1" });
  });

  it("keeps the anonymous shorthand when no arguments are filled", () => {
    const { query, variables } = buildQuery("query", [
      { name: "users", args: [], children: [leaf("id")] },
    ]);
    expect(query.startsWith("query {")).toBe(true);
    expect(variables).toBe("");
  });

  it("uses the given operation name", () => {
    const { query } = buildQuery(
      "mutation",
      [
        {
          name: "createUser",
          args: [{ name: "name", value: '"Keel"', type: "String!" }],
          children: [leaf("id")],
        },
      ],
      "CreateUser",
    );
    expect(query.startsWith("mutation CreateUser($name: String!) {")).toBe(true);
    expect(query).toContain("createUser(name: $name)");
  });

  it("shares one variable for repeated arguments and inlines conflicts", () => {
    const { query, variables } = buildQuery("query", [
      { name: "user", args: [{ name: "id", value: '"1"', type: "ID!" }], children: [] },
      { name: "post", args: [{ name: "id", value: '"2"', type: "ID!" }], children: [] },
    ]);
    expect(query).toContain('user(id: $id)');
    expect(query).toContain('post(id: "2")');
    expect(JSON.parse(variables)).toEqual({ id: "1" });
  });

  it("keeps templates as string variables", () => {
    const { variables } = buildQuery("query", [
      {
        name: "user",
        args: [{ name: "id", value: "{{userId}}", type: "ID" }],
        children: [],
      },
    ]);
    expect(JSON.parse(variables)).toEqual({ id: "{{userId}}" });
  });

  it("parses non-string literals", () => {
    const { variables } = buildQuery("query", [
      {
        name: "users",
        args: [{ name: "limit", value: "2", type: "Int" }],
        children: [],
      },
    ]);
    expect(JSON.parse(variables)).toEqual({ limit: 2 });
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
