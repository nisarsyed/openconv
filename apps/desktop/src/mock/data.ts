import type {
  User,
  Guild,
  Channel,
  Message,
  Role,
  Member,
  FileAttachment,
  PresenceStatus,
} from "../types";

// --- Users (12) ---

export const mockUsers: User[] = [
  {
    id: "a1b2c3d4-0001-4000-8000-000000000001",
    displayName: "Alice Chen",
    email: "alice@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-0002-4000-8000-000000000002",
    displayName: "Bob Martinez",
    email: "bob@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-0003-4000-8000-000000000003",
    displayName: "Charlie Kim",
    email: "charlie@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-0004-4000-8000-000000000004",
    displayName: "Diana Okafor",
    email: "diana@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-0005-4000-8000-000000000005",
    displayName: "Ethan Nakamura",
    email: "ethan@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-0006-4000-8000-000000000006",
    displayName: "Fiona Walsh",
    email: "fiona@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-0007-4000-8000-000000000007",
    displayName: "George Patel",
    email: "george@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-0008-4000-8000-000000000008",
    displayName: "Hannah Berg",
    email: "hannah@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-0009-4000-8000-000000000009",
    displayName: "Isaac Torres",
    email: "isaac@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-000a-4000-8000-000000000010",
    displayName: "Julia Sato",
    email: "julia@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-000b-4000-8000-000000000011",
    displayName: "Kevin Osei",
    email: "kevin@example.com",
    avatarUrl: null,
  },
  {
    id: "a1b2c3d4-000c-4000-8000-000000000012",
    displayName: "Lena Volkov",
    email: "lena@example.com",
    avatarUrl: null,
  },
];

// --- Guilds (4) ---

export const mockGuilds: Guild[] = [
  {
    id: "b1000000-0001-4000-8000-000000000001",
    name: "OpenConv Dev",
    ownerId: mockUsers[0].id,
    iconUrl: null,
  },
  {
    id: "b1000000-0002-4000-8000-000000000002",
    name: "Design Team",
    ownerId: mockUsers[3].id,
    iconUrl: null,
  },
  {
    id: "b1000000-0003-4000-8000-000000000003",
    name: "Gaming Lounge",
    ownerId: mockUsers[6].id,
    iconUrl: null,
  },
  {
    id: "b1000000-0004-4000-8000-000000000004",
    name: "Music Fans",
    ownerId: mockUsers[9].id,
    iconUrl: null,
  },
];

// --- Channels (5-8 per guild) ---

