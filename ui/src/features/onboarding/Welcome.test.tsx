import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Welcome } from "./Welcome";
import { installDefaultResponses, invokeMock, openDialogMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";

describe("Welcome", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    useKeel.setState({ version: "0.1.0" });
  });

  it("renders logo, tagline, cards and footer", () => {
    render(<Welcome />);
    expect(screen.getByText("Keel")).toBeTruthy();
    expect(
      screen.getByText("Your API project as plain text files — browse, send, test, and commit. Fully offline."),
    ).toBeTruthy();
    expect(screen.getByText("Open workspace")).toBeTruthy();
    expect(screen.getByText("Create workspace")).toBeTruthy();
    expect(screen.getByText("Choose an existing Keel folder")).toBeTruthy();
    expect(screen.getByText("Scaffold a new collection")).toBeTruthy();
    expect(screen.getByText("MIT License")).toBeTruthy();
    expect(screen.getByText("Keel 0.1.0")).toBeTruthy();
  });

  it("open workspace with no folder picked does nothing", async () => {
    openDialogMock.mockResolvedValue(null);
    render(<Welcome />);
    fireEvent.click(screen.getByText("Open workspace"));
    await waitFor(() => {
      expect(openDialogMock).toHaveBeenCalledWith({ directory: true });
    });
    expect(useKeel.getState().workspace).toBeNull();
  });

  it("open workspace error surfaces a toast hinting at create flow", async () => {
    openDialogMock.mockResolvedValue("/tmp/not-a-workspace");
    invokeMock.mockImplementation((() =>
      Promise.reject("no collection.yaml")) as unknown as typeof invoke);

    render(<Welcome />);
    fireEvent.click(screen.getByText("Open workspace"));
    await waitFor(() => {
      expect(
        useKeel
          .getState()
          .toasts.some((t) => t.message.includes("use Create workspace to initialize this folder")),
      ).toBe(true);
    });
  });

  it("create workspace flow: pick folder then create", async () => {
    openDialogMock.mockResolvedValue("/tmp/new-collection");
    invokeMock.mockImplementation(((cmd: string) => {
      if (cmd === "workspace_init") {
        return Promise.resolve({ root: "/tmp/new-collection", name: "My API", hasGit: false });
      }
      return Promise.resolve(null);
    }) as unknown as typeof invoke);

    render(<Welcome />);
    fireEvent.click(
      screen.getByText("Scaffold a new collection").closest("button")!,
    );
    expect(screen.getAllByText("Create workspace").length).toBe(2);
    const nameInput = screen.getByDisplayValue("My API") as HTMLInputElement;
    expect(nameInput).toBeTruthy();

    const createBtn = screen.getByRole("button", { name: "Create" }) as HTMLButtonElement;
    expect(createBtn.disabled).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Choose folder…" }));
    await screen.findByText("/tmp/new-collection");
    expect(createBtn.disabled).toBe(false);

    fireEvent.click(createBtn);
    await waitFor(() => {
      const call = invokeMock.mock.calls.find((c) => c[0] === "workspace_init");
      expect(call).toBeTruthy();
      expect(call![1]).toEqual({ path: "/tmp/new-collection", name: "My API" });
    });
    expect(useKeel.getState().workspace?.name).toBe("My API");
  });
});
