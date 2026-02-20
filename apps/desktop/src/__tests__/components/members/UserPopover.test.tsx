import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { UserPopover } from "../../../components/members/UserPopover";

const baseProps = {
  user: {
    id: "user-1",
    displayName: "Alice Chen",
    email: "alice@example.com",
    avatarUrl: null,
  },
  member: {
    userId: "user-1",
    guildId: "guild-1",
    nickname: null,
    roles: ["role-admin", "role-mod"],
    joinedAt: "2025-06-15T00:00:00Z",
  },
  roles: [
    {
      id: "role-admin",
      guildId: "guild-1",
      name: "Admin",
      color: "#e74c3c",
      position: 2,
    },
    {
      id: "role-mod",
      guildId: "guild-1",
      name: "Moderator",
      color: "#e67e22",
      position: 1,
    },
  ],
  presence: "online" as const,
  onClose: vi.fn(),
  anchorRect: {
    top: 100,
    left: 200,
    bottom: 132,
    right: 400,
    width: 200,
    height: 32,
    x: 200,
    y: 100,
    toJSON: () => ({}),
  } as DOMRect,
};

describe("UserPopover", () => {
  it("shows avatar, display name, and roles", () => {
    render(<UserPopover {...baseProps} />);

    expect(screen.getByText("Alice Chen")).toBeInTheDocument();
    expect(screen.getByText("Admin")).toBeInTheDocument();
    expect(screen.getByText("Moderator")).toBeInTheDocument();
  });

  it("shows 'Message' button that is disabled", () => {
    render(<UserPopover {...baseProps} />);

    const messageBtn = screen.getByRole("button", { name: /message/i });
    expect(messageBtn).toBeDisabled();
  });

  it("shows join date", () => {
    render(<UserPopover {...baseProps} />);

    expect(screen.getByText(/Jun 15, 2025/)).toBeInTheDocument();
  });

  it("closes on outside click", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    const { container } = render(
      <div>
        <div data-testid="outside">outside</div>
        <UserPopover {...baseProps} onClose={onClose} />
      </div>,
    );

    await user.click(screen.getByTestId("outside"));
    expect(onClose).toHaveBeenCalled();
  });

  it("closes on Escape key", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(<UserPopover {...baseProps} onClose={onClose} />);

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });

  it("renders roles as colored badges", () => {
    render(<UserPopover {...baseProps} />);

    const adminBadge = screen.getByText("Admin");
    // Role badge text is styled with color (not backgroundColor);
    // the colored dot inside the badge has backgroundColor.
    expect(adminBadge).toHaveStyle({ color: "#e74c3c" });
  });
});