export const mockChannels: Channel[] = [
  // OpenConv Dev (7 channels)
  {
    id: "c1000000-0101-4000-8000-000000000001",
    guildId: mockGuilds[0].id,
    name: "general",
    channelType: "text",
    position: 0,
    category: "General",
  },
  {
    id: "c1000000-0102-4000-8000-000000000002",
    guildId: mockGuilds[0].id,
    name: "announcements",
    channelType: "text",
    position: 1,
    category: "General",
  },
  {
    id: "c1000000-0103-4000-8000-000000000003",
    guildId: mockGuilds[0].id,
    name: "frontend",
    channelType: "text",
    position: 2,
    category: "Development",
  },
  {
    id: "c1000000-0104-4000-8000-000000000004",
    guildId: mockGuilds[0].id,
    name: "backend",
    channelType: "text",
    position: 3,
    category: "Development",
  },
  {
    id: "c1000000-0105-4000-8000-000000000005",
    guildId: mockGuilds[0].id,
    name: "code-review",
    channelType: "text",
    position: 4,
    category: "Development",
  },
  {
    id: "c1000000-0106-4000-8000-000000000006",
    guildId: mockGuilds[0].id,
    name: "General Voice",
    channelType: "voice",
    position: 5,
    category: "Voice",
  },
  {
    id: "c1000000-0107-4000-8000-000000000007",
    guildId: mockGuilds[0].id,
    name: "Pair Programming",
    channelType: "voice",
    position: 6,
    category: "Voice",
  },

  // Design Team (6 channels)
  {
    id: "c1000000-0201-4000-8000-000000000008",
    guildId: mockGuilds[1].id,
    name: "general",
    channelType: "text",
    position: 0,
    category: "General",
  },
  {
    id: "c1000000-0202-4000-8000-000000000009",
    guildId: mockGuilds[1].id,
    name: "inspiration",
    channelType: "text",
    position: 1,
    category: "General",
  },
  {
    id: "c1000000-0203-4000-8000-000000000010",
    guildId: mockGuilds[1].id,
    name: "ui-ux",
    channelType: "text",
    position: 2,
    category: "Design",
  },
  {
    id: "c1000000-0204-4000-8000-000000000011",
    guildId: mockGuilds[1].id,
    name: "branding",
    channelType: "text",
    position: 3,
    category: "Design",
  },
  {
    id: "c1000000-0205-4000-8000-000000000012",
    guildId: mockGuilds[1].id,
    name: "feedback",
    channelType: "text",
    position: 4,
    category: "Design",
  },
  {
    id: "c1000000-0206-4000-8000-000000000013",
    guildId: mockGuilds[1].id,
    name: "Design Call",
    channelType: "voice",
    position: 5,
    category: "Voice",
  },

  // Gaming Lounge (6 channels)
  {
    id: "c1000000-0301-4000-8000-000000000014",
    guildId: mockGuilds[2].id,
    name: "general",
    channelType: "text",
    position: 0,
    category: "Chat",
  },
  {
    id: "c1000000-0302-4000-8000-000000000015",
    guildId: mockGuilds[2].id,
    name: "lfg",
    channelType: "text",
    position: 1,
    category: "Chat",
  },
  {
    id: "c1000000-0303-4000-8000-000000000016",
    guildId: mockGuilds[2].id,
    name: "memes",
    channelType: "text",
    position: 2,
    category: "Fun",
  },
  {
    id: "c1000000-0304-4000-8000-000000000017",
    guildId: mockGuilds[2].id,
    name: "clips",
    channelType: "text",
    position: 3,
    category: "Fun",
  },
  {
    id: "c1000000-0305-4000-8000-000000000018",
    guildId: mockGuilds[2].id,
    name: "strategy",
    channelType: "text",
    position: 4,
    category: "Games",
  },
  {
    id: "c1000000-0306-4000-8000-000000000019",
    guildId: mockGuilds[2].id,
    name: "Game Night",
    channelType: "voice",
    position: 5,
    category: "Voice",
  },

  // Music Fans (5 channels)
  {
    id: "c1000000-0401-4000-8000-000000000020",
    guildId: mockGuilds[3].id,
    name: "general",
    channelType: "text",
    position: 0,
    category: "Chat",
  },
  {
    id: "c1000000-0402-4000-8000-000000000021",
    guildId: mockGuilds[3].id,
    name: "recommendations",
    channelType: "text",
    position: 1,
    category: "Chat",
  },
  {
    id: "c1000000-0403-4000-8000-000000000022",
    guildId: mockGuilds[3].id,
    name: "production",
    channelType: "text",
    position: 2,
    category: "Music",
  },
  {
    id: "c1000000-0404-4000-8000-000000000023",
    guildId: mockGuilds[3].id,
    name: "vinyl-corner",
    channelType: "text",
    position: 3,
    category: "Music",
  },
  {
    id: "c1000000-0405-4000-8000-000000000024",
    guildId: mockGuilds[3].id,
    name: "Listening Party",
    channelType: "voice",
    position: 4,
    category: "Voice",
  },
];

// --- Roles (3 per guild) ---

