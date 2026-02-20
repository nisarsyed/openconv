import { describe, it, expect, beforeEach } from "vitest";
import { seedStores } from "../../mock/seed";
import { useAppStore } from "../../store";

describe("seedStores", () => {
  beforeEach(() => {
    useAppStore.setState(useAppStore.getInitialState());
  });

  it("populates guildsById with entries", () => {
    seedStores();
    const { guildsById } = useAppStore.getState();
    expect(Object.keys(guildsById).length).toBeGreaterThan(0);
  });

  it("populates guildIds array", () => {
    seedStores();
    const { guildIds } = useAppStore.getState();
    expect(guildIds.length).toBeGreaterThan(0);
  });

  it("populates channelsById with entries", () => {
    seedStores();
    const { channelsById } = useAppStore.getState();
    expect(Object.keys(channelsById).length).toBeGreaterThan(0);
  });

  it("populates channelIdsByGuild for each guild", () => {
    seedStores();
    const { channelIdsByGuild, guildIds } = useAppStore.getState();
    for (const guildId of guildIds) {
      expect(channelIdsByGuild[guildId]).toBeDefined();
      expect(channelIdsByGuild[guildId].length).toBeGreaterThan(0);
    }
  });

  it("populates messageIdsByChannel for text channels", () => {
    seedStores();
    const { messageIdsByChannel, channelsById } = useAppStore.getState();
    const textChannelIds = Object.values(channelsById)
      .filter((c) => c.channelType === "text")
      .map((c) => c.id);
    for (const channelId of textChannelIds) {
      expect(messageIdsByChannel[channelId]).toBeDefined();
      expect(messageIdsByChannel[channelId].length).toBeGreaterThan(0);
    }
  });

  it("populates membersById and memberIdsByGuild", () => {
    seedStores();
    const { membersById, memberIdsByGuild, guildIds } =
      useAppStore.getState();
    expect(Object.keys(membersById).length).toBeGreaterThan(0);
    for (const guildId of guildIds) {
      expect(memberIdsByGuild[guildId]).toBeDefined();
    }
  });

  it("populates presenceByUserId", () => {
    seedStores();
    const { presenceByUserId } = useAppStore.getState();
    expect(Object.keys(presenceByUserId).length).toBeGreaterThan(0);
  });

  it("sets hasMore to true for channels with many messages", () => {
    seedStores();
    const { hasMore, messageIdsByChannel } = useAppStore.getState();
    const channelWithMessages = Object.keys(messageIdsByChannel).find(
      (id) => messageIdsByChannel[id].length >= 20,
    );
    if (channelWithMessages) {
      expect(hasMore[channelWithMessages]).toBe(true);
    }
  });
});
