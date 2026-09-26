import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import PathParamsRows, { derivePathNames } from "./PathParamsRows";

describe("derivePathNames", () => {
  it("reads unique :names and ignores the query string", () => {
    expect(derivePathNames("/users/:id/posts/:id?q=:nope")).toEqual(["id"]);
    expect(derivePathNames("https://api.test/:org/repos/:name")).toEqual(["org", "name"]);
    expect(derivePathNames("/plain")).toEqual([]);
  });
});

describe("PathParamsRows", () => {
  it("renders nothing when the url has no path params", () => {
    const { container } = render(<PathParamsRows url="/plain" rows={[]} onChange={vi.fn()} />);
    expect(container.firstChild).toBeNull();
  });

  it("syncs a missing param into the rows", () => {
    const onChange = vi.fn();
    render(<PathParamsRows url="/users/:id" rows={[]} onChange={onChange} />);
    expect(onChange).toHaveBeenCalledWith([{ name: "id", value: "" }]);
  });

  it("edits the value for a named segment", () => {
    const onChange = vi.fn();
    render(
      <PathParamsRows
        url="/users/:id"
        rows={[{ name: "id", value: "" }]}
        onChange={onChange}
      />,
    );
    expect(screen.getByText(":id")).toBeTruthy();
    fireEvent.change(screen.getByPlaceholderText("value"), { target: { value: "42" } });
    expect(onChange).toHaveBeenCalledWith([{ name: "id", value: "42" }]);
  });
});