export const mockRoles: Role[] = mockGuilds.flatMap((guild, gi) => [
  {
    id: `r1000000-${String(gi + 1).padStart(2, "0")}01-4000-8000-000000000001`,
    guildId: guild.id,
    name: "Admin",
    color: "#e74c3c",
    position: 2,
  },
  {
    id: `r1000000-${String(gi + 1).padStart(2, "0")}02-4000-8000-000000000002`,
    guildId: guild.id,
    name: "Moderator",
    color: "#e67e22",
    position: 1,
  },
  {
    id: `r1000000-${String(gi + 1).padStart(2, "0")}03-4000-8000-000000000003`,
    guildId: guild.id,
    name: "Member",
    color: "#3498db",
    position: 0,
  },
]);

// --- Members ---

function getRolesForGuild(guildIndex: number): {
  admin: string;
  mod: string;
  member: string;
} {
  const base = guildIndex * 3;
  return {
    admin: mockRoles[base].id,
    mod: mockRoles[base + 1].id,
    member: mockRoles[base + 2].id,
  };
}

const g0 = getRolesForGuild(0);
const g1 = getRolesForGuild(1);
const g2 = getRolesForGuild(2);
const g3 = getRolesForGuild(3);

export const mockMembers: Member[] = [
  // OpenConv Dev - all 12 users are members
  ...mockUsers.map((u, i) => ({
    userId: u.id,
    guildId: mockGuilds[0].id,
    nickname: null,
    roles: i === 0 ? [g0.admin] : i < 3 ? [g0.mod] : [g0.member],
    joinedAt: new Date(Date.now() - (12 - i) * 86400000 * 7).toISOString(),
  })),
  // Design Team - users 0-7
  ...mockUsers.slice(0, 8).map((u, i) => ({
    userId: u.id,
    guildId: mockGuilds[1].id,
    nickname: null,
    roles: i === 3 ? [g1.admin] : i < 2 ? [g1.mod] : [g1.member],
    joinedAt: new Date(Date.now() - (8 - i) * 86400000 * 5).toISOString(),
  })),
  // Gaming Lounge - users 2-11
  ...mockUsers.slice(2, 12).map((u, i) => ({
    userId: u.id,
    guildId: mockGuilds[2].id,
    nickname: i === 0 ? "CharlieGamer" : null,
    roles: i === 4 ? [g2.admin] : i < 2 ? [g2.mod] : [g2.member],
    joinedAt: new Date(Date.now() - (10 - i) * 86400000 * 3).toISOString(),
  })),
  // Music Fans - users 4-11
  ...mockUsers.slice(4, 12).map((u, i) => ({
    userId: u.id,
    guildId: mockGuilds[3].id,
    nickname: null,
    roles: i === 5 ? [g3.admin] : i < 2 ? [g3.mod] : [g3.member],
    joinedAt: new Date(Date.now() - (8 - i) * 86400000 * 4).toISOString(),
  })),
];

// --- Presence ---

export const mockPresence: Record<string, PresenceStatus> = {
  [mockUsers[0].id]: "online",
  [mockUsers[1].id]: "online",
  [mockUsers[2].id]: "online",
  [mockUsers[3].id]: "online",
  [mockUsers[4].id]: "online",
  [mockUsers[5].id]: "idle",
  [mockUsers[6].id]: "idle",
  [mockUsers[7].id]: "idle",
  [mockUsers[8].id]: "dnd",
  [mockUsers[9].id]: "offline",
  [mockUsers[10].id]: "offline",
  [mockUsers[11].id]: "offline",
};

// --- Messages ---

const MESSAGE_CONTENT_POOL = [
  "Hey everyone, how's it going?",
  "Just pushed a new commit, can someone review?",
  "Has anyone tried the new API endpoint?",
  "I think we should refactor the auth module",
  "Good morning! Ready for the standup?",
  "The build is passing now, finally!",
  "Can we schedule a call to discuss the design?",
  "I found a bug in the message encryption",
  "Nice work on the PR!",
  "Let me check the logs real quick",
  "Anyone up for a code review session?",
  "The database migration went smoothly",
  "I'll be AFK for about an hour",
  "We need to update the documentation",
  "That's a great idea, let's prototype it",
  "Check out this new library I found",
  "The performance improvements look solid",
  "Can we add more test coverage here?",
  "I'm working on the WebSocket implementation",
  "Let's discuss this in tomorrow's meeting",
  "Just deployed to staging, please test",
  "The CI pipeline is running slow today",
  "I love the new dark mode theme!",
  "We should add error boundaries",
  "The UX flow feels much smoother now",
  "Anyone familiar with signal protocol?",
  "Great catch on that edge case",
  "Let me write up a design doc for this",
  "The mobile layout needs some tweaks",
  "Happy Friday everyone!",
];

