import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import EditorTabBar from "./EditorTabBar";

const TABS = [
  { id: "params", label: "Params" },
  { id: "headers", label: "Headers" },
  { id: "auth", label: "Auth" },
  { id: "body", label: "Body" },
];

/** jsdom does no layout, so install fixed metrics for measure() to read. Each
 * tab becomes `tabWidth` wide and the bar becomes `containerWidth` wide. */
function mockLayout(tabWidth: number, containerWidth: number) {
  const measureBtn = screen.getAllByText("Params")[0];
  const container = measureBtn.closest(".relative") as HTMLElement;
  Object.defineProperty(container, "clientWidth", {
    configurable: true,
    get: () => containerWidth,
  });
  Array.from((measureBtn.parentElement as HTMLElement).querySelectorAll("button")).forEach(
    (btn) =>
      Object.defineProperty(btn, "offsetWidth", {
        configurable: true,
        get: () => tabWidth,
      }),
  );
}

/** Re-render with a fresh element so measure() actually runs again (React
 * bails out when handed the identical element twice). */
function mount(active: string, onChange: (id: string) => void) {
  const tree = (
    <EditorTabBar tabs={TABS} active={active} onChange={onChange} />
  );
  const utils = render(tree);
  return {
    ...utils,
    rerender(nextActive: string = active) {
      utils.rerender(<EditorTabBar tabs={TABS} active={nextActive} onChange={onChange} />);
    },
  };
}

describe("EditorTabBar", () => {
  it("renders every tab when there is room", () => {
    const onChange = vi.fn();
    const utils = mount("params", onChange);
    mockLayout(60, 400);
    utils.rerender();

    expect(screen.queryByTitle("More tabs")).toBeNull();
    // Visible tabs render twice: once in the hidden measurement row, once on screen.
    expect(screen.getAllByText("Body")).toHaveLength(2);
    fireEvent.click(screen.getAllByText("Auth")[1]);
    expect(onChange).toHaveBeenCalledWith("auth");
  });

  it("moves tabs that do not fit into the overflow button", () => {
    const utils = mount("params", () => {});
    // 60 + 60 + 36 (overflow) > 150, so only the first tab fits.
    mockLayout(60, 150);
    utils.rerender();

    expect(screen.getByTitle("More tabs")).toBeTruthy();
    expect(screen.getAllByText("Params")).toHaveLength(2);
    expect(screen.getAllByText("Headers")).toHaveLength(1);
    expect(screen.getAllByText("Body")).toHaveLength(1);
  });

  it("keeps the active tab on screen when it would overflow", () => {
    const utils = mount("body", () => {});
    mockLayout(60, 150);
    utils.rerender();

    expect(screen.getAllByText("Body")).toHaveLength(2);
    expect(screen.getAllByText("Params")).toHaveLength(1);
  });

  it("lists hidden tabs in the overflow menu and selects one", () => {
    const onChange = vi.fn();
    const utils = mount("params", onChange);
    mockLayout(60, 150);
    utils.rerender();

    fireEvent.click(screen.getByTitle("More tabs"));
    expect(screen.getByTitle("More tabs").getAttribute("aria-expanded")).toBe("true");
    // Visible-row buttons carry a title; hidden ones only appear in the menu.
    fireEvent.click(screen.getByTitle("Body"));

    expect(onChange).toHaveBeenCalledWith("body");
    expect(screen.queryByTitle("Body")).toBeNull();
    expect(screen.getByTitle("More tabs").getAttribute("aria-expanded")).toBe("false");
  });

  it("closes the overflow menu on Escape", () => {
    const utils = mount("params", () => {});
    mockLayout(60, 150);
    utils.rerender();

    fireEvent.click(screen.getByTitle("More tabs"));
    expect(screen.getByTitle("Body")).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByTitle("Body")).toBeNull();
  });

  it("closes the overflow menu when clicking outside", () => {
    const utils = mount("params", () => {});
    mockLayout(60, 150);
    utils.rerender();

    fireEvent.click(screen.getByTitle("More tabs"));
    expect(screen.getByTitle("Body")).toBeTruthy();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByTitle("Body")).toBeNull();
  });

  it("renders badge counts", () => {
    render(
      <EditorTabBar
        tabs={[{ id: "params", label: "Params", badge: 3 }]}
        active="params"
        onChange={() => {}}
      />,
    );
    expect(screen.getAllByText("3")).toHaveLength(2);
  });
});
