import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Routes, Route, useLocation } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { ServerHeader } from "../../../components/guild/ServerHeader";
import { mockGuilds, mockChannels } from "../../../mock/data";

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function renderServerHeader() {
  const guildId = mockGuilds[0].id;
  const channelId = mockChannels[0].id;

  return renderWithProviders(
    <Routes>
      <Route
        path="/app/guild/:guildId/channel/:channelId"
        element={
          <>
            <ServerHeader />
            <LocationDisplay />
          </>
        }
      />
      <Route
        path="/app/guild/:guildId/settings"
        element={<LocationDisplay />}
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

describe("ServerHeader", () => {
  it("renders the current guild name", () => {
    renderServerHeader();
    expect(screen.getByText("OpenConv Dev")).toBeInTheDocument();
  });

  it("clicking opens dropdown with guild settings, invite, and leave options", async () => {
    const user = userEvent.setup();
    renderServerHeader();

    await user.click(screen.getByText("OpenConv Dev"));

    expect(screen.getByText("Guild Settings")).toBeInTheDocument();
    expect(screen.getByText("Create Invite")).toBeInTheDocument();
    expect(screen.getByText("Leave Guild")).toBeInTheDocument();
  });

  it("clicking guild settings option navigates to guild settings route", async () => {
    const user = userEvent.setup();
    renderServerHeader();

    const guildId = mockGuilds[0].id;

    await user.click(screen.getByText("OpenConv Dev"));
    await user.click(screen.getByText("Guild Settings"));

    expect(screen.getByTestId("location")).toHaveTextContent(
      `/app/guild/${guildId}/settings`,
    );
  });
});
