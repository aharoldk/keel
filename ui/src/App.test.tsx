import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { fireEvent, waitFor } from "@testing-library/react";
import { handleGlobalKeydown } from "./commands";
import { installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";
import { emptyRequestDoc } from "@/api/types";

describe("App global shortcuts", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    const doc = emptyRequestDoc("A");
    useKeel.setState({
      tabs: [{ path: "a.yaml", doc, saved: doc, loading: false, result: null, error: null }],
      activePath: "a.yaml",
    });
    window.addEventListener("keydown", handleGlobalKeydown);
  });

  afterEach(() => {
    window.removeEventListener("keydown", handleGlobalKeydown);
  });

  const invoked = (cmd: string) => invokeMock.mock.calls.some((c) => c[0] === cmd);

  it("Ctrl+Enter sends the active request when focus is not editable", async () => {
    fireEvent.keyDown(document.body, { key: "Enter", ctrlKey: true });
    await waitFor(() => expect(invoked("send_request")).toBe(true));
  });

  it("Ctrl+Enter inside a textarea does not send the request (Bug 7)", () => {
    const ta = document.createElement("textarea");
    document.body.appendChild(ta);
    fireEvent.keyDown(ta, { key: "Enter", ctrlKey: true });
    expect(invoked("send_request")).toBe(false);
    ta.remove();
  });

  it("Ctrl+S saves when focus is not editable", async () => {
    fireEvent.keyDown(document.body, { key: "s", ctrlKey: true });
    await waitFor(() => expect(invoked("request_save")).toBe(true));
  });

  it("uses a remapped save shortcut", async () => {
    useKeel.setState({
      settings: { ...useKeel.getState().settings, shortcuts: { save: "mod+shift+s" } },
    });
    fireEvent.keyDown(document.body, { key: "s", ctrlKey: true });
    expect(invoked("request_save")).toBe(false);
    fireEvent.keyDown(document.body, { key: "s", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(invoked("request_save")).toBe(true));
  });

  it("Ctrl+S inside an input does not save (Bug 7)", () => {
    const input = document.createElement("input");
    document.body.appendChild(input);
    fireEvent.keyDown(input, { key: "s", ctrlKey: true });
    expect(invoked("request_save")).toBe(false);
    input.remove();
  });

  it("Ctrl+Enter inside CodeMirror content does not send the request (Bug 7)", () => {
    const cm = document.createElement("div");
    cm.className = "cm-content";
    document.body.appendChild(cm);
    fireEvent.keyDown(cm, { key: "Enter", ctrlKey: true });
    expect(invoked("send_request")).toBe(false);
    cm.remove();
  });

  it("Ctrl+Shift+R dispatches a rename event for the active request", () => {
    const seen: string[] = [];
    const onRename = (e: Event) => seen.push((e as CustomEvent<string>).detail);
    window.addEventListener("keel:start-rename", onRename);
    fireEvent.keyDown(document.body, { key: "r", ctrlKey: true, shiftKey: true });
    window.removeEventListener("keel:start-rename", onRename);
    expect(seen).toEqual(["a.yaml"]);
  });

  it("Ctrl+Shift+R inside an input does not trigger rename", () => {
    const seen: string[] = [];
    const onRename = (e: Event) => seen.push((e as CustomEvent<string>).detail);
    window.addEventListener("keel:start-rename", onRename);
    const input = document.createElement("input");
    document.body.appendChild(input);
    fireEvent.keyDown(input, { key: "r", ctrlKey: true, shiftKey: true });
    window.removeEventListener("keel:start-rename", onRename);
    input.remove();
    expect(seen).toEqual([]);
  });
});
