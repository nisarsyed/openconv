import { useEffect } from "react";
import type { Notification } from "../../types";

export interface ToastProps {
  notification: Notification;
  onDismiss: (id: string) => void;
}

const typeStyles: Record<Notification["type"], string> = {
  error: "bg-red-950/90 border-red-800/50",
  success: "bg-green-950/90 border-green-800/50",
  info: "bg-[var(--surface-popover)] border-[var(--border-subtle)]",
};

export function Toast({ notification, onDismiss }: ToastProps) {
  useEffect(() => {
    if (notification.dismissAfterMs == null) return;
    const timer = setTimeout(
      () => onDismiss(notification.id),
      notification.dismissAfterMs,
    );
    return () => clearTimeout(timer);
  }, [notification.id, notification.dismissAfterMs, onDismiss]);

  return (
    <div
      className={`flex items-center gap-2 rounded-lg border px-3.5 py-2.5 text-sm text-[var(--text-primary)] shadow-[var(--shadow-lg)] animate-[slideIn_0.3s_ease-out] backdrop-blur-sm ${typeStyles[notification.type]}`}
    >
      <span className="flex-1">{notification.message}</span>
      <button
        aria-label="Dismiss"
        onClick={() => onDismiss(notification.id)}
        className="rounded-md p-0.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors"
      >
        <svg className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
          <path d="M18 6L6 18M6 6l12 12" />
        </svg>
      </button>
    </div>
  );
}
