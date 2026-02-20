import { useAppStore } from "../store";
import type { Message, User } from "../types";
import {
  mockUsers,
  mockGuilds,
  mockChannels,
  mockMessages,
  mockMembers,
  mockRoles,
  mockPresence,
} from "./data";

const INITIAL_PAGE_SIZE = 20;

export function seedStores(): void {
  // Build guildsById and guildIds
  const guildsById: Record<string, (typeof mockGuilds)[number]> = {};
  const guildIds: string[] = [];
  for (const guild of mockGuilds) {
    guildsById[guild.id] = guild;
    guildIds.push(guild.id);
  }

  // Build channelsById and channelIdsByGuild
  const channelsById: Record<string, (typeof mockChannels)[number]> = {};
  const channelIdsByGuild: Record<string, string[]> = {};
  for (const channel of mockChannels) {
    channelsById[channel.id] = channel;
    if (!channelIdsByGuild[channel.guildId]) {
      channelIdsByGuild[channel.guildId] = [];
    }
    channelIdsByGuild[channel.guildId].push(channel.id);
  }
  // Sort channels by position within each guild
  for (const guildId of Object.keys(channelIdsByGuild)) {
    channelIdsByGuild[guildId].sort(
      (a, b) => channelsById[a].position - channelsById[b].position,
    );
  }

  // Build messagesById (all messages for lookup) and messageIdsByChannel (only last 20)
  const messagesById: Record<string, Message> = {};
  const messageIdsByChannel: Record<string, string[]> = {};
  const hasMore: Record<string, boolean> = {};

  // Group messages by channel
  const messagesByChannel: Record<string, Message[]> = {};
  for (const msg of mockMessages) {
    messagesById[msg.id] = msg;
    if (!messagesByChannel[msg.channelId]) {
      messagesByChannel[msg.channelId] = [];
    }
    messagesByChannel[msg.channelId].push(msg);
  }

  // For each text channel, sort by createdAt and take last INITIAL_PAGE_SIZE
  for (const [channelId, msgs] of Object.entries(messagesByChannel)) {
    msgs.sort(
      (a, b) => new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime(),
    );
    const recentIds = msgs.slice(-INITIAL_PAGE_SIZE).map((m) => m.id);
    messageIdsByChannel[channelId] = recentIds;
    hasMore[channelId] = msgs.length > INITIAL_PAGE_SIZE;
  }

  // Build membersById (composite key) and memberIdsByGuild
  const membersById: Record<string, (typeof mockMembers)[number]> = {};
  const memberIdsByGuild: Record<string, string[]> = {};
  for (const member of mockMembers) {
    const key = `${member.guildId}-${member.userId}`;
    membersById[key] = member;
    if (!memberIdsByGuild[member.guildId]) {
      memberIdsByGuild[member.guildId] = [];
    }
    memberIdsByGuild[member.guildId].push(key);
  }

  // Build rolesById and roleIdsByGuild
  const rolesById: Record<string, (typeof mockRoles)[number]> = {};
  const roleIdsByGuild: Record<string, string[]> = {};
  for (const role of mockRoles) {
    rolesById[role.id] = role;
    if (!roleIdsByGuild[role.guildId]) {
      roleIdsByGuild[role.guildId] = [];
    }
    roleIdsByGuild[role.guildId].push(role.id);
  }

  // Set lastVisitedChannelByGuild for each guild (first text channel)
  const lastVisitedChannelByGuild: Record<string, string> = {};
  for (const guildId of guildIds) {
    const guildChannelIds = channelIdsByGuild[guildId] ?? [];
    const firstText = guildChannelIds.find(
      (cid) => channelsById[cid]?.channelType === "text",
    );
    if (firstText) {
      lastVisitedChannelByGuild[guildId] = firstText;
    }
  }

  // Build usersById
  const usersById: Record<string, User> = {};
  for (const user of mockUsers) {
    usersById[user.id] = user;
  }

  useAppStore.setState({
    guildsById,
    guildIds,
    usersById,
    channelsById,
    channelIdsByGuild,
    messagesById,
    messageIdsByChannel,
    hasMore,
    membersById,
    memberIdsByGuild,
    rolesById,
    roleIdsByGuild,
    presenceByUserId: { ...mockPresence },
    lastVisitedGuildId: guildIds[0] ?? null,
    lastVisitedChannelByGuild,
  });
}
