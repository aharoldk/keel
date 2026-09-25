import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CommandPalette } from "./CommandPalette";
import { installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";
import type { TreeNode } from "@/api/types";

const tree: TreeNode[] = [
  {
    path: "users",
    name: "users",
    kind: "folder",
    children: [
      {
        path: "users/get-users.yaml",
        name: "Get Users",
        kind: "request",
        method: "GET",
      },
    ],
  },
  {
    path: "create-pet.yaml",
    name: "Create Pet",
    kind: "request",
    method: "POST",
  },
];

describe("CommandPalette", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    useKeel.setState({
      paletteOpen: true,
      tree,
      envs: [{ fileName: "local.yaml", name: "local", variableCount: 1, secretCount: 0 }],
    });
  });

  it("renders nothing when closed", () => {
    useKeel.setState({ paletteOpen: false });
    const { container } = render(<CommandPalette />);
    expect(container.childElementCount).toBe(0);
  });

  it("lists flattened requests with method badges and group headers", () => {
    render(<CommandPalette />);
    expect(screen.getByText("Get Users")).toBeTruthy();
    expect(screen.getByText("Create Pet")).toBeTruthy();
    expect(screen.getByText("users/get-users.yaml")).toBeTruthy();
    expect(screen.getByText("Requests")).toBeTruthy();
    expect(screen.getByText("Commands")).toBeTruthy();
    expect(screen.getByText("Send request")).toBeTruthy();
    expect(screen.getByText("Environment: local")).toBeTruthy();
    expect(screen.getByText("No environment")).toBeTruthy();
  });

  it("filters with fuzzy score and shows no matches", () => {
    render(<CommandPalette />);
    const input = screen.getByPlaceholderText("Search requests and commands…");
    fireEvent.change(input, { target: { value: "pet" } });
    expect(screen.queryByText("Get Users")).toBeNull();
    expect(screen.getByText("Create Pet")).toBeTruthy();

    fireEvent.change(input, { target: { value: "zzzqqq" } });
    expect(screen.getByText("No matches")).toBeTruthy();
  });

  it("enter opens the active request", async () => {
    invokeMock.mockImplementation(((cmd: string) => {
      if (cmd === "request_read") {
        return Promise.resolve({
          schemaVersion: "1",
          name: "Get Users",
          kind: "request",
          request: { method: "GET", url: "{{baseUrl}}/" },
        });
      }
      return Promise.resolve(null);
    }) as unknown as typeof invoke);

    render(<CommandPalette />);
    const input = screen.getByPlaceholderText("Search requests and commands…");
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => {
      expect(useKeel.getState().activePath).toBe("users/get-users.yaml");
      expect(useKeel.getState().paletteOpen).toBe(false);
    });
  });

  it("runs commands: toggle sidebar, open settings, select env", () => {
    render(<CommandPalette />);
    const run = (q: string) => {
      const input = screen.getByPlaceholderText("Search requests and commands…");
      fireEvent.change(input, { target: { value: q } });
      fireEvent.keyDown(input, { key: "Enter" });
    };

    run("minimize sidebar");
    expect(useKeel.getState().sidebarOpen).toBe(false);

    act(() => {
      useKeel.setState({ paletteOpen: true, sidebarOpen: true });
    });
    run("open settings");
    expect(useKeel.getState().settingsOpen).toBe(true);

    act(() => {
      useKeel.setState({ paletteOpen: true });
    });
    run("environment: local");
    expect(useKeel.getState().activeEnv).toBe("local.yaml");
  });

  it("shows environment commands added after mount (Bug 8)", async () => {
    useKeel.setState({ envs: [] });
    render(<CommandPalette />);
    expect(screen.queryByText("Environment: prod")).toBeNull();

    act(() => {
      useKeel.setState({
        envs: [{ fileName: "prod.yaml", name: "prod", variableCount: 0, secretCount: 0 }],
      });
    });

    expect(await screen.findByText("Environment: prod")).toBeTruthy();

    const input = screen.getByPlaceholderText("Search requests and commands…");
    fireEvent.change(input, { target: { value: "environment: prod" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(useKeel.getState().activeEnv).toBe("prod.yaml");
  });

  it("escape closes the palette", () => {
    render(<CommandPalette />);
    const input = screen.getByPlaceholderText("Search requests and commands…");
    fireEvent.keyDown(input, { key: "Escape" });
    expect(useKeel.getState().paletteOpen).toBe(false);
  });

  it("clicking the overlay closes, clicking the panel does not", () => {
    const { container } = render(<CommandPalette />);
    fireEvent.click(screen.getByPlaceholderText("Search requests and commands…").parentElement!);
    expect(useKeel.getState().paletteOpen).toBe(true);
    fireEvent.click(container.firstChild as Element);
    expect(useKeel.getState().paletteOpen).toBe(false);
  });
});
