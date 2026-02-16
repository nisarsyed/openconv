import { useParams, useNavigate } from "react-router";
import { useAppStore } from "../../store";
import { GuildIcon } from "../guild/GuildIcon";

export function GuildSidebar() {
  const navigate = useNavigate();
  const { guildId: selectedGuildId } = useParams<{ guildId: string }>();

  const guildIds = useAppStore((s) => s.guildIds);
  const guildsById = useAppStore((s) => s.guildsById);
  const channelIdsByGuild = useAppStore((s) => s.channelIdsByGuild);
  const channelsById = useAppStore((s) => s.channelsById);
  const lastVisitedChannelByGuild = useAppStore((s) => s.lastVisitedChannelByGuild);
  const unreadCountByChannel = useAppStore((s) => s.unreadCountByChannel);
  const mentionCountByGuild = useAppStore((s) => s.mentionCountByGuild);
  const setLastVisitedGuild = useAppStore((s) => s.setLastVisitedGuild);
  const openModal = useAppStore((s) => s.openModal);

  const isGuildUnread = (gId: string) => {
    const channelIds = channelIdsByGuild[gId] ?? [];
    return channelIds.some((cid) => (unreadCountByChannel[cid] ?? 0) > 0);
  };

  const handleGuildClick = (gId: string) => {
    let channelId = lastVisitedChannelByGuild[gId];
    if (!channelId) {
      const guildChannelIds = channelIdsByGuild[gId] ?? [];
      channelId =
        guildChannelIds.find((cid) => channelsById[cid]?.channelType === "text") ??
        guildChannelIds[0];
    }
    if (channelId) {
      navigate(`/app/guild/${gId}/channel/${channelId}`);
    }
    setLastVisitedGuild(gId);
  };

  return (
    <nav
      data-testid="guild-sidebar"
      className="flex flex-col items-center bg-[var(--bg-tertiary)] py-3 overflow-y-auto scrollbar-none"
      style={{ width: 72 }}
      aria-label="Guilds"
    >
      {/* Home button */}
      <button
        aria-label="Home"
        disabled
        className="mb-2 ml-3 flex h-12 w-12 items-center justify-center rounded-full bg-[var(--bg-secondary)] text-[var(--text-muted)] opacity-50 cursor-not-allowed"
      >
        <svg className="h-6 w-6" viewBox="0 0 24 24" fill="currentColor">
          <path d="M3 9l9-7 9 7v11a2 2 0 01-2 2H5a2 2 0 01-2-2z" />
          <polyline points="9 22 9 12 15 12 15 22" />
        </svg>
      </button>

      {/* Separator */}
      <div className="mx-auto mb-2 h-0.5 w-8 rounded-full bg-[var(--border-subtle)]" />

      {/* Guild icons */}
      {guildIds.map((gId) => {
        const guild = guildsById[gId];
        if (!guild) return null;
        return (
          <GuildIcon
            key={gId}
            guild={guild}
            isSelected={gId === selectedGuildId}
            isUnread={isGuildUnread(gId)}
            mentionCount={mentionCountByGuild[gId] ?? 0}
            onClick={() => handleGuildClick(gId)}
          />
        );
      })}

      {/* Add guild button */}
      <button
        aria-label="Create guild"
        onClick={() => openModal("createGuild")}
        className="ml-3 mt-2 flex h-12 w-12 items-center justify-center rounded-full bg-[var(--bg-secondary)] text-green-500 transition-all duration-200 hover:rounded-2xl hover:bg-green-500 hover:text-white"
      >
        <svg className="h-6 w-6" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
          <line x1="12" y1="5" x2="12" y2="19" />
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
      </button>
    </nav>
  );
}
