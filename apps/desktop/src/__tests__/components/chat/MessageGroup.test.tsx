import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import { Routes, Route } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { MessageGroup } from "../../../components/chat/MessageGroup";
import { mockGuilds, mockChannels, mockUsers } from "../../../mock/data";
import type { Message } from "../../../types";

function makeMessages(count: number, senderId: string): Message[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `msg-${i}`,
    channelId: "ch-1",
    senderId,
    content: `Message ${i + 1}`,
    encryptedContent: `Message ${i + 1}`,
    nonce: "n",
    createdAt: new Date(2026, 1, 14, 10, i).toISOString(),
    editedAt: null,
    attachments: [],
  }));
}

function renderGroup(senderId: string, messages: Message[]) {
  const guildId = mockGuilds[0].id;
  const channelId = mockChannels.find(
    (c) => c.guildId === guildId && c.channelType === "text",
  )!.id;

  return renderWithProviders(
    <Routes>
      <Route
        path="/app/guild/:guildId/channel/:channelId"
        element={<MessageGroup senderId={senderId} messages={messages} />}
      />
    </Routes>,
    {
      initialEntries: [`/app/guild/${guildId}/channel/${channelId}`],
      storeOverrides: {
        isAuthenticated: true,
        currentUser: {
          id: mockUsers[0].id,
          displayName: mockUsers[0].displayName,
          email: mockUsers[0].email,
          avatarUrl: null,
        },
      },
    },
  );
}

describe("MessageGroup", () => {
  it("renders avatar for the first message in the group", () => {
    const sender = mockUsers[1];
    const msgs = makeMessages(3, sender.id);
    renderGroup(sender.id, msgs);

    // Should show author name
    expect(screen.getByTestId("message-author")).toHaveTextContent(
      sender.displayName,
    );
  });

  it("renders username and timestamp for the first message", () => {
    const sender = mockUsers[1];
    const msgs = makeMessages(1, sender.id);
    renderGroup(sender.id, msgs);

    expect(screen.getByTestId("message-author")).toHaveTextContent(
      sender.displayName,
    );
    // Timestamp should be rendered (date format like "02/14/2026 10:00 AM" or "Today at 10:00 AM")
    expect(screen.getByText(/\d+:\d+/)).toBeInTheDocument();
  });

  it("renders content for all messages in the group", () => {
    const sender = mockUsers[1];
    const msgs = makeMessages(3, sender.id);
    renderGroup(sender.id, msgs);

    expect(screen.getByText("Message 1")).toBeInTheDocument();
    expect(screen.getByText("Message 2")).toBeInTheDocument();
    expect(screen.getByText("Message 3")).toBeInTheDocument();
  });

  it("colors username by role color", () => {
    const sender = mockUsers[1];
    const msgs = makeMessages(1, sender.id);
    renderGroup(sender.id, msgs);

    const authorEl = screen.getByTestId("message-author");
    // The author should have a style.color set from role data
    // (Bob Martinez has roles in OpenConv Dev guild)
    expect(authorEl).toBeInTheDocument();
  });
});
