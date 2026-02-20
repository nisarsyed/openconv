export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "danger" | "ghost";
  size?: "sm" | "md" | "lg";
}

const variantClasses: Record<NonNullable<ButtonProps["variant"]>, string> = {
  primary:
    "accent-gradient text-[var(--text-on-accent)] shadow-[var(--shadow-sm)] hover:shadow-[var(--shadow-md)] hover:brightness-110 active:brightness-95 active:scale-[0.98]",
  secondary:
    "bg-[var(--bg-tertiary)] text-[var(--text-primary)] hover:bg-[var(--interactive-active)] active:scale-[0.98]",
  danger:
    "bg-red-600/90 text-white hover:bg-red-600 active:bg-red-700 active:scale-[0.98]",
  ghost:
    "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)] active:bg-[var(--interactive-active)]",
};

const sizeClasses: Record<NonNullable<ButtonProps["size"]>, string> = {
  sm: "px-2.5 py-1 text-xs",
  md: "px-3.5 py-1.5 text-sm",
  lg: "px-5 py-2.5 text-sm",
};

export function Button({
  variant = "primary",
  size = "md",
  className = "",
  disabled,
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      className={`focus-ring inline-flex items-center justify-center rounded-lg font-semibold tracking-[-0.01em] transition-all duration-200 ease-out ${variantClasses[variant]} ${sizeClasses[size]} ${disabled ? "pointer-events-none cursor-not-allowed opacity-40" : "cursor-pointer"} ${className}`}
      disabled={disabled}
      {...rest}
    >
      {children}
    </button>
  );
}
