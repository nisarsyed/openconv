import { useParams } from "react-router";
import { useAppStore } from "../../store";

export function ChannelHeader() {
  const { channelId } = useParams<{ channelId: string }>();
  const channel = useAppStore((s) =>
    channelId ? s.channelsById[channelId] : undefined,
  );
  const memberListVisible = useAppStore((s) => s.memberListVisible);
  const toggleMemberList = useAppStore((s) => s.toggleMemberList);

  if (!channel) return null;

  const isVoice = channel.channelType === "voice";

  return (
    <header
      data-tauri-drag-region
      className="flex h-12 items-center justify-between border-b border-[var(--border-subtle)] px-4"
      style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
    >
      <h2 className="flex items-center gap-1.5 text-sm font-semibold tracking-[-0.01em] text-[var(--text-primary)]">
        <span className="text-base text-[var(--text-muted)]">
          {isVoice ? "\u{1F50A}" : "#"}
        </span>
        {channel.name}
      </h2>

      <div className="flex items-center gap-0.5">
        {/* Search placeholder */}
        <button
          aria-label="Search"
          className="rounded-lg p-1.5 text-[var(--text-muted)] transition-all duration-150 hover:bg-[var(--interactive-hover)] hover:text-[var(--text-secondary)]"
        >
          <svg className="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path
              fillRule="evenodd"
              d="M9 3.5a5.5 5.5 0 100 11 5.5 5.5 0 000-11zM2 9a7 7 0 1112.452 4.391l3.328 3.329a.75.75 0 11-1.06 1.06l-3.329-3.328A7 7 0 012 9z"
              clipRule="evenodd"
            />
          </svg>
        </button>

        {/* Pinned messages placeholder */}
        <button
          aria-label="Pinned messages"
          className="rounded-lg p-1.5 text-[var(--text-muted)] transition-all duration-150 hover:bg-[var(--interactive-hover)] hover:text-[var(--text-secondary)]"
        >
          <svg className="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M16 12V4h1V2H7v2h1v8l-2 2v2h5.2v6h1.6v-6H18v-2l-2-2z" />
          </svg>
        </button>

        {/* Member list toggle */}
        <button
          aria-label="Toggle member list"
          aria-pressed={memberListVisible}
          onClick={toggleMemberList}
          className={`rounded-lg p-1.5 transition-all duration-150 ${
            memberListVisible
              ? "bg-[var(--interactive-active)] text-[var(--text-primary)]"
              : "text-[var(--text-muted)] hover:bg-[var(--interactive-hover)] hover:text-[var(--text-secondary)]"
          }`}
        >
          <svg className="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
            <path d="M7 8a3 3 0 100-6 3 3 0 000 6zM14.5 9a2.5 2.5 0 100-5 2.5 2.5 0 000 5zM1.615 16.428a1.224 1.224 0 01-.569-1.175 6.002 6.002 0 0111.908 0c.058.467-.172.92-.57 1.174A9.953 9.953 0 017 18a9.953 9.953 0 01-5.385-1.572zM14.5 16h-.106c.07-.297.088-.611.048-.933a7.47 7.47 0 00-1.588-3.755 4.502 4.502 0 015.874 2.636.818.818 0 01-.36.98A7.465 7.465 0 0114.5 16z" />
          </svg>
        </button>
      </div>
    </header>
  );
}
