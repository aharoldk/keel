import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import ResponseViewer from "./ResponseViewer";
import type { SendResult } from "@/api/types";
import { installDefaultResponses, resetStore } from "@/test/helpers";

const mockResult: SendResult = {
  requestId: "req-1",
  status: 200,
  statusText: "OK",
  ok: true,
  timeMs: 42,
  sizeBytes: 128,
  headers: [{ name: "content-type", value: "application/json" }],
  cookies: [],
  contentType: "application/json",
  bodyText: '{\n  "message": "hello world very long line that should wrap nicely"\n}',
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

describe("ResponseViewer", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    localStorage.clear();
  });

  it("renders empty state when no result is present", () => {
    render(<ResponseViewer />);
    expect(screen.getByText("No response yet")).toBeTruthy();
  });

  it("renders response status, meta, and soft wrap toggle", () => {
    render(<ResponseViewer result={mockResult} />);
    expect(screen.getByText("200")).toBeTruthy();
    expect(screen.getByText("OK")).toBeTruthy();

    const wrapBtn = screen.getByTitle("Disable soft wrap");
    expect(wrapBtn).toBeTruthy();
    expect(wrapBtn.getAttribute("aria-pressed")).toBe("true");

    const line = screen.getByText(/"message"/).closest(".flex")?.querySelector("span:last-child");
    expect(line?.className).toContain("whitespace-pre-wrap");
  });

  it("toggles soft wrap when clicking the toggle button", () => {
    render(<ResponseViewer result={mockResult} />);
    const wrapBtn = screen.getByTitle("Disable soft wrap");

    fireEvent.click(wrapBtn);

    expect(screen.getByTitle("Enable soft wrap")).toBeTruthy();
    expect(screen.getByLabelText("Enable soft wrap").getAttribute("aria-pressed")).toBe("false");
    expect(localStorage.getItem("keel.responseWrap")).toBe("false");

    const line = screen.getByText(/"message"/).closest(".flex")?.querySelector("span:last-child");
    expect(line?.className).toContain("whitespace-pre");
    expect(line?.className).not.toContain("whitespace-pre-wrap");

    // Click again to re-enable
    fireEvent.click(screen.getByTitle("Enable soft wrap"));
    expect(screen.getByTitle("Disable soft wrap")).toBeTruthy();
    expect(localStorage.getItem("keel.responseWrap")).toBe("true");
  });

  it("respects saved soft wrap preference from localStorage", () => {
    localStorage.setItem("keel.responseWrap", "false");
    render(<ResponseViewer result={mockResult} />);

    expect(screen.getByTitle("Enable soft wrap")).toBeTruthy();
    const line = screen.getByText(/"message"/).closest(".flex")?.querySelector("span:last-child");
    expect(line?.className).toContain("whitespace-pre");
    expect(line?.className).not.toContain("whitespace-pre-wrap");
  });
});
