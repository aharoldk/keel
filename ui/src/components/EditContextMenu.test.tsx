import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { TextInput } from "./ui";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(() => Promise.resolve()),
  readText: vi.fn(() => Promise.resolve("")),
}));

import { writeText } from "@tauri-apps/plugin-clipboard-manager";

function Host({ readOnly = false }: { readOnly?: boolean }) {
  const [v, setV] = useState("hello");
  return (
    <TextInput
      placeholder="name"
      readOnly={readOnly}
      value={v}
      onChange={(e) => setV(e.target.value)}
    />
  );
}

describe("edit context menu", () => {
  it("shows undo/redo/cut/copy/paste/select-all on right-click", () => {
    render(<Host />);
    fireEvent.contextMenu(screen.getByPlaceholderText("name"));
    for (const label of ["Undo", "Redo", "Cut", "Copy", "Paste", "Select All"]) {
      expect(screen.getByText(label)).toBeTruthy();
    }
  });

  it("shows only copy/select-all for read-only inputs", () => {
    render(<Host readOnly />);
    fireEvent.contextMenu(screen.getByPlaceholderText("name"));
    expect(screen.getByText("Copy")).toBeTruthy();
    expect(screen.getByText("Select All")).toBeTruthy();
    expect(screen.queryByText("Undo")).toBeNull();
    expect(screen.queryByText("Paste")).toBeNull();
  });

  it("copies the selection to the clipboard", async () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("name") as HTMLInputElement;
    input.setSelectionRange(0, 5);
    fireEvent.contextMenu(input);
    fireEvent.click(screen.getByText("Copy"));
    expect(writeText).toHaveBeenCalledWith("hello");
  });

  it("undoes an edit from the menu", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("name") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "hello world" } });
    fireEvent.contextMenu(input);
    fireEvent.click(screen.getByText("Undo"));
    expect(input.value).toBe("hello");
  });

  it("closes when clicking outside", () => {
    render(<Host />);
    fireEvent.contextMenu(screen.getByPlaceholderText("name"));
    expect(screen.getByText("Paste")).toBeTruthy();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByText("Paste")).toBeNull();
  });
});
