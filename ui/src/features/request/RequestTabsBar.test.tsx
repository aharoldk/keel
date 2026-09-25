import { beforeEach, describe, expect, it } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { RequestTabsBar } from "./RequestTabsBar";
import { installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";
import { emptyRequestDoc } from "@/api/types";
import { runCommand } from "@/commands";

function tab(path: string, name: string, dirty = false) {
  const saved = emptyRequestDoc(name);
  const doc = dirty ? { ...saved, name: `${name} edited` } : saved;
  return { path, doc, saved, loading: false, result: null, error: null };
}

describe("RequestTabsBar close confirmation", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    useKeel.setState({
      tabs: [tab("a.yaml", "Alpha", true), tab("b.yaml", "Beta")],
      activePath: "a.yaml",
    });
  });

  it("closes a clean tab immediately", () => {
    render(<RequestTabsBar />);
    fireEvent.click(screen.getAllByTitle("Close tab")[1]);
    expect(useKeel.getState().tabs.map((t) => t.path)).toEqual(["a.yaml"]);
    expect(screen.queryByText("Unsaved changes")).toBeNull();
  });

  it("asks before closing a dirty tab; cancel keeps it open", () => {
    render(<RequestTabsBar />);
    fireEvent.click(screen.getAllByTitle("Close tab")[0]);
    expect(screen.getByText("Unsaved changes")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(useKeel.getState().tabs.map((t) => t.path)).toEqual(["a.yaml", "b.yaml"]);
  });

  it("Don't Save closes without saving", () => {
    render(<RequestTabsBar />);
    fireEvent.click(screen.getAllByTitle("Close tab")[0]);
    fireEvent.click(screen.getByRole("button", { name: "Don't Save" }));
    expect(useKeel.getState().tabs.map((t) => t.path)).toEqual(["b.yaml"]);
    expect(invokeMock.mock.calls.some((c) => c[0] === "request_save")).toBe(false);
  });

  it("Save & Close saves then closes", async () => {
    render(<RequestTabsBar />);
    fireEvent.click(screen.getAllByTitle("Close tab")[0]);
    fireEvent.click(screen.getByRole("button", { name: "Save & Close" }));
    await waitFor(() =>
      expect(useKeel.getState().tabs.map((t) => t.path)).toEqual(["b.yaml"]),
    );
    expect(invokeMock.mock.calls.some((c) => c[0] === "request_save")).toBe(true);
  });

  it("closeTab shortcut prompts for the active dirty tab", async () => {
    render(<RequestTabsBar />);
    act(() => runCommand("closeTab"));
    await screen.findByText("Unsaved changes");
    expect(useKeel.getState().tabs).toHaveLength(2);
  });
});
