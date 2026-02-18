import { useNavigate, useParams } from "react-router";
import type { Channel } from "../../types";
import { useAppStore } from "../../store";

interface ChannelItemProps {
  channel: Channel;
  isSelected: boolean;
  isUnread: boolean;
}

export function ChannelItem({ channel, isSelected, isUnread }: ChannelItemProps) {
  const navigate = useNavigate();
  const { guildId } = useParams<{ guildId: string }>();
  const setLastVisitedChannel = useAppStore((s) => s.setLastVisitedChannel);
  const markChannelRead = useAppStore((s) => s.markChannelRead);

  const handleClick = () => {
    if (!guildId) return;
    navigate(`/app/guild/${guildId}/channel/${channel.id}`);
    setLastVisitedChannel(guildId, channel.id);

    const channelMessages = useAppStore.getState().messageIdsByChannel[channel.id];
    if (channelMessages?.length) {
      markChannelRead(channel.id, channelMessages[channelMessages.length - 1]);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      handleClick();
    }
  };

  const icon = channel.channelType === "voice" ? "\u{1F50A}" : "#";

  return (
    <li
      data-testid={`channel-item-${channel.id}`}
      tabIndex={0}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      className={`group flex items-center gap-1.5 rounded px-2 py-1 mx-2 cursor-pointer text-sm transition-colors ${
        isSelected
          ? "bg-[var(--interactive-active)] text-[var(--text-primary)]"
          : "text-[var(--text-muted)] hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)]"
      }`}
    >
      {/* Unread dot indicator */}
      {isUnread && !isSelected && (
        <span
          data-testid="unread-dot"
          className="h-2 w-2 shrink-0 rounded-full bg-[var(--text-primary)]"
        />
      )}
      <span className="text-base opacity-60">{icon}</span>
      <span className={isUnread && !isSelected ? "font-bold text-[var(--text-primary)]" : ""}>
        {channel.name}
      </span>
      {/* Gear icon on hover */}
      <button
        aria-label={`${channel.name} settings`}
        className="ml-auto hidden rounded p-0.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] group-hover:block"
        onClick={(e) => {
          e.stopPropagation();
          // Channel settings - placeholder for future implementation
        }}
      >
        <svg className="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor">
          <path
            fillRule="evenodd"
            d="M7.84 1.804A1 1 0 018.82 1h2.36a1 1 0 01.98.804l.331 1.652a6.993 6.993 0 011.929 1.115l1.598-.54a1 1 0 011.186.447l1.18 2.044a1 1 0 01-.205 1.251l-1.267 1.113a7.047 7.047 0 010 2.228l1.267 1.113a1 1 0 01.206 1.25l-1.18 2.045a1 1 0 01-1.187.447l-1.598-.54a6.993 6.993 0 01-1.929 1.115l-.33 1.652a1 1 0 01-.98.804H8.82a1 1 0 01-.98-.804l-.331-1.652a6.993 6.993 0 01-1.929-1.115l-1.598.54a1 1 0 01-1.186-.447l-1.18-2.044a1 1 0 01.205-1.251l1.267-1.114a7.05 7.05 0 010-2.227L1.821 7.773a1 1 0 01-.206-1.25l1.18-2.045a1 1 0 011.187-.447l1.598.54A6.993 6.993 0 017.51 3.456l.33-1.652zM10 13a3 3 0 100-6 3 3 0 000 6z"
            clipRule="evenodd"
          />
        </svg>
      </button>
    </li>
  );
}
