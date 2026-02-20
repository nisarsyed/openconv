import type { Message, Guild, Channel, Member, User, FileAttachment } from "../types";
import { useAppStore } from "../store";
import { mockUsers, mockGuilds, mockChannels, mockMessages, mockMembers } from "./data";

function delay(minMs: number, maxMs: number): Promise<void> {
  const ms = minMs + Math.random() * (maxMs - minMs);
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function mockFetchMessages(
  channelId: string,
  before?: string,
  limit: number = 20,
): Promise<Message[]> {
  await delay(200, 500);

  const channelMessages = mockMessages
    .filter((m) => m.channelId === channelId)
    .sort((a, b) => new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime());

  let filtered = channelMessages;
  if (before) {
    const beforeTime = new Date(before).getTime();
    filtered = channelMessages.filter(
      (m) => new Date(m.createdAt).getTime() < beforeTime,
    );
  }

  // Take the last `limit` messages (newest) from the filtered set
  const page = filtered.slice(-limit);
  // Return newest first
  return page.reverse();
}

export async function mockSendMessage(
  channelId: string,
  content: string,
  attachments: FileAttachment[] = [],
): Promise<Message> {
  if (Math.random() < 0.05) {
    await delay(100, 300);
    throw new Error("Failed to send message");
  }

  const id = crypto.randomUUID();
  const senderId = useAppStore.getState().currentUser?.id ?? mockUsers[0].id;
  const message: Message = {
    id,
    channelId,
    senderId,
    content,
    encryptedContent: content,
    nonce: `mock-nonce-${id}`,
    createdAt: new Date().toISOString(),
    editedAt: null,
    attachments,
  };

  // Add optimistically to store
  useAppStore.getState().addMessage(channelId, message);

  await delay(100, 300);
  return message;
}

export async function mockFetchGuilds(): Promise<Guild[]> {
  await delay(300, 300);
  return [...mockGuilds];
}

export async function mockFetchMembers(guildId: string): Promise<Member[]> {
  await delay(200, 200);
  return mockMembers.filter((m) => m.guildId === guildId);
}

export async function mockLogin(email: string): Promise<{
  user: User;
  keyPair: { publicKey: string; privateKey: string };
  token: string;
}> {
  await delay(500, 500);

  const user = mockUsers.find(
    (u) => u.email.toLowerCase() === email.toLowerCase(),
  ) ?? mockUsers[0];

  const id = crypto.randomUUID();
  return {
    user: { ...user },
    keyPair: {
      publicKey: `mock-public-key-${id}`,
      privateKey: `mock-private-key-${id}`,
    },
    token: `mock-token-${id}`,
  };
}

export async function mockRegister(
  email: string,
  displayName: string,
): Promise<{
  user: User;
  keyPair: { publicKey: string; privateKey: string };
  token: string;
}> {
  await delay(500, 500);

  const id = crypto.randomUUID();
  return {
    user: {
      id,
      displayName,
      email,
      avatarUrl: null,
    },
    keyPair: {
      publicKey: `mock-public-key-${id}`,
      privateKey: `mock-private-key-${id}`,
    },
    token: `mock-token-${id}`,
  };
}

export async function mockCreateGuild(name: string): Promise<Guild> {
  await delay(300, 300);

  const id = crypto.randomUUID();
  const ownerId = useAppStore.getState().currentUser?.id ?? mockUsers[0].id;
  return {
    id,
    name,
    ownerId,
    iconUrl: null,
  };
}

export async function mockCreateChannel(
  guildId: string,
  name: string,
  type: "text" | "voice",
  category: string | null,
): Promise<Channel> {
  await delay(200, 200);

  const id = crypto.randomUUID();
  const existing = mockChannels.filter((c) => c.guildId === guildId);
  return {
    id,
    guildId,
    name,
    channelType: type,
    position: existing.length,
    category,
  };
}
