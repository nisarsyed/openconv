import { describe, it, expect, beforeEach } from "vitest";
import { createAppStore } from "./helpers";
import type { Member } from "../../types";

describe("MembersSlice", () => {
  let store: ReturnType<typeof createAppStore>;
  beforeEach(() => {
    store = createAppStore();
  });

  const makeMember = (userId: string): Member => ({
    userId,
    guildId: "guild-1",
    nickname: null,
    roles: ["member"],
    joinedAt: new Date().toISOString(),
  });

  it("fetchMembers populates membersById and memberIdsByGuild", () => {
    const members = [makeMember("user-1"), makeMember("user-2")];
    store.getState().fetchMembers("guild-1", members);
    expect(Object.keys(store.getState().membersById).length).toBe(2);
    expect(store.getState().memberIdsByGuild["guild-1"]).toHaveLength(2);
  });

  it("updateMemberRole modifies role in membersById", () => {
    const members = [makeMember("user-1")];
    store.getState().fetchMembers("guild-1", members);
    const key = store.getState().memberIdsByGuild["guild-1"][0];
    store.getState().updateMemberRole(key, "admin");
    expect(store.getState().membersById[key].roles).toContain("admin");
  });
});
