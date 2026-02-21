import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";
import type { Message } from "../../types";

describe("MessagesSlice", () => {
  let store: ReturnType<typeof createAppStore>;
  beforeEach(() => {
    store = createAppStore();
    store.setState({
      currentUser: {
        id: "u1",
        displayName: "test",
        email: "user@test.com",
        avatarUrl: null,
      },
      keyPair: { publicKey: "pk" },
      isAuthenticated: true,
    });
  });

  const makeMsg = (overrides: Partial<Message> = {}): Message => ({
    id: "msg-1",
    channelId: "ch-1",
    senderId: "user-1",
    content: "Hello",
    encryptedContent: "Hello",
    nonce: "nonce-1",
    createdAt: new Date().toISOString(),
    editedAt: null,
    attachments: [],
    ...overrides,
  });

  it("has empty messagesById and messageIdsByChannel as initial state", () => {
    const fresh = createAppStore();
    const s = fresh.getState();
    expect(s.messagesById).toEqual({});
    expect(s.messageIdsByChannel).toEqual({});
  });

  it("addMessage adds to both messagesById and channel's ID array", () => {
    const msg = makeMsg();
    store.getState().addMessage("ch-1", msg);
    expect(store.getState().messagesById["msg-1"]).toBeDefined();
    expect(store.getState().messageIdsByChannel["ch-1"]).toContain("msg-1");
  });

  it("prependMessages prepends to the beginning of channel's ID array", () => {
    const msg1 = makeMsg({ id: "msg-1" });
    store.getState().addMessage("ch-1", msg1);
    const older = makeMsg({ id: "msg-0" });
    store.getState().prependMessages("ch-1", [older]);
    expect(store.getState().messageIdsByChannel["ch-1"][0]).toBe("msg-0");
  });

  it("sendMessage creates message with encryptedContent and nonce fields", () => {
    store.getState().sendMessage("ch-1", "Hello world", []);
    const ids = store.getState().messageIdsByChannel["ch-1"];
    expect(ids).toHaveLength(1);
    const msg = store.getState().messagesById[ids[0]];
    expect(msg.content).toBe("Hello world");
    expect(msg.encryptedContent).toBeTruthy();
    expect(msg.nonce).toBeTruthy();
    expect(msg.senderId).toBe(store.getState().currentUser!.id);
  });

  it("deleteMessage removes from messagesById and channel's ID array", () => {
    const msg = makeMsg();
    store.getState().addMessage("ch-1", msg);
    store.getState().deleteMessage("msg-1");
    expect(store.getState().messagesById["msg-1"]).toBeUndefined();
    expect(store.getState().messageIdsByChannel["ch-1"]).not.toContain("msg-1");
  });

  it("editMessage updates content in messagesById", () => {
    const msg = makeMsg();
    store.getState().addMessage("ch-1", msg);
    store.getState().editMessage("msg-1", "Updated content");
    const edited = store.getState().messagesById["msg-1"];
    expect(edited.content).toBe("Updated content");
    expect(edited.editedAt).not.toBeNull();
  });

  it("hasMore defaults appropriately for channels", () => {
    expect(store.getState().hasMore).toEqual({});
  });

  it("loadingMessages defaults appropriately", () => {
    expect(store.getState().loadingMessages).toEqual({});
  });
});
