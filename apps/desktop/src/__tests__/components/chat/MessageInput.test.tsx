import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MessageInput } from "../../../components/chat/MessageInput";

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

  it("shows preview chips for selected files", async () => {
    const user = userEvent.setup();
    renderInput();

    const fileInput = screen.getByTestId("file-input");
    const file = new File(["content"], "test.txt", { type: "text/plain" });
    await user.upload(fileInput, file);

    expect(screen.getByText("test.txt")).toBeInTheDocument();
  });

  it("removes file from queue when preview chip X is clicked", async () => {
    const user = userEvent.setup();
    renderInput();

    const fileInput = screen.getByTestId("file-input");
    const file = new File(["content"], "test.txt", { type: "text/plain" });
    await user.upload(fileInput, file);

    expect(screen.getByText("test.txt")).toBeInTheDocument();
    await user.click(screen.getByLabelText("Remove test.txt"));
    expect(screen.queryByText("test.txt")).not.toBeInTheDocument();
  });

  it("shows character count when near the 8192 limit", async () => {
    const user = userEvent.setup();
    renderInput();

    const longText = "a".repeat(7700);
    const textarea = screen.getByPlaceholderText("Message #general");
    await user.click(textarea);
    // Use fireEvent for efficiency with large text
    await user.clear(textarea);

    // Use native fireEvent for perf with large strings
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

  it("opens file input when attachment button clicked", async () => {
    const user = userEvent.setup();
    renderInput();

    const fileInput = screen.getByTestId("file-input") as HTMLInputElement;
    const clickSpy = vi.spyOn(fileInput, "click");

    await user.click(screen.getByLabelText("Attach file"));
    expect(clickSpy).toHaveBeenCalled();
  });
});
