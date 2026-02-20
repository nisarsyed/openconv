import type { Message as MessageType } from "../../types";
import { useAppStore } from "../../store";
import { parseMarkdown } from "./markdownParser";
import { FileAttachment } from "./FileAttachment";
import { MessageActions } from "./MessageActions";
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
  const editMessage = useAppStore((s) => s.editMessage);
  const segments = parseMarkdown(message.content);

  useEffect(() => {
    if (isEditing && editRef.current) {
      editRef.current.focus();
      editRef.current.selectionStart = editRef.current.value.length;
    }
  }, [isEditing]);

  const handleEditSave = () => {
    const trimmed = editText.trim();
    if (trimmed && trimmed !== message.content) {
      editMessage(message.id, trimmed);
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

  return (
    <div
      className="group relative py-0.5 px-4 rounded-md transition-colors duration-100 hover:bg-[var(--interactive-hover)]"
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
            className="w-full resize-none rounded-lg bg-[var(--bg-tertiary)] border border-[var(--border-primary)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:border-[var(--bg-accent)] transition-colors"
            rows={1}
            data-testid="edit-input"
          />
          <div className="mt-1 text-xs text-[var(--text-muted)]">
            escape to <button className="text-[var(--bg-accent)] hover:underline" onClick={() => { setEditText(message.content); setIsEditing(false); }}>cancel</button> · enter to <button className="text-[var(--bg-accent)] hover:underline" onClick={handleEditSave}>save</button>
          </div>
        </div>
      ) : (
        <>
          <div className="text-sm text-[var(--text-primary)] leading-relaxed">
            {segments.map((seg, i) => {
              switch (seg.type) {
                case "bold":
                  return <strong key={i} className="font-semibold">{seg.content}</strong>;
                case "italic":
                  return <em key={i}>{seg.content}</em>;
                case "code":
                  return (
                    <code
                      key={i}
                      className="rounded-md bg-[var(--bg-tertiary)] px-1.5 py-0.5 text-xs font-mono text-[var(--bg-accent)]"
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
              <span className="ml-1 text-[10px] text-[var(--text-muted)]">(edited)</span>
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

      {hovered && !isEditing && (
        <MessageActions
          messageId={message.id}
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
