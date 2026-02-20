import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { TypingIndicator } from "../../../components/chat/TypingIndicator";

describe("TypingIndicator", () => {
  it("renders nothing when no users are typing", () => {
    const { container } = render(<TypingIndicator userNames={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it("renders 'X is typing...' for one user", () => {
    render(<TypingIndicator userNames={["Alice"]} />);
    expect(screen.getByText("Alice is typing...")).toBeInTheDocument();
  });

  it("renders 'X and Y are typing...' for two users", () => {
    render(<TypingIndicator userNames={["Alice", "Bob"]} />);
    expect(screen.getByText("Alice and Bob are typing...")).toBeInTheDocument();
  });

  it("renders 'X, Y, and Z are typing...' for three users", () => {
    render(<TypingIndicator userNames={["Alice", "Bob", "Charlie"]} />);
    expect(
      screen.getByText("Alice, Bob, and Charlie are typing..."),
    ).toBeInTheDocument();
  });

  it("renders 'Several people are typing...' for 4+ users", () => {
    render(
      <TypingIndicator userNames={["Alice", "Bob", "Charlie", "Diana"]} />,
    );
    expect(
      screen.getByText("Several people are typing..."),
    ).toBeInTheDocument();
  });
});
