import { useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";

const TYPING_DEBOUNCE_MS = 3000;

export function useTypingIndicator(channelId: string): {
  typingUsers: string[];
  sendTyping: () => void;
} {
  const typingUsers = useAppStore(
    (s) => s.typingUsers[channelId] ?? [],
  );
  const lastSentRef = useRef<number>(0);

  const sendTyping = useCallback(() => {
    const now = Date.now();
    if (now - lastSentRef.current < TYPING_DEBOUNCE_MS) {
      return;
    }
    lastSentRef.current = now;
    invoke("ws_send_typing", { channelId }).catch(console.error);
  }, [channelId]);

  return { typingUsers, sendTyping };
}
