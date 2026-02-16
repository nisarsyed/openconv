import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  mockFetchMessages,
  mockSendMessage,
  mockFetchGuilds,
  mockFetchMembers,
  mockLogin,
  mockRegister,
  mockCreateGuild,
  mockCreateChannel,
} from "../../mock/api";
import { mockChannels, mockGuilds } from "../../mock/data";

const UUID_REGEX =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

describe("mockFetchMessages", () => {
  const textChannel = mockChannels.find((c) => c.channelType === "text")!;

  it("returns paginated messages with correct shape", async () => {
    const messages = await mockFetchMessages(textChannel.id);
    expect(messages.length).toBeGreaterThan(0);
    expect(messages.length).toBeLessThanOrEqual(20);
    for (const msg of messages) {
      expect(msg.id).toMatch(UUID_REGEX);
      expect(msg.channelId).toBe(textChannel.id);
      expect(msg.senderId).toBeTruthy();
      expect(msg.content).toBeDefined();
      expect(msg.encryptedContent).toBeDefined();
      expect(msg.nonce).toBeTruthy();
      expect(msg.createdAt).toBeTruthy();
    }
  });

  it("respects 'before' parameter for pagination", async () => {
    const firstPage = await mockFetchMessages(textChannel.id);
    expect(firstPage.length).toBeGreaterThan(0);
    const oldest = firstPage[firstPage.length - 1];
    const secondPage = await mockFetchMessages(textChannel.id, oldest.createdAt);
    if (secondPage.length > 0) {
      const firstPageIds = new Set(firstPage.map((m) => m.id));
      for (const msg of secondPage) {
        expect(firstPageIds.has(msg.id)).toBe(false);
        expect(new Date(msg.createdAt).getTime()).toBeLessThan(
          new Date(oldest.createdAt).getTime(),
        );
      }
    }
  });

  it("returns empty array when no more messages", async () => {
    let page = await mockFetchMessages(textChannel.id);
    let iterations = 0;
    while (page.length > 0 && iterations < 20) {
      const oldest = page[page.length - 1];
      page = await mockFetchMessages(textChannel.id, oldest.createdAt);
      iterations++;
    }
    expect(page.length).toBe(0);
  });

  it("resolves after a delay (not instant)", async () => {
    const start = Date.now();
    await mockFetchMessages(textChannel.id);
    const elapsed = Date.now() - start;
    expect(elapsed).toBeGreaterThanOrEqual(180);
  });
});

describe("mockSendMessage", () => {
  it("returns created message with encryptedContent and nonce", async () => {
    const randomSpy = vi.spyOn(Math, "random").mockReturnValue(0.5);
    const textChannel = mockChannels.find((c) => c.channelType === "text")!;
    const msg = await mockSendMessage(textChannel.id, "Hello world");
    randomSpy.mockRestore();
    expect(msg.id).toMatch(UUID_REGEX);
    expect(msg.channelId).toBe(textChannel.id);
    expect(msg.encryptedContent).toBe("Hello world");
    expect(msg.nonce).toMatch(/^mock-nonce-/);
    expect(msg.content).toBe("Hello world");
  });

  it("fails approximately 5% of the time", async () => {
    vi.useFakeTimers();
    const textChannel = mockChannels.find((c) => c.channelType === "text")!;
    let failures = 0;
    const runs = 200;
    for (let i = 0; i < runs; i++) {
      let failed = false;
      const promise = mockSendMessage(textChannel.id, `msg-${i}`)
        .catch(() => { failed = true; });
      await vi.advanceTimersByTimeAsync(500);
      await promise;
      if (failed) failures++;
    }
    vi.useRealTimers();
    const rate = failures / runs;
    expect(rate).toBeGreaterThan(0.005);
    expect(rate).toBeLessThan(0.20);
  });
});

describe("mockLogin", () => {
  it("returns user with keyPair and token", async () => {
    const result = await mockLogin("alice@example.com");
    expect(result.user).toBeDefined();
    expect(result.user.id).toMatch(UUID_REGEX);
    expect(result.user.displayName).toBeTruthy();
    expect(result.user.email).toBeTruthy();
    expect(result.keyPair.publicKey).toBeTruthy();
    expect(result.keyPair.privateKey).toBeTruthy();
    expect(result.token).toBeTruthy();
  });

  it("resolves after a delay", async () => {
    const start = Date.now();
    await mockLogin("alice@example.com");
    const elapsed = Date.now() - start;
    expect(elapsed).toBeGreaterThanOrEqual(450);
  });
});

describe("mockRegister", () => {
  it("returns user with keyPair and token", async () => {
    const result = await mockRegister("new@example.com", "New User");
    expect(result.user).toBeDefined();
    expect(result.user.id).toMatch(UUID_REGEX);
    expect(result.user.displayName).toBe("New User");
    expect(result.user.email).toBe("new@example.com");
    expect(result.keyPair.publicKey).toBeTruthy();
    expect(result.keyPair.privateKey).toBeTruthy();
    expect(result.token).toBeTruthy();
  });
});

describe("mockFetchGuilds", () => {
  it("returns guild list", async () => {
    const guilds = await mockFetchGuilds();
    expect(guilds.length).toBeGreaterThan(0);
    for (const guild of guilds) {
      expect(guild.id).toMatch(UUID_REGEX);
      expect(guild.name).toBeTruthy();
      expect(guild.ownerId).toBeTruthy();
    }
  });
});

describe("mockFetchMembers", () => {
  it("returns member list for a guild", async () => {
    const guild = mockGuilds[0];
    const members = await mockFetchMembers(guild.id);
    expect(members.length).toBeGreaterThan(0);
    for (const member of members) {
      expect(member.userId).toBeTruthy();
      expect(member.guildId).toBe(guild.id);
      expect(member.roles).toBeDefined();
    }
  });
});

describe("mockCreateGuild", () => {
  it("returns created guild after delay", async () => {
    const start = Date.now();
    const guild = await mockCreateGuild("Test Guild");
    const elapsed = Date.now() - start;
    expect(elapsed).toBeGreaterThanOrEqual(250);
    expect(guild.id).toMatch(UUID_REGEX);
    expect(guild.name).toBe("Test Guild");
  });
});

describe("mockCreateChannel", () => {
  it("returns created channel after delay", async () => {
    const guild = mockGuilds[0];
    const start = Date.now();
    const channel = await mockCreateChannel(
      guild.id,
      "new-channel",
      "text",
      "General",
    );
    const elapsed = Date.now() - start;
    expect(elapsed).toBeGreaterThanOrEqual(150);
    expect(channel.id).toMatch(UUID_REGEX);
    expect(channel.name).toBe("new-channel");
    expect(channel.guildId).toBe(guild.id);
    expect(channel.channelType).toBe("text");
    expect(channel.category).toBe("General");
  });
});
