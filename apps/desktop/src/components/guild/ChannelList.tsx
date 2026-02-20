import { useMemo } from "react";
import { useParams } from "react-router";
import { useAppStore } from "../../store";
import { ChannelCategory } from "./ChannelCategory";
import type { Channel } from "../../types";

export function ChannelList() {
  const { guildId, channelId } = useParams<{
    guildId: string;
    channelId: string;
  }>();
  const channelsById = useAppStore((s) => s.channelsById);
  const channelIdsByGuild = useAppStore((s) => s.channelIdsByGuild);
  const unreadCountByChannel = useAppStore((s) => s.unreadCountByChannel);

  const guildChannels = useMemo(() => {
    if (!guildId) return [];
    const ids = channelIdsByGuild[guildId] ?? [];
    return ids.map((id) => channelsById[id]).filter(Boolean) as Channel[];
  }, [guildId, channelIdsByGuild, channelsById]);

  const unreadChannelIds = useMemo(() => {
    const set = new Set<string>();
    for (const ch of guildChannels) {
      if ((unreadCountByChannel[ch.id] ?? 0) > 0) {
        set.add(ch.id);
      }
    }
    return set;
  }, [guildChannels, unreadCountByChannel]);

  const grouped = useMemo(() => {
    const map = new Map<string, Channel[]>();
    for (const channel of guildChannels) {
      const cat = channel.category ?? "Uncategorized";
      if (!map.has(cat)) map.set(cat, []);
      map.get(cat)!.push(channel);
    }
    return map;
  }, [guildChannels]);

  return (
    <nav className="flex-1 overflow-y-auto py-2" aria-label="Channel list">
      {Array.from(grouped.entries()).map(([category, channels]) => (
        <ChannelCategory
          key={category}
          name={category}
          channels={channels}
          selectedChannelId={channelId}
          unreadChannelIds={unreadChannelIds}
        />
      ))}
    </nav>
  );
}
