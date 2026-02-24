export function UnreadDivider() {
  return (
    <div
      className="my-2 flex items-center gap-3 px-4"
      role="separator"
      aria-label="New Messages"
      data-testid="unread-divider"
    >
      <div className="flex-1 border-t border-[var(--bg-accent)]" />
      <span className="text-[11px] font-semibold tracking-wider text-[var(--bg-accent)]">
        NEW MESSAGES
      </span>
      <div className="flex-1 border-t border-[var(--bg-accent)]" />
    </div>
  );
}
