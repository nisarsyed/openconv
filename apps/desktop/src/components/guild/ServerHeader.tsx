import { useParams, useNavigate } from "react-router";
import { useAppStore } from "../../store";
import { Dropdown, type DropdownItem } from "../ui/Dropdown";

export function ServerHeader() {
  const { guildId } = useParams<{ guildId: string }>();
  const guild = useAppStore((s) =>
    guildId ? s.guildsById[guildId] : undefined,
  );
  const openModal = useAppStore((s) => s.openModal);
  const leaveGuild = useAppStore((s) => s.leaveGuild);
  const navigate = useNavigate();

  if (!guild) return null;

  const items: DropdownItem[] = [
    { id: "settings", label: "Guild Settings" },
    { id: "invite", label: "Create Invite" },
    { id: "leave", label: "Leave Guild", danger: true },
  ];

  const handleSelect = (itemId: string) => {
    switch (itemId) {
      case "settings":
        navigate(`/app/guild/${guild.id}/settings`);
        break;
      case "invite":
        openModal("invite", { guildId: guild.id });
        break;
      case "leave":
        openModal("confirm", {
          title: "Leave Guild",
          message: `Are you sure you want to leave ${guild.name}?`,
          onConfirm: () => leaveGuild(guild.id),
        });
        break;
    }
  };

  return (
    <div
      data-tauri-drag-region
      className="flex h-12 items-center border-b border-[var(--border-subtle)] px-4"
      style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
    >
      <Dropdown
        trigger={
          <button className="flex w-full items-center justify-between text-sm font-semibold tracking-[-0.01em] text-[var(--text-primary)] transition-colors hover:text-[var(--text-secondary)]">
            <span className="truncate">{guild.name}</span>
            <svg
              className="ml-1.5 h-4 w-4 text-[var(--text-muted)]"
              viewBox="0 0 20 20"
              fill="currentColor"
            >
              <path
                fillRule="evenodd"
                d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z"
                clipRule="evenodd"
              />
            </svg>
          </button>
        }
        items={items}
        onSelect={handleSelect}
      />
    </div>
  );
}
