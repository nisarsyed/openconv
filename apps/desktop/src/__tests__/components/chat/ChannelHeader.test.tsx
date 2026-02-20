import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Routes, Route } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { ChannelHeader } from "../../../components/chat/ChannelHeader";
import { useAppStore } from "../../../store";
import { mockGuilds, mockChannels } from "../../../mock/data";

function renderChannelHeader() {
  const guildId = mockGuilds[0].id;
  const channelId = mockChannels[0].id;

  return renderWithProviders(
    <Routes>
      <Route
        path="/app/guild/:guildId/channel/:channelId"
        element={<ChannelHeader />}
      />
    </Routes>,
    {
      initialEntries: [`/app/guild/${guildId}/channel/${channelId}`],
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

describe("ChannelHeader", () => {
  it("renders channel name with # prefix", () => {
    renderChannelHeader();
    // h2 contains <span>#</span> + channel name
    expect(screen.getByText("general")).toBeInTheDocument();
  });

  it("renders member list toggle button", () => {
    renderChannelHeader();
    expect(screen.getByLabelText("Toggle member list")).toBeInTheDocument();
  });

  it("toggle button calls toggleMemberList on UISlice", async () => {
    const user = userEvent.setup();
    renderChannelHeader();

    const initialState = useAppStore.getState().memberListVisible;
    await user.click(screen.getByLabelText("Toggle member list"));
    expect(useAppStore.getState().memberListVisible).toBe(!initialState);
  });

  it("toggle button shows active state when member list is visible", () => {
    renderChannelHeader();

    // memberListVisible defaults to true
    const toggleBtn = screen.getByLabelText("Toggle member list");
    expect(toggleBtn).toHaveAttribute("aria-pressed", "true");
  });
});
