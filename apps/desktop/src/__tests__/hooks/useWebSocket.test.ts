import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useAppStore } from "../../store";

// Track registered event listeners
type EventCallback = (event: { payload: unknown }) => void;
const listeners = new Map<string, EventCallback>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, callback: EventCallback) => {
    listeners.set(event, callback);
    return () => {
      listeners.delete(event);
    };
  }),
}));

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

describe("useWebSocket", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    listeners.clear();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    useAppStore.setState(useAppStore.getInitialState(), true);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("calls ws_connect on mount", async () => {
    // Dynamically import to get fresh module with mocks
    const { useWebSocket } = await import("../../hooks/useWebSocket");

    await act(async () => {
      renderHook(() => useWebSocket());
    });

    expect(mockInvoke).toHaveBeenCalledWith("ws_connect");
  });

  it("updates connectionState on ws:state events", async () => {
    const { useWebSocket } = await import("../../hooks/useWebSocket");

    await act(async () => {
      renderHook(() => useWebSocket());
    });

    const stateCallback = listeners.get("ws:state");
    expect(stateCallback).toBeDefined();

    act(() => {
      stateCallback!({ payload: { status: "Authenticated" } });
    });

    expect(useAppStore.getState().connectionState).toEqual({
      status: "Authenticated",
    });
  });

  it("dispatches messages to MessagesSlice on ws:message", async () => {
    const { useWebSocket } = await import("../../hooks/useWebSocket");

    await act(async () => {
      renderHook(() => useWebSocket());
    });

    const messageCallback = listeners.get("ws:message");
    expect(messageCallback).toBeDefined();

    act(() => {
      messageCallback!({
        payload: {
          channel_id: "ch-1",
          message_id: "msg-1",
          sender_id: "user-1",
          plaintext: "Hello world",
          status: "delivered",
          created_at: "2026-02-24T00:00:00Z",
        },
      });
    });

    const state = useAppStore.getState();
    expect(state.messagesById["msg-1"]).toBeDefined();
    expect(state.messagesById["msg-1"].content).toBe("Hello world");
    expect(state.messagesById["msg-1"].channelId).toBe("ch-1");
    expect(state.messageIdsByChannel["ch-1"]).toContain("msg-1");
  });

  it("handles ws:message with decrypt_failed status", async () => {
    const { useWebSocket } = await import("../../hooks/useWebSocket");

    await act(async () => {
      renderHook(() => useWebSocket());
    });

    const messageCallback = listeners.get("ws:message");

    act(() => {
      messageCallback!({
        payload: {
          channel_id: "ch-1",
          message_id: "msg-2",
          sender_id: "user-2",
          plaintext: null,
          status: "decrypt_failed",
          created_at: "2026-02-24T00:00:00Z",
        },
      });
    });

    const msg = useAppStore.getState().messagesById["msg-2"];
    expect(msg).toBeDefined();
    expect(msg.status).toBe("decrypt_failed");
    expect(msg.content).toBe("");
  });

  it("handles ws:message_updated events", async () => {
    const { useWebSocket } = await import("../../hooks/useWebSocket");

    // Seed a message first
    useAppStore.getState().addMessage("ch-1", {
      id: "msg-1",
      channelId: "ch-1",
      senderId: "user-1",
      content: "Original",
      encryptedContent: "",
      nonce: "",
      createdAt: "2026-02-24T00:00:00Z",
      editedAt: null,
      attachments: [],
    });

    await act(async () => {
      renderHook(() => useWebSocket());
    });

    const updatedCallback = listeners.get("ws:message_updated");
    expect(updatedCallback).toBeDefined();

    act(() => {
      updatedCallback!({
        payload: {
          channel_id: "ch-1",
          message_id: "msg-1",
          sender_id: "user-1",
          plaintext: "Edited content",
          status: "delivered",
          edited_at: "2026-02-24T01:00:00Z",
        },
      });
    });

    const msg = useAppStore.getState().messagesById["msg-1"];
    expect(msg.content).toBe("Edited content");
    expect(msg.editedAt).toBe("2026-02-24T01:00:00Z");
  });

  it("handles ws:message_deleted events", async () => {
    const { useWebSocket } = await import("../../hooks/useWebSocket");

    // Seed a message
    useAppStore.getState().addMessage("ch-1", {
      id: "msg-1",
      channelId: "ch-1",
      senderId: "user-1",
      content: "To delete",
      encryptedContent: "",
      nonce: "",
      createdAt: "2026-02-24T00:00:00Z",
      editedAt: null,
      attachments: [],
    });

    await act(async () => {
      renderHook(() => useWebSocket());
    });

    const deletedCallback = listeners.get("ws:message_deleted");
    expect(deletedCallback).toBeDefined();

    act(() => {
      deletedCallback!({
        payload: {
          channel_id: "ch-1",
          message_id: "msg-1",
        },
      });
    });

    expect(useAppStore.getState().messagesById["msg-1"]).toBeUndefined();
  });

  it("updates typing users on ws:typing events", async () => {
    const { useWebSocket } = await import("../../hooks/useWebSocket");

    await act(async () => {
      renderHook(() => useWebSocket());
    });

    const typingCallback = listeners.get("ws:typing");
    expect(typingCallback).toBeDefined();

    act(() => {
      typingCallback!({
        payload: { channel_id: "ch-1", user_id: "user-1" },
      });
    });

    expect(useAppStore.getState().typingUsers["ch-1"]).toContain("user-1");
  });

  it("cleans up listeners and disconnects on unmount", async () => {
    const { useWebSocket } = await import("../../hooks/useWebSocket");

    let unmount: () => void;
    await act(async () => {
      const result = renderHook(() => useWebSocket());
      unmount = result.unmount;
    });

    // Listeners should be registered
    expect(listeners.size).toBeGreaterThan(0);

    await act(async () => {
      unmount!();
    });

    expect(mockInvoke).toHaveBeenCalledWith("ws_disconnect");
  });
});
