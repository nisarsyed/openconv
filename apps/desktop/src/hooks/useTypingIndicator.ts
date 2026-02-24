import { useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";

const TYPING_DEBOUNCE_MS = 3000;
const EMPTY_TYPING: string[] = [];

export function useTypingIndicator(channelId: string): {
  typingNames: string[];
  onKeyPress: () => void;
} {
  const typingUserIds = useAppStore(
    (s) => s.typingUsers[channelId] ?? EMPTY_TYPING,
  );
  const lastSentRef = useRef<number>(0);

  const typingNames = useMemo(() => {
    const state = useAppStore.getState();
    return typingUserIds
      .map((uid) => state.usersById[uid]?.displayName)
      .filter(Boolean) as string[];
  }, [typingUserIds]);

  const onKeyPress = useCallback(() => {
    if (!channelId) return;
    const now = Date.now();
    if (now - lastSentRef.current < TYPING_DEBOUNCE_MS) {
      return;
    }
    lastSentRef.current = now;
    invoke("ws_send_typing", { channelId }).catch(console.error);
  }, [channelId]);

  return { typingNames, onKeyPress };
}
