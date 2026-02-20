import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemberItem } from "../../../components/members/MemberItem";

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
    roles: ["role-1"],
    joinedAt: "2025-06-15T00:00:00Z",
  },
  presence: "online" as const,
  roleColor: "#e74c3c",
  onClick: vi.fn(),
};

describe("MemberItem", () => {
  it("renders avatar, display name, and status dot", () => {
    render(<MemberItem {...baseProps} />);

    expect(screen.getByLabelText("Alice Chen")).toBeInTheDocument(); // Avatar initials
    expect(screen.getByText("Alice Chen")).toBeInTheDocument();
    expect(screen.getByLabelText("Online")).toBeInTheDocument();
  });

  it("displays nickname instead of displayName when nickname is set", () => {
    render(
      <MemberItem
        {...baseProps}
        member={{ ...baseProps.member, nickname: "ServerNick" }}
      />,
    );

    expect(screen.getByText("ServerNick")).toBeInTheDocument();
    expect(screen.queryByText("Alice Chen")).not.toBeInTheDocument();
  });

  it("display name is colored by role color", () => {
    render(<MemberItem {...baseProps} />);

    const nameEl = screen.getByText("Alice Chen");
    expect(nameEl).toHaveStyle({ color: "#e74c3c" });
  });

  it("clicking the item calls onClick", async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    render(<MemberItem {...baseProps} onClick={onClick} />);

    await user.click(screen.getByRole("button"));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("renders initials avatar when no avatarUrl", () => {
    render(<MemberItem {...baseProps} />);

    // Avatar with initials should show "AC" for "Alice Chen"
    expect(screen.getByLabelText("Alice Chen")).toHaveTextContent("AC");
  });

  it("applies dimmed class when offline", () => {
    render(<MemberItem {...baseProps} presence="offline" />);

    const button = screen.getByRole("button");
    expect(button.className).toMatch(/dimmed|opacity/);
  });
});
