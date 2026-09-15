import { invoke } from "@tauri-apps/api/core";
import type { Message } from "../../types";

interface DecryptFailedMessageProps {
  message: Message;
}

function getDecryptErrorText(reason: Message["failureReason"]): string {
  switch (reason) {
    case "SessionNotFound":
      return "Unable to decrypt \u2014 session not established";
    case "SessionCorrupted":
      return "Unable to decrypt \u2014 session error";
    default:
      return "Unable to decrypt this message";
  }
}

function isRetryable(reason: Message["failureReason"]): boolean {
  return reason === "SessionNotFound" || reason === "SessionCorrupted";
}

export function DecryptFailedMessage({ message }: DecryptFailedMessageProps) {
  const handleRetry = () => {
    invoke("retry_decrypt", { messageId: message.id }).catch(console.error);
  };

  return (
    <div
      className="flex items-center gap-2 rounded-md px-4 py-2 opacity-60"
      data-testid={`decrypt-failed-${message.id}`}
    >
      <svg
        className="h-4 w-4 shrink-0 text-[var(--text-muted)]"
        viewBox="0 0 24 24"
        fill="currentColor"
      >
        <path d="M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zm-6 9c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zm3.1-9H8.9V6c0-1.71 1.39-3.1 3.1-3.1s3.1 1.39 3.1 3.1v2z" />
      </svg>
      <span className="text-sm text-[var(--text-muted)] italic">
        {getDecryptErrorText(message.failureReason)}
      </span>
      {isRetryable(message.failureReason) && (
        <button
          type="button"
          aria-label="Retry decryption"
          onClick={handleRetry}
          className="rounded bg-[var(--bg-tertiary)] px-2 py-0.5 text-xs text-[var(--text-secondary)] hover:bg-[var(--interactive-hover)]"
        >
          Retry
        </button>
      )}
    </div>
  );
}
