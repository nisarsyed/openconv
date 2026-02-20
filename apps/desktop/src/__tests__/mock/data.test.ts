import { describe, it, expect } from "vitest";
import {
  mockGuilds,
  mockChannels,
  mockUsers,
  mockMessages,
  mockRoles,
  mockMembers,
} from "../../mock/data";

const UUID_REGEX =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

describe("mock data - guilds", () => {
  it("includes at least 4 guilds", () => {
    expect(mockGuilds.length).toBeGreaterThanOrEqual(4);
  });

  it("every guild has a valid UUID id", () => {
    for (const guild of mockGuilds) {
      expect(guild.id).toMatch(UUID_REGEX);
    }
  });

  it("every guild has name and ownerId", () => {
    for (const guild of mockGuilds) {
      expect(guild.name).toBeTruthy();
      expect(guild.ownerId).toMatch(UUID_REGEX);
    }
  });
});

describe("mock data - channels", () => {
  it("each guild has 5-8 channels", () => {
    for (const guild of mockGuilds) {
      const guildChannels = mockChannels.filter((c) => c.guildId === guild.id);
      expect(guildChannels.length).toBeGreaterThanOrEqual(5);
      expect(guildChannels.length).toBeLessThanOrEqual(8);
    }
  });

  it("channels have valid types (text or voice)", () => {
    for (const channel of mockChannels) {
      expect(["text", "voice"]).toContain(channel.channelType);
    }
  });

  it("channels include categories", () => {
    const categories = new Set(
      mockChannels.map((c) => c.category).filter(Boolean),
    );
    expect(categories.size).toBeGreaterThanOrEqual(2);
  });

  it("every channel has a valid UUID id", () => {
    for (const channel of mockChannels) {
      expect(channel.id).toMatch(UUID_REGEX);
    }
  });
});

describe("mock data - users", () => {
  it("includes at least 12 users", () => {
    expect(mockUsers.length).toBeGreaterThanOrEqual(12);
  });

  it("every user has a valid UUID id", () => {
    for (const user of mockUsers) {
      expect(user.id).toMatch(UUID_REGEX);
    }
  });

  it("every user has displayName and email", () => {
    for (const user of mockUsers) {
      expect(user.displayName).toBeTruthy();
      expect(user.email).toBeTruthy();
    }
  });
});

describe("mock data - messages", () => {
  it("messages exist for text channels", () => {
    const textChannels = mockChannels.filter((c) => c.channelType === "text");
    for (const channel of textChannels) {
      const channelMessages = mockMessages.filter(
        (m) => m.channelId === channel.id,
      );
      expect(channelMessages.length).toBeGreaterThan(0);
    }
  });

  it("messages have encryptedContent and nonce fields populated", () => {
    for (const msg of mockMessages) {
      expect(msg.encryptedContent).toBeTruthy();
      expect(msg.nonce).toBeTruthy();
    }
  });

  it("messages have valid timestamps", () => {
    for (const msg of mockMessages) {
      const date = new Date(msg.createdAt);
      expect(date.getTime()).not.toBeNaN();
    }
  });

  it("some messages include file attachments", () => {
    const withAttachments = mockMessages.filter(
      (m) => m.attachments.length > 0,
    );
    expect(withAttachments.length).toBeGreaterThan(0);
  });

  it("all message IDs are valid UUIDs", () => {
    for (const msg of mockMessages) {
      expect(msg.id).toMatch(UUID_REGEX);
    }
  });
});

describe("mock data - roles", () => {
  it("each guild has roles defined", () => {
    for (const guild of mockGuilds) {
      const guildRoles = mockRoles.filter((r) => r.guildId === guild.id);
      expect(guildRoles.length).toBeGreaterThanOrEqual(1);
    }
  });
});

describe("mock data - members", () => {
  it("each guild has members", () => {
    for (const guild of mockGuilds) {
      const guildMembers = mockMembers.filter((m) => m.guildId === guild.id);
      expect(guildMembers.length).toBeGreaterThan(0);
    }
  });

  it("member userIds reference existing users", () => {
    const userIds = new Set(mockUsers.map((u) => u.id));
    for (const member of mockMembers) {
      expect(userIds.has(member.userId)).toBe(true);
    }
  });
});
