import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import PreviewPane, { isPreviewable } from "./PreviewPane";
import type { SendResult } from "@/api/types";

function result(patch: Partial<SendResult>): SendResult {
  return {
    requestId: "1",
    status: 200,
    statusText: "OK",
    ok: true,
    timeMs: 1,
    sizeBytes: 1,
    headers: [],
    cookies: [],
    contentType: null,
    bodyText: null,
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
    ...patch,
  };
}

describe("isPreviewable", () => {
  it("accepts html, images and pdf, ignoring parameters", () => {
    expect(isPreviewable("text/html; charset=utf-8")).toBe(true);
    expect(isPreviewable("image/png")).toBe(true);
    expect(isPreviewable("application/pdf")).toBe(true);
    expect(isPreviewable("application/json")).toBe(false);
    expect(isPreviewable(null)).toBe(false);
  });
});

describe("PreviewPane", () => {
  it("renders an image from the base64 body", () => {
    render(<PreviewPane result={result({ contentType: "image/png", bodyBase64: "aGk=" })} />);
    const img = screen.getByAltText("Response preview") as HTMLImageElement;
    expect(img.src).toBe("data:image/png;base64,aGk=");
  });

  it("renders html in a sandboxed iframe", () => {
    render(<PreviewPane result={result({ contentType: "text/html", bodyText: "<p>hi</p>" })} />);
    const frame = screen.getByTitle("preview") as HTMLIFrameElement;
    expect(frame.getAttribute("sandbox")).toBe("");
    expect(frame.getAttribute("srcdoc")).toBe("<p>hi</p>");
  });

  it("embeds a pdf", () => {
    const { container } = render(
      <PreviewPane result={result({ contentType: "application/pdf", bodyBase64: "JVBE" })} />,
    );
    const obj = container.querySelector("object") as HTMLObjectElement;
    expect(obj.type).toBe("application/pdf");
    expect(obj.data).toBe("data:application/pdf;base64,JVBE");
  });

  it("falls back when the type cannot be previewed", () => {
    render(<PreviewPane result={result({ contentType: "application/json" })} />);
    expect(screen.getByText(/No preview available/)).toBeTruthy();
  });
});
