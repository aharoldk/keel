import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import KVEditor from "./KVEditor";
import type { KV } from "@/api/types";

describe("KVEditor", () => {
  it("adds a row", () => {
    const onChange = vi.fn();
    render(<KVEditor rows={[]} onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(onChange).toHaveBeenCalledWith([{ name: "", value: "", enabled: true }]);
  });

  it("edits the name and removes the row", () => {
    const onChange = vi.fn();
    const rows: KV[] = [{ name: "A", value: "1", enabled: true }];
    render(<KVEditor rows={rows} onChange={onChange} />);
    fireEvent.change(screen.getByPlaceholderText("Name"), { target: { value: "Accept" } });
    expect(onChange).toHaveBeenCalledWith([{ name: "Accept", value: "1", enabled: true }]);
    fireEvent.click(screen.getByTitle("Remove row"));
    expect(onChange).toHaveBeenCalledWith([]);
  });

  it("disables a row from its checkbox", () => {
    const onChange = vi.fn();
    render(<KVEditor rows={[{ name: "A", value: "1", enabled: true }]} onChange={onChange} />);
    fireEvent.click(screen.getByRole("checkbox"));
    expect(onChange).toHaveBeenCalledWith([{ name: "A", value: "1", enabled: false }]);
  });

  it("switches a text row to a file row", () => {
    const onChange = vi.fn();
    render(
      <KVEditor rows={[{ name: "file", value: "x", enabled: true }]} onChange={onChange} allowFile />,
    );
    fireEvent.click(screen.getByTitle(/Switch to file row/));
    expect(onChange).toHaveBeenCalledWith([
      { name: "file", value: "", enabled: true, kind: "file" },
    ]);
  });
});
