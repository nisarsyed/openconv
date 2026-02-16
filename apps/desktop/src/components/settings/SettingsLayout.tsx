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
      <nav className="flex w-52 shrink-0 flex-col bg-[var(--bg-secondary)] py-4">
        <div className="flex-1 space-y-0.5 px-2">
          {sections.map((section) => (
            <button
              key={section.id}
              className={`w-full rounded px-3 py-1.5 text-left text-sm font-medium transition-colors ${
                section.id === activeId
                  ? "bg-[var(--bg-tertiary)] text-[var(--text-primary)]"
                  : "text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)] hover:text-[var(--text-primary)]"
              }`}
              onClick={() => setActiveId(section.id)}
            >
              {section.label}
            </button>
          ))}
        </div>

        {navFooter && (
          <div className="border-t border-[var(--border-subtle)] px-2 pt-2">
            {navFooter}
          </div>
        )}
      </nav>

      <div data-testid="settings-content" className="relative flex-1 overflow-y-auto p-8">
        <button
          aria-label="Close settings"
          onClick={() => navigate(-1)}
          className="absolute right-4 top-4 rounded p-1 text-[var(--text-muted)] hover:bg-[var(--bg-tertiary)] hover:text-[var(--text-primary)] transition-colors"
        >
          <svg className="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
            <path
              fillRule="evenodd"
              d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z"
              clipRule="evenodd"
            />
          </svg>
        </button>
        {activeSection?.content}
      </div>
    </div>
  );
}
