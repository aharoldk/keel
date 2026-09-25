import { describe, expect, it } from "vitest";
import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { TextInput } from "./ui";

describe("TextInput undo/redo", () => {
  function Host() {
    const [v, setV] = useState("");
    return <TextInput placeholder="name" value={v} onChange={(e) => setV(e.target.value)} />;
  }

  it("Ctrl+Z restores the previous value through the consumer's onChange", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("name") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "hello" } });
    expect(input.value).toBe("hello");
    fireEvent.keyDown(input, { key: "z", ctrlKey: true });
    expect(input.value).toBe("");
  });

  it("Ctrl+Shift+Z redoes the undone value", () => {
    render(<Host />);
    const input = screen.getByPlaceholderText("name") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "hello" } });
    fireEvent.keyDown(input, { key: "z", ctrlKey: true });
    expect(input.value).toBe("");
    fireEvent.keyDown(input, { key: "z", ctrlKey: true, shiftKey: true });
    expect(input.value).toBe("hello");
  });
});
