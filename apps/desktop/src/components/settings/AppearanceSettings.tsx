import { useAppStore } from "../../store";
import { Button } from "../ui/Button";

export function AppearanceSettings() {
  const theme = useAppStore((s) => s.theme);
  const toggleTheme = useAppStore((s) => s.toggleTheme);

  return (
    <div>
      <h2 className="mb-6 text-xl font-bold text-[var(--text-primary)]">Appearance</h2>

      <div className="space-y-4">
        <div>
          <h3 className="mb-2 text-xs font-semibold uppercase text-[var(--text-secondary)]">
            Theme
          </h3>
          <Button
            variant="secondary"
            aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}
            onClick={toggleTheme}
          >
            {theme === "dark" ? "Dark" : "Light"} — Click to switch
          </Button>
        </div>
      </div>
    </div>
  );
}
