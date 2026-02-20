import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NewMessagesBar } from "../../../components/chat/NewMessagesBar";

describe("NewMessagesBar", () => {
  it("is hidden when visible is false", () => {
    const { container } = render(
      <NewMessagesBar visible={false} onScrollToBottom={vi.fn()} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("is visible when visible is true", () => {
    render(<NewMessagesBar visible={true} onScrollToBottom={vi.fn()} />);
    expect(screen.getByText(/jump to latest/i)).toBeInTheDocument();
  });

  it("calls onScrollToBottom when clicked", async () => {
    const user = userEvent.setup();
    const onScrollToBottom = vi.fn();
    render(
      <NewMessagesBar visible={true} onScrollToBottom={onScrollToBottom} />,
    );

    await user.click(screen.getByText(/jump to latest/i));
    expect(onScrollToBottom).toHaveBeenCalled();
  });
});
