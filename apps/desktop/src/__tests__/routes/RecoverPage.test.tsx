import { describe, it, expect } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Routes, Route } from "react-router";
import { RecoverPage } from "../../routes/RecoverPage";

describe("RecoverPage", () => {
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

  it("shows success message after submission", async () => {
    const user = userEvent.setup();
    renderRecoverPage();
    await user.type(screen.getByLabelText(/email/i), "test@example.com");
    await user.click(screen.getByRole("button", { name: /send recovery/i }));
    await waitFor(() => {
      expect(screen.getByText(/check your email/i)).toBeInTheDocument();
    });
  });

  it("has a link back to login page", () => {
    renderRecoverPage();
    const link = screen.getByRole("link", { name: /back to login|log in/i });
    expect(link).toBeInTheDocument();
  });
});
