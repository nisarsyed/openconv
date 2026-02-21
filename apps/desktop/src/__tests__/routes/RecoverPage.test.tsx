import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Routes, Route } from "react-router";
import { RecoverPage } from "../../routes/RecoverPage";
import { useAppStore } from "../../store";

vi.mock("../../bindings", () => ({
  commands: {
    authRecoverStart: vi.fn(),
    authRecoverVerify: vi.fn(),
    authRecoverComplete: vi.fn(),
  },
}));

import { commands } from "../../bindings";

describe("RecoverPage", () => {
  beforeEach(() => {
    useAppStore.setState(useAppStore.getInitialState());
    vi.mocked(commands.authRecoverStart).mockReset();
  });

  function renderRecoverPage() {
    return render(
      <MemoryRouter initialEntries={["/recover"]}>
        <Routes>
          <Route path="/recover" element={<RecoverPage />} />
          <Route path="/login" element={<div>Login Page</div>} />
        </Routes>
      </MemoryRouter>,
    );
  }

  it("renders email input and submit button", () => {
    renderRecoverPage();
    expect(screen.getByLabelText(/email/i)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /send recovery/i }),
    ).toBeInTheDocument();
  });

  it("shows verification code step after email submission", async () => {
    const user = userEvent.setup();
    vi.mocked(commands.authRecoverStart).mockResolvedValue({
      status: "ok",
      data: null,
    });
    renderRecoverPage();
    await user.type(screen.getByLabelText(/email/i), "test@example.com");
    await user.click(screen.getByRole("button", { name: /send recovery/i }));
    await waitFor(() => {
      expect(screen.getByText(/sent a recovery code/i)).toBeInTheDocument();
    });
  });

  it("has a link back to login page", () => {
    renderRecoverPage();
    const link = screen.getByRole("link", {
      name: /back to login|log in/i,
    });
    expect(link).toBeInTheDocument();
  });
});
