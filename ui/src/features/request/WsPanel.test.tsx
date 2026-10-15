import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import WsPanel, { clearWsSessionStore } from "./WsPanel";
import { api } from "@/api/client";
import { listen } from "@tauri-apps/api/event";
import { emptyRequestDoc, type RequestDoc, type WsEvent } from "@/api/types";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(() => Promise.resolve()),
  readText: vi.fn(() => Promise.resolve("")),
}));

vi.mock("@/api/client", () => ({
  api: {
    wsConnect: vi.fn(),
    wsSend: vi.fn(),
    wsClose: vi.fn(),
  },
}));

type WsListener = (event: { payload: WsEvent }) => void;

describe("WsPanel", () => {
  let listeners: WsListener[];
  let unlistenMock: () => void;
  const mockPath = "ws-request.yaml";

  beforeEach(() => {
    clearWsSessionStore();
    vi.clearAllMocks();
    listeners = [];
    unlistenMock = vi.fn();

    vi.mocked(listen).mockImplementation(((
      _eventName: string,
      cb: WsListener,
    ) => {
      listeners.push(cb);
      return Promise.resolve(unlistenMock);
    }) as unknown as typeof listen);

    vi.mocked(api.wsConnect).mockResolvedValue();
    vi.mocked(api.wsSend).mockResolvedValue();
    vi.mocked(api.wsClose).mockResolvedValue();
  });

  function emitWsEvent(payload: WsEvent) {
    act(() => {
      for (const listener of listeners) {
        listener({ payload });
      }
    });
  }

  function createTestDoc(overrides?: Partial<RequestDoc>): RequestDoc {
    const doc = emptyRequestDoc("Test WS");
    doc.request.url = "ws://127.0.0.1:8080/ws";
    doc.websocket = { protocols: "v1, v2" };
    return { ...doc, ...overrides };
  }

  it("renders closed state initially with disabled input", () => {
    const doc = createTestDoc();
    const setDoc = vi.fn();
    const toast = vi.fn();

    render(<WsPanel path={mockPath} doc={doc} setDoc={setDoc} toast={toast} />);

    expect(screen.getByText("closed")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Connect" })).toBeTruthy();
    expect((screen.getByPlaceholderText("protocol") as HTMLInputElement).value).toBe("v1, v2");

    const messageInput = screen.getByPlaceholderText("Message") as HTMLInputElement;
    expect(messageInput.disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("calls setDoc when protocols input changes", () => {
    const doc = createTestDoc();
    const setDoc = vi.fn();
    const toast = vi.fn();

    render(<WsPanel path={mockPath} doc={doc} setDoc={setDoc} toast={toast} />);

    const protocolInput = screen.getByPlaceholderText("protocol");
    fireEvent.change(protocolInput, { target: { value: "chat, binary" } });

    expect(setDoc).toHaveBeenCalledWith({
      ...doc,
      websocket: { protocols: "chat, binary" },
    });
  });

  it("calls api.wsConnect with trimmed url, headers, and split protocols on Connect click", async () => {
    const doc = createTestDoc({
      request: {
        method: "GET",
        url: "  ws://127.0.0.1:9000/chat  ",
        headers: [{ name: "Authorization", value: "Bearer token123", enabled: true }],
      },
      websocket: { protocols: "chat, graphql-ws,  " },
    });
    const setDoc = vi.fn();
    const toast = vi.fn();

    render(<WsPanel path={mockPath} doc={doc} setDoc={setDoc} toast={toast} />);

    fireEvent.click(screen.getByRole("button", { name: "Connect" }));

    await waitFor(() => {
      expect(api.wsConnect).toHaveBeenCalledWith(
        mockPath,
        "ws://127.0.0.1:9000/chat",
        [{ name: "Authorization", value: "Bearer token123", enabled: true }],
        ["chat", "graphql-ws"],
      );
    });
  });

  it("shows error toast if api.wsConnect fails", async () => {
    vi.mocked(api.wsConnect).mockRejectedValueOnce(new Error("Connection refused"));
    const doc = createTestDoc();
    const toast = vi.fn();

    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={toast} />);

    fireEvent.click(screen.getByRole("button", { name: "Connect" }));

    await waitFor(() => {
      expect(toast).toHaveBeenCalledWith("Error: Connection refused", "error");
    });
  });

  it("updates state and shows connected frame on open event", async () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({
      sessionId: mockPath,
      kind: "open",
      data: "chat",
    });

    await waitFor(() => {
      expect(screen.getByText("open")).toBeTruthy();
    });

    expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
    expect((screen.getByPlaceholderText("Message") as HTMLInputElement).disabled).toBe(false);
    expect((screen.getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByText("connected (chat)")).toBeTruthy();
  });

  it("ignores events from other sessionIds", () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({
      sessionId: "other-session.yaml",
      kind: "open",
    });

    expect(screen.getByText("closed")).toBeTruthy();
    expect(screen.queryByText("connected")).toBeNull();
  });

  it("sends message on Send button click and renders outgoing frame", async () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({ sessionId: mockPath, kind: "open" });

    const messageInput = screen.getByPlaceholderText("Message");
    fireEvent.change(messageInput, { target: { value: '{"action":"ping"}' } });

    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      expect(api.wsSend).toHaveBeenCalledWith(mockPath, '{"action":"ping"}');
    });

    expect(screen.getByText(/→ {"action":"ping"}/)).toBeTruthy();
    expect((messageInput as HTMLInputElement).value).toBe("");
  });

  it("sends message when pressing Enter", async () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({ sessionId: mockPath, kind: "open" });

    const messageInput = screen.getByPlaceholderText("Message");
    fireEvent.change(messageInput, { target: { value: "hello enter" } });
    fireEvent.keyDown(messageInput, { key: "Enter" });

    await waitFor(() => {
      expect(api.wsSend).toHaveBeenCalledWith(mockPath, "hello enter");
    });

    expect(screen.getByText(/→ hello enter/)).toBeTruthy();
  });

  it("does not call api.wsSend when input is empty", () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({ sessionId: mockPath, kind: "open" });

    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(api.wsSend).not.toHaveBeenCalled();
  });

  it("shows error toast if api.wsSend fails", async () => {
    vi.mocked(api.wsSend).mockRejectedValueOnce(new Error("Broken pipe"));
    const doc = createTestDoc();
    const toast = vi.fn();

    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={toast} />);

    emitWsEvent({ sessionId: mockPath, kind: "open" });

    fireEvent.change(screen.getByPlaceholderText("Message"), { target: { value: "test" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      expect(toast).toHaveBeenCalledWith("Error: Broken pipe", "error");
    });
  });

  it("renders inbound message frame when kind is message", () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({
      sessionId: mockPath,
      kind: "message",
      data: "pong response from server",
    });

    expect(screen.getByText(/← pong response from server/)).toBeTruthy();
  });

  it("transitions to closed on close event and displays closed frame", async () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({ sessionId: mockPath, kind: "open" });
    expect(screen.getByText("open")).toBeTruthy();

    emitWsEvent({ sessionId: mockPath, kind: "close" });

    await waitFor(() => {
      expect(screen.getAllByText("closed").length).toBeGreaterThanOrEqual(2);
    });
    expect(screen.getByRole("button", { name: "Connect" })).toBeTruthy();
    expect((screen.getByPlaceholderText("Message") as HTMLInputElement).disabled).toBe(true);
  });

  it("transitions to closed on error event and displays error frame", async () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({ sessionId: mockPath, kind: "open" });

    emitWsEvent({
      sessionId: mockPath,
      kind: "error",
      data: "Connection reset by peer",
    });

    await waitFor(() => {
      expect(screen.getByText("closed")).toBeTruthy();
    });
    expect(screen.getByText("Connection reset by peer")).toBeTruthy();
  });

  it("calls api.wsClose when Disconnect button is clicked", async () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({ sessionId: mockPath, kind: "open" });

    fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));

    await waitFor(() => {
      expect(api.wsClose).toHaveBeenCalledWith(mockPath);
    });
  });

  it("cleans up listener and closes connection on unmount", async () => {
    const doc = createTestDoc();
    const { unmount } = render(
      <WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />,
    );

    await waitFor(() => {
      expect(listen).toHaveBeenCalledWith("ws://message", expect.any(Function));
    });

    unmount();

    expect(unlistenMock).toHaveBeenCalled();
    expect(api.wsClose).toHaveBeenCalledWith(mockPath);
  });

  it("switches tabs between Message, Params, Headers, Auth, and Docs", () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    // Click Params tab
    fireEvent.click(screen.getByTitle("Params"));
    expect(screen.getByText(/Handshake Query Parameters/)).toBeTruthy();

    // Click Headers tab
    fireEvent.click(screen.getByTitle("Headers"));
    expect(screen.getByText(/Handshake HTTP Headers/)).toBeTruthy();

    // Click Auth tab
    fireEvent.click(screen.getByTitle("Auth"));
    expect(screen.getByText(/No auth/i)).toBeTruthy();

    // Click Docs tab
    fireEvent.click(screen.getByTitle("Docs"));
    expect(screen.getByText("Description")).toBeTruthy();

    // Click Message tab
    fireEvent.click(screen.getByTitle("Message"));
    expect(screen.getByPlaceholderText("Message")).toBeTruthy();
  });

  it("formats JSON draft using Beautify button", () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    const messageInput = screen.getByPlaceholderText("Message") as HTMLTextAreaElement;
    fireEvent.change(messageInput, { target: { value: '{"name":"keel","count":42}' } });

    fireEvent.click(screen.getByRole("button", { name: /Beautify/ }));

    expect(messageInput.value).toBe('{\n  "name": "keel",\n  "count": 42\n}');
  });

  it("loads preset snippet into message composer", () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    const messageInput = screen.getByPlaceholderText("Message") as HTMLTextAreaElement;
    fireEvent.click(screen.getByRole("button", { name: "Ping" }));

    expect(messageInput.value).toBe('{"type":"ping"}');
  });

  it("filters messages by search text and direction", () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({ sessionId: mockPath, kind: "open" });
    emitWsEvent({ sessionId: mockPath, kind: "message", data: "apple event" });
    emitWsEvent({ sessionId: mockPath, kind: "message", data: "banana notification" });

    expect(screen.getByText(/← apple event/)).toBeTruthy();
    expect(screen.getByText(/← banana notification/)).toBeTruthy();

    // Filter by text
    const filterInput = screen.getByPlaceholderText("Filter messages...");
    fireEvent.change(filterInput, { target: { value: "apple" } });

    expect(screen.getByText(/← apple event/)).toBeTruthy();
    expect(screen.queryByText(/← banana notification/)).toBeNull();

    // Clear filter
    fireEvent.change(filterInput, { target: { value: "" } });
    expect(screen.getByText(/← banana notification/)).toBeTruthy();
  });

  it("clears messages stream on clear button click", () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({ sessionId: mockPath, kind: "message", data: "to be cleared" });
    expect(screen.getByText(/← to be cleared/)).toBeTruthy();

    fireEvent.click(screen.getByTitle("Clear messages"));
    expect(screen.queryByText(/← to be cleared/)).toBeNull();
    expect(screen.getByText("No messages")).toBeTruthy();
  });

  it("sends binary frame when binary format is selected", async () => {
    const doc = createTestDoc();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    emitWsEvent({ sessionId: mockPath, kind: "open" });

    // Switch to binary format
    fireEvent.click(screen.getByRole("button", { name: "binary" }));

    const messageInput = screen.getByPlaceholderText("Message");
    fireEvent.change(messageInput, { target: { value: "aGVsbG8=" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      expect(api.wsSend).toHaveBeenCalledWith(mockPath, "aGVsbG8=", true);
    });
  });

  it("loads frame payload back into composer using Use in composer button", () => {
    const doc = createTestDoc();
    const toast = vi.fn();
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={toast} />);

    emitWsEvent({
      sessionId: mockPath,
      kind: "message",
      data: '{"from":"server","id":99}',
    });

    const useButton = screen.getByTitle("Use in composer");
    fireEvent.click(useButton);

    const messageInput = screen.getByPlaceholderText("Message") as HTMLTextAreaElement;
    expect(messageInput.value).toBe('{"from":"server","id":99}');
    expect(toast).toHaveBeenCalledWith("Loaded into composer", "info");
  });

  it("remembers to connect when returning to the tab if it was connected", async () => {
    const doc = createTestDoc();
    const { unmount } = render(
      <WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />,
    );

    // User connects
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() => {
      expect(api.wsConnect).toHaveBeenCalledTimes(1);
    });

    emitWsEvent({ sessionId: mockPath, kind: "open" });
    await waitFor(() => {
      expect(screen.getByText("open")).toBeTruthy();
    });

    // User leaves the tab (unmount)
    unmount();

    // User returns to the tab (remount)
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    // It remembered that it was connected, so it automatically reconnects!
    await waitFor(() => {
      expect(api.wsConnect).toHaveBeenCalledTimes(2);
    });
  });

  it("does not auto-connect on return if user explicitly disconnected", async () => {
    const doc = createTestDoc();
    const { unmount } = render(
      <WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />,
    );

    // User connects
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    emitWsEvent({ sessionId: mockPath, kind: "open" });
    await waitFor(() => {
      expect(screen.getByText("open")).toBeTruthy();
    });

    // User clicks Disconnect
    fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
    await waitFor(() => {
      expect(screen.getByText("closed")).toBeTruthy();
    });

    // User leaves the tab
    unmount();

    // Reset mock call count
    vi.mocked(api.wsConnect).mockClear();

    // User returns to the tab
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    // Should NOT auto-connect
    expect(api.wsConnect).not.toHaveBeenCalled();
  });

  it("preserves messages across tab switches", async () => {
    const doc = createTestDoc();
    const { unmount } = render(
      <WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />,
    );

    emitWsEvent({ sessionId: mockPath, kind: "open" });
    emitWsEvent({ sessionId: mockPath, kind: "message", data: "first saved message" });

    expect(screen.getByText(/← first saved message/)).toBeTruthy();

    // Switch tab (unmount)
    unmount();

    // Return to tab (remount)
    render(<WsPanel path={mockPath} doc={doc} setDoc={vi.fn()} toast={vi.fn()} />);

    // Message is still visible
    expect(screen.getByText(/← first saved message/)).toBeTruthy();
  });
});
