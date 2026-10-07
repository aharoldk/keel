import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { AiPanel } from "./AiPanel";
import { installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";
import type { RequestDoc } from "@/api/types";

const doc: RequestDoc = {
  schemaVersion: "1",
  name: "Get users",
  kind: "request",
  request: { method: "GET", url: "/users" },
};

describe("AiPanel", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("asks to enable AI when the provider is off", () => {
    render(<AiPanel />);
    expect(screen.getByText(/AI is off/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open AI settings" }));
    expect(useKeel.getState().settingsOpen).toBe(true);
  });

  it("generates a script from the open request", async () => {
    invokeMock.mockImplementation(((cmd: string) => {
      if (cmd === "ai_generate") return Promise.resolve("log(status())\n");
      return Promise.resolve(null);
    }) as unknown as typeof invoke);

    useKeel.setState({
      settings: { ...useKeel.getState().settings, aiProvider: "openai", aiModel: "gpt-4o" },
      activePath: "users.yaml",
      tabs: [
        { path: "users.yaml", doc, saved: doc, loading: false, result: null, error: null },
      ],
    });
    render(<AiPanel />);
    fireEvent.change(screen.getByLabelText("AI prompt"), { target: { value: "log the status" } });
    fireEvent.click(screen.getByRole("button", { name: "Generate" }));
    await screen.findByText("log(status())");
    const call = invokeMock.mock.calls.find((c) => c[0] === "ai_generate");
    expect((call![1] as { args: { kind: string } }).args.kind).toBe("script");

    fireEvent.click(screen.getByRole("button", { name: "Apply to request" }));
    await waitFor(() => {
      expect(useKeel.getState().tabs[0].doc.scripts?.postResponse).toBe("log(status())");
    });
  });
});
