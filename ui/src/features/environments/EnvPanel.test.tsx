import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { EnvPanel, EnvironmentEditor } from "./EnvPanel";
import { installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";

const envSummary = {
  fileName: "local.yaml",
  name: "local",
  description: "Dev",
  variableCount: 3,
  secretCount: 2,
};

describe("EnvPanel", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    useKeel.setState({ envs: [envSummary], activeEnv: null });
  });

  it("renders rows with counts and opens the editor on click without changing the active env", () => {
    useKeel.setState({ activeEnv: "other.yaml" });
    render(<EnvPanel />);
    expect(screen.getByText("local")).toBeTruthy();
    expect(screen.getByText("3 vars · 2 secrets")).toBeTruthy();
    fireEvent.click(screen.getByText("local"));
    expect(useKeel.getState().activeEnv).toBe("other.yaml");
    expect(useKeel.getState().activeEditor).toBe("env:local.yaml");
    expect(useKeel.getState().editorTabs).toEqual([
      { kind: "environment", fileName: "local.yaml" },
    ]);
  });

  it("opening an env does not highlight the sidebar row", () => {
    useKeel.setState({ activeEnv: "local.yaml" });
    const { container } = render(<EnvPanel />);
    expect(container.querySelector(".bg-accent-soft")).toBeNull();
    fireEvent.click(screen.getByText("local"));
    expect(container.querySelector(".bg-accent-soft")).toBeNull();
  });

  it("opens the editor in the content area, loads doc + keychain state, and saves", async () => {
    invokeMock.mockImplementation(((cmd: string) => {
      if (cmd === "env_read") {
        return Promise.resolve({
          schemaVersion: "1",
          name: "local",
          description: "Dev box",
          variables: { baseUrl: "http://localhost" },
          secrets: { apiToken: "fallback-token" },
        });
      }
      if (cmd === "secret_list") return Promise.resolve(["apiToken"]);
      if (cmd === "env_values_read") return Promise.resolve({});
      return Promise.resolve(cmd === "env_list" ? [envSummary] : null);
    }) as unknown as typeof invoke);

    render(<EnvPanel />);
    fireEvent.click(screen.getByTitle("Edit"));
    expect(useKeel.getState().activeEditor).toBe("env:local.yaml");
    render(<EnvironmentEditor fileName="local.yaml" />);

    await screen.findByDisplayValue("Dev box");
    expect(screen.getByDisplayValue("baseUrl")).toBeTruthy();
    expect(screen.getByText("apiToken")).toBeTruthy();
    expect(screen.getByTitle("Current value stored in keychain (wins over default)")).toBeTruthy();

    // Setting a variable's current value stores a local override.
    const currentInput = screen.getByPlaceholderText("current value");
    fireEvent.change(currentInput, { target: { value: "http://localhost:9999" } });
    fireEvent.blur(currentInput);
    await waitFor(() => {
      const set = invokeMock.mock.calls.find((c) => c[0] === "env_value_set");
      expect(set).toBeTruthy();
      expect(set![1]).toMatchObject({
        fileName: "local.yaml",
        name: "baseUrl",
        value: "http://localhost:9999",
      });
    });

    // A secret's default value is editable and committed to the env doc.
    const defaultInput = screen.getByPlaceholderText("default value (fallback)");
    fireEvent.change(defaultInput, { target: { value: "updated-default" } });

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const save = invokeMock.mock.calls.find((c) => c[0] === "env_save");
      expect(save).toBeTruthy();
      const doc = (
        save![1] as { doc: { variables: Record<string, string>; secrets: Record<string, string> } }
      ).doc;
      expect(doc.variables.baseUrl).toBe("http://localhost");
      expect(doc.secrets).toEqual({ apiToken: "updated-default" });
    });
    expect(useKeel.getState().toasts.some((t) => t.kind === "success")).toBe(true);
  });

  it("creates an environment via the header Plus button", async () => {
    render(<EnvPanel />);
    fireEvent.click(screen.getByTitle("New environment"));
    const nameInput = screen.getByPlaceholderText("Name");
    fireEvent.change(nameInput, { target: { value: "staging" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => {
      const save = invokeMock.mock.calls.find((c) => c[0] === "env_save");
      expect(save).toBeTruthy();
      expect(save![1]).toMatchObject({
        fileName: null,
        doc: { name: "staging", schemaVersion: "1" },
      });
    });
  });

  it("deletes via confirm modal", async () => {
    render(<EnvPanel />);
    fireEvent.click(screen.getByTitle("Delete"));
    fireEvent.click(screen.getByText("Delete", { selector: "button" }));
    await waitFor(() => {
      expect(invokeMock.mock.calls.some((c) => c[0] === "env_delete")).toBe(true);
    });
  });
});