function generateMessages(
  channelId: string,
  count: number,
  senderIds: string[],
  baseUuidPrefix: string,
): Message[] {
  const messages: Message[] = [];
  const now = Date.now();
  const sevenDaysMs = 7 * 24 * 60 * 60 * 1000;
  let currentSenderIdx = 0;
  let messagesFromCurrentSender = 0;
  const maxFromSameSender = 4;

  for (let i = 0; i < count; i++) {
    // Switch sender every 3-5 messages
    if (
      messagesFromCurrentSender >= maxFromSameSender ||
      (messagesFromCurrentSender >= 2 && i % 3 === 0)
    ) {
      currentSenderIdx = (currentSenderIdx + 1) % senderIds.length;
      messagesFromCurrentSender = 0;
    }
    messagesFromCurrentSender++;

    const timeOffset = sevenDaysMs - (i / count) * sevenDaysMs;
    const jitter = (((i * 7 + 13) % 30) + 1) * 60 * 1000; // 1-30 min jitter
    const timestamp = new Date(now - timeOffset + jitter).toISOString();
    const content =
      MESSAGE_CONTENT_POOL[(i * 7 + 3) % MESSAGE_CONTENT_POOL.length];
    const msgId = `${baseUuidPrefix}-${String(i + 1).padStart(4, "0")}-4000-8000-000000000000`;

    const attachments: FileAttachment[] = [];
    // ~7% of messages get attachments
    if (i % 14 === 0 && i > 0) {
      const isImage = i % 28 === 0;
      attachments.push({
        id: `f1000000-${baseUuidPrefix.slice(-4)}-${String(i).padStart(4, "0")}-8000-000000000000`,
        fileName: isImage ? "screenshot.png" : "document.pdf",
        fileSize: isImage ? 245760 : 102400,
        mimeType: isImage ? "image/png" : "application/pdf",
        url: isImage
          ? "https://placeholder.test/screenshot.png"
          : "https://placeholder.test/document.pdf",
        thumbnailUrl: isImage
          ? "https://placeholder.test/screenshot-thumb.png"
          : null,
      });
    }

    const editedAt =
      i % 25 === 0 && i > 0
        ? new Date(now - timeOffset + jitter + 300000).toISOString()
        : null;

    messages.push({
      id: msgId,
      channelId,
      senderId: senderIds[currentSenderIdx],
      content,
      encryptedContent: content,
      nonce: `mock-nonce-${msgId}`,
      createdAt: timestamp,
      editedAt,
      attachments,
    });
  }

  return messages;
}

// Generate messages for all text channels
const textChannels = mockChannels.filter((c) => c.channelType === "text");
const allUserIds = mockUsers.map((u) => u.id);

export const mockMessages: Message[] = textChannels.flatMap((channel, ci) => {
  const prefix = `d1${String(ci + 1).padStart(2, "0")}0000`;
  // Use a subset of users per channel for realistic conversations
  const channelSenders = [
    allUserIds[(ci * 3) % allUserIds.length],
    allUserIds[(ci * 3 + 1) % allUserIds.length],
    allUserIds[(ci * 3 + 2) % allUserIds.length],
    allUserIds[(ci * 3 + 4) % allUserIds.length],
    allUserIds[(ci * 3 + 7) % allUserIds.length],
  ];
  return generateMessages(channel.id, 100, channelSenders, prefix);
});
