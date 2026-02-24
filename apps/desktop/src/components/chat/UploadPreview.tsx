interface UploadPreviewProps {
  fileName: string;
  fileSize: number;
  progress: number; // 0 to 1
  onCancel: () => void;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function UploadPreview({
  fileName,
  fileSize,
  progress,
  onCancel,
}: UploadPreviewProps) {
  const percent = Math.round(progress * 100);

  return (
    <div className="flex items-center gap-3 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-secondary)] p-3">
      <svg
        className="h-8 w-8 shrink-0 text-[var(--text-muted)]"
        viewBox="0 0 24 24"
        fill="currentColor"
      >
        <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8l-6-6zm4 18H6V4h7v5h5v11z" />
      </svg>
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium text-[var(--text-primary)]">
          {fileName}
        </div>
        <div className="text-xs text-[var(--text-muted)]">
          {formatFileSize(fileSize)}
          {progress > 0 && progress < 1 && ` \u2022 ${percent}%`}
        </div>
        {progress > 0 && progress < 1 && (
          <div className="mt-1 h-1 overflow-hidden rounded-full bg-[var(--border-subtle)]">
            <div
              className="h-full rounded-full bg-[var(--bg-accent)] transition-all duration-200"
              style={{ width: `${percent}%` }}
            />
          </div>
        )}
      </div>
      <button
        onClick={onCancel}
        className="rounded-lg p-1 text-[var(--text-muted)] transition-colors hover:text-[var(--text-primary)]"
        aria-label="Cancel upload"
      >
        <svg
          className="h-4 w-4"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={2}
        >
          <path d="M18 6L6 18M6 6l12 12" />
        </svg>
      </button>
    </div>
  );
}
