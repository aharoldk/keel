import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Dashboard } from "./Dashboard";
import {
  defaultResponses,
  installDefaultResponses,
  invokeMock,
  resetStore,
} from "@/test/helpers";
import { useKeel } from "@/state/store";
import { emptyRequestDoc } from "@/api/types";

const current = { root: "/ws/current", name: "My API", hasGit: true };
const other = { root: "/ws/one", name: "Billing", hasGit: false };

function setup() {
  useKeel.setState({
    workspace: current,
    settings: {
      theme: "dark",
      requestTimeoutSec: 30,
      followRedirects: true,
      saveOnSend: false,
      editorFontSize: 13,
      lastWorkspace: current.root,
      recentWorkspaces: [current.root, other.root],
    },
  });
  invokeMock.mockImplementation(((cmd: string, args?: { path?: string }) => {
    if (cmd === "workspace_peek") {
      return Promise.resolve(args?.path === other.root ? other : null);
    }
    return Promise.resolve(structuredClone(defaultResponses[cmd] ?? null));
  }) as unknown as typeof invoke);
}

describe("Dashboard", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    setup();
  });

  it("lists projects and filters them from the search field", async () => {
    render(<Dashboard />);
    await screen.findByText("Billing");
    expect(screen.getByText("My API")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Search projects"), { target: { value: "bill" } });
    expect(screen.queryByText("My API")).toBeNull();
    expect(screen.getByText("Billing")).toBeTruthy();
  });

  it("renames a project", async () => {
    render(<Dashboard />);
    await screen.findByText("My API");
    fireEvent.click(screen.getAllByTitle("Rename project")[0]);
    const input = screen.getByDisplayValue("My API");
    fireEvent.change(input, { target: { value: "Renamed" } });
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    await waitFor(() => expect(invokeMock.mock.calls.some((c) => c[0] === "collection_save")).toBe(true));
    expect(screen.getByText("Renamed")).toBeTruthy();
  });

  it("removes a project from the list without deleting files", async () => {
    render(<Dashboard />);
    await screen.findByText("Billing");
    fireEvent.click(screen.getAllByTitle("Delete project")[1]);
    await screen.findByText("Delete project");
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByText("Billing")).toBeNull());
    expect(useKeel.getState().settings.recentWorkspaces).toEqual([current.root]);
    expect(invokeMock.mock.calls.some((c) => c[0] === "node_delete")).toBe(false);
  });

  it("shows its own AI screen without touching the project AI panel", async () => {
    render(<Dashboard />);
    fireEvent.click(screen.getByRole("button", { name: "AI" }));
    expect(screen.getByText("AI is coming in the next release.")).toBeTruthy();
    expect(screen.queryByLabelText("Search projects")).toBeNull();
    expect(useKeel.getState().aiOpen).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Projects" }));
    expect(screen.getByLabelText("Search projects")).toBeTruthy();
  });

  it("asks before switching away from unsaved changes", async () => {
    const saved = emptyRequestDoc("A");
    useKeel.setState({
      tabs: [{ path: "a.yaml", doc: { ...saved, name: "edited" }, saved, loading: false, result: null, error: null }],
      activePath: "a.yaml",
    });
    render(<Dashboard />);
    await screen.findByText("Billing");
    fireEvent.click(screen.getByText("Billing"));
    await screen.findByText("Unsaved changes");
    expect(invokeMock.mock.calls.some((c) => c[0] === "workspace_open")).toBe(false);
  });
});
