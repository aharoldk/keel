import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { WorkspaceMenu } from "./WorkspaceMenu";
import {
  defaultResponses,
  installDefaultResponses,
  invokeMock,
  openDialogMock,
  resetStore,
} from "@/test/helpers";
import { useKeel } from "@/state/store";
import { emptyRequestDoc } from "@/api/types";

const workspace = { root: "/ws/current", name: "My API", hasGit: true };

function setupStore({ dirty = false }: { dirty?: boolean } = {}) {
  const saved = emptyRequestDoc("A");
  const doc = dirty ? { ...saved, name: "A edited" } : saved;
  useKeel.setState({
    workspace,
    settings: {
      theme: "dark",
      requestTimeoutSec: 30,
      followRedirects: true,
      saveOnSend: false,
      editorFontSize: 13,
      lastWorkspace: workspace.root,
      recentWorkspaces: ["/ws/current", "/ws/one", "/ws/two"],
    },
    tabs: [{ path: "a.yaml", doc, saved, loading: false, result: null, error: null }],
    activePath: "a.yaml",
  });
}

function mockWorkspaceOpen() {
  invokeMock.mockImplementation(((cmd: string, args?: { path?: string }) => {
    if (cmd === "workspace_open") {
      return Promise.resolve({ root: args?.path, name: "Other", hasGit: false });
    }
    return Promise.resolve(structuredClone(defaultResponses[cmd] ?? null));
  }) as unknown as typeof invoke);
}

const invoked = (cmd: string) => invokeMock.mock.calls.some((c) => c[0] === cmd);

describe("WorkspaceMenu", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    setupStore();
  });

  it("opens a menu with New Project, Open Project and recents (current excluded)", () => {
    render(<WorkspaceMenu />);
    fireEvent.click(screen.getByRole("button", { name: /My API/ }));
    expect(screen.getByText("New Project…")).toBeTruthy();
    expect(screen.getByText("Open Project…")).toBeTruthy();
    expect(screen.getByText("Recent projects")).toBeTruthy();
    expect(screen.getByText("one")).toBeTruthy();
    expect(screen.getByText("two")).toBeTruthy();
    expect(screen.queryByText("current")).toBeNull();
  });

  it("switches to a recent project immediately when there are no unsaved changes", async () => {
    mockWorkspaceOpen();
    render(<WorkspaceMenu />);
    fireEvent.click(screen.getByRole("button", { name: /My API/ }));
    fireEvent.click(screen.getByText("one"));
    await waitFor(() => expect(invoked("workspace_open")).toBe(true));
    const call = invokeMock.mock.calls.find((c) => c[0] === "workspace_open");
    expect(call![1]).toEqual({ path: "/ws/one" });
    // the switched-to project is moved to the front of the recents
    await waitFor(() => {
      const setCall = invokeMock.mock.calls.find((c) => c[0] === "settings_set");
      expect(setCall).toBeTruthy();
      expect(
        (setCall![1] as { settings: { recentWorkspaces: string[] } }).settings
          .recentWorkspaces[0],
      ).toBe("/ws/one");
    });
    expect(screen.queryByText("Unsaved changes")).toBeNull();
  });

  it("shows a confirmation modal when switching with unsaved changes; cancel aborts", async () => {
    setupStore({ dirty: true });
    render(<WorkspaceMenu />);
    fireEvent.click(screen.getByRole("button", { name: /My API/ }));
    fireEvent.click(screen.getByText("one"));
    await screen.findByText("Unsaved changes");
    expect(invoked("workspace_open")).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByText("Unsaved changes")).toBeNull());
    expect(invoked("workspace_open")).toBe(false);
    expect(useKeel.getState().workspace?.root).toBe("/ws/current");
  });

  it("Don't Save switches without saving", async () => {
    setupStore({ dirty: true });
    mockWorkspaceOpen();
    render(<WorkspaceMenu />);
    fireEvent.click(screen.getByRole("button", { name: /My API/ }));
    fireEvent.click(screen.getByText("one"));
    await screen.findByText("Unsaved changes");
    fireEvent.click(screen.getByRole("button", { name: "Don't Save" }));
    await waitFor(() => expect(invoked("workspace_open")).toBe(true));
    expect(invoked("request_save")).toBe(false);
  });

  it("Save All & Continue saves dirty tabs before switching", async () => {
    setupStore({ dirty: true });
    mockWorkspaceOpen();
    render(<WorkspaceMenu />);
    fireEvent.click(screen.getByRole("button", { name: /My API/ }));
    fireEvent.click(screen.getByText("one"));
    await screen.findByText("Unsaved changes");
    fireEvent.click(screen.getByRole("button", { name: "Save All & Continue" }));
    await waitFor(() => expect(invoked("workspace_open")).toBe(true));
    const calls = invokeMock.mock.calls.map((c) => c[0]);
    expect(calls.indexOf("request_save")).toBeGreaterThanOrEqual(0);
    expect(calls.indexOf("request_save")).toBeLessThan(calls.indexOf("workspace_open"));
  });

  it("New Project is guarded too: confirm discard, then create modal opens", async () => {
    setupStore({ dirty: true });
    render(<WorkspaceMenu />);
    fireEvent.click(screen.getByRole("button", { name: /My API/ }));
    fireEvent.click(screen.getByText("New Project…"));
    await screen.findByText("Unsaved changes");
    fireEvent.click(screen.getByRole("button", { name: "Don't Save" }));
    await screen.findByText("Create workspace");
    expect(invoked("workspace_open")).toBe(false);
  });

  it("Open Project picks a folder via the native dialog", async () => {
    mockWorkspaceOpen();
    openDialogMock.mockResolvedValue("/ws/picked");
    render(<WorkspaceMenu />);
    fireEvent.click(screen.getByRole("button", { name: /My API/ }));
    fireEvent.click(screen.getByText("Open Project…"));
    await waitFor(() => expect(openDialogMock).toHaveBeenCalledWith({ directory: true }));
    await waitFor(() => expect(invoked("workspace_open")).toBe(true));
    const call = invokeMock.mock.calls.find((c) => c[0] === "workspace_open");
    expect(call![1]).toEqual({ path: "/ws/picked" });
  });
});
