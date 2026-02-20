export interface AvatarProps {
  src?: string | null;
  name: string;
  size?: "sm" | "md" | "lg";
  className?: string;
}

const sizeMap = { sm: 24, md: 32, lg: 48 };

const palette = [
  "#d4a054",
  "#22c55e",
  "#ef4444",
  "#a78bfa",
  "#f97316",
  "#06b6d4",
  "#ec4899",
  "#14b8a6",
  "#8b5cf6",
  "#eab308",
];

function hashName(name: string): number {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return Math.abs(hash);
}

function getInitials(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) return "?";
  const parts = trimmed.split(/\s+/);
  if (parts.length === 1) return parts[0][0].toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

export function Avatar({
  src,
  name,
  size = "md",
  className = "",
}: AvatarProps) {
  const px = sizeMap[size];
  const fontSize = Math.round(px * 0.38);

  if (src) {
    return (
      <img
        src={src}
        alt={name}
        width={px}
        height={px}
        className={`rounded-full object-cover ring-1 ring-[var(--border-subtle)] ${className}`}
        style={{ width: px, height: px }}
      />
    );
  }

  const bg = palette[hashName(name) % palette.length];
  return (
    <div
      className={`flex items-center justify-center rounded-full font-semibold ring-1 ring-white/10 select-none ${className}`}
      style={{
        width: px,
        height: px,
        backgroundColor: bg,
        fontSize,
        color: "#fff",
      }}
      aria-label={name}
    >
      {getInitials(name)}
    </div>
  );
}
