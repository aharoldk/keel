import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Toolbar } from "./Toolbar";
import { installDefaultResponses, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
    startDragging: vi.fn(),
  }),
}));

describe("Toolbar", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("opens the dashboard and hides project, search, and environment", () => {
    useKeel.setState({ workspace: { root: "/ws", name: "My API", hasGit: false } });
    const { rerender } = render(<Toolbar />);
    fireEvent.click(screen.getByRole("button", { name: "Dashboard" }));
    expect(useKeel.getState().contentPanel).toEqual({ kind: "dashboard" });
    rerender(<Toolbar />);
    expect(screen.queryByRole("button", { name: /My API/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Search/ })).toBeNull();
    expect(screen.queryByTitle("Environment")).toBeNull();
    expect(screen.queryByTitle("AI")).toBeNull();
    expect(screen.queryByRole("button", { name: "Dashboard" })).toBeNull();
  });

  it("opens the command palette from search", () => {
    render(<Toolbar />);
    fireEvent.click(screen.getByRole("button", { name: /Search/ }));
    expect(useKeel.getState().paletteOpen).toBe(true);
  });

  it("selects an environment and can clear it", () => {
    useKeel.setState({
      envs: [{ fileName: "local.yaml", name: "local", variableCount: 2, secretCount: 1 }],
    });
    const selectEnv = vi.fn();
    useKeel.setState({ selectEnv });
    render(<Toolbar />);
    fireEvent.click(screen.getByTitle("Environment"));
    fireEvent.click(screen.getByTitle("local.yaml"));
    expect(selectEnv).toHaveBeenCalledWith("local.yaml");

    fireEvent.click(screen.getByTitle("Environment"));
    fireEvent.click(screen.getAllByRole("button", { name: "No environment" })[1]);
    expect(selectEnv).toHaveBeenCalledWith(null);
  });

  it("closes the environment menu on Escape", () => {
    render(<Toolbar />);
    fireEvent.click(screen.getByTitle("Environment"));
    expect(screen.getByRole("button", { name: "Manage environments" })).toBeTruthy();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("button", { name: "Manage environments" })).toBeNull();
  });

  it("toggles AI and opens settings", () => {
    render(<Toolbar />);
    fireEvent.click(screen.getByTitle("AI"));
    expect(useKeel.getState().aiOpen).toBe(true);
    fireEvent.click(screen.getByTitle("Settings"));
    expect(useKeel.getState().settingsOpen).toBe(true);
  });

  it("drives the window controls", () => {
    render(<Toolbar />);
    expect(screen.getByTitle("Minimize")).toBeTruthy();
    expect(screen.getByTitle("Maximize")).toBeTruthy();
    expect(screen.getByTitle("Close")).toBeTruthy();
    fireEvent.click(screen.getByTitle("Minimize"));
    fireEvent.click(screen.getByTitle("Maximize"));
    fireEvent.click(screen.getByTitle("Close"));
  });
});
