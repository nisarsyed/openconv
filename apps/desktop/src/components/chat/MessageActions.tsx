import { useAppStore } from "../../store";

interface MessageActionsProps {
  messageId: string;
  isOwn: boolean;
  onEdit?: () => void;
}

export function MessageActions({ messageId, isOwn, onEdit }: MessageActionsProps) {
  const deleteMessage = useAppStore((s) => s.deleteMessage);
  const openModal = useAppStore((s) => s.openModal);

  return (
    <div
      className="absolute -top-3 right-4 flex items-center gap-0.5 rounded border border-[var(--border-subtle)] bg-[var(--bg-secondary)] p-0.5 shadow-sm"
      data-testid="message-actions"
    >
      <button
        aria-label="Reply"
        className="rounded p-1 text-[var(--text-muted)] hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)]"
      >
        <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
          <path d="M10 9V5l-7 7 7 7v-4.1c5 0 8.5 1.6 11 5.1-1-5-4-10-11-11z" />
        </svg>
      </button>
      <button
        aria-label="Add reaction"
        className="rounded p-1 text-[var(--text-muted)] hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)]"
      >
        <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zm-5-6c.78 2.34 2.72 4 5 4s4.22-1.66 5-4H7zm1-2h2V9H8v3zm6 0h2V9h-2v3z" />
        </svg>
      </button>
      {isOwn && (
        <>
          <button
            aria-label="Edit message"
            onClick={onEdit}
            className="rounded p-1 text-[var(--text-muted)] hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)]"
          >
            <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
              <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a1 1 0 000-1.41l-2.34-2.34a1 1 0 00-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z" />
            </svg>
          </button>
          <button
            aria-label="Delete message"
            onClick={() =>
              openModal("confirm", {
                title: "Delete Message",
                message: "Are you sure you want to delete this message?",
                onConfirm: () => deleteMessage(messageId),
              })
            }
            className="rounded p-1 text-[var(--status-danger)] hover:bg-[var(--interactive-hover)]"
          >
            <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
              <path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z" />
            </svg>
          </button>
        </>
      )}
    </div>
  );
}
