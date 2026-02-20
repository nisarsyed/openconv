import { useState, useEffect } from "react";

export interface FilePreviewProps {
  file: File;
  onRemove: () => void;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function FilePreview({ file, onRemove }: FilePreviewProps) {
  const isImage = file.type.startsWith("image/");
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    if (!isImage) return;
    const blobUrl = URL.createObjectURL(file);
    setUrl(blobUrl);
    return () => URL.revokeObjectURL(blobUrl);
  }, [file, isImage]);

  return (
    <div className="flex items-center gap-2 rounded bg-[var(--bg-tertiary)] px-2 py-1.5 text-sm">
      {isImage && url ? (
        <img
          src={url}
          alt={file.name}
          className="h-8 w-8 rounded object-cover"
        />
      ) : (
        <span className="max-w-[160px] truncate text-[var(--text-secondary)]">
          {file.name} ({formatSize(file.size)})
        </span>
      )}
      {isImage && (
        <span className="max-w-[120px] truncate text-[var(--text-secondary)]">
          {file.name}
        </span>
      )}
      <button
        aria-label="Remove file"
        onClick={onRemove}
        className="ml-auto text-[var(--text-muted)] transition-colors hover:text-[var(--text-primary)]"
      >
        &#x2715;
      </button>
    </div>
  );
}
