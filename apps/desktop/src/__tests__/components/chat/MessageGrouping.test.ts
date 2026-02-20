import { describe, it, expect } from "vitest";
import { groupMessages } from "../../../components/chat/groupMessages";
import type { Message } from "../../../types";

function makeMessage(
  overrides: Partial<Message> & {
    id: string;
    senderId: string;
    createdAt: string;
  },
): Message {
  return {
    channelId: "ch-1",
    content: "hello",
    encryptedContent: "hello",
    nonce: "n",
    editedAt: null,
    attachments: [],
    ...overrides,
  };
}

describe("groupMessages", () => {
  it("returns empty array for empty input", () => {
    expect(groupMessages([])).toEqual([]);
  });

  it("creates a single group for one message", () => {
    const items = groupMessages([
      makeMessage({
        id: "1",
        senderId: "u1",
        createdAt: "2026-02-14T10:00:00Z",
      }),
    ]);
    expect(items).toHaveLength(2); // date-separator + group
    expect(items[0]).toMatchObject({
      type: "date-separator",
      date: "2026-02-14",
    });
    expect(items[1]).toMatchObject({ type: "message-group", senderId: "u1" });
  });

  it("groups consecutive messages from same author within 5 min", () => {
    const items = groupMessages([
      makeMessage({
        id: "1",
        senderId: "u1",
        createdAt: "2026-02-14T10:00:00Z",
      }),
      makeMessage({
        id: "2",
        senderId: "u1",
        createdAt: "2026-02-14T10:01:00Z",
      }),
      makeMessage({
        id: "3",
        senderId: "u1",
        createdAt: "2026-02-14T10:02:00Z",
      }),
    ]);
    // 1 date separator + 1 group
    expect(items).toHaveLength(2);
    const group = items[1];
    expect(group.type).toBe("message-group");
    if (group.type === "message-group") {
      expect(group.messages).toHaveLength(3);
    }
  });

  it("splits into new group on author change", () => {
    const items = groupMessages([
      makeMessage({
        id: "1",
        senderId: "u1",
        createdAt: "2026-02-14T10:00:00Z",
      }),
      makeMessage({
        id: "2",
        senderId: "u1",
        createdAt: "2026-02-14T10:01:00Z",
      }),
      makeMessage({
        id: "3",
        senderId: "u2",
        createdAt: "2026-02-14T10:02:00Z",
      }),
      makeMessage({
        id: "4",
        senderId: "u2",
        createdAt: "2026-02-14T10:03:00Z",
      }),
    ]);
    // 1 date sep + 2 groups
    expect(items).toHaveLength(3);
    expect(items[1]).toMatchObject({ type: "message-group", senderId: "u1" });
    expect(items[2]).toMatchObject({ type: "message-group", senderId: "u2" });
  });

  it("splits into new group after 5 minute gap", () => {
    const items = groupMessages([
      makeMessage({
        id: "1",
        senderId: "u1",
        createdAt: "2026-02-14T10:00:00Z",
      }),
      makeMessage({
        id: "2",
        senderId: "u1",
        createdAt: "2026-02-14T10:06:00Z",
      }),
    ]);
    // 1 date sep + 2 groups
    expect(items).toHaveLength(3);
    expect(items[1]).toMatchObject({ type: "message-group", senderId: "u1" });
    expect(items[2]).toMatchObject({ type: "message-group", senderId: "u1" });
  });

  it("inserts date separator on date boundary", () => {
    const items = groupMessages([
      makeMessage({
        id: "1",
        senderId: "u1",
        createdAt: "2026-02-14T23:59:00Z",
      }),
      makeMessage({
        id: "2",
        senderId: "u1",
        createdAt: "2026-02-15T00:01:00Z",
      }),
    ]);
    // date-sep(14) + group + date-sep(15) + group
    expect(items).toHaveLength(4);
    expect(items[0]).toMatchObject({
      type: "date-separator",
      date: "2026-02-14",
    });
    expect(items[1]).toMatchObject({ type: "message-group" });
    expect(items[2]).toMatchObject({
      type: "date-separator",
      date: "2026-02-15",
    });
    expect(items[3]).toMatchObject({ type: "message-group" });
  });

  it("handles mixed grouping scenarios", () => {
    const items = groupMessages([
      makeMessage({
        id: "1",
        senderId: "u1",
        createdAt: "2026-02-14T10:00:00Z",
      }),
      makeMessage({
        id: "2",
        senderId: "u1",
        createdAt: "2026-02-14T10:01:00Z",
      }),
      makeMessage({
        id: "3",
        senderId: "u2",
        createdAt: "2026-02-14T10:02:00Z",
      }),
      makeMessage({
        id: "4",
        senderId: "u2",
        createdAt: "2026-02-14T10:03:00Z",
      }),
      makeMessage({
        id: "5",
        senderId: "u2",
        createdAt: "2026-02-14T10:10:00Z",
      }),
    ]);
    // date-sep + groupA(2) + groupB(2) + groupB(1)
    expect(items).toHaveLength(4);
    if (items[1].type === "message-group")
      expect(items[1].messages).toHaveLength(2);
    if (items[2].type === "message-group")
      expect(items[2].messages).toHaveLength(2);
    if (items[3].type === "message-group")
      expect(items[3].messages).toHaveLength(1);
  });
});
