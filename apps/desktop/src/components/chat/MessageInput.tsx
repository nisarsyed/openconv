import { useState, useRef } from "react";

const MAX_MESSAGE_SIZE = 8192;
const CHAR_WARN_THRESHOLD = MAX_MESSAGE_SIZE - 500;

interface MessageInputProps {
  onSend: (content: string, files: File[]) => void;
  channelName: string;
}

export function MessageInput({ onSend, channelName }: MessageInputProps) {
  const [text, setText] = useState("");
  const [files, setFiles] = useState<File[]>([]);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const charCount = text.length;
  const overLimit = charCount > MAX_MESSAGE_SIZE;
  const showCharCount = charCount >= CHAR_WARN_THRESHOLD;
  const canSend = (text.trim().length > 0 || files.length > 0) && !overLimit;

  const handleSend = () => {
    if (!canSend) return;
    onSend(text.trim(), files);
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
    }
  };

  const handleTextChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setText(e.target.value);
    const el = e.target;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 144)}px`;
  };

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files) {
      setFiles((prev) => [...prev, ...Array.from(e.target.files!)]);
    }
    e.target.value = "";
  };

  const removeFile = (index: number) => {
    setFiles((prev) => prev.filter((_, i) => i !== index));
  };

  return (
    <div className="px-4 pb-4 pt-1">
      {/* File previews */}
      {files.length > 0 && (
        <div className="mb-2 flex flex-wrap gap-2">
          {files.map((file, i) => (
            <div
              key={`${file.name}-${i}`}
              className="flex items-center gap-1.5 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border-subtle)] px-2.5 py-1.5 text-xs text-[var(--text-primary)]"
              data-testid="file-preview"
            >
              <span className="max-w-[120px] truncate">{file.name}</span>
              <button
                onClick={() => removeFile(i)}
                className="text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors"
                aria-label={`Remove ${file.name}`}
              >
                <svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="flex items-end gap-2 rounded-xl bg-[var(--bg-secondary)] border border-[var(--border-subtle)] px-3 py-2 transition-all duration-200 focus-within:border-[var(--bg-accent)]/40 focus-within:shadow-[0_0_0_3px_var(--bg-accent-subtle)]">
        {/* Attachment button */}
        <button
          onClick={() => fileInputRef.current?.click()}
          aria-label="Attach file"
          className="mb-0.5 rounded-lg p-1 text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
        >
          <svg className="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5}>
            <path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48" />
          </svg>
        </button>
        <input
          ref={fileInputRef}
          type="file"
          multiple
          className="hidden"
          onChange={handleFileSelect}
          data-testid="file-input"
        />

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
          className="mb-0.5 rounded-lg p-1 text-[var(--bg-accent)] hover:text-[var(--bg-accent-hover)] disabled:opacity-20 disabled:cursor-not-allowed transition-all duration-150"
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
