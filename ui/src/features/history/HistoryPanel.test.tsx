import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { HistoryPanel } from "./HistoryPanel";
import { installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";

const entry = {
  ts: "2024-01-01T10:30:00Z",
  method: "GET" as const,
  url: "http://localhost:8080/users?page=1",
  status: 200,
  ok: true,
  timeMs: 12,
  env: null,
  requestPath: null,
};

describe("HistoryPanel", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("shows empty state", async () => {
    render(<HistoryPanel />);
    await screen.findByText("No requests yet");
  });

  it("renders entries with method, status and time; missing file toasts on click", async () => {
    invokeMock.mockImplementation(((cmd: string) =>
      Promise.resolve(
        cmd === "history_pins" ? [] : structuredClone([entry]),
      )) as unknown as typeof invoke);
    render(<HistoryPanel />);
    await screen.findByText("http://localhost:8080/users?page=1");
    expect(screen.getByText("GET")).toBeTruthy();
    expect(screen.getByText("200")).toBeTruthy();

    fireEvent.click(screen.getByTitle("http://localhost:8080/users?page=1"));
    await waitFor(() => {
      expect(
        useKeel.getState().toasts.some((t) => t.message === "Request file no longer exists"),
      ).toBe(true);
    });
  });

  it("opens the request file when requestPath exists", async () => {
    invokeMock.mockImplementation(((cmd: string) => {
      if (cmd === "history_list") {
        return Promise.resolve([structuredClone({ ...entry, requestPath: "users.yaml" })]);
      }
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

    render(<HistoryPanel />);
    await screen.findByText("http://localhost:8080/users?page=1");
    fireEvent.click(screen.getByTitle("http://localhost:8080/users?page=1"));

    await waitFor(() => {
      expect(useKeel.getState().activePath).toBe("users.yaml");
      expect(useKeel.getState().tabs[0]?.doc.name).toBe("Get Users");
    });
  });

  it("clears history via confirm modal", async () => {
    invokeMock.mockImplementation(((cmd: string) =>
      Promise.resolve(
        cmd === "history_pins" ? [] : structuredClone([entry]),
      )) as unknown as typeof invoke);
    render(<HistoryPanel />);
    await screen.findByText("http://localhost:8080/users?page=1");

    fireEvent.click(screen.getByTitle("Clear history"));
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));

    await waitFor(() => {
      expect(invokeMock.mock.calls.some((c) => c[0] === "history_clear")).toBe(true);
    });
  });
});
