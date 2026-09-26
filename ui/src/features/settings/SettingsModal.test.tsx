import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SettingsModal } from "./SettingsModal";
import { installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";

describe("SettingsModal", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("renders nothing when closed", () => {
    const { container } = render(<SettingsModal />);
    expect(container.childElementCount).toBe(0);
  });

  it("opens with settings draft and saves to the backend", async () => {
    useKeel.setState({
      settingsOpen: true,
      workspace: { root: "/tmp/demo", name: "Demo API", hasGit: true },
      version: "1.0.0",
    });
    render(<SettingsModal />);

    expect(screen.getByText("Settings")).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Workspace" }));
    expect(screen.getByText("Demo API")).toBeTruthy();
    expect(screen.getByText("/tmp/demo")).toBeTruthy();
    expect(screen.getByText("MIT licensed")).toBeTruthy();

    fireEvent.click(screen.getByRole("tab", { name: "Network" }));
    const checkboxes = screen.getAllByRole("checkbox");
    expect((checkboxes[0] as HTMLButtonElement).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(checkboxes[0]);
    expect((checkboxes[0] as HTMLButtonElement).getAttribute("aria-checked")).toBe("false");

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const call = invokeMock.mock.calls.find((c) => c[0] === "settings_set");
      expect(call).toBeTruthy();
      const settings = (call![1] as { settings: { followRedirects: boolean } }).settings;
      expect(settings.followRedirects).toBe(false);
    });
    expect(useKeel.getState().settingsOpen).toBe(false);
  });

  it("theme select updates draft", () => {
    useKeel.setState({ settingsOpen: true });
    render(<SettingsModal />);
    const themeSelect = screen.getAllByRole("combobox")[0] as HTMLSelectElement;
    fireEvent.change(themeSelect, { target: { value: "light" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    return waitFor(() => {
      const call = invokeMock.mock.calls.find((c) => c[0] === "settings_set");
      expect((call![1] as { settings: { theme: string } }).settings.theme).toBe("light");
    });
  });

  it("records a shortcut and persists auto save", async () => {
    useKeel.setState({ settingsOpen: true });
    render(<SettingsModal />);

    fireEvent.click(screen.getByRole("checkbox", { name: "Enabled" }));
    fireEvent.click(screen.getByRole("tab", { name: "Shortcuts" }));
    const saveShortcut = screen.getByRole("textbox", { name: "Save shortcut" });
    fireEvent.keyDown(saveShortcut, { key: "s", ctrlKey: true, shiftKey: true });

    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const call = invokeMock.mock.calls.find((c) => c[0] === "settings_set");
      const settings = (call![1] as { settings: { autoSave: boolean; shortcuts: { save: string } } }).settings;
      expect(settings.autoSave).toBe(true);
      expect(settings.shortcuts.save).toBe("mod+shift+s");
    });
  });

  it("clamps editor font size on save", async () => {
    useKeel.setState({ settingsOpen: true });
    render(<SettingsModal />);
    fireEvent.click(screen.getByRole("tab", { name: "Editor" }));
    const fontInput = screen.getByLabelText("Editor font size");
    fireEvent.change(fontInput, { target: { value: "99" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const call = invokeMock.mock.calls.find((c) => c[0] === "settings_set");
      expect((call![1] as { settings: { editorFontSize: number } }).settings.editorFontSize).toBe(24);
    });
  });
});
