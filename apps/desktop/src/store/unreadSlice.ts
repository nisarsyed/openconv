import type { SliceCreator } from "./index";

export interface UnreadSlice {
  lastReadByChannel: Record<string, string>;
  unreadCountByChannel: Record<string, number>;
  mentionCountByGuild: Record<string, number>;
  markChannelRead: (channelId: string, lastMessageId: string) => void;
  incrementUnread: (channelId: string) => void;
  incrementMention: (guildId: string) => void;
  resetGuildMentions: (guildId: string) => void;
}

export const createUnreadSlice: SliceCreator<UnreadSlice> = (set) => ({
  lastReadByChannel: {},
  unreadCountByChannel: {},
  mentionCountByGuild: {},

  markChannelRead: (channelId, lastMessageId) =>
    set((draft) => {
      draft.lastReadByChannel[channelId] = lastMessageId;
      draft.unreadCountByChannel[channelId] = 0;
    }),

  incrementUnread: (channelId) =>
    set((draft) => {
      draft.unreadCountByChannel[channelId] =
        (draft.unreadCountByChannel[channelId] ?? 0) + 1;
    }),

  incrementMention: (guildId) =>
    set((draft) => {
      draft.mentionCountByGuild[guildId] =
        (draft.mentionCountByGuild[guildId] ?? 0) + 1;
    }),

  resetGuildMentions: (guildId) =>
    set((draft) => {
      draft.mentionCountByGuild[guildId] = 0;
    }),
});
