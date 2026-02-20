import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Routes, Route } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { ChannelSidebar } from "../../../components/layout/ChannelSidebar";
import { useAppStore } from "../../../store";
import { mockGuilds, mockChannels } from "../../../mock/data";

function renderChannelSidebar(
  route?: string,
  overrides?: Record<string, unknown>,
) {
  const guildId = mockGuilds[0].id;
  const channelId = mockChannels[0].id;
  const entry = route ?? `/app/guild/${guildId}/channel/${channelId}`;

  return renderWithProviders(
    <Routes>
      <Route
        path="/app/guild/:guildId/channel/:channelId"
        element={<ChannelSidebar />}
      />
    </Routes>,
    {
      initialEntries: [entry],
      storeOverrides: {
        isAuthenticated: true,
        currentUser: {
          id: "a1b2c3d4-0001-4000-8000-000000000001",
          displayName: "Alice Chen",
          email: "alice@example.com",
          avatarUrl: null,
        },
        ...overrides,
      },
    },
  );
}

describe("ChannelSidebar", () => {
  it("renders channels grouped by category", () => {
    renderChannelSidebar();

    // OpenConv Dev guild has categories: General, Development, Voice
    // CSS uppercase class transforms display but not DOM text content
    expect(screen.getByText("General")).toBeInTheDocument();
    expect(screen.getByText("Development")).toBeInTheDocument();
    expect(screen.getByText("Voice")).toBeInTheDocument();
  });

  it("category header is clickable and toggles children visibility", async () => {
    const user = userEvent.setup();
    renderChannelSidebar();

    // Click "Development" category to collapse (CSS uppercase for display only)
    const devCategory = screen.getByText("Development");
    await user.click(devCategory);

    // The channels under Development should be hidden
    expect(screen.queryByText("frontend")).not.toBeInTheDocument();

    // Click again to expand
    await user.click(devCategory);
    expect(screen.getByText("frontend")).toBeInTheDocument();
  });

  it("selected channel has highlighted background", () => {
    renderChannelSidebar();

    // First channel (general) is selected via the route
    const channelItem = screen
      .getByText("general")
      .closest("[data-testid^='channel-item-']");
    expect(channelItem).toHaveClass("bg-[var(--interactive-active)]");
  });

  it("unread channel shows bold text", () => {
    const guild = mockGuilds[0];
    const frontendChannel = mockChannels.find(
      (c) => c.guildId === guild.id && c.name === "frontend",
    )!;

    renderChannelSidebar(undefined, {
      unreadCountByChannel: { [frontendChannel.id]: 2 },
    });

    const channelItem = screen.getByText("frontend");
    expect(channelItem).toHaveClass("font-semibold");
  });

  it("clicking channel navigates to the channel route", async () => {
    const user = userEvent.setup();
    renderChannelSidebar();

    const guild = mockGuilds[0];
    const frontendChannel = mockChannels.find(
      (c) => c.guildId === guild.id && c.name === "frontend",
    )!;

    await user.click(screen.getByText("frontend"));

    // Should update lastVisitedChannel
    expect(useAppStore.getState().lastVisitedChannelByGuild[guild.id]).toBe(
      frontendChannel.id,
    );
  });
});
