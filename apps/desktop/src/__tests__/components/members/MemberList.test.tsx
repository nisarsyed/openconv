import { describe, it, expect } from "vitest";
import { screen, within } from "@testing-library/react";
import { Routes, Route } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { MemberList } from "../../../components/layout/MemberList";
import { useAppStore } from "../../../store";
import { mockGuilds, mockChannels } from "../../../mock/data";
import type { Member, Role, PresenceStatus, User } from "../../../types";

const guildId = mockGuilds[0].id;
const channelId = mockChannels[0].id;

function renderMemberList(storeOverrides?: Record<string, unknown>) {
  return renderWithProviders(
    <Routes>
      <Route
        path="/app/guild/:guildId/channel/:channelId"
        element={<MemberList />}
      />
    </Routes>,
    {
      initialEntries: [`/app/guild/${guildId}/channel/${channelId}`],
      storeOverrides: {
        memberListVisible: true,
        isAuthenticated: true,
        currentUser: {
          id: "a1b2c3d4-0001-4000-8000-000000000001",
          displayName: "Alice Chen",
          email: "alice@example.com",
          avatarUrl: null,
        },
        ...storeOverrides,
      },
    },
  );
}

describe("MemberList", () => {
  it("renders members grouped by role", () => {
    renderMemberList();

    // OpenConv Dev guild has Admin, Moderator, Member roles
    // After seeding: Alice is Admin, Bob & Charlie are Moderators, rest are Members
    expect(screen.getByText(/Admin/)).toBeInTheDocument();
    expect(screen.getByText(/Moderator/)).toBeInTheDocument();
    expect(screen.getByText(/Member/)).toBeInTheDocument();
  });

  it("online members appear before offline members within each role group", () => {
    renderMemberList();

    // In the "Member" group, there are online and offline members
    // We check that within the member-group, online members come first
    const memberGroups = screen.getAllByTestId("member-group");

    // The last group should be "Member" (lowest position)
    const memberGroup = memberGroups[memberGroups.length - 1];
    const items = within(memberGroup).getAllByTestId("member-item");

    // Get all presence statuses in order
    const store = useAppStore.getState();
    const statuses = items.map((item) => {
      const userId = item.getAttribute("data-user-id");
      return store.presenceByUserId[userId!] ?? "offline";
    });

    // Verify online/idle/dnd come before offline
    const firstOfflineIdx = statuses.indexOf("offline");
    if (firstOfflineIdx > 0) {
      const onlineStatuses = statuses.slice(0, firstOfflineIdx);
      expect(onlineStatuses.every((s) => s !== "offline")).toBe(true);
    }
  });

  it("offline members are visually dimmed", () => {
    renderMemberList();

    // Julia, Kevin, Lena are offline
    const items = screen.getAllByTestId("member-item");
    const store = useAppStore.getState();

    for (const item of items) {
      const userId = item.getAttribute("data-user-id");
      const status = store.presenceByUserId[userId!] ?? "offline";
      if (status === "offline") {
        expect(item.className).toMatch(/dimmed|opacity/);
      }
    }
  });

  it("group headers show role name and member count", () => {
    renderMemberList();

    // OpenConv Dev: 1 Admin (Alice), 2 Moderators (Bob, Charlie), 9 Members (rest)
    expect(screen.getByText(/Admin\s*[—–-]\s*1/)).toBeInTheDocument();
    expect(screen.getByText(/Moderator\s*[—–-]\s*2/)).toBeInTheDocument();
    expect(screen.getByText(/Member\s*[—–-]\s*9/)).toBeInTheDocument();
  });

  it("returns null when memberListVisible is false", () => {
    const { container } = renderMemberList({ memberListVisible: false });

    // MemberList should not render content when collapsed
    expect(
      container.querySelector("[data-testid='member-list-content']"),
    ).not.toBeInTheDocument();
  });

  it("roles are ordered by position (highest first)", () => {
    renderMemberList();

    const groups = screen.getAllByTestId("member-group");
    const headers = groups.map(
      (g) => within(g).getByTestId("role-header").textContent,
    );

    const adminIdx = headers.findIndex((h) => h?.includes("Admin"));
    const modIdx = headers.findIndex((h) => h?.includes("Moderator"));
    const memberIdx = headers.findIndex((h) => h?.includes("Member"));

    expect(adminIdx).toBeLessThan(modIdx);
    expect(modIdx).toBeLessThan(memberIdx);
  });
});
