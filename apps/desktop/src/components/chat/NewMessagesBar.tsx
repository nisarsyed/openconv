interface NewMessagesBarProps {
  visible: boolean;
  onScrollToBottom: () => void;
}

export function NewMessagesBar({
  visible,
  onScrollToBottom,
}: NewMessagesBarProps) {
  if (!visible) return null;

  return (
    <button
      onClick={onScrollToBottom}
      className="accent-gradient animate-slide-up absolute bottom-3 left-1/2 z-10 -translate-x-1/2 rounded-full px-4 py-1.5 text-sm font-semibold text-[var(--text-on-accent)] shadow-[var(--shadow-md)] transition-all duration-200 hover:shadow-[var(--shadow-lg)]"
    >
      New messages — Click to jump to latest
    </button>
  );
}
