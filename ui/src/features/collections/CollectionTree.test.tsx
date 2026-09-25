import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CollectionTree } from "./CollectionTree";
import { installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";

const tree = [
  {
    path: "get-flow-id.yaml",
    name: "Get Flow ID",
    kind: "request" as const,
    method: "GET" as const,
  },
];

const treeWithFolder = [
  {
    path: "users",
    name: "users",
    kind: "folder" as const,
    children: [],
  },
  ...tree,
];

describe("CollectionTree drag and drop", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    useKeel.setState({ tree: treeWithFolder });
  });

  it("drops a request onto a folder", async () => {
    render(<CollectionTree />);
    const request = screen.getByText("Get Flow ID").closest("[role=treeitem]")!;
    const folder = screen.getByText("users").closest("[role=treeitem]")!;
    const store = new Map<string, string>();
    const data = {
      effectAllowed: "none",
      dropEffect: "none",
      setData: (type: string, value: string) => store.set(type, value),
      getData: (type: string) => store.get(type) ?? "",
    };
    vi.spyOn(folder, "getBoundingClientRect").mockReturnValue({
      top: 0, height: 28, bottom: 28, left: 0, right: 100, width: 100, x: 0, y: 0, toJSON() {},
    });
    fireEvent.dragStart(request, { dataTransfer: data });
    fireEvent.dragOver(folder, { dataTransfer: data });
    (folder as HTMLElement).dispatchEvent(
      Object.assign(new Event("drop", { bubbles: true }), { clientY: 14, dataTransfer: data }),
    );

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "node_move" &&
            (c[1] as { path: string; dest: string }).path === "get-flow-id.yaml" &&
            (c[1] as { path: string; dest: string }).dest === "users",
        ),
      ).toBe(true),
    );
  });

  it("drops above a sibling to reorder", async () => {
    const second = { path: "list.yaml", name: "List", kind: "request" as const, method: "GET" as const };
    useKeel.setState({ tree: [...treeWithFolder, second] });
    render(<CollectionTree />);
    const source = screen.getByText("List").closest("[role=treeitem]")!;
    const target = screen.getByText("Get Flow ID").closest("[role=treeitem]")!;
    const store = new Map<string, string>();
    const data = {
      effectAllowed: "none",
      dropEffect: "none",
      setData: (type: string, value: string) => store.set(type, value),
      getData: (type: string) => store.get(type) ?? "",
    };
    vi.spyOn(target, "getBoundingClientRect").mockReturnValue({
      top: 0, height: 28, bottom: 28, left: 0, right: 100, width: 100, x: 0, y: 0, toJSON() {},
    });
    fireEvent.dragStart(source, { dataTransfer: data });
    fireEvent.drop(target, { dataTransfer: data, clientY: 2 });

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "node_reorder" &&
            (c[1] as { path: string; target: string; before: boolean }).path === "list.yaml" &&
            (c[1] as { path: string; target: string; before: boolean }).target === "get-flow-id.yaml" &&
            (c[1] as { path: string; target: string; before: boolean }).before === true,
        ),
      ).toBe(true),
    );
  });
});

describe("CollectionTree rename", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    useKeel.setState({ tree });
  });

  it("right-click → Rename shows an inline input that stays open", async () => {
    render(<CollectionTree />);
    fireEvent.contextMenu(screen.getByText("Get Flow ID"));
    fireEvent.mouseDown(screen.getByText("Rename"));

    const input = await screen.findByDisplayValue("Get Flow ID");
    // the input must remain mounted (regression: blur dismissed it instantly)
    expect(input).toBeTruthy();
  });

  it("commits the inline rename on Enter", async () => {
    render(<CollectionTree />);
    fireEvent.contextMenu(screen.getByText("Get Flow ID"));
    fireEvent.mouseDown(screen.getByText("Rename"));

    const input = await screen.findByDisplayValue("Get Flow ID");
    fireEvent.change(input, { target: { value: "Get Flow ID v2" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "request_rename" &&
            (c[1] as { path: string; newName: string }).path === "get-flow-id.yaml" &&
            (c[1] as { path: string; newName: string }).newName === "Get Flow ID v2",
        ),
      ).toBe(true),
    );
  });

  it("Escape cancels the inline rename", async () => {
    render(<CollectionTree />);
    fireEvent.contextMenu(screen.getByText("Get Flow ID"));
    fireEvent.mouseDown(screen.getByText("Rename"));

    const input = await screen.findByDisplayValue("Get Flow ID");
    fireEvent.keyDown(input, { key: "Escape" });

    expect(screen.queryByDisplayValue("Get Flow ID")).toBeNull();
    expect(invokeMock.mock.calls.some((c) => c[0] === "request_rename")).toBe(false);
  });
});

describe("CollectionTree search", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    useKeel.setState({
      tree: [
        {
          path: "users",
          name: "users",
          kind: "folder",
          children: [
            {
              path: "users/list.yaml",
              name: "List users",
              kind: "request",
              method: "GET",
              url: "{{baseUrl}}/users",
            },
          ],
        },
        {
          path: "auth/login.yaml",
          name: "Login",
          kind: "request",
          method: "POST",
          url: "{{baseUrl}}/auth/login",
        },
      ],
    });
  });

  it("filters by name and keeps the parent folder", () => {
    render(<CollectionTree />);
    fireEvent.change(screen.getByLabelText("Search requests"), { target: { value: "login" } });
    expect(screen.queryByText("List users")).toBeNull();
    expect(screen.getByText("Login")).toBeTruthy();
    expect(screen.queryByText("users")).toBeNull();
  });

  it("matches a URL and shows no matches when nothing hits", () => {
    render(<CollectionTree />);
    fireEvent.change(screen.getByLabelText("Search requests"), { target: { value: "/users" } });
    expect(screen.getByText("List users")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Search requests"), { target: { value: "nope" } });
    expect(screen.getByText(/No matches/)).toBeTruthy();
  });
});
