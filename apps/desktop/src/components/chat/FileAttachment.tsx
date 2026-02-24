import { convertFileSrc } from "@tauri-apps/api/core";
import type { FileAttachment as FileAttachmentType } from "../../types";
import { useAppStore } from "../../store";

interface FileAttachmentProps {
  attachment: FileAttachmentType;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const IMAGE_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
]);

function resolveImageSrc(attachment: FileAttachmentType): string {
  if (attachment.thumbnailLocalPath) return convertFileSrc(attachment.thumbnailLocalPath);
  if (attachment.localPath) return convertFileSrc(attachment.localPath);
  return attachment.thumbnailUrl ?? attachment.url;
}

function resolveFullSrc(attachment: FileAttachmentType): string {
  if (attachment.localPath) return convertFileSrc(attachment.localPath);
  return attachment.url;
}

export function FileAttachment({ attachment }: FileAttachmentProps) {
  const openModal = useAppStore((s) => s.openModal);
  const isImage = IMAGE_TYPES.has(attachment.mimeType);

  if (isImage) {
    const thumbSrc = resolveImageSrc(attachment);
    const fullSrc = resolveFullSrc(attachment);

    return (
      <button
        className="mt-1.5 block max-w-[300px] cursor-pointer overflow-hidden rounded-lg border border-[var(--border-subtle)]"
        onClick={() => openModal("imageViewer", { imageUrl: fullSrc })}
      >
        <img
          src={thumbSrc}
          alt={attachment.fileName}
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
          {attachment.fileName}
        </div>
        <div className="text-xs text-[var(--text-muted)]">
          {formatFileSize(attachment.fileSize)}
        </div>
      </div>
      <a
        href={attachment.localPath ? convertFileSrc(attachment.localPath) : attachment.url}
        download={attachment.fileName}
        className="rounded-lg p-1.5 text-[var(--text-muted)] transition-colors hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)]"
        aria-label={`Download ${attachment.fileName}`}
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
      </a>
    </div>
  );
}

export { formatFileSize };
