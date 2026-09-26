import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { ConsolePanel } from "./ConsolePanel";
import { installDefaultResponses, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";
import { emptyRequestDoc } from "@/api/types";
import type { SendResult } from "@/api/types";

function tabWith(result: Partial<SendResult> | null) {
  const doc = emptyRequestDoc("Get");
  useKeel.setState({
    activePath: "get.yaml",
    tabs: [
      {
        path: "get.yaml",
        doc,
        saved: doc,
        loading: false,
        result: result as SendResult | null,
        error: null,
      },
    ],
  });
}

describe("ConsolePanel", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("shows the empty hint when there is no output", () => {
    render(<ConsolePanel />);
    expect(screen.getByText(/No console output/)).toBeTruthy();
  });

  it("lists script logs and a script error for the active request", () => {
    tabWith({ scriptLogs: ["hello", "world"], scriptError: "boom" });
    render(<ConsolePanel />);
    expect(screen.getByText("get.yaml")).toBeTruthy();
    expect(screen.getByText("hello")).toBeTruthy();
    expect(screen.getByText("world")).toBeTruthy();
    expect(screen.getByText("boom")).toBeTruthy();
  });

  it("closes from the header button", () => {
    useKeel.setState({ consoleOpen: true });
    render(<ConsolePanel />);
    fireEvent.click(screen.getByTitle("Close console"));
    expect(useKeel.getState().consoleOpen).toBe(false);
  });

  it("grows upward while dragging and remembers the height", () => {
    render(<ConsolePanel />);
    const handle = screen.getByTitle("Drag to resize");
    fireEvent.mouseDown(handle, { clientY: 400 });
    fireEvent.mouseMove(window, { clientY: 300 });
    expect(handle.parentElement?.style.height).toBe("276px");
    fireEvent.mouseUp(window);
    expect(localStorage.getItem("keel.consoleHeight")).toBe("276");
  });
});
