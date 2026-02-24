import type { Message as MessageType } from "../../types";
import { useAppStore } from "../../store";
import { invoke } from "@tauri-apps/api/core";
import { parseMarkdown } from "./markdownParser";
import { FileAttachment } from "./FileAttachment";
import { MessageActions } from "./MessageActions";
import { DecryptFailedMessage } from "./DecryptFailedMessage";
import { useState, useRef, useEffect } from "react";

interface MessageProps {
  message: MessageType;
  isOwn: boolean;
}

export function Message({ message, isOwn }: MessageProps) {
  const [hovered, setHovered] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [editText, setEditText] = useState(message.content);
  const editRef = useRef<HTMLTextAreaElement>(null);
  const segments = parseMarkdown(message.content);

  useEffect(() => {
    if (isEditing && editRef.current) {
      editRef.current.focus();
      editRef.current.selectionStart = editRef.current.value.length;
    }
  }, [isEditing]);

  // Render decrypt-failed tombstone
  if (message.status === "decrypt_failed") {
    return <DecryptFailedMessage message={message} />;
  }

  const handleEditSave = async () => {
    const trimmed = editText.trim();
    if (trimmed && trimmed !== message.content) {
      try {
        await invoke("edit_message", {
          channelId: message.channelId,
          messageId: message.id,
          newPlaintext: trimmed,
        });
      } catch {
        // Edit failed - revert
      }
    }
    setIsEditing(false);
  };

  const handleEditKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleEditSave();
    } else if (e.key === "Escape") {
      setEditText(message.content);
      setIsEditing(false);
    }
  };

  const isPending = message.status === "pending";
  const isFailed = message.status === "failed";

  return (
    <div
      className={`group relative rounded-md px-4 py-0.5 transition-colors duration-100 hover:bg-[var(--interactive-hover)] ${isPending ? "opacity-50" : ""}`}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      data-testid={`message-${message.id}`}
    >
      {isEditing ? (
        <div className="py-1">
          <textarea
            ref={editRef}
            value={editText}
            onChange={(e) => setEditText(e.target.value)}
            onKeyDown={handleEditKeyDown}
            className="w-full resize-none rounded-lg border border-[var(--border-primary)] bg-[var(--bg-tertiary)] px-3 py-2 text-sm text-[var(--text-primary)] transition-colors focus:border-[var(--bg-accent)] focus:outline-none"
            rows={1}
            data-testid="edit-input"
          />
          <div className="mt-1 text-xs text-[var(--text-muted)]">
            escape to{" "}
            <button
              className="text-[var(--bg-accent)] hover:underline"
              onClick={() => {
                setEditText(message.content);
                setIsEditing(false);
              }}
            >
              cancel
            </button>{" "}
            · enter to{" "}
            <button
              className="text-[var(--bg-accent)] hover:underline"
              onClick={handleEditSave}
            >
              save
            </button>
          </div>
        </div>
      ) : (
        <>
          <div className="flex items-center gap-1.5">
            <div className="text-sm leading-relaxed text-[var(--text-primary)]">
              {segments.map((seg, i) => {
                switch (seg.type) {
                  case "bold":
                    return (
                      <strong key={i} className="font-semibold">
                        {seg.content}
                      </strong>
                    );
                  case "italic":
                    return <em key={i}>{seg.content}</em>;
                  case "code":
                    return (
                      <code
                        key={i}
                        className="rounded-md bg-[var(--bg-tertiary)] px-1.5 py-0.5 font-mono text-xs text-[var(--bg-accent)]"
                      >
                        {seg.content}
                      </code>
                    );
                  case "link":
                    return (
                      <a
                        key={i}
                        href={seg.url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-[var(--bg-accent)] hover:underline"
                      >
                        {seg.url}
                      </a>
                    );
                  default:
                    return <span key={i}>{seg.content}</span>;
                }
              })}
              {message.editedAt && (
                <span className="ml-1 text-[10px] text-[var(--text-muted)]">
                  (edited)
                </span>
              )}
            </div>
            {isPending && (
              <svg
                className="h-3 w-3 text-[var(--text-muted)]"
                viewBox="0 0 24 24"
                fill="currentColor"
                data-testid="pending-icon"
              >
                <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10 10-4.5 10-10S17.5 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zm.5-13H11v6l5.2 3.2.8-1.3-4.5-2.7V7z" />
              </svg>
            )}
            {isFailed && (
              <span
                className="text-xs text-red-400"
                data-testid="failed-indicator"
              >
                Failed to send
              </span>
            )}
          </div>

          {message.attachments.length > 0 && (
            <div className="mt-1.5">
              {message.attachments.map((att) => (
                <FileAttachment key={att.id} attachment={att} />
              ))}
            </div>
          )}
        </>
      )}

      {hovered && !isEditing && !isPending && !isFailed && (
        <MessageActions
          messageId={message.id}
          channelId={message.channelId}
          isOwn={isOwn}
          onEdit={() => {
            setEditText(message.content);
            setIsEditing(true);
          }}
        />
      )}
    </div>
  );
}
