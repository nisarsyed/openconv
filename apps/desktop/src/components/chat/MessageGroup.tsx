import type { Message as MessageType } from "../../types";
import { useAppStore } from "../../store";
import { useParams } from "react-router";
import { Avatar } from "../ui/Avatar";
import { Message } from "./Message";

interface MessageGroupProps {
  senderId: string;
  messages: MessageType[];
}

function formatTimestamp(iso: string): string {
  const date = new Date(iso);
  const now = new Date();
  const isToday =
    date.getDate() === now.getDate() &&
    date.getMonth() === now.getMonth() &&
    date.getFullYear() === now.getFullYear();

  const time = date.toLocaleTimeString("en-US", {
    hour: "numeric",
    minute: "2-digit",
  });

  if (isToday) return `Today at ${time}`;
  return `${date.toLocaleDateString("en-US", { month: "2-digit", day: "2-digit", year: "numeric" })} ${time}`;
}

export function MessageGroup({ senderId, messages }: MessageGroupProps) {
  const { guildId } = useParams<{ guildId: string }>();
  const user = useAppStore((s) => s.usersById[senderId]);
  const currentUserId = useAppStore((s) => s.currentUser?.id);
  const member = useAppStore((s) =>
    guildId ? s.membersById[`${guildId}-${senderId}`] : undefined,
  );
  const nameColor = useAppStore((s) => {
    if (!guildId) return undefined;
    const m = s.membersById[`${guildId}-${senderId}`];
    if (!m?.roles) return undefined;
    let highest: { position: number; color: string } | null = null;
    for (const rid of m.roles) {
      const role = s.rolesById[rid];
      if (role && (!highest || role.position > highest.position)) {
        highest = { position: role.position, color: role.color };
      }
    }
    return highest?.color;
  });

  const displayName = member?.nickname ?? user?.displayName ?? "Unknown User";
  const avatarUrl = user?.avatarUrl ?? null;
  const isOwn = senderId === currentUserId;

  return (
    <div className="mt-4 first:mt-0" data-testid="message-group">
      <div className="flex gap-3 px-4 pt-1.5">
        <div className="w-10 shrink-0 pt-0.5">
          <Avatar src={avatarUrl} name={displayName} size="md" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span
              className="text-sm font-semibold hover:underline cursor-pointer tracking-[-0.01em]"
              style={nameColor ? { color: nameColor } : undefined}
              data-testid="message-author"
            >
              {displayName}
            </span>
            <span className="text-[11px] text-[var(--text-muted)]">
              {formatTimestamp(messages[0].createdAt)}
            </span>
          </div>
        </div>
      </div>

      {messages.map((msg) => (
        <div key={msg.id} className="pl-[52px]">
          <Message message={msg} isOwn={isOwn} />
        </div>
      ))}
    </div>
  );
}
