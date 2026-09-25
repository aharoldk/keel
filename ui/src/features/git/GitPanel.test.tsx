import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { GitPanel } from "./GitPanel";
import { installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";
import type { GitStatus } from "@/api/types";

const withRepo: GitStatus = {
  hasRepo: true,
  branch: "main",
  entries: [
    { path: "users.yaml", status: "modified", staged: false },
    { path: "pet.yaml", status: "added", staged: true },
  ],
  remoteUrl: null,
  ahead: null,
  behind: null,
};

describe("GitPanel", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("shows spinner placeholder when git status is unknown", () => {
    const { container } = render(<GitPanel />);
    expect(container.querySelector(".animate-spin")).toBeTruthy();
  });

  it("offers repository init when no repo", async () => {
    useKeel.setState({
      git: { hasRepo: false, branch: null, entries: [], remoteUrl: null, ahead: null, behind: null },
    });
    render(<GitPanel />);
    fireEvent.click(await screen.findByText("Initialize repository"));
    await waitFor(() => {
      expect(invokeMock.mock.calls.some((c) => c[0] === "git_init")).toBe(true);
    });
  });

  it("renders branch, staged and unstaged entries with status chips", async () => {
    useKeel.setState({ git: withRepo });
    render(<GitPanel />);
    await screen.findByText("main");
    expect(screen.getByText("Staged (1)")).toBeTruthy();
    expect(screen.getByText("Changes (1)")).toBeTruthy();
    expect(screen.getByText("users.yaml")).toBeTruthy();
    expect(screen.getByText("pet.yaml")).toBeTruthy();
  });

  it("stages a file via hover action", async () => {
    useKeel.setState({ git: structuredClone(withRepo) });
    render(<GitPanel />);
    fireEvent.click(await screen.findByTitle("Stage"));
    await waitFor(() => {
      const call = invokeMock.mock.calls.find((c) => c[0] === "git_stage");
      expect(call).toBeTruthy();
      expect(call![1]).toEqual({ paths: ["users.yaml"] });
    });
  });

  it("stage all / unstage all pass null", async () => {
    useKeel.setState({ git: structuredClone(withRepo) });
    render(<GitPanel />);
    fireEvent.click(screen.getByRole("button", { name: "Stage all" }));
    await waitFor(() => {
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "git_stage" && c[1] !== null && (c[1] as { paths: string[] | null }).paths === null),
      ).toBe(true);
    });
    fireEvent.click(screen.getByRole("button", { name: "Unstage all" }));
    await waitFor(() => {
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "git_unstage" && c[1] !== null && (c[1] as { paths: string[] | null }).paths === null),
      ).toBe(true);
    });
  });

  it("commit disabled without message or staged entries; commits with message", async () => {
    useKeel.setState({ git: structuredClone(withRepo) });
    render(<GitPanel />);
    const commitBtn = await screen.findByRole("button", { name: /Commit/ });
    expect((commitBtn as HTMLButtonElement).disabled).toBe(true);

    fireEvent.change(screen.getByPlaceholderText("Commit message"), {
      target: { value: "add pet endpoint" },
    });
    expect((commitBtn as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(commitBtn);

    await waitFor(() => {
      const call = invokeMock.mock.calls.find((c) => c[0] === "git_commit");
      expect(call).toBeTruthy();
      expect((call![1] as { message: string }).message).toBe("add pet endpoint");
    });
    expect(useKeel.getState().toasts.some((t) => t.message === "Committed")).toBe(true);
  });

  it("shows commit history rows", async () => {
    invokeMock.mockImplementation(((cmd: string) => {
      if (cmd === "git_log") {
        return Promise.resolve([
          {
            oid: "abcdef1234567890",
            shortOid: "abcdef1",
            message: "initial commit",
            author: "keel",
            time: "2024-01-01T00:00:00Z",
          },
        ]);
      }
      return Promise.resolve(null);
    }) as unknown as typeof invoke);

    useKeel.setState({ git: structuredClone(withRepo) });
    render(<GitPanel />);
    expect(await screen.findByText("abcdef1")).toBeTruthy();
    expect(screen.getByText("initial commit")).toBeTruthy();
    expect(screen.getByText(/keel ·/)).toBeTruthy();
  });

  it("opens diff modal on row click", async () => {
    invokeMock.mockImplementation(((cmd: string) => {
      if (cmd === "git_diff_file") return Promise.resolve("--- a/users.yaml\n+++ b/users.yaml");
      if (cmd === "git_status") return Promise.resolve(structuredClone(withRepo));
      return Promise.resolve(null);
    }) as unknown as typeof invoke);

    useKeel.setState({ git: structuredClone(withRepo) });
    render(<GitPanel />);
    fireEvent.click(await screen.findByText("users.yaml"));
    await screen.findByText(/Diff — users.yaml/);
    await waitFor(() => {
      expect(screen.getByText(/--- a\/users.yaml/)).toBeTruthy();
    });
  });
});
