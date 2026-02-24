export function DmListView() {
  return (
    <div className="flex flex-1 items-center justify-center text-[var(--text-muted)]">
      <div className="text-center">
        <h2 className="mb-2 text-lg font-semibold text-[var(--text-primary)]">
          Direct Messages
        </h2>
        <p className="text-sm">
          Select a conversation or start a new message.
        </p>
      </div>
    </div>
  );
}
