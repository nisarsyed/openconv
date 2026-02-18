import { useState, useEffect } from "react";
import { useNavigate } from "react-router";

export interface SettingsSection {
  id: string;
  label: string;
  content: React.ReactNode;
}

export interface SettingsLayoutProps {
  sections: SettingsSection[];
  navFooter?: React.ReactNode;
}

export function SettingsLayout({ sections, navFooter }: SettingsLayoutProps) {
  const navigate = useNavigate();
  const [activeId, setActiveId] = useState(sections[0]?.id ?? "");

  const activeSection = sections.find((s) => s.id === activeId);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        navigate(-1);
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [navigate]);

  return (
    <div className="flex h-full">
      <nav className="flex w-52 shrink-0 flex-col bg-[var(--bg-secondary)] py-5">
        <div className="flex-1 space-y-0.5 px-2.5">
          {sections.map((section) => (
            <button
              key={section.id}
              className={`w-full rounded-lg px-3 py-1.5 text-left text-sm font-medium transition-all duration-150 ${
                section.id === activeId
                  ? "bg-[var(--interactive-active)] text-[var(--text-primary)]"
                  : "text-[var(--text-secondary)] hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)]"
              }`}
              onClick={() => setActiveId(section.id)}
            >
              {section.label}
            </button>
          ))}
        </div>

        {navFooter && (
          <div className="border-t border-[var(--border-subtle)] mx-2.5 pt-3 mt-3">
            {navFooter}
          </div>
        )}
      </nav>

      <div data-testid="settings-content" className="relative flex-1 overflow-y-auto p-8">
        <button
          aria-label="Close settings"
          onClick={() => navigate(-1)}
          className="absolute right-4 top-4 rounded-lg p-1.5 text-[var(--text-muted)] hover:bg-[var(--interactive-hover)] hover:text-[var(--text-primary)] transition-all duration-150"
        >
          <svg className="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
            <path d="M18 6L6 18M6 6l12 12" />
          </svg>
        </button>
        {activeSection?.content}
      </div>
    </div>
  );
}
