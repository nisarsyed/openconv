import { useNavigate } from "react-router";
import { useAppStore } from "../../store";
import { Avatar } from "../ui/Avatar";
import { StatusDot } from "../ui/StatusDot";

export function UserPanel() {
  const navigate = useNavigate();
  const currentUser = useAppStore((s) => s.currentUser);
  const presenceByUserId = useAppStore((s) => s.presenceByUserId);

  if (!currentUser) return null;

  const status = presenceByUserId[currentUser.id] ?? "offline";

  return (
    <div className="flex items-center gap-2 border-t border-[var(--border-subtle)] bg-[var(--bg-tertiary)] px-2 py-2">
      <div className="relative">
        <Avatar src={currentUser.avatarUrl} name={currentUser.displayName} size="md" />
        <span className="absolute -bottom-0.5 -right-0.5">
          <StatusDot status={status} size="sm" />
        </span>
      </div>

      <span className="flex-1 truncate text-sm font-medium text-[var(--text-primary)]">
        {currentUser.displayName}
      </span>

      <button
        aria-label="User settings"
        onClick={() => navigate("/app/settings")}
        className="rounded p-1 text-[var(--text-muted)] hover:bg-[var(--bg-secondary)] hover:text-[var(--text-primary)] transition-colors"
      >
        <svg className="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
          <path
            fillRule="evenodd"
            d="M7.84 1.804A1 1 0 018.82 1h2.36a1 1 0 01.98.804l.331 1.652a6.993 6.993 0 011.929 1.115l1.598-.54a1 1 0 011.186.447l1.18 2.044a1 1 0 01-.205 1.251l-1.267 1.113a7.047 7.047 0 010 2.228l1.267 1.113a1 1 0 01.206 1.25l-1.18 2.045a1 1 0 01-1.187.447l-1.598-.54a6.993 6.993 0 01-1.929 1.115l-.33 1.652a1 1 0 01-.98.804H8.82a1 1 0 01-.98-.804l-.331-1.652a6.993 6.993 0 01-1.929-1.115l-1.598.54a1 1 0 01-1.186-.447l-1.18-2.044a1 1 0 01.205-1.251l1.267-1.114a7.05 7.05 0 010-2.227L1.821 7.773a1 1 0 01-.206-1.25l1.18-2.045a1 1 0 011.187-.447l1.598.54A6.993 6.993 0 017.51 3.456l.33-1.652zM10 13a3 3 0 100-6 3 3 0 000 6z"
            clipRule="evenodd"
          />
        </svg>
      </button>
    </div>
  );
}
