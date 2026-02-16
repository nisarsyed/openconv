export interface SpinnerProps {
  size?: "sm" | "md" | "lg";
  className?: string;
}

const sizeMap = { sm: 16, md: 24, lg: 32 };

export function Spinner({ size = "md", className = "" }: SpinnerProps) {
  const px = sizeMap[size];
  return (
    <span
      role="status"
      aria-label="Loading"
      className={`inline-block animate-spin rounded-full border-2 border-[var(--bg-accent)] border-t-transparent ${className}`}
      style={{ width: px, height: px }}
    />
  );
}
