import { useAppStore } from "../../store";

export function AppearanceSettings() {
  const theme = useAppStore((s) => s.theme);
  const toggleTheme = useAppStore((s) => s.toggleTheme);

  return (
    <div>
      <h2 className="mb-6 text-xl font-bold text-[var(--text-primary)] tracking-[-0.02em]">Appearance</h2>

      <div className="space-y-6">
        <div>
          <h3 className="mb-3 text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
            Theme
          </h3>
          <div className="flex gap-3">
            <button
              onClick={theme === "dark" ? undefined : toggleTheme}
              className={`flex flex-col items-center gap-2 rounded-xl border-2 p-4 transition-all duration-200 ${
                theme === "dark"
                  ? "border-[var(--bg-accent)] shadow-[var(--shadow-glow)]"
                  : "border-[var(--border-subtle)] hover:border-[var(--border-primary)]"
              }`}
            >
              <div className="h-16 w-24 rounded-lg bg-[#161616] border border-[#2a2a2a] flex items-end p-1.5 gap-1">
                <div className="h-full w-3 rounded-sm bg-[#111]" />
                <div className="h-full flex-1 rounded-sm bg-[#1c1c1c]" />
              </div>
              <span className="text-xs font-medium text-[var(--text-secondary)]">Dark</span>
            </button>
            <button
              onClick={theme === "light" ? undefined : toggleTheme}
              className={`flex flex-col items-center gap-2 rounded-xl border-2 p-4 transition-all duration-200 ${
                theme === "light"
                  ? "border-[var(--bg-accent)] shadow-[var(--shadow-glow)]"
                  : "border-[var(--border-subtle)] hover:border-[var(--border-primary)]"
              }`}
            >
              <div className="h-16 w-24 rounded-lg bg-[#faf7f2] border border-[#e8e4dc] flex items-end p-1.5 gap-1">
                <div className="h-full w-3 rounded-sm bg-[#e6e0d6]" />
                <div className="h-full flex-1 rounded-sm bg-[#f0ebe3]" />
              </div>
              <span className="text-xs font-medium text-[var(--text-secondary)]">Light</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
