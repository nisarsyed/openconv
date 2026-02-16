interface DateSeparatorProps {
  date: string;
}

function formatDate(dateStr: string): string {
  const d = new Date(dateStr + "T00:00:00");
  return d.toLocaleDateString("en-US", {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

export function DateSeparator({ date }: DateSeparatorProps) {
  return (
    <div className="my-4 flex items-center gap-2 px-4" role="separator" aria-label={formatDate(date)}>
      <div className="flex-1 border-t border-[var(--border-subtle)]" />
      <span className="text-xs font-semibold text-[var(--text-muted)]">
        {formatDate(date)}
      </span>
      <div className="flex-1 border-t border-[var(--border-subtle)]" />
    </div>
  );
}
