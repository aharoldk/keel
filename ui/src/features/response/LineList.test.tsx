import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import LineList from "./LineList";

const lines = Array.from({ length: 1000 }, (_, i) => `line ${i + 1}`);

const scroller = () => screen.getByText("line 1").closest(".overflow-auto") as HTMLElement;

describe("LineList", () => {
  it("mounts only a window of rows around the viewport", () => {
    render(<LineList lines={lines} renderLine={(l) => l} />);

    expect(screen.getByText("line 1")).toBeTruthy();
    // 20 rows of overscan are mounted past the (zero-height jsdom) viewport…
    expect(screen.getByText("line 20")).toBeTruthy();
    // …but the rest of the 1000-line body is not in the DOM at all.
    expect(screen.queryByText("line 500")).toBeNull();
    expect(screen.queryByText("line 1000")).toBeNull();
  });

  it("renders every row when the body is short", () => {
    render(<LineList lines={["a", "b", "c"]} renderLine={(l) => l} />);

    expect(screen.getByText("a")).toBeTruthy();
    expect(screen.getByText("b")).toBeTruthy();
    expect(screen.getByText("c")).toBeTruthy();
  });

  it("moves the window when the container scrolls", () => {
    render(<LineList lines={lines} renderLine={(l) => l} />);
    const el = scroller();

    Object.defineProperty(el, "scrollTop", { configurable: true, value: 24 * 400 });
    fireEvent.scroll(el);

    // The window now covers rows ~381–420 (24px rows, 20 rows of overscan).
    expect(screen.getByText("line 401")).toBeTruthy();
    expect(screen.queryByText("line 1")).toBeNull();
    expect(screen.queryByText("line 1000")).toBeNull();
  });

  it("shows a line-number gutter when asked", () => {
    render(<LineList lines={lines} gutter renderLine={(l) => l} />);

    expect(screen.getByText("1")).toBeTruthy();
    expect(screen.getByText("2")).toBeTruthy();
  });

  it("re-syncs the window when the content changes", () => {
    const { rerender } = render(<LineList lines={lines} renderLine={(l) => l} />);
    const el = scroller();
    Object.defineProperty(el, "scrollTop", { configurable: true, value: 24 * 400 });
    fireEvent.scroll(el);
    expect(screen.getByText("line 401")).toBeTruthy();

    // A new body replaces the lines; the window must not be left stale.
    rerender(<LineList lines={["only line"]} renderLine={(l) => l} />);
    expect(screen.getByText("only line")).toBeTruthy();
    expect(screen.queryByText("line 401")).toBeNull();
  });

  it("wraps lines when wrap is enabled", () => {
    render(<LineList lines={["long line content"]} wrap renderLine={(l) => l} />);
    const content = screen.getByText("long line content");
    expect(content.className).toContain("whitespace-pre-wrap");
    expect(content.className).toContain("break-all");
    expect(content.closest(".overflow-auto")?.className).toContain("overflow-x-hidden");
  });

  it("does not wrap lines when wrap is disabled", () => {
    render(<LineList lines={["long line content"]} wrap={false} renderLine={(l) => l} />);
    const content = screen.getByText("long line content");
    expect(content.className).toContain("whitespace-pre");
    expect(content.className).not.toContain("whitespace-pre-wrap");
    expect(content.closest(".overflow-auto")?.className).not.toContain("overflow-x-hidden");
  });

  it("moves the window when scrolled with wrap enabled", () => {
    render(<LineList lines={lines} wrap renderLine={(l) => l} />);
    const el = scroller();

    Object.defineProperty(el, "scrollTop", { configurable: true, value: 24 * 400 });
    fireEvent.scroll(el);

    expect(screen.getByText("line 401")).toBeTruthy();
    expect(screen.queryByText("line 1")).toBeNull();
    expect(screen.queryByText("line 1000")).toBeNull();
  });

  it("toggles wrap mode without losing the active window", () => {
    const { rerender } = render(<LineList lines={lines} wrap={false} renderLine={(l) => l} />);
    expect(screen.getByText("line 1").className).toContain("whitespace-pre");

    rerender(<LineList lines={lines} wrap renderLine={(l) => l} />);
    expect(screen.getByText("line 1").className).toContain("whitespace-pre-wrap");
  });
});
