import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { SplitPane } from "./SplitPane";

describe("SplitPane", () => {
  it("sizes the first pane from the initial height", () => {
    render(<SplitPane top={<div>top</div>} bottom={<div>bottom</div>} initial={200} />);
    expect(screen.getByText("top").parentElement?.style.height).toBe("200px");
  });

  it("resizes vertically while dragging", () => {
    const { container } = render(
      <SplitPane top={<div>top</div>} bottom={<div>bottom</div>} initial={200} min={100} max={400} />,
    );
    const handle = container.querySelector(".cursor-row-resize") as HTMLElement;
    fireEvent.mouseDown(handle);
    fireEvent.mouseMove(window, { clientY: 300 });
    expect(screen.getByText("top").parentElement?.style.height).toBe("300px");
    expect(document.body.style.cursor).toBe("row-resize");
    expect(document.body.style.userSelect).toBe("none");
    fireEvent.mouseUp(window);
    expect(document.body.style.cursor).toBe("");
    expect(document.body.style.userSelect).toBe("");
  });

  it("clamps the size to min and max", () => {
    const { container } = render(
      <SplitPane top={<div>top</div>} bottom={<div>bottom</div>} initial={200} min={100} max={400} />,
    );
    const handle = container.querySelector(".cursor-row-resize") as HTMLElement;
    fireEvent.mouseDown(handle);
    fireEvent.mouseMove(window, { clientY: 50 });
    expect(screen.getByText("top").parentElement?.style.height).toBe("100px");
    fireEvent.mouseMove(window, { clientY: 900 });
    expect(screen.getByText("top").parentElement?.style.height).toBe("400px");
  });

  it("resizes width when horizontal", () => {
    const { container } = render(
      <SplitPane
        direction="horizontal"
        top={<div>left</div>}
        bottom={<div>right</div>}
        initial={180}
      />,
    );
    const handle = container.querySelector(".cursor-col-resize") as HTMLElement;
    fireEvent.mouseDown(handle);
    fireEvent.mouseMove(window, { clientX: 260 });
    expect(screen.getByText("left").parentElement?.style.width).toBe("260px");
  });
});
