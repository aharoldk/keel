import { beforeEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import VariableInput from "./VariableInput";
import type { VariableSuggestion } from "@/features/request/variables";

const VARS: VariableSuggestion[] = [
  { name: "baseUrl", source: "env", value: "https://xxxx" },
  { name: "apiKey", source: "secret" },
  { name: "timeout", source: "collection", value: "30" },
];

describe("VariableInput", () => {
  const onChange = vi.fn();

  beforeEach(() => {
    onChange.mockClear();
  });

  function setup(initial = "") {
    return render(
      <VariableInput value={initial} onChange={onChange} variables={VARS} placeholder="url" />,
    );
  }

  it("shows suggestions after typing {{", () => {
    setup("");
    const input = screen.getByPlaceholderText("url");
    fireEvent.change(input, { target: { value: "{{", selectionStart: 2 } });
    expect(screen.getByText("baseUrl")).toBeTruthy();
    expect(screen.getByText("apiKey")).toBeTruthy();
  });

  it("shows the current value next to a suggestion", () => {
    setup("");
    const input = screen.getByPlaceholderText("url");
    fireEvent.change(input, { target: { value: "{{", selectionStart: 2 } });
    expect(screen.getByText("https://xxxx")).toBeTruthy();
    expect(screen.getByText("30")).toBeTruthy();
  });

  it("hides the value for secrets", () => {
    setup("");
    const input = screen.getByPlaceholderText("url");
    fireEvent.change(input, { target: { value: "{{api", selectionStart: 5 } });
    expect(screen.getByText("apiKey")).toBeTruthy();
    expect(screen.queryByText("https://xxxx")).toBeNull();
  });

  it("filters suggestions as the user types inside the braces", () => {
    setup("");
    const input = screen.getByPlaceholderText("url");
    fireEvent.change(input, { target: { value: "{{ba", selectionStart: 4 } });
    expect(screen.getByText("baseUrl")).toBeTruthy();
    expect(screen.queryByText("apiKey")).toBeNull();
  });

  it("inserts the variable and closing braces on Enter", () => {
    function Host() {
      const [v, setV] = useState("");
      return <VariableInput value={v} onChange={setV} variables={VARS} placeholder="url" />;
    }
    render(<Host />);
    const input = screen.getByPlaceholderText("url") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "{{ba", selectionStart: 4 } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(input.value).toBe("{{baseUrl}}");
  });

  it("replaces the whole token when a variable name is selected", () => {
    function Host() {
      const [v, setV] = useState("https://{{FLOW}}/users");
      return <VariableInput value={v} onChange={setV} variables={VARS} placeholder="url" />;
    }
    render(<Host />);
    const input = screen.getByPlaceholderText("url") as HTMLInputElement;
    // Double-click selects the name between the braces.
    fireEvent.select(input, { target: { selectionStart: 10, selectionEnd: 14 } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(input.value).toBe("https://{{baseUrl}}/users");
  });

  it("does not open the dropdown when there is no unclosed {{", () => {
    setup("");
    const input = screen.getByPlaceholderText("url");
    fireEvent.change(input, {
      target: { value: "https://{{baseUrl}}/users", selectionStart: 25 },
    });
    expect(screen.queryByText("baseUrl")).toBeNull();
  });

  it("closes the dropdown on Escape", () => {
    setup("");
    const input = screen.getByPlaceholderText("url");
    fireEvent.change(input, { target: { value: "{{", selectionStart: 2 } });
    expect(screen.getByText("baseUrl")).toBeTruthy();
    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByText("baseUrl")).toBeNull();
  });

  describe("highlight layer", () => {
    function spanFor(container: HTMLElement, text: string) {
      return [...container.querySelectorAll("[aria-hidden] span")].find(
        (el) => el.textContent === text,
      ) as HTMLElement | undefined;
    }

    it("colors each variable by its source", () => {
      const { container } = setup("{{baseUrl}}/{{apiKey}}/{{timeout}}");
      expect(spanFor(container, "{{baseUrl}}")?.style.color).toBe("var(--ok)");
      expect(spanFor(container, "{{apiKey}}")?.style.color).toBe("var(--warn)");
      expect(spanFor(container, "{{timeout}}")?.style.color).toBe("var(--accent)");
    });

    it("colors unknown variables with the danger color", () => {
      const { container } = setup("{{baseUrl}}/{{missing}}");
      const missing = spanFor(container, "{{missing}}");
      expect(missing?.getAttribute("data-var-source")).toBe("unknown");
      expect(missing?.style.color).toBe("var(--danger)");
    });

    it("leaves plain text and unclosed braces uncolored", () => {
      const { container } = setup("https://{{baseUrl}}/users {{");
      expect(spanFor(container, "https://")?.style.color).toBe("");
      expect(spanFor(container, "/users {{")?.style.color).toBe("");
    });
  });

  describe("undo/redo", () => {
    function Host({ initial = "" }: { initial?: string }) {
      const [v, setV] = useState(initial);
      return <VariableInput value={v} onChange={setV} variables={VARS} placeholder="url" />;
    }

    it("merges rapid typing into one undo step", () => {
      render(<Host />);
      const input = screen.getByPlaceholderText("url") as HTMLInputElement;
      fireEvent.change(input, { target: { value: "a" } });
      fireEvent.change(input, { target: { value: "ab" } });
      fireEvent.keyDown(input, { key: "z", ctrlKey: true });
      expect(input.value).toBe("");
    });

    it("undoes separate edit bursts one at a time, then redoes", () => {
      const now = vi.spyOn(Date, "now");
      try {
        now.mockReturnValue(1000);
        render(<Host />);
        const input = screen.getByPlaceholderText("url") as HTMLInputElement;
        fireEvent.change(input, { target: { value: "a" } });
        now.mockReturnValue(2000);
        fireEvent.change(input, { target: { value: "ab" } });
        fireEvent.keyDown(input, { key: "z", ctrlKey: true });
        expect(input.value).toBe("a");
        fireEvent.keyDown(input, { key: "z", ctrlKey: true });
        expect(input.value).toBe("");
        fireEvent.keyDown(input, { key: "z", ctrlKey: true, shiftKey: true });
        expect(input.value).toBe("a");
        fireEvent.keyDown(input, { key: "y", ctrlKey: true });
        expect(input.value).toBe("ab");
      } finally {
        now.mockRestore();
      }
    });

    it("makes an autocomplete pick undoable", () => {
      render(<Host />);
      const input = screen.getByPlaceholderText("url") as HTMLInputElement;
      fireEvent.change(input, { target: { value: "{{ba", selectionStart: 4 } });
      fireEvent.keyDown(input, { key: "Enter" });
      expect(input.value).toBe("{{baseUrl}}");
      fireEvent.keyDown(input, { key: "z", ctrlKey: true });
      expect(input.value).toBe("");
    });

    it("does not undo into a value set from outside", () => {
      const onChange = vi.fn();
      const { rerender } = render(
        <VariableInput value="one" onChange={onChange} variables={VARS} placeholder="url" />,
      );
      rerender(
        <VariableInput value="two" onChange={onChange} variables={VARS} placeholder="url" />,
      );
      fireEvent.keyDown(screen.getByPlaceholderText("url"), { key: "z", ctrlKey: true });
      expect(onChange).not.toHaveBeenCalled();
    });
  });
});
