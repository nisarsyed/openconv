import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DecryptFailedMessage } from "../../../components/chat/DecryptFailedMessage";
import type { Message } from "../../../types";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

function makeDecryptFailedMessage(
  overrides: Partial<Message> = {},
): Message {
  return {
    id: "msg-1",
    channelId: "ch-1",
    senderId: "user-1",
    content: "",
    encryptedContent: "",
    nonce: "",
    createdAt: "2026-02-24T00:00:00Z",
    editedAt: null,
    attachments: [],
    status: "decrypt_failed",
    ...overrides,
  };
}

describe("DecryptFailedMessage", () => {
  it("renders tombstone with 'Unable to decrypt' text", () => {
    const msg = makeDecryptFailedMessage();
    render(<DecryptFailedMessage message={msg} />);
    expect(screen.getByText(/unable to decrypt/i)).toBeInTheDocument();
  });

  it("shows retry button for SessionNotFound failure", () => {
    const msg = makeDecryptFailedMessage({
      failureReason: "SessionNotFound",
    });
    render(<DecryptFailedMessage message={msg} />);
    expect(screen.getByText(/unable to decrypt/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /retry/i })).toBeInTheDocument();
  });

  it("shows retry button for SessionCorrupted failure", () => {
    const msg = makeDecryptFailedMessage({
      failureReason: "SessionCorrupted",
    });
    render(<DecryptFailedMessage message={msg} />);
    expect(screen.getByRole("button", { name: /retry/i })).toBeInTheDocument();
  });

  it("does not show retry button for DecryptionFailed", () => {
    const msg = makeDecryptFailedMessage({
      failureReason: "DecryptionFailed",
    });
    render(<DecryptFailedMessage message={msg} />);
    expect(screen.getByText(/unable to decrypt/i)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /retry/i }),
    ).not.toBeInTheDocument();
  });

  it("does not show retry button when failureReason is undefined", () => {
    const msg = makeDecryptFailedMessage({ failureReason: undefined });
    render(<DecryptFailedMessage message={msg} />);
    expect(
      screen.queryByRole("button", { name: /retry/i }),
    ).not.toBeInTheDocument();
  });

  it("calls retry_decrypt Tauri command when retry is clicked", async () => {
    const user = userEvent.setup();
    const msg = makeDecryptFailedMessage({
      failureReason: "SessionNotFound",
    });
    render(<DecryptFailedMessage message={msg} />);

    await user.click(screen.getByRole("button", { name: /retry/i }));

    expect(mockInvoke).toHaveBeenCalledWith("retry_decrypt", {
      messageId: "msg-1",
    });
  });

  it("shows specific text for SessionNotFound", () => {
    const msg = makeDecryptFailedMessage({
      failureReason: "SessionNotFound",
    });
    render(<DecryptFailedMessage message={msg} />);
    expect(
      screen.getByText(/session not established/i),
    ).toBeInTheDocument();
  });

  it("shows specific text for SessionCorrupted", () => {
    const msg = makeDecryptFailedMessage({
      failureReason: "SessionCorrupted",
    });
    render(<DecryptFailedMessage message={msg} />);
    expect(screen.getByText(/session error/i)).toBeInTheDocument();
  });
});
