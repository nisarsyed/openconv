import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Routes, Route } from "react-router";
import { LoginPage } from "../../routes/LoginPage";
import { useAppStore } from "../../store";

vi.mock("../../mock/api", () => ({
  mockLogin: vi.fn(),
}));

import { mockLogin } from "../../mock/api";

describe("LoginPage", () => {
  beforeEach(() => {
    useAppStore.setState(useAppStore.getInitialState());
    vi.mocked(mockLogin).mockReset();
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

  it("renders email input and login button (no password field)", () => {
    renderLoginPage();
    expect(screen.getByLabelText(/email/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /log in/i })).toBeInTheDocument();
    expect(screen.queryByLabelText(/password/i)).not.toBeInTheDocument();
  });

  it("disables login button when email is empty", () => {
    renderLoginPage();
    expect(screen.getByRole("button", { name: /log in/i })).toBeDisabled();
  });

  it("enables login button when email has content", async () => {
    const user = userEvent.setup();
    renderLoginPage();
    await user.type(screen.getByLabelText(/email/i), "test@example.com");
    expect(screen.getByRole("button", { name: /log in/i })).toBeEnabled();
  });

  it("calls mockLogin on form submit with the entered email", async () => {
    const user = userEvent.setup();
    vi.mocked(mockLogin).mockResolvedValue({
      user: {
        id: "u1",
        displayName: "Test",
        email: "test@example.com",
        avatarUrl: null,
      },
      keyPair: { publicKey: "pk", privateKey: "sk" },
      token: "token-123",
    });
    renderLoginPage();
    await user.type(screen.getByLabelText(/email/i), "test@example.com");
    await user.click(screen.getByRole("button", { name: /log in/i }));
    expect(mockLogin).toHaveBeenCalledWith("test@example.com");
  });

  it("navigates to /app on successful login", async () => {
    const user = userEvent.setup();
    vi.mocked(mockLogin).mockResolvedValue({
      user: {
        id: "u1",
        displayName: "Test",
        email: "test@example.com",
        avatarUrl: null,
      },
      keyPair: { publicKey: "pk", privateKey: "sk" },
      token: "token-123",
    });
    renderLoginPage();
    await user.type(screen.getByLabelText(/email/i), "test@example.com");
    await user.click(screen.getByRole("button", { name: /log in/i }));
    await waitFor(() => {
      expect(screen.getByText("App Content")).toBeInTheDocument();
    });
  });

  it("shows error message on login failure", async () => {
    const user = userEvent.setup();
    vi.mocked(mockLogin).mockRejectedValue(new Error("Login failed"));
    renderLoginPage();
    await user.type(screen.getByLabelText(/email/i), "test@example.com");
    await user.click(screen.getByRole("button", { name: /log in/i }));
    await waitFor(() => {
      expect(screen.getByText(/login failed/i)).toBeInTheDocument();
    });
  });

  it("shows loading state during login", async () => {
    const user = userEvent.setup();
    let resolveLogin!: (value: any) => void;
    vi.mocked(mockLogin).mockReturnValue(
      new Promise((resolve) => {
        resolveLogin = resolve;
      }),
    );
    renderLoginPage();
    await user.type(screen.getByLabelText(/email/i), "test@example.com");
    await user.click(screen.getByRole("button", { name: /log in/i }));
    expect(screen.getByRole("button", { name: /logging in/i })).toBeDisabled();
    resolveLogin({
      user: {
        id: "u1",
        displayName: "Test",
        email: "test@example.com",
        avatarUrl: null,
      },
      keyPair: { publicKey: "pk", privateKey: "sk" },
      token: "token-123",
    });
    await waitFor(() => {
      expect(screen.queryByText(/logging in/i)).not.toBeInTheDocument();
    });
  });

  it("has a link to the register page", () => {
    renderLoginPage();
    const link = screen.getByRole("link", { name: /register|create account/i });
    expect(link).toBeInTheDocument();
  });

  it("has a link to the recovery page", () => {
    renderLoginPage();
    const link = screen.getByRole("link", { name: /recover|forgot/i });
    expect(link).toBeInTheDocument();
  });
});
