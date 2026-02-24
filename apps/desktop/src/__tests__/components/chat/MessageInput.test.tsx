import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MessageInput } from "../../../components/chat/MessageInput";

const mockOpen = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => mockOpen(...args),
}));

function renderInput(onSend = vi.fn()) {
  return {
    onSend,
    ...render(<MessageInput onSend={onSend} channelName="general" />),
  };
}

describe("MessageInput", () => {
  it("renders a textarea", () => {
    renderInput();
    expect(screen.getByPlaceholderText("Message #general")).toBeInTheDocument();
  });

  it("sends message on Enter key", async () => {
    const user = userEvent.setup();
    const { onSend } = renderInput();

    const textarea = screen.getByPlaceholderText("Message #general");
    await user.type(textarea, "hello");
    await user.keyboard("{Enter}");

    expect(onSend).toHaveBeenCalledWith("hello", []);
    expect(textarea).toHaveValue("");
  });

  it("inserts newline on Shift+Enter", async () => {
    const user = userEvent.setup();
    const { onSend } = renderInput();

    const textarea = screen.getByPlaceholderText("Message #general");
    await user.type(textarea, "line1");
    await user.keyboard("{Shift>}{Enter}{/Shift}");

    expect(onSend).not.toHaveBeenCalled();
    expect(textarea).toHaveValue("line1\n");
  });

  it("disables send button when textarea is empty", () => {
    renderInput();
    const sendButton = screen.getByLabelText("Send message");
    expect(sendButton).toBeDisabled();
  });

  it("enables send button when textarea has content", async () => {
    const user = userEvent.setup();
    renderInput();

    await user.type(screen.getByPlaceholderText("Message #general"), "hi");
    expect(screen.getByLabelText("Send message")).not.toBeDisabled();
  });

  it("opens native file dialog when attachment button clicked", async () => {
    const user = userEvent.setup();
    mockOpen.mockResolvedValue(null);
    renderInput();

    await user.click(screen.getByLabelText("Attach file"));
    expect(mockOpen).toHaveBeenCalledWith({ multiple: true });
  });

  it("shows file preview chip after selecting a file via dialog", async () => {
    const user = userEvent.setup();
    mockOpen.mockResolvedValue(["/home/user/docs/test.txt"]);
    renderInput();

    await user.click(screen.getByLabelText("Attach file"));
    expect(screen.getByText("test.txt")).toBeInTheDocument();
  });

  it("removes file from queue when preview chip X is clicked", async () => {
    const user = userEvent.setup();
    mockOpen.mockResolvedValue(["/home/user/docs/test.txt"]);
    renderInput();

    await user.click(screen.getByLabelText("Attach file"));
    expect(screen.getByText("test.txt")).toBeInTheDocument();

    await user.click(screen.getByLabelText("Remove test.txt"));
    expect(screen.queryByText("test.txt")).not.toBeInTheDocument();
  });

  it("sends file paths with message", async () => {
    const user = userEvent.setup();
    mockOpen.mockResolvedValue(["/home/user/docs/test.txt"]);
    const { onSend } = renderInput();

    await user.click(screen.getByLabelText("Attach file"));
    const textarea = screen.getByPlaceholderText("Message #general");
    await user.type(textarea, "check this out");
    await user.keyboard("{Enter}");

    expect(onSend).toHaveBeenCalledWith("check this out", [
      "/home/user/docs/test.txt",
    ]);
  });

  it("shows character count when near the 8192 limit", async () => {
    const user = userEvent.setup();
    renderInput();

    const longText = "a".repeat(7700);
    const textarea = screen.getByPlaceholderText("Message #general");
    await user.click(textarea);
    await user.clear(textarea);

    const { fireEvent } = await import("@testing-library/react");
    fireEvent.change(textarea, { target: { value: longText } });

    expect(screen.getByTestId("char-count")).toBeInTheDocument();
  });

  it("does not show character count when well under limit", async () => {
    const user = userEvent.setup();
    renderInput();

    await user.type(screen.getByPlaceholderText("Message #general"), "hello");
    expect(screen.queryByTestId("char-count")).not.toBeInTheDocument();
  });

  it("disables send when over character limit", async () => {
    renderInput();

    const textarea = screen.getByPlaceholderText("Message #general");
    const { fireEvent } = await import("@testing-library/react");
    fireEvent.change(textarea, { target: { value: "a".repeat(8193) } });

    expect(screen.getByLabelText("Send message")).toBeDisabled();
    expect(screen.getByTestId("char-count")).toHaveTextContent("8193 / 8192");
  });
});
