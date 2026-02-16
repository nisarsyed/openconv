import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Routes, Route, useLocation } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { UserPanel } from "../../../components/members/UserPanel";
import { mockGuilds, mockChannels } from "../../../mock/data";

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function renderUserPanel() {
  const guildId = mockGuilds[0].id;
  const channelId = mockChannels[0].id;

  return renderWithProviders(
    <Routes>
      <Route
        path="/app/guild/:guildId/channel/:channelId"
        element={
          <>
            <UserPanel />
            <LocationDisplay />
          </>
        }
      />
      <Route
        path="/app/settings"
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
        presenceByUserId: {
          "a1b2c3d4-0001-4000-8000-000000000001": "online",
        },
      },
    },
  );
}

describe("UserPanel", () => {
  it("renders current user avatar and display name", () => {
    renderUserPanel();
    expect(screen.getByText("Alice Chen")).toBeInTheDocument();
  });

  it("renders status indicator dot", () => {
    renderUserPanel();
    expect(screen.getByLabelText("Online")).toBeInTheDocument();
  });

  it("gear icon click navigates to /app/settings", async () => {
    const user = userEvent.setup();
    renderUserPanel();

    await user.click(screen.getByLabelText("User settings"));

    expect(screen.getByTestId("location")).toHaveTextContent("/app/settings");
  });
});
