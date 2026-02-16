import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";

describe("UnreadSlice", () => {
  let store: ReturnType<typeof createAppStore>;
  beforeEach(() => { store = createAppStore(); });

  it("markChannelRead updates lastReadByChannel and resets unreadCountByChannel", () => {
    store.getState().incrementUnread("ch-1");
    store.getState().incrementUnread("ch-1");
    store.getState().markChannelRead("ch-1", "msg-5");
    expect(store.getState().lastReadByChannel["ch-1"]).toBe("msg-5");
    expect(store.getState().unreadCountByChannel["ch-1"]).toBe(0);
  });

  it("incrementUnread increases unreadCountByChannel", () => {
    store.getState().incrementUnread("ch-1");
    store.getState().incrementUnread("ch-1");
    expect(store.getState().unreadCountByChannel["ch-1"]).toBe(2);
  });

  it("incrementMention increases mentionCountByGuild", () => {
    store.getState().incrementMention("guild-1");
    expect(store.getState().mentionCountByGuild["guild-1"]).toBe(1);
  });

  it("resetGuildMentions clears mention count for guild", () => {
    store.getState().incrementMention("guild-1");
    store.getState().resetGuildMentions("guild-1");
    expect(store.getState().mentionCountByGuild["guild-1"]).toBe(0);
  });
});
