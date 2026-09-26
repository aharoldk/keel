import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { FlowPanel } from "./FlowPanel";
import {
  defaultResponses,
  invokeMock,
  resetStore,
  saveDialogMock,
} from "@/test/helpers";
import { useKeel } from "@/state/store";

const flowTree = [
  { path: "deploy", name: "Deploy", kind: "folder" as const, children: [] },
  { path: "release.yaml", name: "Release", kind: "flow" as const },
];

const releaseFlow = {
  schemaVersion: "1",
  name: "Release",
  kind: "flow" as const,
  steps: ["release.yaml"],
};

const sendResult = {
  requestId: "r1",
  status: 200,
  statusText: "OK",
  ok: true,
  timeMs: 12,
  sizeBytes: 2,
  headers: [],
  cookies: [],
  contentType: "application/json",
  bodyText: "{}",
  bodyBase64: null,
  truncated: false,
  error: null,
  variablesUsed: [],
  missingVariables: [],
  secretsUsed: [],
  testResults: [],
  scriptLogs: [],
  scriptError: null,
  timeline: [],
};

function install(extra: Record<string, unknown> = {}, tree = flowTree) {
  const responses: Record<string, unknown> = {
    ...defaultResponses,
    flow_tree: tree,
    ...extra,
  };
  invokeMock.mockImplementation(((cmd: string) =>
    Promise.resolve(
      structuredClone(responses[cmd] ?? null),
    )) as unknown as typeof invoke);
}

function dragOnto(source: Element, target: Element, clientY: number) {
  vi.spyOn(target, "getBoundingClientRect").mockReturnValue({
    top: 0, height: 28, bottom: 28, left: 0, right: 100, width: 100, x: 0, y: 0, toJSON() {},
  });
  document.elementFromPoint = () => target as HTMLElement;
  fireEvent.mouseDown(source, { clientX: 10, clientY: 0, button: 0 });
  fireEvent.mouseMove(document, { clientX: 14, clientY });
  fireEvent.mouseUp(document, { clientX: 14, clientY });
}

describe("FlowPanel drag and drop", () => {
  beforeEach(() => {
    resetStore();
    useKeel.setState({
      workspace: { root: "/w", name: "w", hasGit: false },
      tree: [
        { path: "release.yaml", name: "Release", kind: "request", method: "POST" },
      ],
    });
  });

  it("moves a flow into a folder when dropped on it", async () => {
    install();
    render(<FlowPanel />);
    const flow = (await screen.findByText("Release")).closest("[role=treeitem]")!;
    const folder = screen.getByText("Deploy").closest("[role=treeitem]")!;
    dragOnto(flow, folder, 14);

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "flow_move" &&
            (c[1] as { path: string; dest: string }).path === "release.yaml" &&
            (c[1] as { path: string; dest: string }).dest === "deploy",
        ),
      ).toBe(true),
    );
  });

  it("reorders a flow when dropped above a sibling", async () => {
    install({}, [
      { path: "build.yaml", name: "Build", kind: "flow" },
      { path: "release.yaml", name: "Release", kind: "flow" },
    ]);
    render(<FlowPanel />);
    const source = (await screen.findByText("Release")).closest("[role=treeitem]")!;
    const target = screen.getByText("Build").closest("[role=treeitem]")!;
    dragOnto(source, target, 2);

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "flow_reorder" &&
            (c[1] as { path: string; target: string; before: boolean }).path === "release.yaml" &&
            (c[1] as { path: string; target: string; before: boolean }).target === "build.yaml" &&
            (c[1] as { path: string; target: string; before: boolean }).before === true,
        ),
      ).toBe(true),
    );
  });
});

describe("FlowPanel run", () => {
  beforeEach(() => {
    resetStore();
    useKeel.setState({
      workspace: { root: "/w", name: "w", hasGit: false },
      tree: [
        { path: "release.yaml", name: "Release", kind: "request", method: "POST" },
      ],
    });
  });

  it("runs a flow from the row icon", async () => {
    install({ flow_read: releaseFlow, send_request: sendResult });
    render(<FlowPanel />);
    fireEvent.click(await screen.findByLabelText("Run Release"));

    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "send_request")).toBe(true),
    );
    await waitFor(() => expect(useKeel.getState().flowRun?.running).toBe(false));
    const run = useKeel.getState().flowRun!;
    expect(run.name).toBe("Release");
    expect(Object.values(run.results)[0]?.status).toBe("ok");
  });
});

describe("FlowPanel context menu", () => {
  beforeEach(() => {
    resetStore();
    useKeel.setState({
      workspace: { root: "/w", name: "w", hasGit: false },
      tree: [
        { path: "release.yaml", name: "Release", kind: "request", method: "POST" },
      ],
    });
  });

  it("offers Run, Edit, Duplicate, Export and Delete on right click", async () => {
    install();
    render(<FlowPanel />);
    fireEvent.contextMenu(await screen.findByText("Release"));

    expect(screen.getByText("Run")).toBeTruthy();
    expect(screen.getByText("Edit")).toBeTruthy();
    expect(screen.getByText("Duplicate")).toBeTruthy();
    expect(screen.getByText("Export…")).toBeTruthy();
    expect(screen.getByText("Delete")).toBeTruthy();
  });

  it("duplicates a flow", async () => {
    install({ flow_duplicate: "release-copy.yaml" });
    render(<FlowPanel />);
    fireEvent.contextMenu(await screen.findByText("Release"));
    fireEvent.mouseDown(screen.getByText("Duplicate"));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "flow_duplicate" &&
            (c[1] as { path: string }).path === "release.yaml",
        ),
      ).toBe(true),
    );
  });

  it("deletes a flow", async () => {
    install();
    render(<FlowPanel />);
    fireEvent.contextMenu(await screen.findByText("Release"));
    fireEvent.mouseDown(screen.getByText("Delete"));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "flow_delete" &&
            (c[1] as { fileName: string }).fileName === "release.yaml",
        ),
      ).toBe(true),
    );
  });

  it("exports a flow to a chosen file", async () => {
    install({ flow_read: releaseFlow, flow_to_yaml: "name: Release" });
    saveDialogMock.mockResolvedValue("/tmp/release.yaml");
    render(<FlowPanel />);
    fireEvent.contextMenu(await screen.findByText("Release"));
    fireEvent.mouseDown(screen.getByText("Export…"));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) =>
            c[0] === "save_response" &&
            (c[1] as { path: string }).path === "/tmp/release.yaml",
        ),
      ).toBe(true),
    );
  });
});
