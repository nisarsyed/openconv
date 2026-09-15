import { useEffect, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import type {
  WsConnectionState,
  WsMessagePayload,
  WsMessageUpdatedPayload,
  WsMessageDeletedPayload,
  WsTypingPayload,
  WsPresencePayload,
  WsMemberPayload,
  WsReplayCompletePayload,
  WsReadyDataPayload,
} from "../types/ws";
import type { Guild, Channel } from "../types";

export function useWebSocket(): void {
  const setConnectionState = useAppStore((s) => s.setConnectionState);
  const setTypingUsers = useAppStore((s) => s.setTypingUsers);
  const typingTimersRef = useRef(
    new Map<string, ReturnType<typeof setTimeout>>(),
  );

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    const typingTimers = typingTimersRef.current;

    async function setup() {
      // Listen for connection state changes
      unlisteners.push(
        await listen<WsConnectionState>("ws:state", (event) => {
          setConnectionState(event.payload);
        }),
      );

      // Listen for incoming messages
      unlisteners.push(
        await listen<WsMessagePayload>("ws:message", (event) => {
          const p = event.payload;
          const store = useAppStore.getState();
          store.addMessage(p.channel_id, {
            id: p.message_id,
            channelId: p.channel_id,
            senderId: p.sender_id,
            content: p.plaintext ?? "",
            encryptedContent: "",
            nonce: "",
            createdAt: p.created_at,
            editedAt: null,
            attachments: [],
            status: p.status as "delivered" | "decrypt_failed",
            failureReason: p.failure_reason as
              | "SessionNotFound"
              | "SessionCorrupted"
              | "DecryptionFailed"
              | undefined,
          });
        }),
      );

      // Listen for message updates (edits)
      unlisteners.push(
        await listen<WsMessageUpdatedPayload>("ws:message_updated", (event) => {
          const p = event.payload;
          useAppStore.setState((draft) => {
            const msg = draft.messagesById[p.message_id];
            if (msg) {
              msg.content = p.plaintext ?? msg.content;
              msg.editedAt = p.edited_at;
            }
          });
        }),
      );

      // Listen for message deletions
      unlisteners.push(
        await listen<WsMessageDeletedPayload>("ws:message_deleted", (event) => {
          const p = event.payload;
          useAppStore.getState().deleteMessage(p.message_id);
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

          // Clear existing timer for this user
          const timerKey = `${channel_id}:${user_id}`;
          const existing = typingTimers.get(timerKey);
          if (existing) clearTimeout(existing);

          // Auto-expire typing indicator after 5 seconds
          typingTimers.set(
            timerKey,
            setTimeout(() => {
              typingTimers.delete(timerKey);
              const updated =
                useAppStore.getState().typingUsers[channel_id] ?? [];
              setTypingUsers(
                channel_id,
                updated.filter((id) => id !== user_id),
              );
            }, 5000),
          );
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

      // Listen for replay completion
      unlisteners.push(
        await listen<WsReplayCompletePayload>(
          "ws:replay_complete",
          (_event) => {
            // Replay complete - will be used for offline catchup in later sections
          },
        ),
      );

      // Listen for ready data (guilds/channels after authentication)
      unlisteners.push(
        await listen<WsReadyDataPayload>("ws:ready_data", (event) => {
          const p = event.payload;
          const store = useAppStore.getState();

          // Set current user
          store.setCurrentUser({
            id: p.user_id,
            displayName: p.display_name,
            email: p.email,
            avatarUrl: p.avatar_url,
          });

          // Map guilds
          const guilds: Guild[] = p.guilds.map((g) => ({
            id: g.id,
            name: g.name,
            ownerId: g.owner_id,
            iconUrl: g.icon_url,
          }));
          store.setGuilds(guilds);

          // Flatten and map channels
          const channels: Channel[] = p.guilds.flatMap((g) =>
            g.channels.map((ch) => ({
              id: ch.id,
              guildId: ch.guild_id,
              name: ch.name,
              channelType: (ch.channel_type === "voice" ? "voice" : "text") as
                | "text"
                | "voice",
              position: ch.position,
              category: null,
            })),
          );
          store.setChannels(channels);
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
      // Clean up typing timers
      for (const timer of typingTimers.values()) {
        clearTimeout(timer);
      }
      typingTimers.clear();
      invoke("ws_disconnect").catch(console.error);
    };
  }, [setConnectionState, setTypingUsers]);
}
