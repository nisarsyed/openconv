import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";

describe("ChannelsSlice", () => {
  let store: ReturnType<typeof createAppStore>;
  beforeEach(() => {
    store = createAppStore();
  });

  it("has empty channelsById and channelIdsByGuild as initial state", () => {
    const s = store.getState();
    expect(s.channelsById).toEqual({});
    expect(s.channelIdsByGuild).toEqual({});
    expect(s.lastVisitedChannelByGuild).toEqual({});
  });

  it("setLastVisitedChannel updates lastVisitedChannelByGuild for correct guild", () => {
    store.getState().setLastVisitedChannel("guild-1", "channel-1");
    expect(store.getState().lastVisitedChannelByGuild["guild-1"]).toBe(
      "channel-1",
    );
  });

  it("createChannel adds to channelsById and channelIdsByGuild", () => {
    store.getState().createChannel("guild-1", "general", "text", "General");
    const s = store.getState();
    const ids = s.channelIdsByGuild["guild-1"];
    expect(ids).toHaveLength(1);
    expect(s.channelsById[ids[0]]).toBeDefined();
    expect(s.channelsById[ids[0]].name).toBe("general");
  });

  it("deleteChannel removes from channelsById and channelIdsByGuild", () => {
    store.getState().createChannel("guild-1", "general", "text", null);
    const id = store.getState().channelIdsByGuild["guild-1"][0];
    store.getState().deleteChannel(id);
    expect(store.getState().channelsById[id]).toBeUndefined();
    expect(store.getState().channelIdsByGuild["guild-1"]).not.toContain(id);
  });

  it("channels are ordered by position within a guild", () => {
    store.getState().createChannel("guild-1", "second", "text", null);
    store.getState().createChannel("guild-1", "first", "text", null);
    const ids = store.getState().channelIdsByGuild["guild-1"];
    const channels = ids.map((id) => store.getState().channelsById[id]);
    for (let i = 1; i < channels.length; i++) {
      expect(channels[i].position).toBeGreaterThanOrEqual(
        channels[i - 1].position,
      );
    }
  });
});
