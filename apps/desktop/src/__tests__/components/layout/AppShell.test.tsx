import { describe, it, expect, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import { Routes, Route } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { resetMockPlatform } from "../../helpers/mockTauri";
import { AppShell } from "../../../components/layout/AppShell";
import { mockGuilds, mockChannels } from "../../../mock/data";

beforeEach(() => {
  resetMockPlatform();
});

function renderAppShell(route?: string) {
  const guildId = mockGuilds[0].id;
  const channelId = mockChannels[0].id;
  const entry = route ?? `/app/guild/${guildId}/channel/${channelId}`;

  return renderWithProviders(
    <Routes>
      <Route path="/app" element={<AppShell />}>
        <Route path="guild/:guildId/channel/:channelId" element={<div data-testid="channel-view">Channel</div>} />
        <Route path="settings" element={<div>Settings</div>} />
        <Route index element={<div>Welcome</div>} />
      </Route>
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
      },
    },
  );
}

describe("AppShell", () => {
  it("renders guild sidebar, channel sidebar, main content, and member list regions", () => {
    renderAppShell();

    expect(screen.getByTestId("guild-sidebar")).toBeInTheDocument();
    expect(screen.getByTestId("channel-sidebar")).toBeInTheDocument();
    expect(screen.getByTestId("main-content")).toBeInTheDocument();
    expect(screen.getByTestId("member-list")).toBeInTheDocument();
  });

  it("guild sidebar is a fixed-width element outside the PanelGroup", () => {
    renderAppShell();

    const guildSidebar = screen.getByTestId("guild-sidebar");
    expect(guildSidebar).toHaveStyle({ width: "68px" });
  });

  it("channel sidebar, main content, and member list are Panels in a PanelGroup", () => {
    renderAppShell();

    // react-resizable-panels v4 Group renders with data-group attribute
    const panelGroup = document.querySelector("[data-group]");
    expect(panelGroup).toBeInTheDocument();

    // Three panels inside the group
    const panels = document.querySelectorAll("[data-panel]");
    expect(panels.length).toBe(3);
  });

  it("does not render DragRegion on non-macOS", async () => {
    renderAppShell();

    // DragRegion is implemented in Section 10; AppShell currently doesn't render it
    const dragRegion = screen.queryByTestId("drag-region-overlay");
    expect(dragRegion).not.toBeInTheDocument();
  });

  it("renders outlet content for matching route", () => {
    renderAppShell();
    expect(screen.getByTestId("channel-view")).toBeInTheDocument();
  });
});
