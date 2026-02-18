interface AuthPageLayoutProps {
  subtitle: string;
  children: React.ReactNode;
}

export function AuthPageLayout({ subtitle, children }: AuthPageLayoutProps) {
  return (
    <div className="relative min-h-screen flex items-center justify-center overflow-hidden bg-[var(--bg-tertiary)]">
      {/* Ambient gradient orbs */}
      <div className="pointer-events-none absolute inset-0 overflow-hidden">
        <div
          className="absolute -top-32 -left-32 h-96 w-96 rounded-full opacity-20 blur-[100px]"
          style={{ background: "var(--bg-accent)" }}
        />
        <div
          className="absolute -bottom-32 -right-32 h-96 w-96 rounded-full opacity-10 blur-[120px]"
          style={{ background: "var(--bg-accent)" }}
        />
      </div>

      <div className="relative z-10 w-full max-w-[420px] mx-4">
        <div className="glass rounded-2xl border border-[var(--border-subtle)] p-8 shadow-[var(--shadow-lg)] animate-scale-in">
          <h1 className="text-2xl font-bold text-center mb-0.5 tracking-[-0.03em] text-[var(--text-primary)]">
            OpenConv
          </h1>
          <p className="text-sm text-center mb-8 text-[var(--text-muted)]">
            {subtitle}
          </p>
          {children}
        </div>
      </div>
    </div>
  );
}
