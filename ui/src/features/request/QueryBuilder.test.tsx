import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import QueryBuilder from "./QueryBuilder";
import { emptyRequestDoc, type GqlSchema, type RequestDoc } from "@/api/types";
import { invokeMock } from "@/test/helpers";

const sampleSchema: GqlSchema = {
  queryType: "Query",
  mutationType: "Mutation",
  types: [
    {
      kind: "OBJECT",
      name: "Query",
      fields: [
        {
          name: "me",
          args: [],
          type: { kind: "OBJECT", name: "User" },
        },
        {
          name: "users",
          args: [],
          type: { kind: "LIST", ofType: { kind: "OBJECT", name: "User" } },
        },
      ],
      enumValues: [],
    },
    {
      kind: "OBJECT",
      name: "Mutation",
      fields: [
        {
          name: "createUser",
          args: [
            {
              name: "name",
              type: { kind: "NON_NULL", ofType: { kind: "SCALAR", name: "String" } },
            },
          ],
          type: { kind: "OBJECT", name: "User" },
        },
      ],
      enumValues: [],
    },
    {
      kind: "OBJECT",
      name: "User",
      fields: [
        { name: "id", args: [], type: { kind: "SCALAR", name: "ID" } },
        { name: "name", args: [], type: { kind: "SCALAR", name: "String" } },
      ],
      enumValues: [],
    },
  ],
};

describe("QueryBuilder", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("renders header, controls, and calls onMinimize", () => {
    const doc = emptyRequestDoc("GraphQL Test");
    doc.protocol = "graphql";
    doc.request.url = "http://localhost:4000/graphql";
    const onMinimize = vi.fn();
    const setDoc = vi.fn();
    const toast = vi.fn();

    render(
      <QueryBuilder
        doc={doc}
        setDoc={setDoc}
        toast={toast}
        onMinimize={onMinimize}
      />
    );

    expect(screen.getByText("Query builder")).toBeDefined();
    const minimizeBtn = screen.getByTitle("Minimize query builder");
    expect(minimizeBtn).toBeDefined();

    fireEvent.click(minimizeBtn);
    expect(onMinimize).toHaveBeenCalledTimes(1);
  });

  it("warns if loading schema without a concrete URL", async () => {
    const doc = emptyRequestDoc("GraphQL Test");
    doc.protocol = "graphql";
    doc.request.url = "{{baseUrl}}/graphql";
    const toast = vi.fn();

    render(
      <QueryBuilder
        doc={doc}
        setDoc={vi.fn()}
        toast={toast}
      />
    );

    const loadBtn = screen.getByRole("button", { name: "Load schema" });
    fireEvent.click(loadBtn);

    expect(toast).toHaveBeenCalledWith(
      "Set a concrete GraphQL endpoint URL first",
      "error"
    );
  });

  it("loads schema via graphql_introspect and allows picking fields", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "graphql_introspect") {
        return Promise.resolve(sampleSchema);
      }
      return Promise.resolve(null);
    });

    const doc: RequestDoc = {
      ...emptyRequestDoc("GraphQL Test"),
      protocol: "graphql",
      request: {
        method: "POST",
        url: "http://localhost:4000/graphql",
        headers: [{ name: "Authorization", value: "Bearer xyz", enabled: true }],
        body: { type: "graphql", query: "", variables: "" },
      },
    };
    const setDoc = vi.fn();
    const toast = vi.fn();

    render(
      <QueryBuilder
        doc={doc}
        setDoc={setDoc}
        toast={toast}
      />
    );

    const loadBtn = screen.getByRole("button", { name: "Load schema" });
    fireEvent.click(loadBtn);

    await waitFor(() => {
      expect(screen.getByText("Schema fields")).toBeDefined();
    });

    expect(screen.getByText("me")).toBeDefined();
    expect(screen.getByText("users")).toBeDefined();

    // Pick 'me'
    fireEvent.click(screen.getByText("me"));

    const useQueryBtn = screen.getByRole("button", { name: "Use query" });
    expect(useQueryBtn.hasAttribute("disabled")).toBe(false);

    fireEvent.click(useQueryBtn);
    expect(setDoc).toHaveBeenCalledWith(
      expect.objectContaining({
        protocol: "graphql",
        request: expect.objectContaining({
          body: expect.objectContaining({
            type: "graphql",
            query: expect.stringContaining("me"),
          }),
        }),
      })
    );
  });
});
