import { useState, useMemo, useCallback } from "react";
import { useParams } from "react-router";
import { useAppStore } from "../../store";
import type { Member, Role, PresenceStatus } from "../../types";
import { MemberGroup } from "../members/MemberGroup";
import { MemberItem } from "../members/MemberItem";
import { UserPopover } from "../members/UserPopover";

interface RoleGroup {
  role: Role;
  members: Member[];
}

const ONLINE_FALLBACK_ROLE: Role = {
  id: "__online__",
  guildId: "",
  name: "Online",
  color: "var(--text-primary)",
  position: -1,
};

function groupMembersByRole(
  memberKeys: string[],
  membersById: Record<string, Member>,
  rolesById: Record<string, Role>,
  presenceByUserId: Record<string, PresenceStatus>,
): RoleGroup[] {
  const groups = new Map<string, { role: Role; online: Member[]; offline: Member[] }>();

  for (const key of memberKeys) {
    const member = membersById[key];
    if (!member) continue;

    // Find highest role
    let highestRole: Role | null = null;
    for (const roleId of member.roles) {
      const role = rolesById[roleId];
      if (role && (!highestRole || role.position > highestRole.position)) {
        highestRole = role;
      }
    }

    // Fallback for members with no valid roles
    if (!highestRole) {
      highestRole = ONLINE_FALLBACK_ROLE;
    }

    const status = presenceByUserId[member.userId] ?? "offline";
    const isOnline = status !== "offline";

    if (!groups.has(highestRole.id)) {
      groups.set(highestRole.id, { role: highestRole, online: [], offline: [] });
    }
    const group = groups.get(highestRole.id)!;
    if (isOnline) {
      group.online.push(member);
    } else {
      group.offline.push(member);
    }
  }

  return [...groups.values()]
    .sort((a, b) => b.role.position - a.role.position)
    .map((g) => ({
      role: g.role,
      members: [...g.online, ...g.offline],
    }));
}

const EMPTY_KEYS: string[] = [];

export function MemberList() {
  const { guildId } = useParams<{ guildId: string }>();
  const memberListVisible = useAppStore((s) => s.memberListVisible);
  const memberKeys = useAppStore(
    (s) => (guildId ? s.memberIdsByGuild[guildId] : undefined) ?? EMPTY_KEYS,
  );
  const membersById = useAppStore((s) => s.membersById);
  const usersById = useAppStore((s) => s.usersById);
  const rolesById = useAppStore((s) => s.rolesById);
  const presenceByUserId = useAppStore((s) => s.presenceByUserId);

  const [popoverMemberId, setPopoverMemberId] = useState<string | null>(null);
  const [popoverAnchorRect, setPopoverAnchorRect] = useState<DOMRect | null>(null);

  const groups = useMemo(
    () => groupMembersByRole(memberKeys, membersById, rolesById, presenceByUserId),
    [memberKeys, membersById, rolesById, presenceByUserId],
  );

  const handlePopoverClose = useCallback(() => {
    setPopoverMemberId(null);
    setPopoverAnchorRect(null);
  }, []);

  if (!memberListVisible) return null;

  function handleMemberClick(memberKey: string, event: React.MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    setPopoverMemberId(memberKey);
    setPopoverAnchorRect(rect);
  }

  // Resolve popover data
  const popoverMember = popoverMemberId ? membersById[popoverMemberId] : null;
  const popoverUser = popoverMember ? usersById[popoverMember.userId] : null;
  const popoverRoles = popoverMember
    ? popoverMember.roles
        .map((rid) => rolesById[rid])
        .filter((r): r is Role => !!r)
        .sort((a, b) => b.position - a.position)
    : [];
  const popoverPresence = popoverMember
    ? presenceByUserId[popoverMember.userId] ?? "offline"
    : "offline";

  return (
    <div data-testid="member-list-content" className="h-full overflow-y-auto pt-2" style={{ scrollbarWidth: "none" }}>
      {groups.map((group) => (
        <MemberGroup key={group.role.id} role={group.role} count={group.members.length}>
          {group.members.map((member) => {
            const user = usersById[member.userId];
            if (!user) return null;
            const memberKey = `${member.guildId}-${member.userId}`;
            return (
              <MemberItem
                key={memberKey}
                user={user}
                member={member}
                presence={presenceByUserId[member.userId] ?? "offline"}
                roleColor={group.role.color}
                onClick={(e: React.MouseEvent) => handleMemberClick(memberKey, e)}
              />
            );
          })}
        </MemberGroup>
      ))}

      {popoverMember && popoverUser && (
        <UserPopover
          user={popoverUser}
          member={popoverMember}
          roles={popoverRoles}
          presence={popoverPresence}
          onClose={handlePopoverClose}
          anchorRect={popoverAnchorRect}
        />
      )}
    </div>
  );
}
