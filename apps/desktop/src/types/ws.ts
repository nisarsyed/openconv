export type WsConnectionState =
  | { status: "Disconnected" }
  | { status: "Connecting"; attempt: number }
  | { status: "Connected" }
  | { status: "Authenticated" }
  | { status: "Reconnecting"; attempt: number; next_retry_ms: number }
  | { status: "Failed"; reason: string };

export interface WsMessagePayload {
  channel_id: string;
  message_id: string;
  sender_id: string;
  plaintext: string | null;
  /** "delivered", "pending", "decrypt_failed" */
  status: string;
  failure_reason?: string;
  created_at: string;
}

export interface WsMessageUpdatedPayload {
  channel_id: string;
  message_id: string;
  sender_id: string;
  plaintext: string | null;
  /** "delivered", "decrypt_failed" */
  status: string;
  edited_at: string;
}

export interface WsMessageDeletedPayload {
  channel_id: string;
  message_id: string;
}

export interface WsTypingPayload {
  channel_id: string;
  user_id: string;
}

export interface WsPresencePayload {
  user_id: string;
  status: string;
}

export interface WsMemberPayload {
  event: "joined" | "left";
  guild_id: string;
  user_id: string;
}

export interface WsReplayCompletePayload {
  channel_id: string;
}

export interface WsErrorPayload {
  code: number;
  message: string;
}

export interface WsReadyDataPayload {
  user_id: string;
  display_name: string;
  email: string;
  avatar_url: string | null;
  guilds: WsGuildPayload[];
}

export interface WsGuildPayload {
  id: string;
  name: string;
  owner_id: string;
  icon_url: string | null;
  channels: WsChannelPayload[];
}

export interface WsChannelPayload {
  id: string;
  guild_id: string;
  name: string;
  channel_type: string;
  position: number;
}
