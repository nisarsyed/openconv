export interface StatusDotProps {
  status: "online" | "idle" | "dnd" | "offline";
  size?: "sm" | "md";
  className?: string;
}

const statusLabels: Record<StatusDotProps["status"], string> = {
  online: "Online",
  idle: "Idle",
  dnd: "Do Not Disturb",
  offline: "Offline",
};

const statusColors: Record<StatusDotProps["status"], string> = {
  online: "var(--status-online)",
  idle: "var(--status-idle)",
  dnd: "var(--status-dnd)",
  offline: "var(--status-offline)",
};

const sizeMap = { sm: 8, md: 12 };

export function StatusDot({ status, size = "md", className = "" }: StatusDotProps) {
  const px = sizeMap[size];
  return (
    <span
      aria-label={statusLabels[status]}
      className={`inline-block rounded-full ring-2 ring-[var(--bg-secondary)] ${className}`}
      style={{
        width: px,
        height: px,
        backgroundColor: statusColors[status],
      }}
    />
  );
}
