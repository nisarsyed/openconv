import { useState, useEffect, useCallback, useRef } from "react";
import { useAppStore } from "../../store";

interface ImageViewerProps {
  imageUrl: string;
  allImages?: string[];
}

export function ImageViewer({ imageUrl, allImages }: ImageViewerProps) {
  const closeModal = useAppStore((s) => s.closeModal);
  const hasMultiple = allImages && allImages.length > 1;
  const initialIndex = hasMultiple ? allImages.indexOf(imageUrl) : 0;
  const [currentIndex, setCurrentIndex] = useState(
    initialIndex >= 0 ? initialIndex : 0,
  );
  const overlayRef = useRef<HTMLDivElement>(null);

  const currentUrl = hasMultiple ? allImages[currentIndex] : imageUrl;

  const goNext = useCallback(() => {
    if (hasMultiple) {
      setCurrentIndex((i) => (i + 1) % allImages.length);
    }
  }, [hasMultiple, allImages]);

  const goPrev = useCallback(() => {
    if (hasMultiple) {
      setCurrentIndex((i) => (i - 1 + allImages.length) % allImages.length);
    }
  }, [hasMultiple, allImages]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        closeModal();
      } else if (e.key === "ArrowRight") {
        goNext();
      } else if (e.key === "ArrowLeft") {
        goPrev();
      }
    },
    [closeModal, goNext, goPrev],
  );

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    overlayRef.current?.focus();
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) {
      closeModal();
    }
  };

  return (
    <div
      ref={overlayRef}
      data-testid="image-viewer-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label="Image viewer"
      tabIndex={-1}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/80"
      onClick={handleBackdropClick}
    >
      {/* Close button */}
      <button
        onClick={closeModal}
        aria-label="Close"
        className="absolute top-4 right-4 z-10 text-white/80 hover:text-white text-2xl leading-none p-2"
      >
        &#x2715;
      </button>

      {/* Previous arrow */}
      {hasMultiple && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            goPrev();
          }}
          aria-label="Previous image"
          className="absolute left-4 z-10 text-white/80 hover:text-white text-3xl leading-none p-2"
        >
          &#x2039;
        </button>
      )}

      <img
        src={currentUrl}
        alt="Full size preview"
        className="max-h-[90vh] max-w-[90vw] object-contain"
        onClick={(e) => e.stopPropagation()}
      />

      {/* Next arrow */}
      {hasMultiple && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            goNext();
          }}
          aria-label="Next image"
          className="absolute right-4 z-10 text-white/80 hover:text-white text-3xl leading-none p-2"
        >
          &#x203A;
        </button>
      )}

      {/* Image counter */}
      {hasMultiple && (
        <div className="absolute bottom-4 left-1/2 -translate-x-1/2 text-white/80 text-sm">
          {currentIndex + 1} / {allImages.length}
        </div>
      )}
    </div>
  );
}
