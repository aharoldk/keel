import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import SuggestionInput from "./SuggestionInput";

const OPTIONS = ["pkg.Users", "pkg.Orders", "other.Billing"];

function Host({ initial = "", options = OPTIONS }: { initial?: string; options?: string[] }) {
  const [v, setV] = useState(initial);
  return (
    <SuggestionInput value={v} onChange={setV} options={options} placeholder="package.Service" />
  );
}

describe("SuggestionInput", () => {
  it("shows every option on focus", () => {
    render(<Host />);
    fireEvent.focus(screen.getByPlaceholderText("package.Service"));
    expect(screen.getByText("pkg.Users")).toBeTruthy();
    expect(screen.getByText("pkg.Orders")).toBeTruthy();
    expect(screen.getByText("other.Billing")).toBeTruthy();
  });

  it("filters options as the user types", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("package.Service");
    fireEvent.change(input, { target: { value: "pkg." } });
    expect(screen.getByText("pkg.Users")).toBeTruthy();
    expect(screen.queryByText("other.Billing")).toBeNull();
  });

  it("filters case-insensitively", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("package.Service");
    fireEvent.change(input, { target: { value: "BILL" } });
    expect(screen.getByText("other.Billing")).toBeTruthy();
    expect(screen.queryByText("pkg.Users")).toBeNull();
  });

  it("picks the highlighted option on Enter", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("package.Service") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "pkg." } });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(input.value).toBe("pkg.Orders");
    expect(screen.queryByText("pkg.Users")).toBeNull();
  });

  it("picks the first match on Enter without arrows", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("package.Service") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "bill" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(input.value).toBe("other.Billing");
  });

  it("wraps around with arrow keys", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("package.Service");
    fireEvent.focus(input);
    fireEvent.keyDown(input, { key: "ArrowUp" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect((input as HTMLInputElement).value).toBe("other.Billing");
  });

  it("picks on mousedown", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("package.Service");
    fireEvent.focus(input);
    fireEvent.mouseDown(screen.getByText("pkg.Orders"));
    expect((input as HTMLInputElement).value).toBe("pkg.Orders");
  });

  it("closes the dropdown on Escape", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("package.Service");
    fireEvent.focus(input);
    expect(screen.getByText("pkg.Users")).toBeTruthy();
    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByText("pkg.Users")).toBeNull();
  });

  it("closes the dropdown on an outside mousedown", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("package.Service");
    fireEvent.focus(input);
    expect(screen.getByText("pkg.Users")).toBeTruthy();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByText("pkg.Users")).toBeNull();
  });

  it("renders no dropdown when there are no options", () => {
    render(<Host options={[]} />);
    const input = screen.getByPlaceholderText("package.Service");
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "x" } });
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("keeps the dropdown open after typing a non-matching value", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("package.Service");
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "zzz" } });
    expect(screen.queryByRole("listbox")).toBeNull();
    // Erase one character — the filtered list must come back.
    fireEvent.change(input, { target: { value: "zz" } });
    expect(screen.queryByRole("listbox")).toBeNull();
    fireEvent.change(input, { target: { value: "" } });
    expect(screen.queryByRole("listbox")).toBeTruthy();
  });

  it("does not hijack undo/redo keys", () => {
    const onChange = vi.fn();
    render(
      <SuggestionInput value="pkg.Users" onChange={onChange} options={OPTIONS} placeholder="x" />,
    );
    const input = screen.getByPlaceholderText("x");
    fireEvent.focus(input);
    fireEvent.keyDown(input, { key: "z", ctrlKey: true });
    expect(screen.getByText("pkg.Users")).toBeTruthy();
  });
});
