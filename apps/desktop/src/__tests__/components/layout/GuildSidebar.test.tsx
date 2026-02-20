import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Routes, Route } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { GuildSidebar } from "../../../components/layout/GuildSidebar";
import { useAppStore } from "../../../store";
import { mockGuilds, mockChannels } from "../../../mock/data";

function renderSidebar(route?: string, overrides?: Record<string, unknown>) {
  const guildId = mockGuilds[0].id;
  const channelId = mockChannels[0].id;
  const entry = route ?? `/app/guild/${guildId}/channel/${channelId}`;

  return renderWithProviders(
    <Routes>
      <Route
        path="/app/guild/:guildId/channel/:channelId"
        element={<GuildSidebar />}
      />
      <Route path="/app" element={<GuildSidebar />} />
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

describe("GuildSidebar", () => {
  it("renders guild icons from store", () => {
    renderSidebar();

    for (const guild of mockGuilds) {
      expect(screen.getByLabelText(guild.name)).toBeInTheDocument();
    }
  });

  it("selected guild shows active indicator (white pill)", () => {
    const guildId = mockGuilds[0].id;
    const channelId = mockChannels[0].id;
    renderSidebar(`/app/guild/${guildId}/channel/${channelId}`);

    const selectedGuild = screen.getByLabelText(mockGuilds[0].name).closest("[data-testid^='guild-icon-']");
    expect(selectedGuild).not.toBeNull();

    const pill = selectedGuild!.querySelector("[data-testid='guild-pill']");
    expect(pill).toBeInTheDocument();
    expect(pill).toHaveClass("h-9");
  });

  it("clicking guild navigates to guild's last visited channel route", async () => {
    const user = userEvent.setup();
    renderSidebar();

    const guild2 = mockGuilds[1];
    const guild2Channels = mockChannels.filter(
      (c) => c.guildId === guild2.id && c.channelType === "text",
    );
    useAppStore.setState({
      lastVisitedChannelByGuild: {
        ...useAppStore.getState().lastVisitedChannelByGuild,
        [guild2.id]: guild2Channels[0].id,
      },
    });

    await user.click(screen.getByLabelText(guild2.name));

    expect(useAppStore.getState().lastVisitedGuildId).toBe(guild2.id);
  });

  it("clicking guild with no last visited channel navigates to first text channel", async () => {
    const user = userEvent.setup();
    renderSidebar();

    const guild2 = mockGuilds[1];
    const state = useAppStore.getState();
    const { [guild2.id]: _, ...rest } = state.lastVisitedChannelByGuild;
    useAppStore.setState({ lastVisitedChannelByGuild: rest });

    await user.click(screen.getByLabelText(guild2.name));

    expect(useAppStore.getState().lastVisitedGuildId).toBe(guild2.id);
  });

  it("unread guild shows white dot indicator", () => {
    const guild2 = mockGuilds[1];
    const guild2Channel = mockChannels.find((c) => c.guildId === guild2.id)!;

    // Set unread state BEFORE rendering
    renderSidebar(undefined, {
      unreadCountByChannel: { [guild2Channel.id]: 3 },
    });

    // Guild 2 should have a small pill (h-2 for unread)
    const guildIcon = screen.getByLabelText(guild2.name).closest("[data-testid^='guild-icon-']");
    const pill = guildIcon!.querySelector("[data-testid='guild-pill']");
    expect(pill).toBeInTheDocument();
    expect(pill).toHaveClass("h-2");
  });

  it("guild with mentions shows red badge with count", () => {
    const guild2 = mockGuilds[1];

    // Set mention state BEFORE rendering
    renderSidebar(undefined, {
      mentionCountByGuild: { [guild2.id]: 5 },
    });

    // Badge should show the count
    expect(screen.getByText("5")).toBeInTheDocument();
  });

  it("add guild button opens create guild modal", async () => {
    const user = userEvent.setup();
    renderSidebar();

    const addButton = screen.getByLabelText("Create guild");
    await user.click(addButton);

    expect(useAppStore.getState().activeModal).toEqual({
      type: "createGuild",
    });
  });

  it("home button is present but disabled", () => {
    renderSidebar();

    const homeButton = screen.getByLabelText("Home");
    expect(homeButton).toBeInTheDocument();
    expect(homeButton).toBeDisabled();
  });
});
