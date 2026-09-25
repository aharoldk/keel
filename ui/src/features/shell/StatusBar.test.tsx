import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { StatusBar } from "./StatusBar";
import { Sidebar } from "./Sidebar";
import { resetStore, installDefaultResponses } from "@/test/helpers";
import { useKeel } from "@/state/store";
import { emptyRequestDoc } from "@/api/types";

describe("StatusBar", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("renders workspace, env, branch and version", () => {
    useKeel.setState({
      version: "1.2.3",
      workspace: { root: "/tmp/demo", name: "Demo API", hasGit: true },
      activeEnv: "local.yaml",
      git: { hasRepo: true, branch: "main", entries: [] },
      activePath: null,
      tabs: [],
    });
    render(<StatusBar />);
    expect(screen.getByText("Demo API")).toBeTruthy();
    expect(screen.getByText("local.yaml")).toBeTruthy();
    expect(screen.getByText("main")).toBeTruthy();
    expect(screen.getByText("v1.2.3")).toBeTruthy();
  });

  it("shows active request path with dirty marker", () => {
    const doc = emptyRequestDoc("Get Users");
    useKeel.setState({
      workspace: { root: "/tmp/demo", name: "Demo API", hasGit: true },
      activePath: "users.yaml",
      tabs: [
        {
          path: "users.yaml",
          doc: { ...doc, description: "changed" },
          saved: doc,
          loading: false,
          result: null,
          error: null,
        },
      ],
    });
    render(<StatusBar />);
    expect(screen.getByText("users.yaml")).toBeTruthy();
    expect(screen.getByTitle("Unsaved changes")).toBeTruthy();
  });

  it("shows no environment fallback", () => {
    render(<StatusBar />);
    expect(screen.getByText("no environment")).toBeTruthy();
    expect(screen.getByText("No workspace")).toBeTruthy();
  });
});

describe("Sidebar", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("renders panel switcher buttons", () => {
    render(<Sidebar />);
    expect(screen.getByTitle("Collections")).toBeTruthy();
    expect(screen.getByTitle("Environments")).toBeTruthy();
    expect(screen.getByTitle("History")).toBeTruthy();
    expect(screen.getByTitle("Git")).toBeTruthy();
  });

  it("minimizes to the icon rail when closed", () => {
    useKeel.setState({ sidebarOpen: false, sidebarPanel: "collections" });
    render(<Sidebar />);
    // rail stays visible, panel content is hidden
    expect(screen.getByTitle("Collections")).toBeTruthy();
    expect(screen.queryByTitle("New request")).toBeNull();
  });

  it("minimizes via the rail button and re-expands via a panel icon", () => {
    render(<Sidebar />);
    fireEvent.click(screen.getByRole("button", { name: "Minimize sidebar" }));
    expect(useKeel.getState().sidebarOpen).toBe(false);
    fireEvent.click(screen.getByTitle("History"));
    expect(useKeel.getState().sidebarOpen).toBe(true);
    expect(useKeel.getState().sidebarPanel).toBe("history");
  });

  it("switches panels on click", () => {
    render(<Sidebar />);
    fireEvent.click(screen.getByTitle("History"));
    expect(useKeel.getState().sidebarPanel).toBe("history");
    expect(useKeel.getState().sidebarOpen).toBe(true);
  });

  it("resizes the sidebar panel by dragging the handle", () => {
    render(<Sidebar />);
    const handle = screen.getByTestId("sidebar-drag-handle");
    const panel = handle.parentElement as HTMLElement;
    expect(panel.style.width).toBe("240px");
    fireEvent.mouseDown(handle, { clientX: 280 });
    fireEvent.mouseMove(window, { clientX: 380 });
    expect(panel.style.width).toBe("340px");
    fireEvent.mouseUp(window);
    expect(localStorage.getItem("keel.sidebarWidth")).toBe("340");
  });
});
