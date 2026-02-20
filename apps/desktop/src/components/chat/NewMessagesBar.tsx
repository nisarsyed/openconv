interface NewMessagesBarProps {
  visible: boolean;
  onScrollToBottom: () => void;
}

export function NewMessagesBar({ visible, onScrollToBottom }: NewMessagesBarProps) {
  if (!visible) return null;

  return (
    <button
      onClick={onScrollToBottom}
      className="absolute bottom-3 left-1/2 z-10 -translate-x-1/2 rounded-full accent-gradient px-4 py-1.5 text-sm font-semibold text-[var(--text-on-accent)] shadow-[var(--shadow-md)] hover:shadow-[var(--shadow-lg)] transition-all duration-200 animate-slide-up"
    >
      New messages — Click to jump to latest
    </button>
  );
}
