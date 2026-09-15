import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Routes, Route } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { ChannelView } from "../../../components/chat/ChannelView";
import { useAppStore } from "../../../store";
import type { AppStore } from "../../../store";
import type { Message } from "../../../types";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// Mock react-virtuoso to avoid infinite loop in jsdom
vi.mock("react-virtuoso", () => ({
  Virtuoso: ({
    data,
    itemContent,
  }: {
    data: unknown[];
    itemContent: (index: number, item: unknown) => React.ReactNode;
  }) => (
    <div data-testid="message-view-list">
      {data?.map((item, i) => (
        <div key={i}>{itemContent(i, item)}</div>
      ))}
    </div>
  ),
}));

const TEST_GUILD_ID = "guild-1";
const TEST_CHANNEL_ID = "ch-1";
const TEST_USER_ID = "user-me";

const baseOverrides: Partial<AppStore> = {
  isAuthenticated: true,
  currentUser: {
    id: TEST_USER_ID,
    displayName: "Test User",
    email: "test@test.com",
    avatarUrl: null,
  },
  channelsById: {
    [TEST_CHANNEL_ID]: {
      id: TEST_CHANNEL_ID,
      guildId: TEST_GUILD_ID,
      name: "general",
      channelType: "text",
      position: 0,
      category: null,
    },
  },
};

function renderChannelView(extraOverrides: Partial<AppStore> = {}) {
  return renderWithProviders(
    <Routes>
      <Route
        path="/app/guild/:guildId/channel/:channelId"
        element={<ChannelView />}
      />
    </Routes>,
    {
      initialEntries: [
        `/app/guild/${TEST_GUILD_ID}/channel/${TEST_CHANNEL_ID}`,
      ],
      seed: false,
      storeOverrides: { ...baseOverrides, ...extraOverrides },
    },
  );
}

function makeMessage(overrides: Partial<Message> = {}): Message {
  return {
    id: overrides.id ?? "msg-1",
    channelId: TEST_CHANNEL_ID,
    senderId: "user-other",
    content: "Hello world",
    encryptedContent: "",
    nonce: "",
    createdAt: "2026-02-24T00:00:00Z",
    editedAt: null,
    attachments: [],
    ...overrides,
  };
}

describe("ChannelView integration", () => {
  beforeEach(() => {
    mockInvoke.mockReset().mockResolvedValue(undefined);
  });

  it("calls send_message Tauri command when user sends a message", async () => {
    const user = userEvent.setup();
    renderChannelView();

    const textarea = screen.getByPlaceholderText("Message #general");
    await user.type(textarea, "Hello{Enter}");

    expect(mockInvoke).toHaveBeenCalledWith("send_message", {
      channelId: TEST_CHANNEL_ID,
      guildId: TEST_GUILD_ID,
      plaintext: "Hello",
    });
  });

  it("does not use mockSendMessage", async () => {
    const user = userEvent.setup();
    renderChannelView();

    const textarea = screen.getByPlaceholderText("Message #general");
    await user.type(textarea, "Test{Enter}");

    expect(mockInvoke).toHaveBeenCalledWith("send_message", expect.any(Object));
  });

  it("shows typing indicator from store", () => {
    renderChannelView({
      usersById: {
        "user-a": {
          id: "user-a",
          displayName: "Alice",
          avatarUrl: null,
          email: "a@t.com",
        },
      },
      typingUsers: {
        [TEST_CHANNEL_ID]: ["user-a"],
      },
    });

    expect(screen.getByText(/Alice is typing/)).toBeInTheDocument();
  });

  it("shows DecryptFailedMessage for messages with status decrypt_failed", () => {
    const failedMsg = makeMessage({
      id: "msg-fail",
      content: "",
      status: "decrypt_failed",
      failureReason: "SessionNotFound",
    });

    renderChannelView({
      messagesById: { "msg-fail": failedMsg },
      messageIdsByChannel: { [TEST_CHANNEL_ID]: ["msg-fail"] },
    });

    expect(screen.getByText(/unable to decrypt/i)).toBeInTheDocument();
  });

  it("does not simulate mock typing indicators", () => {
    renderChannelView();

    const state = useAppStore.getState();
    const typing = state.typingUsers[TEST_CHANNEL_ID] ?? [];
    expect(typing).toEqual([]);
  });
});
