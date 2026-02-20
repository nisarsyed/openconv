import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Message } from "../../../components/chat/Message";
import type { Message as MessageType } from "../../../types";
import { useAppStore } from "../../../store";

function makeMsg(overrides?: Partial<MessageType>): MessageType {
  return {
    id: "msg-1",
    channelId: "ch-1",
    senderId: "u1",
    content: "hello world",
    encryptedContent: "hello world",
    nonce: "n",
    createdAt: "2026-02-14T10:00:00Z",
    editedAt: null,
    attachments: [],
    ...overrides,
  };
}

// Wrap in a minimal store provider
function renderMessage(msg: MessageType, isOwn = false) {
  // Reset store to have initial state
  useAppStore.setState(useAppStore.getInitialState(), true);
  return render(<Message message={msg} isOwn={isOwn} />);
}

describe("Message", () => {
  it("renders text content", () => {
    renderMessage(makeMsg({ content: "hello world" }));
    expect(screen.getByText("hello world")).toBeInTheDocument();
  });

  it("renders bold markdown", () => {
    renderMessage(makeMsg({ content: "this is **bold** text" }));
    const boldEl = screen.getByText("bold");
    expect(boldEl.tagName).toBe("STRONG");
  });

  it("renders italic markdown", () => {
    renderMessage(makeMsg({ content: "this is *italic* text" }));
    const italicEl = screen.getByText("italic");
    expect(italicEl.tagName).toBe("EM");
  });

  it("renders inline code markdown", () => {
    renderMessage(makeMsg({ content: "use `console.log()` here" }));
    const codeEl = screen.getByText("console.log()");
    expect(codeEl.tagName).toBe("CODE");
  });

  it("renders links as anchor tags", () => {
    renderMessage(makeMsg({ content: "visit https://example.com today" }));
    const link = screen.getByText("https://example.com");
    expect(link.tagName).toBe("A");
    expect(link).toHaveAttribute("href", "https://example.com");
  });

  it("renders file attachments", () => {
    renderMessage(
      makeMsg({
        attachments: [
          {
            id: "att-1",
            fileName: "doc.pdf",
            fileSize: 1048576,
            mimeType: "application/pdf",
            url: "https://example.com/doc.pdf",
            thumbnailUrl: null,
          },
        ],
      }),
    );
    expect(screen.getByText("doc.pdf")).toBeInTheDocument();
    expect(screen.getByText("1.0 MB")).toBeInTheDocument();
  });

  it("shows action bar on hover", () => {
    renderMessage(makeMsg(), true);
    const container = screen.getByTestId("message-msg-1");

    // Initially no actions
    expect(screen.queryByTestId("message-actions")).not.toBeInTheDocument();

    // Hover
    fireEvent.mouseEnter(container);
    expect(screen.getByTestId("message-actions")).toBeInTheDocument();

    // Leave
    fireEvent.mouseLeave(container);
    expect(screen.queryByTestId("message-actions")).not.toBeInTheDocument();
  });
});
