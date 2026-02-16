interface AuthPageLayoutProps {
  subtitle: string;
  children: React.ReactNode;
}

export function AuthPageLayout({ subtitle, children }: AuthPageLayoutProps) {
  return (
    <div
      className="min-h-screen flex items-center justify-center"
      style={{ backgroundColor: "var(--bg-primary)" }}
    >
      <div
        className="w-full max-w-[400px] p-8 rounded-lg"
        style={{ backgroundColor: "var(--bg-secondary)" }}
      >
        <h1 className="text-2xl font-bold text-center mb-1" style={{ color: "var(--text-primary)" }}>
          OpenConv
        </h1>
        <h2 className="text-sm text-center mb-6" style={{ color: "var(--text-secondary)" }}>
          {subtitle}
        </h2>
        {children}
      </div>
    </div>
  );
}
