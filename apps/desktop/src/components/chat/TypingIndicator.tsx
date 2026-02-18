interface TypingIndicatorProps {
  userNames: string[];
}

export function TypingIndicator({ userNames }: TypingIndicatorProps) {
  if (userNames.length === 0) return null;

  let text: string;
  if (userNames.length === 1) {
    text = `${userNames[0]} is typing`;
  } else if (userNames.length === 2) {
    text = `${userNames[0]} and ${userNames[1]} are typing`;
  } else if (userNames.length === 3) {
    text = `${userNames[0]}, ${userNames[1]}, and ${userNames[2]} are typing`;
  } else {
    text = "Several people are typing";
  }

  return (
    <div className="flex items-center gap-1.5 px-4 py-1 text-xs text-[var(--text-muted)]">
      <span className="inline-flex gap-0.5">
        <span className="animate-bounce [animation-delay:0ms] h-1 w-1 rounded-full bg-[var(--bg-accent)] opacity-70" />
        <span className="animate-bounce [animation-delay:150ms] h-1 w-1 rounded-full bg-[var(--bg-accent)] opacity-70" />
        <span className="animate-bounce [animation-delay:300ms] h-1 w-1 rounded-full bg-[var(--bg-accent)] opacity-70" />
      </span>
      <span>{text}...</span>
    </div>
  );
}
