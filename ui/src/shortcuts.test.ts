import { describe, expect, it } from "vitest";
import { comboFor, eventToCombo, formatCombo, matchesCombo, shortcutConflict } from "./shortcuts";

const ev = (key: string, mods: { ctrl?: boolean; meta?: boolean; alt?: boolean; shift?: boolean } = {}) => ({
  key,
  ctrlKey: !!mods.ctrl,
  metaKey: !!mods.meta,
  altKey: !!mods.alt,
  shiftKey: !!mods.shift,
});

describe("shortcuts", () => {
  it("falls back to the default combo", () => {
    expect(comboFor("save")).toBe("mod+s");
    expect(comboFor("save", { save: "mod+shift+s" })).toBe("mod+shift+s");
  });

  it("formats combos for display", () => {
    expect(formatCombo("mod+s", false)).toBe("Ctrl+S");
    expect(formatCombo("mod+shift+enter", false)).toBe("Ctrl+Shift+Enter");
    expect(formatCombo("mod+k", true)).toBe("⌘K");
    expect(formatCombo("mod+alt+arrowright", false)).toBe("Ctrl+Alt+→");
  });

  it("records a chord and ignores bare modifiers", () => {
    expect(eventToCombo(ev("s", { ctrl: true }))).toBe("mod+s");
    expect(eventToCombo(ev("S", { meta: true, shift: true }))).toBe("mod+shift+s");
    expect(eventToCombo(ev("Control", { ctrl: true }))).toBeNull();
    expect(eventToCombo(ev("s"))).toBeNull();
  });

  it("matches the configured combo", () => {
    expect(matchesCombo(ev("s", { ctrl: true }), "mod+s")).toBe(true);
    expect(matchesCombo(ev("s", { ctrl: true, shift: true }), "mod+s")).toBe(false);
  });

  it("detects a combo already used by another action", () => {
    expect(shortcutConflict("mod+k", "save", { palette: "mod+k" })).toBe("palette");
    expect(shortcutConflict("mod+shift+x", "save")).toBeNull();
  });
});
