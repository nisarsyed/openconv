import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, fireEvent, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { InviteModal } from "../../../components/modals/InviteModal";
import { mockGuilds } from "../../../mock/data";

const guildId = mockGuilds[0].id;

const mockWriteText = vi.fn().mockResolvedValue(undefined);

function renderModal() {
  return renderWithProviders(<InviteModal guildId={guildId} />);
}

beforeEach(() => {
  mockWriteText.mockClear();
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText: mockWriteText },
    writable: true,
    configurable: true,
  });
});

describe("InviteModal", () => {
  it("renders generated invite link", () => {
    renderModal();

    const linkInput = screen.getByRole("textbox");
    expect(linkInput).toBeInTheDocument();
    expect((linkInput as HTMLInputElement).value).toMatch(
      /https:\/\/openconv\.app\/invite\/.+/,
    );
  });

  it("copy button copies link to clipboard", async () => {
    renderModal();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /copy/i }));
    });

    expect(mockWriteText).toHaveBeenCalledWith(
      expect.stringMatching(/https:\/\/openconv\.app\/invite\/.+/),
    );
  });

  it("renders expiration dropdown with options", () => {
    renderModal();

    const select = screen.getByLabelText(/expir/i);
    expect(select).toBeInTheDocument();

    const options = select.querySelectorAll("option");
    const optionTexts = Array.from(options).map((o) => o.textContent);
    expect(optionTexts).toContain("1 hour");
    expect(optionTexts).toContain("1 day");
    expect(optionTexts).toContain("7 days");
    expect(optionTexts).toContain("Never");
  });

  it("shows confirmation feedback after copy", async () => {
    renderModal();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /copy/i }));
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /copied/i }),
      ).toBeInTheDocument();
    });
  });
});
