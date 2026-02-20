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
    <div
      data-testid={`guild-icon-${guild.id}`}
      className="relative mb-2 flex w-full items-center justify-center"
    >
      {/* Pill indicator */}
      {(isSelected || isUnread) && (
        <div
          data-testid="guild-pill"
          className={`absolute left-0 w-1 rounded-r-full bg-[var(--text-primary)] transition-all duration-200 ${
            isSelected ? "h-9" : "h-2"
          }`}
        />
      )}

      <Tooltip content={guild.name} position="right">
        <button
          aria-label={guild.name}
          onClick={onClick}
          className={`relative flex h-11 w-11 items-center justify-center text-sm font-semibold transition-all duration-200 ${
            isSelected
              ? "rounded-xl bg-[var(--bg-accent)] text-[var(--text-on-accent)] shadow-[var(--shadow-glow)]"
              : "rounded-2xl bg-[var(--bg-secondary)] text-[var(--text-primary)] hover:rounded-xl hover:bg-[var(--bg-accent)] hover:text-[var(--text-on-accent)] hover:shadow-[var(--shadow-glow)]"
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
            <span className="absolute -right-1 -bottom-1">
              <Badge count={mentionCount} />
            </span>
          )}
        </button>
      </Tooltip>
    </div>
  );
}
