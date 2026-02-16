import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";

describe("GuildsSlice", () => {
  let store: ReturnType<typeof createAppStore>;
  beforeEach(() => {
    store = createAppStore();
    store.getState().login("owner@example.com");
  });

  it("has empty guildsById and guildIds as initial state", () => {
    const fresh = createAppStore();
    const s = fresh.getState();
    expect(s.guildsById).toEqual({});
    expect(s.guildIds).toEqual([]);
    expect(s.lastVisitedGuildId).toBeNull();
  });

  it("setLastVisitedGuild updates lastVisitedGuildId", () => {
    store.getState().setLastVisitedGuild("guild-1");
    expect(store.getState().lastVisitedGuildId).toBe("guild-1");
  });

  it("createGuild adds to guildsById and guildIds", () => {
    store.getState().createGuild("My Guild", null);
    const s = store.getState();
    expect(s.guildIds).toHaveLength(1);
    const id = s.guildIds[0];
    expect(s.guildsById[id]).toBeDefined();
    expect(s.guildsById[id].name).toBe("My Guild");
  });

  it("updateGuild modifies fields in guildsById", () => {
    store.getState().createGuild("Old Name", null);
    const id = store.getState().guildIds[0];
    store.getState().updateGuild(id, { name: "New Name" });
    expect(store.getState().guildsById[id].name).toBe("New Name");
  });

  it("leaveGuild removes from guildsById and guildIds", () => {
    store.getState().createGuild("Test Guild", null);
    const id = store.getState().guildIds[0];
    store.getState().leaveGuild(id);
    expect(store.getState().guildsById[id]).toBeUndefined();
    expect(store.getState().guildIds).not.toContain(id);
  });
});
