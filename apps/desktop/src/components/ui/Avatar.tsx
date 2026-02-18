export interface AvatarProps {
  src?: string | null;
  name: string;
  size?: "sm" | "md" | "lg";
  className?: string;
}

const sizeMap = { sm: 24, md: 32, lg: 48 };

const palette = [
  "#06b6d4", "#10b981", "#f59e0b", "#ec4899",
  "#ef4444", "#f97316", "#8b5cf6", "#14b8a6",
  "#6366f1", "#0ea5e9",
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

export function Avatar({ src, name, size = "md", className = "" }: AvatarProps) {
  const px = sizeMap[size];
  const fontSize = Math.round(px * 0.4);

  if (src) {
    return (
      <img
        src={src}
        alt={name}
        width={px}
        height={px}
        className={`rounded-full object-cover ${className}`}
        style={{ width: px, height: px }}
      />
    );
  }

  const bg = palette[hashName(name) % palette.length];
  return (
    <div
      className={`flex items-center justify-center rounded-full text-white font-semibold select-none ${className}`}
      style={{ width: px, height: px, backgroundColor: bg, fontSize }}
      aria-label={name}
    >
      {getInitials(name)}
    </div>
  );
}
