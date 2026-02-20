export interface BadgeProps {
  count?: number;
  color?: string;
  children?: React.ReactNode;
  className?: string;
}

export function Badge({ count, color, children, className = "" }: BadgeProps) {
  if (!children && (!count || count <= 0)) return null;

  return (
    <span
      className={`inline-flex items-center justify-center rounded-full px-1.5 py-0.5 text-[10px] font-bold leading-none text-white shadow-sm ${className}`}
      style={{ backgroundColor: color ?? "#ef4444", minWidth: 18 }}
    >
      {children ?? count}
    </span>
  );
}
