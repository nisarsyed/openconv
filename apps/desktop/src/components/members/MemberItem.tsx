import type { User, Member, PresenceStatus } from "../../types";
import { Avatar } from "../ui/Avatar";
import { StatusDot } from "../ui/StatusDot";

export interface MemberItemProps {
  user: User;
  member: Member;
  presence: PresenceStatus;
  roleColor: string;
  onClick: (e: React.MouseEvent) => void;
}

export function MemberItem({ user, member, presence, roleColor, onClick }: MemberItemProps) {
  const displayName = member.nickname ?? user.displayName;
  const isOffline = presence === "offline";

  return (
    <button
      data-testid="member-item"
      data-user-id={user.id}
      className={`flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left transition-all duration-150 hover:bg-[var(--interactive-hover)] ${isOffline ? "opacity-40" : ""}`}
      onClick={onClick}
    >
      <div className="relative shrink-0">
        <Avatar src={user.avatarUrl} name={displayName} size="md" />
        <span className="absolute -bottom-0.5 -right-0.5">
          <StatusDot status={presence} size="sm" />
        </span>
      </div>
      <span
        className="truncate text-sm font-medium"
        style={{ color: roleColor }}
      >
        {displayName}
      </span>
    </button>
  );
}
