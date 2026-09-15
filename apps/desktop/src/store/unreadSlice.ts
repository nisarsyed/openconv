import { invoke } from "@tauri-apps/api/core";
import type { SliceCreator } from "./index";

export interface ReadPositionInfo {
  channelId: string;
  lastReadMessageId: string;
  lastReadCreatedAt: number;
  unreadCount: number;
}

export interface UnreadSlice {
  lastReadByChannel: Record<string, string>;
  lastReadCreatedAtByChannel: Record<string, number>;
  unreadCountByChannel: Record<string, number>;
  mentionCountByGuild: Record<string, number>;
  markChannelRead: (
    channelId: string,
    lastMessageId: string,
    lastMessageCreatedAt?: number,
  ) => void;
  incrementUnread: (channelId: string) => void;
  incrementMention: (guildId: string) => void;
  resetGuildMentions: (guildId: string) => void;
  initializeReadPositions: (positions: ReadPositionInfo[]) => void;
  setUnreadCount: (channelId: string, count: number) => void;
}

export const createUnreadSlice: SliceCreator<UnreadSlice> = (set) => ({
  lastReadByChannel: {},
  lastReadCreatedAtByChannel: {},
  unreadCountByChannel: {},
  mentionCountByGuild: {},

  markChannelRead: (channelId, lastMessageId, lastMessageCreatedAt) => {
    set((draft) => {
      draft.lastReadByChannel[channelId] = lastMessageId;
      draft.unreadCountByChannel[channelId] = 0;
      if (lastMessageCreatedAt !== undefined) {
        draft.lastReadCreatedAtByChannel[channelId] = lastMessageCreatedAt;
      }
    });
    // Fire-and-forget: update backend cache (convert ms to seconds for Rust)
    const createdAtSec = Math.floor(
      (lastMessageCreatedAt ?? Date.now()) / 1000,
    );
    invoke("mark_channel_read", {
      channelId,
      lastMessageId,
      lastMessageCreatedAt: createdAtSec,
    }).catch((e: unknown) => {
      if (import.meta.env.DEV) console.warn("mark_channel_read failed:", e);
    });
  },

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

  initializeReadPositions: (positions) =>
    set((draft) => {
      for (const pos of positions) {
        draft.lastReadByChannel[pos.channelId] = pos.lastReadMessageId;
        draft.lastReadCreatedAtByChannel[pos.channelId] = pos.lastReadCreatedAt;
        draft.unreadCountByChannel[pos.channelId] = pos.unreadCount;
      }
    }),

  setUnreadCount: (channelId, count) =>
    set((draft) => {
      draft.unreadCountByChannel[channelId] = count;
    }),
});
