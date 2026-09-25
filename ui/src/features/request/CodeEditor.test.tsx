import { beforeEach, describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import CodeEditor from "./CodeEditor";
import { installDefaultResponses, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";

function injectedCss(): string {
  return Array.from(document.querySelectorAll("style"))
    .map((s) => s.textContent ?? "")
    .join("\n");
}

describe("CodeEditor theme", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("uses the dark color scheme and theme-variable syntax colors in dark mode", () => {
    const { container } = render(<CodeEditor value='{"a": "b"}' language="json" readOnly />);
    const css = injectedCss();
    expect(css).toMatch(/color-scheme:\s*dark/);

    // string tokens must use the class that references the theme CSS
    // variable — not basicSetup's light-background defaultHighlightStyle
    const m = /\.([^\s{]+)\s*\{\s*color:\s*var\(--method-post\)/.exec(css);
    expect(m, "a highlight rule using var(--method-post)").toBeTruthy();
    const stringToken = container.querySelector(`.${m![1]}`);
    expect(stringToken?.textContent).toBe('"b"');
  });

  it("switches to the light color scheme when the theme is light", () => {
    useKeel.setState({ settings: { ...useKeel.getState().settings, theme: "light" } });
    render(<CodeEditor value="plain" readOnly />);
    expect(injectedCss()).toMatch(/color-scheme:\s*light/);
  });
});
