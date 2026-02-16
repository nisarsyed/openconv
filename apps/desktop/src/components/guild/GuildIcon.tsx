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
    <div data-testid={`guild-icon-${guild.id}`} className="relative flex items-center justify-center mb-2 group">
      {/* Pill indicator */}
      <div
        data-testid="guild-pill"
        className={`absolute left-0 w-1 rounded-r-full bg-white transition-all duration-200 ${
          isSelected
            ? "h-10"
            : isUnread
              ? "h-2"
              : "h-0 group-hover:h-5"
        }`}
      />

      {/* Icon */}
      <Tooltip content={guild.name} position="right">
        <button
          aria-label={guild.name}
          onClick={onClick}
          className={`relative ml-3 flex h-12 w-12 items-center justify-center text-lg font-semibold text-[var(--text-primary)] transition-all duration-200 ${
            isSelected
              ? "rounded-2xl bg-[var(--bg-accent)] text-white"
              : "rounded-full bg-[var(--bg-tertiary)] hover:rounded-2xl hover:bg-[var(--bg-accent)] hover:text-white"
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

          {/* Mention badge */}
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
