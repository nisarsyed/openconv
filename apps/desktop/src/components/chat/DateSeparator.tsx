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
    <div className="my-5 flex items-center gap-3 px-4" role="separator" aria-label={formatDate(date)}>
      <div className="divider-fade flex-1" />
      <span className="text-[11px] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
        {formatDate(date)}
      </span>
      <div className="divider-fade flex-1" />
    </div>
  );
}
