import { describe, it, expect } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Routes, Route } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { GuildSettings } from "../../../components/settings/GuildSettings";
import { useAppStore } from "../../../store";
import { mockGuilds } from "../../../mock/data";

const guildId = mockGuilds[0].id;

function renderGuildSettings() {
  return renderWithProviders(
    <Routes>
      <Route path="/app/guild/:guildId/settings" element={<GuildSettings />} />
    </Routes>,
    {
      initialEntries: [`/app/guild/${guildId}/settings`],
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

describe("GuildSettings", () => {
  it("renders guild name input pre-filled", () => {
    renderGuildSettings();

    const nameInput = screen.getByLabelText(/guild name/i);
    expect(nameInput).toHaveValue("OpenConv Dev");
  });

  it("save changes updates guild name", async () => {
    const user = userEvent.setup();
    renderGuildSettings();

    const nameInput = screen.getByLabelText(/guild name/i);
    await user.clear(nameInput);
    await user.type(nameInput, "New Guild Name");

    await user.click(screen.getByRole("button", { name: /save/i }));

    expect(useAppStore.getState().guildsById[guildId].name).toBe(
      "New Guild Name",
    );
  });

  it("roles section lists existing roles", async () => {
    const user = userEvent.setup();
    renderGuildSettings();

    await user.click(screen.getByText("Roles"));

    expect(screen.getByText("Admin")).toBeInTheDocument();
    expect(screen.getByText("Moderator")).toBeInTheDocument();
    expect(screen.getByText("Member")).toBeInTheDocument();
  });

  it("channels section lists existing channels", async () => {
    const user = userEvent.setup();
    renderGuildSettings();

    await user.click(screen.getByText("Channels"));

    expect(screen.getByText("general")).toBeInTheDocument();
    expect(screen.getByText("announcements")).toBeInTheDocument();
  });

  it("members section lists guild members", async () => {
    const user = userEvent.setup();
    renderGuildSettings();

    await user.click(screen.getByText("Members"));

    expect(screen.getByText("Alice Chen")).toBeInTheDocument();
    expect(screen.getByText("Bob Martinez")).toBeInTheDocument();
  });
});
