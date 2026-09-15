import type { Channel } from "../types";
import type { SliceCreator } from "./index";

export interface ChannelsSlice {
  channelsById: Record<string, Channel>;
  channelIdsByGuild: Record<string, string[]>;
  lastVisitedChannelByGuild: Record<string, string>;
  setChannels: (channels: Channel[]) => void;
  setLastVisitedChannel: (guildId: string, channelId: string) => void;
  createChannel: (
    guildId: string,
    name: string,
    type: "text" | "voice",
    category: string | null,
  ) => void;
  deleteChannel: (id: string) => void;
}

export const createChannelsSlice: SliceCreator<ChannelsSlice> = (set, get) => ({
  channelsById: {},
  channelIdsByGuild: {},
  lastVisitedChannelByGuild: {},

  setChannels: (channels) =>
    set((draft) => {
      draft.channelsById = {};
      draft.channelIdsByGuild = {};
      // Sort by position before inserting
      const sorted = [...channels].sort((a, b) => a.position - b.position);
      for (const ch of sorted) {
        draft.channelsById[ch.id] = ch;
        if (!draft.channelIdsByGuild[ch.guildId]) {
          draft.channelIdsByGuild[ch.guildId] = [];
        }
        draft.channelIdsByGuild[ch.guildId].push(ch.id);
      }
    }),

  setLastVisitedChannel: (guildId, channelId) =>
    set((draft) => {
      draft.lastVisitedChannelByGuild[guildId] = channelId;
    }),

  createChannel: (guildId, name, type, category) =>
    set((draft) => {
      const id = crypto.randomUUID();
      const existing = draft.channelIdsByGuild[guildId] ?? [];
      const position = existing.length;
      const channel: Channel = {
        id,
        guildId,
        name,
        channelType: type,
        position,
        category,
      };
      draft.channelsById[id] = channel;
      if (!draft.channelIdsByGuild[guildId]) {
        draft.channelIdsByGuild[guildId] = [];
      }
      draft.channelIdsByGuild[guildId].push(id);
    }),

  deleteChannel: (id) =>
    set((draft) => {
      const channel = draft.channelsById[id];
      if (!channel) return;
      const guildId = channel.guildId;
      delete draft.channelsById[id];
      if (draft.channelIdsByGuild[guildId]) {
        draft.channelIdsByGuild[guildId] = draft.channelIdsByGuild[
          guildId
        ].filter((cid) => cid !== id);
      }
      if (draft.lastVisitedChannelByGuild[guildId] === id) {
        delete draft.lastVisitedChannelByGuild[guildId];
      }
    }),
});
