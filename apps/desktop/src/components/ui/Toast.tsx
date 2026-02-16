import { useEffect } from "react";
import type { Notification } from "../../types";

export interface ToastProps {
  notification: Notification;
  onDismiss: (id: string) => void;
}

const typeStyles: Record<Notification["type"], string> = {
  error: "bg-red-900/80 border-red-700",
  success: "bg-green-900/80 border-green-700",
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
      className={`flex items-center gap-2 rounded border px-3 py-2 text-sm text-[var(--text-primary)] shadow-lg animate-[slideIn_0.3s_ease-out] ${typeStyles[notification.type]}`}
    >
      <span className="flex-1">{notification.message}</span>
      <button
        aria-label="Dismiss"
        onClick={() => onDismiss(notification.id)}
        className="text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors"
      >
        &#x2715;
      </button>
    </div>
  );
}
