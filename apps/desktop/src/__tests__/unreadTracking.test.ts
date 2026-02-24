import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "../store";
import { groupMessages } from "../components/chat/groupMessages";
import type { Message } from "../types";

function makeMessage(id: string, createdAt: string, senderId = "u1"): Message {
  return {
    id,
    channelId: "c1",
    senderId,
    content: `Message ${id}`,
    encryptedContent: "",
    nonce: "",
    createdAt,
    editedAt: null,
    attachments: [],
  };
}

describe("UnreadSlice extensions", () => {
  let store: ReturnType<typeof createAppStore>;

  beforeEach(() => {
    store = createAppStore();
  });

  it("should track unread counts per channel", () => {
    store.getState().incrementUnread("c1");
    store.getState().incrementUnread("c1");
    store.getState().incrementUnread("c1");
    expect(store.getState().unreadCountByChannel["c1"]).toBe(3);
  });

  it("should clear unread count when channel is viewed", () => {
    store.getState().incrementUnread("c1");
    store.getState().incrementUnread("c1");
    store.getState().incrementUnread("c1");
    store.getState().incrementUnread("c1");
    store.getState().incrementUnread("c1");
    store.getState().markChannelRead("c1", "msg-latest", 1700000000);
    expect(store.getState().unreadCountByChannel["c1"]).toBe(0);
  });

  it("should initialize read positions from backend data", () => {
    store.getState().initializeReadPositions([
      {
        channelId: "c1",
        lastReadMessageId: "m5",
        lastReadCreatedAt: 1700000000,
        unreadCount: 3,
      },
      {
        channelId: "c2",
        lastReadMessageId: "m10",
        lastReadCreatedAt: 1700001000,
        unreadCount: 0,
      },
    ]);

    expect(store.getState().unreadCountByChannel["c1"]).toBe(3);
    expect(store.getState().unreadCountByChannel["c2"]).toBe(0);
    expect(store.getState().lastReadByChannel["c1"]).toBe("m5");
    expect(store.getState().lastReadCreatedAtByChannel["c1"]).toBe(1700000000);
  });

  it("should set unread count for a specific channel", () => {
    store.getState().setUnreadCount("c1", 7);
    expect(store.getState().unreadCountByChannel["c1"]).toBe(7);
  });

  it("should track lastReadCreatedAtByChannel when marking read", () => {
    store.getState().markChannelRead("c1", "m5", 1700000500);
    expect(store.getState().lastReadCreatedAtByChannel["c1"]).toBe(1700000500);
  });
});

describe("groupMessages with unread divider", () => {
  it("should insert unread divider after last-read message", () => {
    const messages = [
      makeMessage("m1", "2026-02-20T10:00:00Z"),
      makeMessage("m2", "2026-02-20T10:01:00Z"),
      makeMessage("m3", "2026-02-20T10:02:00Z"),
      makeMessage("m4", "2026-02-20T10:03:00Z"),
    ];

    const items = groupMessages(messages, "m2");

    const dividerIndex = items.findIndex((i) => i.type === "unread-divider");
    expect(dividerIndex).toBeGreaterThan(-1);

    // The divider should be after the group containing m2 and before the group containing m3
    const beforeDivider = items[dividerIndex - 1];
    expect(beforeDivider.type).toBe("message-group");
    if (beforeDivider.type === "message-group") {
      expect(beforeDivider.messages.some((m) => m.id === "m2")).toBe(true);
    }
  });

  it("should not insert divider when lastReadMessageId is not provided", () => {
    const messages = [
      makeMessage("m1", "2026-02-20T10:00:00Z"),
      makeMessage("m2", "2026-02-20T10:01:00Z"),
    ];

    const items = groupMessages(messages);
    expect(items.some((i) => i.type === "unread-divider")).toBe(false);
  });

  it("should not insert divider when lastReadMessageId is the last message", () => {
    const messages = [
      makeMessage("m1", "2026-02-20T10:00:00Z"),
      makeMessage("m2", "2026-02-20T10:01:00Z"),
    ];

    const items = groupMessages(messages, "m2");
    expect(items.some((i) => i.type === "unread-divider")).toBe(false);
  });
});
