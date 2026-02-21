import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Routes, Route } from "react-router";
import { LoginPage } from "../../routes/LoginPage";
import { useAppStore } from "../../store";

vi.mock("../../bindings", () => ({
  commands: {
    authLogin: vi.fn(),
  },
}));

import { commands } from "../../bindings";

describe("LoginPage", () => {
  beforeEach(() => {
    useAppStore.setState(useAppStore.getInitialState());
    vi.mocked(commands.authLogin).mockReset();
  });

  function renderLoginPage() {
    return render(
      <MemoryRouter initialEntries={["/login"]}>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route path="/app" element={<div>App Content</div>} />
          <Route path="/register" element={<div>Register Page</div>} />
          <Route path="/recover" element={<div>Recover Page</div>} />
        </Routes>
      </MemoryRouter>,
    );
  }

  it("renders a Log In button (no email field)", () => {
    renderLoginPage();
    expect(screen.getByRole("button", { name: /log in/i })).toBeInTheDocument();
    expect(screen.queryByLabelText(/email/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/password/i)).not.toBeInTheDocument();
  });

  it("navigates to /app on successful login", async () => {
    const user = userEvent.setup();
    vi.mocked(commands.authLogin).mockResolvedValue({
      status: "ok",
      data: { user_id: "u1", public_key: "pk", device_id: "d1" },
    });
    renderLoginPage();
    await user.click(screen.getByRole("button", { name: /log in/i }));
    await waitFor(() => {
      expect(screen.getByText("App Content")).toBeInTheDocument();
    });
  });

  it("shows error message on login failure", async () => {
    const user = userEvent.setup();
    vi.mocked(commands.authLogin).mockResolvedValue({
      status: "error",
      error: { message: "No identity found" },
    });
    renderLoginPage();
    await user.click(screen.getByRole("button", { name: /log in/i }));
    await waitFor(() => {
      expect(screen.getByText(/no identity found/i)).toBeInTheDocument();
    });
  });

  it("has a link to the register page", () => {
    renderLoginPage();
    const link = screen.getByRole("link", {
      name: /register|create account/i,
    });
    expect(link).toBeInTheDocument();
  });

  it("has a link to the recovery page", () => {
    renderLoginPage();
    const link = screen.getByRole("link", { name: /recover|forgot/i });
    expect(link).toBeInTheDocument();
  });
});
