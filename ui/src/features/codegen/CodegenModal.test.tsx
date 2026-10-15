import { beforeEach, describe, expect, it } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen } from "@testing-library/react";
import { CodegenModal } from "./CodegenModal";
import { installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";

describe("CodegenModal", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("generates a curl with resolved variables by default", async () => {
    invokeMock.mockImplementation(((cmd: string) =>
      Promise.resolve(cmd === "generate_code" ? "curl -X GET 'https://api.example.com/users'" : null)) as unknown as typeof invoke);

    useKeel.setState({ codegenFor: "users.yaml", activeEnv: "local" });
    render(<CodegenModal />);

    await screen.findByText("curl -X GET 'https://api.example.com/users'");
    const call = invokeMock.mock.calls.find((c) => c[0] === "generate_code");
    expect((call![1] as { resolve: boolean }).resolve).toBe(true);
  });

  it("keeps variables unresolved when the toggle is unchecked", async () => {
    invokeMock.mockImplementation(((cmd: string) =>
      Promise.resolve(cmd === "generate_code" ? "curl -X GET '{{baseUrl}}/users'" : null)) as unknown as typeof invoke);

    useKeel.setState({ codegenFor: "users.yaml", activeEnv: "local" });
    render(<CodegenModal />);

    fireEvent.click(screen.getByLabelText(/Resolve variables/));
    await screen.findByText("curl -X GET '{{baseUrl}}/users'");

    const calls = invokeMock.mock.calls.filter((c) => c[0] === "generate_code");
    const args = (calls[calls.length - 1][1] as { resolve: boolean; envName: string });
    expect(args.resolve).toBe(false);
    expect(args.envName).toBe("local");
  });

  it("warns that resolved values include secrets", async () => {
    useKeel.setState({ codegenFor: "users.yaml", activeEnv: "local" });
    render(<CodegenModal />);

    // Resolution is on by default, so the secret warning is visible upfront.
    expect(screen.getByText(/Do not commit or share/)).toBeTruthy();
    fireEvent.click(screen.getByLabelText(/Resolve variables/));
    expect(screen.queryByText(/Do not commit or share/)).toBeNull();
  });
});
