import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useAppStore } from "../../store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

describe("useTypingIndicator", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockInvoke.mockReset().mockResolvedValue(undefined);
    useAppStore.setState(useAppStore.getInitialState(), true);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("debounces typing sends to max 1 per 3s", async () => {
    const { useTypingIndicator } = await import(
      "../../hooks/useTypingIndicator"
    );

    const { result } = renderHook(() => useTypingIndicator("ch-1"));

    // First call should send
    act(() => {
      result.current.onKeyPress();
    });
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke).toHaveBeenCalledWith("ws_send_typing", {
      channelId: "ch-1",
    });

    // Immediate second call should be debounced
    act(() => {
      result.current.onKeyPress();
    });
    expect(mockInvoke).toHaveBeenCalledTimes(1);

    // After 1 second, still debounced
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    act(() => {
      result.current.onKeyPress();
    });
    expect(mockInvoke).toHaveBeenCalledTimes(1);

    // After 3 seconds total, should send again
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    act(() => {
      result.current.onKeyPress();
    });
    expect(mockInvoke).toHaveBeenCalledTimes(2);
  });

  it("returns typing user names from store", async () => {
    const { useTypingIndicator } = await import(
      "../../hooks/useTypingIndicator"
    );

    // Seed users and typing state
    useAppStore.setState({
      usersById: {
        "user-1": {
          id: "user-1",
          displayName: "Alice",
          avatarUrl: null,
          email: "a@test.com",
        },
        "user-2": {
          id: "user-2",
          displayName: "Bob",
          avatarUrl: null,
          email: "b@test.com",
        },
      },
      typingUsers: {
        "ch-1": ["user-1", "user-2"],
      },
    });

    const { result } = renderHook(() => useTypingIndicator("ch-1"));

    expect(result.current.typingNames).toEqual(["Alice", "Bob"]);
  });

  it("returns empty array when no one is typing", async () => {
    const { useTypingIndicator } = await import(
      "../../hooks/useTypingIndicator"
    );

    const { result } = renderHook(() => useTypingIndicator("ch-1"));

    expect(result.current.typingNames).toEqual([]);
  });
});
