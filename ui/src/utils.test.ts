import { describe, expect, it } from "vitest";
import { formatTime, fuzzyScore, isEditableTarget, methodVar, statusClass } from "./utils";

describe("fuzzyScore", () => {
  it("returns 0 for empty query", () => {
    expect(fuzzyScore("", "anything")).toBe(0);
  });

  it("returns -1 when no subsequence match", () => {
    expect(fuzzyScore("xyz", "get users")).toBe(-1);
  });

  it("scores a match above 0", () => {
    expect(fuzzyScore("get", "get users")).toBeGreaterThan(0);
  });

  it("ranks prefix matches higher than scattered ones", () => {
    expect(fuzzyScore("get", "get users")).toBeGreaterThan(fuzzyScore("get", "a big extra get"));
  });
});

describe("statusClass", () => {
  it("maps status ranges to theme colors", () => {
    expect(statusClass(null)).toBe("text-fg-2");
    expect(statusClass(200)).toBe("text-ok");
    expect(statusClass(302)).toBe("text-info");
    expect(statusClass(404)).toBe("text-warn");
    expect(statusClass(500)).toBe("text-danger");
  });
});

describe("methodVar", () => {
  it("returns css variable per method", () => {
    expect(methodVar("GET")).toBe("var(--method-get)");
  });
});

describe("formatTime", () => {
  it("formats a valid timestamp", () => {
    expect(formatTime("2024-01-01T10:30:00Z")).toMatch(/:.*\d/);
  });
  it("falls back to raw input for invalid ts", () => {
    expect(formatTime("not-a-date")).toBe("not-a-date");
  });
});

describe("isEditableTarget", () => {
  it("detects input, textarea and select elements", () => {
    for (const tag of ["input", "textarea", "select"]) {
      expect(isEditableTarget(document.createElement(tag))).toBe(true);
    }
  });

  it("detects contenteditable descendants", () => {
    const host = document.createElement("div");
    host.setAttribute("contenteditable", "true");
    const child = document.createElement("span");
    host.appendChild(child);
    document.body.appendChild(host);
    expect(isEditableTarget(child)).toBe(true);
    host.remove();
  });

  it("detects CodeMirror content", () => {
    const cm = document.createElement("div");
    cm.className = "cm-content";
    const line = document.createElement("div");
    cm.appendChild(line);
    document.body.appendChild(cm);
    expect(isEditableTarget(line)).toBe(true);
    cm.remove();
  });

  it("returns false for non-editable targets", () => {
    expect(isEditableTarget(document.body)).toBe(false);
    expect(isEditableTarget(document.createElement("button"))).toBe(false);
    expect(isEditableTarget(null)).toBe(false);
  });
});
