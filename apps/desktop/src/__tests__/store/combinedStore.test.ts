import { describe, it, expect } from "vitest";
import { createAppStore } from "./helpers";

describe("Combined Store", () => {
  it("all slices are accessible from single store", () => {
    const store = createAppStore();
    const s = store.getState();
    // Auth
    expect(s).toHaveProperty("currentUser");
    // Guilds
    expect(s).toHaveProperty("guildsById");
    // Channels
    expect(s).toHaveProperty("channelsById");
    // Messages
    expect(s).toHaveProperty("messagesById");
    // Members
    expect(s).toHaveProperty("membersById");
    // Presence
    expect(s).toHaveProperty("presenceByUserId");
    // Unread
    expect(s).toHaveProperty("lastReadByChannel");
    // UI
    expect(s).toHaveProperty("theme");
  });

  it("cross-slice access works via get()", () => {
    const store = createAppStore();
    store.getState().login("test@example.com");
    store.getState().createGuild("Test Guild", null);
    const guildId = store.getState().guildIds[0];
    const guild = store.getState().guildsById[guildId];
    expect(guild.ownerId).toBe(store.getState().currentUser!.id);
  });

  it("immer allows mutation syntax in actions", () => {
    const store = createAppStore();
    store.getState().login("test@example.com");
    store.getState().createGuild("Guild A", null);
    store.getState().createGuild("Guild B", null);
    expect(store.getState().guildIds).toHaveLength(2);
  });

  it("persist partialize only saves the expected keys", async () => {
    // Import the real store to test persist config
    const { useAppStore } = await import("../../store/index");
    const persistOptions = (useAppStore as unknown as { persist: { getOptions: () => { partialize?: (state: Record<string, unknown>) => Record<string, unknown> } } }).persist.getOptions();
    expect(persistOptions.partialize).toBeDefined();

    const fakeState = {
      // Keys that SHOULD be persisted
      lastVisitedGuildId: "guild-1",
      lastVisitedChannelByGuild: { "guild-1": "ch-1" },
      theme: "dark" as const,
      channelSidebarVisible: true,
      memberListVisible: false,
      // Keys that should NOT be persisted
      currentUser: { id: "u1" },
      guildsById: { g1: {} },
      messagesById: { m1: {} },
    };

    const partialized = persistOptions.partialize!(fakeState);
    const keys = Object.keys(partialized);
    expect(keys).toEqual([
      "lastVisitedGuildId",
      "lastVisitedChannelByGuild",
      "theme",
      "channelSidebarVisible",
      "memberListVisible",
    ]);
    expect(partialized).not.toHaveProperty("currentUser");
    expect(partialized).not.toHaveProperty("guildsById");
    expect(partialized).not.toHaveProperty("messagesById");
  });
});
