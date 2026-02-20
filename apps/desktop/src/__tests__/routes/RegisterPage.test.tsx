import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Routes, Route } from "react-router";
import { RegisterPage } from "../../routes/RegisterPage";
import { useAppStore } from "../../store";

vi.mock("../../mock/api", () => ({
  mockRegister: vi.fn(),
}));

import { mockRegister } from "../../mock/api";

describe("RegisterPage", () => {
  beforeEach(() => {
    useAppStore.setState(useAppStore.getInitialState());
    vi.mocked(mockRegister).mockReset();
  });

  function renderRegisterPage() {
    return render(
      <MemoryRouter initialEntries={["/register"]}>
        <Routes>
          <Route path="/register" element={<RegisterPage />} />
          <Route path="/app" element={<div>App Content</div>} />
          <Route path="/login" element={<div>Login Page</div>} />
        </Routes>
      </MemoryRouter>,
    );
  }

  it("renders email and display name inputs", () => {
    renderRegisterPage();
    expect(screen.getByLabelText(/email/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/display name/i)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /create account/i }),
    ).toBeInTheDocument();
  });

  it("validates email format -- shows error for invalid email", async () => {
    const user = userEvent.setup();
    renderRegisterPage();
    await user.type(screen.getByLabelText(/email/i), "notanemail");
    await user.type(screen.getByLabelText(/display name/i), "Valid Name");
    await user.click(screen.getByRole("button", { name: /create account/i }));
    await waitFor(() => {
      expect(screen.getByText(/valid email/i)).toBeInTheDocument();
    });
  });

  it("validates display name minimum length", async () => {
    const user = userEvent.setup();
    renderRegisterPage();
    await user.type(screen.getByLabelText(/email/i), "valid@example.com");
    await user.type(screen.getByLabelText(/display name/i), "A");
    await user.click(screen.getByRole("button", { name: /create account/i }));
    await waitFor(() => {
      expect(screen.getByText(/at least 2/i)).toBeInTheDocument();
    });
  });

  it("calls mockRegister on valid form submit", async () => {
    const user = userEvent.setup();
    vi.mocked(mockRegister).mockResolvedValue({
      user: {
        id: "u1",
        displayName: "New User",
        email: "new@example.com",
        avatarUrl: null,
      },
      keyPair: { publicKey: "pk", privateKey: "sk" },
      token: "token-123",
    });
    renderRegisterPage();
    await user.type(screen.getByLabelText(/email/i), "new@example.com");
    await user.type(screen.getByLabelText(/display name/i), "New User");
    await user.click(screen.getByRole("button", { name: /create account/i }));
    expect(mockRegister).toHaveBeenCalledWith("new@example.com", "New User");
  });

  it("navigates to /app on successful registration", async () => {
    const user = userEvent.setup();
    vi.mocked(mockRegister).mockResolvedValue({
      user: {
        id: "u1",
        displayName: "New User",
        email: "new@example.com",
        avatarUrl: null,
      },
      keyPair: { publicKey: "pk", privateKey: "sk" },
      token: "token-123",
    });
    renderRegisterPage();
    await user.type(screen.getByLabelText(/email/i), "new@example.com");
    await user.type(screen.getByLabelText(/display name/i), "New User");
    await user.click(screen.getByRole("button", { name: /create account/i }));
    await waitFor(() => {
      expect(screen.getByText("App Content")).toBeInTheDocument();
    });
  });

  it("has a link back to login page", () => {
    renderRegisterPage();
    const link = screen.getByRole("link", { name: /log in/i });
    expect(link).toBeInTheDocument();
  });
});
