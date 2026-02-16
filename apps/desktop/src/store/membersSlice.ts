import type { Member, Role } from "../types";
import type { SliceCreator } from "./index";

export interface MembersSlice {
  membersById: Record<string, Member>;
  memberIdsByGuild: Record<string, string[]>;
  rolesById: Record<string, Role>;
  roleIdsByGuild: Record<string, string[]>;
  fetchMembers: (guildId: string, members: Member[]) => void;
  updateMemberRole: (memberId: string, roleId: string) => void;
}

export const createMembersSlice: SliceCreator<MembersSlice> = (set) => ({
  membersById: {},
  memberIdsByGuild: {},
  rolesById: {},
  roleIdsByGuild: {},

  fetchMembers: (guildId, members) =>
    set((draft) => {
      const ids: string[] = [];
      for (const member of members) {
        const key = `${guildId}-${member.userId}`;
        draft.membersById[key] = member;
        ids.push(key);
      }
      draft.memberIdsByGuild[guildId] = ids;
    }),

  updateMemberRole: (memberId, roleId) =>
    set((draft) => {
      const member = draft.membersById[memberId];
      if (member && !member.roles.includes(roleId)) {
        member.roles.push(roleId);
      }
    }),
});
