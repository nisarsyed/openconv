import { convertFileSrc } from "@tauri-apps/api/core";
import { useAppStore } from "../../store";

interface FilePreviewCardProps {
  fileId: string;
  fileName: string;
  fileSize: number;
  mimeType: string;
  localPath: string | null;
  thumbnailPath: string | null;
  isDownloading: boolean;
  onDownload: () => void;
}

const IMAGE_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
]);

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function FilePreviewCard({
  fileName,
  fileSize,
  mimeType,
  localPath,
  thumbnailPath,
  isDownloading,
  onDownload,
}: FilePreviewCardProps) {
  const openModal = useAppStore((s) => s.openModal);
  const isImage = IMAGE_TYPES.has(mimeType);

  if (isImage && thumbnailPath) {
    const thumbSrc = convertFileSrc(thumbnailPath);
    const fullSrc = localPath ? convertFileSrc(localPath) : thumbSrc;

    return (
      <button
        className="mt-1.5 block max-w-[300px] cursor-pointer overflow-hidden rounded-lg border border-[var(--border-subtle)]"
        onClick={() => openModal("imageViewer", { imageUrl: fullSrc })}
      >
        <img
          src={thumbSrc}
          alt={fileName}
          className="max-h-[300px] w-auto rounded-lg object-cover"
          loading="lazy"
        />
      </button>
    );
  }

  return (
    <div className="mt-1.5 flex max-w-[400px] items-center gap-3 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-secondary)] p-3">
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
        </div>
      </div>
      {localPath ? (
        <button
          onClick={onDownload}
          className="rounded-lg p-1.5 text-[var(--text-muted)] transition-colors hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)]"
          aria-label={`Save ${fileName}`}
        >
          <svg
            className="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3" />
          </svg>
        </button>
      ) : (
        <button
          onClick={onDownload}
          disabled={isDownloading}
          className="rounded-lg p-1.5 text-[var(--text-muted)] transition-colors hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)] disabled:opacity-50"
          aria-label={isDownloading ? "Downloading..." : `Download ${fileName}`}
        >
          {isDownloading ? (
            <svg
              className="h-4 w-4 animate-spin"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
            >
              <circle cx="12" cy="12" r="10" opacity="0.25" />
              <path d="M12 2a10 10 0 0110 10" />
            </svg>
          ) : (
            <svg
              className="h-4 w-4"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3" />
            </svg>
          )}
        </button>
      )}
    </div>
  );
}
