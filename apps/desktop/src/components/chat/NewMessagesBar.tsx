interface NewMessagesBarProps {
  visible: boolean;
  onScrollToBottom: () => void;
}

export function NewMessagesBar({ visible, onScrollToBottom }: NewMessagesBarProps) {
  if (!visible) return null;

  return (
    <button
      onClick={onScrollToBottom}
      className="absolute bottom-2 left-1/2 z-10 -translate-x-1/2 rounded-full bg-[var(--bg-accent)] px-4 py-1.5 text-sm font-medium text-white shadow-lg hover:brightness-110 transition-all"
    >
      New messages — Click to jump to latest
    </button>
  );
}
