interface AuthPageLayoutProps {
  subtitle: string;
  children: React.ReactNode;
}

export function AuthPageLayout({ subtitle, children }: AuthPageLayoutProps) {
  return (
    <div className="relative flex min-h-screen items-center justify-center overflow-hidden bg-[var(--bg-tertiary)]">
      {/* Ambient gradient orbs */}
      <div className="pointer-events-none absolute inset-0 overflow-hidden">
        <div
          className="absolute -top-32 -left-32 h-96 w-96 rounded-full opacity-20 blur-[100px]"
          style={{ background: "var(--bg-accent)" }}
        />
        <div
          className="absolute -right-32 -bottom-32 h-96 w-96 rounded-full opacity-10 blur-[120px]"
          style={{ background: "var(--bg-accent)" }}
        />
      </div>

      <div className="relative z-10 mx-4 w-full max-w-[420px]">
        <div className="glass animate-scale-in rounded-2xl border border-[var(--border-subtle)] p-8 shadow-[var(--shadow-lg)]">
          <h1 className="mb-0.5 text-center text-2xl font-bold tracking-[-0.03em] text-[var(--text-primary)]">
            OpenConv
          </h1>
          <p className="mb-8 text-center text-sm text-[var(--text-muted)]">
            {subtitle}
          </p>
          {children}
        </div>
      </div>
    </div>
  );
}
