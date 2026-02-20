import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Routes, Route, useLocation } from "react-router";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { UserSettings } from "../../../components/settings/UserSettings";
import { useAppStore } from "../../../store";

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function renderUserSettings() {
  return renderWithProviders(
    <Routes>
      <Route path="/app/settings" element={<UserSettings />} />
      <Route path="/login" element={<LocationDisplay />} />
    </Routes>,
    {
      initialEntries: ["/app/settings"],
      storeOverrides: {
        isAuthenticated: true,
        currentUser: {
          id: "user-1",
          displayName: "Alice Chen",
          email: "alice@example.com",
          avatarUrl: null,
        },
      },
    },
  );
}

describe("UserSettings", () => {
  it("renders display name and email fields", () => {
    renderUserSettings();

    const nameInput = screen.getByLabelText(/display name/i);
    expect(nameInput).toHaveValue("Alice Chen");

    expect(screen.getByLabelText(/email/i)).toHaveValue("alice@example.com");
  });

  it("save button is disabled until changes are made", () => {
    renderUserSettings();

    const saveBtn = screen.getByRole("button", { name: /save/i });
    expect(saveBtn).toBeDisabled();
  });

  it("save button calls updateProfile with changed name", async () => {
    const user = userEvent.setup();
    renderUserSettings();

    const nameInput = screen.getByLabelText(/display name/i);
    await user.clear(nameInput);
    await user.type(nameInput, "New Name");

    const saveBtn = screen.getByRole("button", { name: /save/i });
    expect(saveBtn).not.toBeDisabled();

    await user.click(saveBtn);

    const state = useAppStore.getState();
    expect(state.currentUser?.displayName).toBe("New Name");
  });

  it("appearance section has theme toggle", async () => {
    const user = userEvent.setup();
    renderUserSettings();

    await user.click(screen.getByText("Appearance"));

    // Theme UI uses separate Dark/Light buttons
    const themeButtons = screen.getAllByRole("button", { name: /dark|light/i });
    expect(themeButtons.length).toBeGreaterThanOrEqual(2);
  });

  it("theme toggle calls toggleTheme", async () => {
    const user = userEvent.setup();
    renderUserSettings();

    await user.click(screen.getByText("Appearance"));

    const initialTheme = useAppStore.getState().theme;
    // Click the opposite theme button to trigger toggleTheme
    const targetName = initialTheme === "dark" ? /light/i : /dark/i;
    const themeButton = screen.getByRole("button", { name: targetName });
    await user.click(themeButton);

    expect(useAppStore.getState().theme).toBe(
      initialTheme === "dark" ? "light" : "dark",
    );
  });

  it("log out button calls logout and redirects to /login", async () => {
    const user = userEvent.setup();
    renderUserSettings();

    await user.click(screen.getByRole("button", { name: /log out/i }));

    expect(useAppStore.getState().isAuthenticated).toBe(false);
    expect(screen.getByTestId("location")).toHaveTextContent("/login");
  });
});
