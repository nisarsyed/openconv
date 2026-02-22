import { useEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import type {
  WsConnectionState,
  WsMessagePayload,
  WsTypingPayload,
  WsPresencePayload,
  WsMemberPayload,
} from "../types/ws";

export function useWebSocket(): void {
  const setConnectionState = useAppStore((s) => s.setConnectionState);
  const setTypingUsers = useAppStore((s) => s.setTypingUsers);

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    async function setup() {
      // Listen for connection state changes
      unlisteners.push(
        await listen<WsConnectionState>("ws:state", (event) => {
          setConnectionState(event.payload);
        }),
      );

      // Listen for incoming messages
      unlisteners.push(
        await listen<WsMessagePayload>("ws:message", (_event) => {
          // Message dispatching will be handled by section-06 messaging pipeline
        }),
      );

      // Listen for typing indicators
      unlisteners.push(
        await listen<WsTypingPayload>("ws:typing", (event) => {
          const { channel_id, user_id } = event.payload;
          const current = useAppStore.getState().typingUsers[channel_id] ?? [];
          if (!current.includes(user_id)) {
            setTypingUsers(channel_id, [...current, user_id]);
          }
          // Auto-expire typing indicator after 5 seconds
          setTimeout(() => {
            const updated =
              useAppStore.getState().typingUsers[channel_id] ?? [];
            setTypingUsers(
              channel_id,
              updated.filter((id) => id !== user_id),
            );
          }, 5000);
        }),
      );

      // Listen for presence updates
      unlisteners.push(
        await listen<WsPresencePayload>("ws:presence", (_event) => {
          // Presence updates will be dispatched to PresenceSlice
        }),
      );

      // Listen for member events
      unlisteners.push(
        await listen<WsMemberPayload>("ws:member", (_event) => {
          // Member events will be dispatched to MembersSlice
        }),
      );

      // Connect
      try {
        await invoke("ws_connect");
      } catch (e) {
        console.error("Failed to connect WebSocket:", e);
      }
    }

    setup();

    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
      invoke("ws_disconnect").catch(console.error);
    };
  }, [setConnectionState, setTypingUsers]);
}
