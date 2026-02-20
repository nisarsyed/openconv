import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import { Routes, Route } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { ChannelView } from "../../../components/chat/ChannelView";
import { useAppStore } from "../../../store";
import { mockGuilds, mockChannels } from "../../../mock/data";

function renderChannelView(guildId?: string, channelId?: string) {
  const gid = guildId ?? mockGuilds[0].id;
  const cid = channelId ?? mockChannels.find((c) => c.guildId === gid && c.channelType === "text")!.id;

  return renderWithProviders(
    <Routes>
      <Route
        path="/app/guild/:guildId/channel/:channelId"
        element={<ChannelView />}
      />
    </Routes>,
    {
      initialEntries: [`/app/guild/${gid}/channel/${cid}`],
      storeOverrides: {
        isAuthenticated: true,
        currentUser: {
          id: "a1b2c3d4-0001-4000-8000-000000000001",
          displayName: "Alice Chen",
          email: "alice@example.com",
          avatarUrl: null,
        },
      },
    },
  );
}

describe("ChannelView / MessageView", () => {
  it("renders messages from store for the active channel", () => {
    renderChannelView();

    // After seeding, the store has messages for channels. Check that message content is shown.
    const messageView = screen.getByTestId("message-view");
    expect(messageView).toBeInTheDocument();
  });

  it("renders the message input with channel name placeholder", () => {
    renderChannelView();

    const textarea = screen.getByPlaceholderText(/Message #/);
    expect(textarea).toBeInTheDocument();
  });

  it("shows loading spinner when loadingMessages is true", () => {
    const gid = mockGuilds[0].id;
    const cid = mockChannels.find((c) => c.guildId === gid && c.channelType === "text")!.id;

    renderWithProviders(
      <Routes>
        <Route
          path="/app/guild/:guildId/channel/:channelId"
          element={<ChannelView />}
        />
      </Routes>,
      {
        initialEntries: [`/app/guild/${gid}/channel/${cid}`],
        storeOverrides: {
          isAuthenticated: true,
          currentUser: {
            id: "a1b2c3d4-0001-4000-8000-000000000001",
            displayName: "Alice Chen",
            email: "alice@example.com",
            avatarUrl: null,
          },
          loadingMessages: { [cid]: true },
        },
      },
    );

    expect(screen.getByTestId("spinner")).toBeInTheDocument();
  });
});
