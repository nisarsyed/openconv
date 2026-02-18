import { Tooltip } from "../ui/Tooltip";
import { Badge } from "../ui/Badge";
import type { Guild } from "../../types";

interface GuildIconProps {
  guild: Guild;
  isSelected: boolean;
  isUnread: boolean;
  mentionCount: number;
  onClick: () => void;
}

export function GuildIcon({
  guild,
  isSelected,
  isUnread,
  mentionCount,
  onClick,
}: GuildIconProps) {
  const initial = guild.name.charAt(0).toUpperCase();

  return (
    <div data-testid={`guild-icon-${guild.id}`} className="relative flex w-full items-center justify-center mb-1.5">
      {/* Pill indicator — only visible when selected or unread */}
      {(isSelected || isUnread) && (
        <div
          data-testid="guild-pill"
          className={`absolute left-0 w-1 rounded-r-full bg-[var(--text-primary)] ${
            isSelected ? "h-8" : "h-2"
          }`}
        />
      )}

      <Tooltip content={guild.name} position="right">
        <button
          aria-label={guild.name}
          onClick={onClick}
          className={`relative flex h-10 w-10 items-center justify-center rounded-xl text-sm font-semibold transition-colors duration-150 ${
            isSelected
              ? "bg-[var(--bg-accent)] text-white"
              : "bg-[var(--bg-secondary)] text-[var(--text-primary)] hover:bg-[var(--bg-accent)] hover:text-white"
          }`}
        >
          {guild.iconUrl ? (
            <img
              src={guild.iconUrl}
              alt={guild.name}
              className="h-full w-full rounded-[inherit] object-cover"
            />
          ) : (
            initial
          )}

          {mentionCount > 0 && (
            <span className="absolute -bottom-1 -right-1">
              <Badge count={mentionCount} />
            </span>
          )}
        </button>
      </Tooltip>
    </div>
  );
}
