import { useState, useRef } from "react";
import { open } from "@tauri-apps/plugin-dialog";

const MAX_MESSAGE_SIZE = 8192;
const CHAR_WARN_THRESHOLD = MAX_MESSAGE_SIZE - 500;

export interface SelectedFile {
  name: string;
  path: string;
}

interface MessageInputProps {
  onSend: (content: string, filePaths: string[]) => void;
  channelName: string;
  onKeyPress?: () => void;
}

export function MessageInput({ onSend, channelName, onKeyPress }: MessageInputProps) {
  const [text, setText] = useState("");
  const [files, setFiles] = useState<SelectedFile[]>([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const charCount = text.length;
  const overLimit = charCount > MAX_MESSAGE_SIZE;
  const showCharCount = charCount >= CHAR_WARN_THRESHOLD;
  const canSend = (text.trim().length > 0 || files.length > 0) && !overLimit;

  const handleSend = () => {
    if (!canSend) return;
    onSend(
      text.trim(),
      files.map((f) => f.path),
    );
    setText("");
    setFiles([]);
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    } else {
      onKeyPress?.();
    }
  };

  const handleTextChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setText(e.target.value);
    const el = e.target;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 144)}px`;
  };

  const handleAttachClick = async () => {
    const selected = await open({ multiple: true });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    setFiles((prev) => [
      ...prev,
      ...paths.map((p) => ({
        name: p.split(/[/\\]/).pop() ?? p,
        path: p,
      })),
    ]);
  };

  const removeFile = (index: number) => {
    setFiles((prev) => prev.filter((_, i) => i !== index));
  };

  return (
    <div className="px-4 pt-1 pb-4">
      {/* File previews */}
      {files.length > 0 && (
        <div className="mb-2 flex flex-wrap gap-2">
          {files.map((file, i) => (
            <div
              key={`${file.path}-${i}`}
              className="flex items-center gap-1.5 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-secondary)] px-2.5 py-1.5 text-xs text-[var(--text-primary)]"
              data-testid="file-preview"
            >
              <span className="max-w-[120px] truncate">{file.name}</span>
              <button
                onClick={() => removeFile(i)}
                className="text-[var(--text-muted)] transition-colors hover:text-[var(--text-primary)]"
                aria-label={`Remove ${file.name}`}
              >
                <svg
                  className="h-3 w-3"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth={2}
                >
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="flex items-end gap-2 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-secondary)] px-3 py-2 transition-all duration-200 focus-within:border-[var(--bg-accent)]/40 focus-within:shadow-[0_0_0_3px_var(--bg-accent-subtle)]">
        {/* Attachment button */}
        <button
          onClick={handleAttachClick}
          aria-label="Attach file"
          className="mb-0.5 rounded-lg p-1 text-[var(--text-muted)] transition-colors hover:text-[var(--text-secondary)]"
        >
          <svg
            className="h-5 w-5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={1.5}
          >
            <path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48" />
          </svg>
        </button>

        {/* Textarea */}
        <textarea
          ref={textareaRef}
          value={text}
          onChange={handleTextChange}
          onKeyDown={handleKeyDown}
          placeholder={`Message #${channelName}`}
          rows={1}
          className="flex-1 resize-none bg-transparent text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none"
          style={{ maxHeight: 144 }}
        />

        {/* Send button */}
        <button
          onClick={handleSend}
          disabled={!canSend}
          aria-label="Send message"
          className="mb-0.5 rounded-lg p-1 text-[var(--bg-accent)] transition-all duration-150 hover:text-[var(--bg-accent-hover)] disabled:cursor-not-allowed disabled:opacity-20"
        >
          <svg className="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
            <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
          </svg>
        </button>
      </div>

      {/* Character count */}
      {showCharCount && (
        <div
          className={`mt-1.5 text-right text-xs ${overLimit ? "text-red-400" : "text-[var(--text-muted)]"}`}
          data-testid="char-count"
        >
          {charCount} / {MAX_MESSAGE_SIZE}
        </div>
      )}
    </div>
  );
}
