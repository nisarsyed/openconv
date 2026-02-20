import { useEffect, useRef, useCallback } from "react";
import { createPortal } from "react-dom";

export interface ModalProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: React.ReactNode;
}

export function Modal({ open, onClose, title, children }: ModalProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if (e.key === "Tab" && dialogRef.current) {
        const focusable = dialogRef.current.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    },
    [onClose],
  );

  useEffect(() => {
    if (!open) return;
    previousFocusRef.current = document.activeElement as HTMLElement;
    document.addEventListener("keydown", handleKeyDown);

    requestAnimationFrame(() => {
      if (dialogRef.current) {
        const focusable = dialogRef.current.querySelector<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        );
        focusable?.focus();
      }
    });

    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      previousFocusRef.current?.focus();
    };
  }, [open, handleKeyDown]);

  if (!open) return null;

  return createPortal(
    <div
      data-testid="modal-backdrop"
      className="animate-fade-in fixed inset-0 z-50 flex items-center justify-center"
      style={{ backgroundColor: "var(--surface-overlay)" }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        className="glass animate-scale-in relative max-w-lg min-w-[340px] rounded-xl border border-[var(--border-subtle)] p-5 shadow-[var(--shadow-lg)]"
        style={{ backgroundColor: "var(--surface-popover)" }}
      >
        <button
          onClick={onClose}
          aria-label="Close"
          className="absolute top-3.5 right-3.5 rounded-md p-1 text-[var(--text-muted)] transition-all duration-150 hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)]"
        >
          <svg
            className="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
          >
            <path d="M18 6L6 18M6 6l12 12" />
          </svg>
        </button>
        {title && (
          <h2 className="mb-4 text-lg font-semibold tracking-[-0.01em] text-[var(--text-primary)]">
            {title}
          </h2>
        )}
        {children}
      </div>
    </div>,
    document.body,
  );
}
