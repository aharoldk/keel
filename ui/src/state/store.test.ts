import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { defaultResponses, installDefaultResponses, invokeMock, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";
import {
  emptyRequestDoc,
  type RequestDoc,
  type RunnerItem,
  type RunnerSummary,
} from "@/api/types";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const listenMock = vi.mocked(listen);

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function fallbackResponses(cmd: string) {
  return Promise.resolve(structuredClone(defaultResponses[cmd] ?? null));
}

const workspace = { root: "/tmp/demo", name: "Demo", hasGit: false };

function makeTab(path: string, doc: RequestDoc, saved: RequestDoc) {
  return { path, doc, saved, loading: false, result: null, error: null };
}

describe("store", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
    useKeel.setState({
      workspace,
      runnerOpen: false,
      runnerRunId: null,
      runnerFolder: null,
      runnerItems: [],
      runnerSummary: null,
      runnerStarting: false,
    });
  });

  describe("save-on-send", () => {
    it("aborts the send when the save fails (Bug 1)", async () => {
      const original = emptyRequestDoc("A");
      const edited = { ...original, description: "dirty" };
      useKeel.setState({
        settings: { ...useKeel.getState().settings, saveOnSend: true },
        tabs: [makeTab("a.yaml", edited, original)],
        activePath: "a.yaml",
      });
      invokeMock.mockImplementation(((cmd: string) => {
        if (cmd === "request_save") return Promise.reject(new Error("read-only filesystem"));
        return fallbackResponses(cmd);
      }) as unknown as typeof invoke);

      await useKeel.getState().sendActive();

      expect(invokeMock.mock.calls.some((c) => c[0] === "send_request")).toBe(false);
      expect(useKeel.getState().toasts.some((t) => t.kind === "error")).toBe(true);
    });

    it("sends from disk once the save succeeds", async () => {
      const original = emptyRequestDoc("A");
      const edited = { ...original, description: "dirty" };
      useKeel.setState({
        settings: { ...useKeel.getState().settings, saveOnSend: true },
        tabs: [makeTab("a.yaml", edited, original)],
        activePath: "a.yaml",
      });
      invokeMock.mockImplementation(((cmd: string) => {
        if (cmd === "workspace_load_tree") {
          return Promise.resolve([{ path: "a.yaml", name: "A", kind: "request" }]);
        }
        return fallbackResponses(cmd);
      }) as unknown as typeof invoke);

      await useKeel.getState().sendActive();

      const call = invokeMock.mock.calls.find((c) => c[0] === "send_request");
      expect(call).toBeTruthy();
      expect((call![1] as { doc: unknown }).doc).toBeNull();
    });
  });

  describe("auto save", () => {
    it("saves dirty tabs after the configured delay", async () => {
      vi.useFakeTimers();
      const original = emptyRequestDoc("A");
      const edited = { ...original, description: "typed" };
      useKeel.setState({
        settings: { ...useKeel.getState().settings, autoSave: true, autoSaveInterval: 200 },
        tabs: [makeTab("a.yaml", original, original)],
        activePath: "a.yaml",
      });
      useKeel.getState().updateDoc("a.yaml", edited);
      expect(invokeMock.mock.calls.some((c) => c[0] === "request_save")).toBe(false);
      await vi.advanceTimersByTimeAsync(200);
      const call = invokeMock.mock.calls.find((c) => c[0] === "request_save");
      expect((call![1] as { doc: RequestDoc }).doc).toEqual(edited);
      vi.useRealTimers();
    });

    it("does not save when auto save is off", async () => {
      vi.useFakeTimers();
      const original = emptyRequestDoc("A");
      useKeel.setState({
        settings: { ...useKeel.getState().settings, autoSave: false },
        tabs: [makeTab("a.yaml", original, original)],
      });
      useKeel.getState().updateDoc("a.yaml", { ...original, description: "typed" });
      await vi.advanceTimersByTimeAsync(5000);
      expect(invokeMock.mock.calls.some((c) => c[0] === "request_save")).toBe(false);
      vi.useRealTimers();
    });
  });

  describe("saveTab", () => {
    it("keeps the tab dirty when edits happen during an in-flight save (Bug 2)", async () => {
      const original = emptyRequestDoc("A");
      const edited = { ...original, description: "edited while saving" };
      useKeel.setState({
        tabs: [makeTab("a.yaml", original, original)],
        activePath: "a.yaml",
      });
      const save = deferred<void>();
      invokeMock.mockImplementation(((cmd: string) => {
        if (cmd === "request_save") return save.promise;
        if (cmd === "workspace_load_tree") {
          return Promise.resolve([{ path: "a.yaml", name: "A", kind: "request" }]);
        }
        return fallbackResponses(cmd);
      }) as unknown as typeof invoke);

      const p = useKeel.getState().saveTab("a.yaml");
      // user types while the save is in flight
      useKeel.getState().updateDoc("a.yaml", edited);
      save.resolve(undefined);
      await p;

      const call = invokeMock.mock.calls.find((c) => c[0] === "request_save");
      expect((call![1] as { doc: RequestDoc }).doc).toEqual(original);
      const tab = useKeel.getState().tabs.find((t) => t.path === "a.yaml")!;
      expect(tab.saved).toEqual(original);
      expect(tab.doc).toEqual(edited);
      expect(JSON.stringify(tab.doc)).not.toBe(JSON.stringify(tab.saved));
    });
  });

  describe("openRequest", () => {
    it("creates a single tab for two concurrent opens of the same path (Bug 3)", async () => {
      const read = deferred<RequestDoc>();
      invokeMock.mockImplementation(((cmd: string) => {
        if (cmd === "request_read") return read.promise;
        return fallbackResponses(cmd);
      }) as unknown as typeof invoke);

      const p1 = useKeel.getState().openRequest("a.yaml");
      const p2 = useKeel.getState().openRequest("a.yaml");
      read.resolve(emptyRequestDoc("A"));
      await Promise.all([p1, p2]);

      const matches = useKeel.getState().tabs.filter((t) => t.path === "a.yaml");
      expect(matches).toHaveLength(1);
      expect(useKeel.getState().activePath).toBe("a.yaml");
    });
  });

  describe("runner events", () => {
    it("buffers events that arrive before the run id is known (Bug 4)", async () => {
      const handlers = new Map<string, (e: { payload: unknown }) => void>();
      listenMock.mockImplementation((async (
        name: string,
        cb: (e: { payload: unknown }) => void,
      ) => {
        handlers.set(name, cb);
        return () => {};
      }) as unknown as typeof listen);
      await useKeel.getState().init();

      const run = deferred<string>();
      invokeMock.mockImplementation(((cmd: string) => {
        if (cmd === "run_folder") return run.promise;
        return fallbackResponses(cmd);
      }) as unknown as typeof invoke);

      const item: RunnerItem = {
        path: "a.yaml",
        name: "A",
        method: "GET",
        status: "passed",
        timeMs: 1,
        sizeBytes: 10,
        testsTotal: 0,
        testsPassed: 0,
      };
      const summary: RunnerSummary = {
        total: 1,
        passed: 1,
        failed: 0,
        errored: 0,
        skipped: 0,
        durationMs: 1,
      };

      const p = useKeel.getState().startRun("folder");
      // the backend spawns the run before run_folder resolves, so events can
      // arrive while runnerRunId is still null
      handlers.get("runner://update")!({ payload: { runId: "run-1", kind: "item", item } });
      handlers.get("runner://update")!({ payload: { runId: "run-1", kind: "done", summary } });
      run.resolve("run-1");
      await p;

      const s = useKeel.getState();
      expect(s.runnerRunId).toBe("run-1");
      expect(s.runnerItems).toHaveLength(1);
      expect(s.runnerItems[0].path).toBe("a.yaml");
      expect(s.runnerSummary).toEqual(summary);
    });

    it("drops events for unknown runs when no run is starting", async () => {
      const handlers = new Map<string, (e: { payload: unknown }) => void>();
      listenMock.mockImplementation((async (
        name: string,
        cb: (e: { payload: unknown }) => void,
      ) => {
        handlers.set(name, cb);
        return () => {};
      }) as unknown as typeof listen);
      await useKeel.getState().init();

      handlers.get("runner://update")!({
        payload: { runId: "stale", kind: "done", summary: null },
      });

      expect(useKeel.getState().runnerItems).toHaveLength(0);
      expect(useKeel.getState().runnerSummary).toBeNull();
    });
  });

  describe("setRunnerOpen", () => {
    it("cancels the live run when the modal is closed mid-run (Bug 5)", () => {
      useKeel.setState({
        runnerOpen: true,
        runnerRunId: "run-1",
        runnerFolder: "folder",
        runnerSummary: null,
      });

      useKeel.getState().setRunnerOpen(false);

      const call = invokeMock.mock.calls.find((c) => c[0] === "run_cancel");
      expect(call).toBeTruthy();
      expect(call![1]).toEqual({ runId: "run-1" });
      const s = useKeel.getState();
      expect(s.runnerOpen).toBe(false);
      expect(s.runnerRunId).toBeNull();
      expect(s.runnerFolder).toBeNull();
    });

    it("does not cancel when the run already finished", () => {
      useKeel.setState({
        runnerOpen: true,
        runnerRunId: "run-1",
        runnerFolder: "folder",
        runnerSummary: { total: 1, passed: 1, failed: 0, errored: 0, skipped: 0, durationMs: 1 },
      });

      useKeel.getState().setRunnerOpen(false);

      expect(invokeMock.mock.calls.some((c) => c[0] === "run_cancel")).toBe(false);
      expect(useKeel.getState().runnerRunId).toBeNull();
    });
  });

  describe("theme", () => {
    beforeEach(() => {
      delete document.documentElement.dataset.theme;
    });

    it("applies the dark theme from backend settings on init", async () => {
      await useKeel.getState().init();
      expect(document.documentElement.dataset.theme).toBe("dark");
    });

    it("falls back to dark when the backend omits the theme", async () => {
      invokeMock.mockImplementation(((cmd: string) => {
        if (cmd === "settings_get") {
          return Promise.resolve({
            requestTimeoutSec: 30,
            followRedirects: true,
            saveOnSend: false,
            editorFontSize: 13,
            lastWorkspace: null,
          });
        }
        return fallbackResponses(cmd);
      }) as unknown as typeof invoke);

      await useKeel.getState().init();
      expect(document.documentElement.dataset.theme).toBe("dark");
    });

    it("updates data-theme when the theme setting changes", async () => {
      await useKeel.getState().saveSettings({ theme: "light" });
      expect(document.documentElement.dataset.theme).toBe("light");

      await useKeel.getState().saveSettings({ theme: "dark" });
      expect(document.documentElement.dataset.theme).toBe("dark");
    });
  });

  describe("refreshTree", () => {
    it("keeps dirty tabs but drops clean tabs for vanished files (Bug 6)", async () => {
      const original = emptyRequestDoc("A");
      const edited = { ...original, description: "dirty" };
      useKeel.setState({
        tabs: [
          makeTab("dirty.yaml", edited, original),
          makeTab("clean.yaml", original, original),
        ],
        activePath: "clean.yaml",
      });
      // default workspace_load_tree resolves to [] — both files "vanished"

      await useKeel.getState().refreshTree();

      expect(useKeel.getState().tabs.map((t) => t.path)).toEqual(["dirty.yaml"]);
      expect(useKeel.getState().activePath).toBe("dirty.yaml");
    });
  });
});
