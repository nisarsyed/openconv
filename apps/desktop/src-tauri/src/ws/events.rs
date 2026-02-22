use openconv_shared::api::ws::{PresenceStatus, ServerMessage};
use openconv_shared::ids::{ChannelId, GuildId, MessageId, UserId};
use tauri::{AppHandle, Emitter};

use super::state::WsConnectionState;

/// Payload emitted for ws:message events (new incoming message).
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsMessagePayload {
    pub channel_id: ChannelId,
    pub message_id: MessageId,
    pub sender_id: UserId,
    pub ciphertext: Vec<u8>,
    pub message_type: String,
    pub created_at: String,
}

/// Payload emitted for ws:message_updated events.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsMessageUpdatedPayload {
    pub channel_id: ChannelId,
    pub message_id: MessageId,
    pub sender_id: UserId,
    pub ciphertext: Vec<u8>,
    pub message_type: String,
    pub edited_at: String,
}

/// Payload emitted for ws:typing events.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsTypingPayload {
    pub channel_id: ChannelId,
    pub user_id: UserId,
}

/// Payload emitted for ws:presence events.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsPresencePayload {
    pub user_id: UserId,
    pub status: PresenceStatus,
}

/// Payload emitted for ws:member events.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsMemberPayload {
    pub event: String,
    pub guild_id: GuildId,
    pub user_id: UserId,
}

/// Payload emitted for ws:error events.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsErrorPayload {
    pub code: u32,
    pub message: String,
}

/// Event name constants.
pub const EVENT_STATE: &str = "ws:state";
pub const EVENT_MESSAGE: &str = "ws:message";
pub const EVENT_MESSAGE_UPDATED: &str = "ws:message_updated";
pub const EVENT_MESSAGE_DELETED: &str = "ws:message_deleted";
pub const EVENT_TYPING: &str = "ws:typing";
pub const EVENT_PRESENCE: &str = "ws:presence";
pub const EVENT_MEMBER: &str = "ws:member";
pub const EVENT_REPLAY_COMPLETE: &str = "ws:replay_complete";
pub const EVENT_ERROR: &str = "ws:error";

/// Emit a ws:state event with the current connection state.
pub fn emit_state(app: &AppHandle, state: &WsConnectionState) {
    let _ = app.emit(EVENT_STATE, state);
}

/// Emit a ws:message event for a new incoming message.
pub fn emit_message(app: &AppHandle, payload: &WsMessagePayload) {
    let _ = app.emit(EVENT_MESSAGE, payload);
}

/// Emit a ws:message_updated event.
pub fn emit_message_updated(app: &AppHandle, payload: &WsMessageUpdatedPayload) {
    let _ = app.emit(EVENT_MESSAGE_UPDATED, payload);
}

/// Emit a ws:message_deleted event.
pub fn emit_message_deleted(app: &AppHandle, channel_id: &ChannelId, message_id: &MessageId) {
    #[derive(serde::Serialize, Clone)]
    struct Payload {
        channel_id: ChannelId,
        message_id: MessageId,
    }
    let _ = app.emit(
        EVENT_MESSAGE_DELETED,
        &Payload {
            channel_id: *channel_id,
            message_id: *message_id,
        },
    );
}

/// Emit a ws:typing event.
pub fn emit_typing(app: &AppHandle, channel_id: &ChannelId, user_id: &UserId) {
    let _ = app.emit(
        EVENT_TYPING,
        &WsTypingPayload {
            channel_id: *channel_id,
            user_id: *user_id,
        },
    );
}

/// Emit a ws:presence event.
pub fn emit_presence(app: &AppHandle, user_id: &UserId, status: &PresenceStatus) {
    let _ = app.emit(
        EVENT_PRESENCE,
        &WsPresencePayload {
            user_id: *user_id,
            status: *status,
        },
    );
}

/// Emit a ws:member event for join or leave.
pub fn emit_member(app: &AppHandle, event: &str, guild_id: &GuildId, user_id: &UserId) {
    let _ = app.emit(
        EVENT_MEMBER,
        &WsMemberPayload {
            event: event.to_string(),
            guild_id: *guild_id,
            user_id: *user_id,
        },
    );
}

/// Emit a ws:replay_complete event.
pub fn emit_replay_complete(app: &AppHandle, channel_id: &ChannelId) {
    #[derive(serde::Serialize, Clone)]
    struct Payload {
        channel_id: ChannelId,
    }
    let _ = app.emit(
        EVENT_REPLAY_COMPLETE,
        &Payload {
            channel_id: *channel_id,
        },
    );
}

/// Emit a ws:error event.
pub fn emit_error(app: &AppHandle, code: u32, message: &str) {
    let _ = app.emit(
        EVENT_ERROR,
        &WsErrorPayload {
            code,
            message: message.to_string(),
        },
    );
}

/// Dispatch a ServerMessage to the appropriate event emission.
/// Returns true if the message was handled.
pub fn dispatch_server_message(app: &AppHandle, msg: &ServerMessage) -> bool {
    match msg {
        ServerMessage::MessageCreated {
            channel_id,
            message_id,
            sender_id,
            ciphertext,
            message_type,
            created_at,
        } => {
            emit_message(
                app,
                &WsMessagePayload {
                    channel_id: *channel_id,
                    message_id: *message_id,
                    sender_id: *sender_id,
                    ciphertext: ciphertext.clone(),
                    message_type: message_type.clone(),
                    created_at: created_at.to_rfc3339(),
                },
            );
            true
        }
        ServerMessage::MessageUpdated {
            channel_id,
            message_id,
            sender_id,
            ciphertext,
            message_type,
            edited_at,
        } => {
            emit_message_updated(
                app,
                &WsMessageUpdatedPayload {
                    channel_id: *channel_id,
                    message_id: *message_id,
                    sender_id: *sender_id,
                    ciphertext: ciphertext.clone(),
                    message_type: message_type.clone(),
                    edited_at: edited_at.to_rfc3339(),
                },
            );
            true
        }
        ServerMessage::MessageDeleted {
            channel_id,
            message_id,
        } => {
            emit_message_deleted(app, channel_id, message_id);
            true
        }
        ServerMessage::TypingStarted {
            channel_id,
            user_id,
        } => {
            emit_typing(app, channel_id, user_id);
            true
        }
        ServerMessage::PresenceUpdate { user_id, status } => {
            emit_presence(app, user_id, status);
            true
        }
        ServerMessage::MemberJoined { guild_id, user_id } => {
            emit_member(app, "joined", guild_id, user_id);
            true
        }
        ServerMessage::MemberLeft { guild_id, user_id } => {
            emit_member(app, "left", guild_id, user_id);
            true
        }
        ServerMessage::ReplayComplete { channel_id } => {
            emit_replay_complete(app, channel_id);
            true
        }
        ServerMessage::Error { code, message } => {
            emit_error(app, *code, message);
            true
        }
        // Ready and Pong are handled by the connection loop, not event dispatch
        ServerMessage::Ready { .. } | ServerMessage::Pong { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_event_names_use_ws_prefix() {
        let events = [
            EVENT_STATE,
            EVENT_MESSAGE,
            EVENT_MESSAGE_UPDATED,
            EVENT_MESSAGE_DELETED,
            EVENT_TYPING,
            EVENT_PRESENCE,
            EVENT_MEMBER,
            EVENT_REPLAY_COMPLETE,
            EVENT_ERROR,
        ];
        for event in events {
            assert!(
                event.starts_with("ws:"),
                "Event '{event}' does not use ws: prefix"
            );
        }
    }

    #[test]
    fn ws_message_payload_serializes() {
        let payload = WsMessagePayload {
            channel_id: ChannelId::new(),
            message_id: MessageId::new(),
            sender_id: UserId::new(),
            ciphertext: vec![1, 2, 3],
            message_type: "signal".into(),
            created_at: "2025-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert!(json.get("channel_id").is_some());
        assert!(json.get("message_id").is_some());
        assert!(json.get("sender_id").is_some());
        assert_eq!(json["message_type"], "signal");
    }

    #[test]
    fn ws_typing_payload_serializes() {
        let payload = WsTypingPayload {
            channel_id: ChannelId::new(),
            user_id: UserId::new(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert!(json.get("channel_id").is_some());
        assert!(json.get("user_id").is_some());
    }

    #[test]
    fn ws_presence_payload_serializes() {
        let payload = WsPresencePayload {
            user_id: UserId::new(),
            status: PresenceStatus::Online,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert!(json.get("user_id").is_some());
        assert_eq!(json["status"], "Online");
    }

    #[test]
    fn ws_member_payload_serializes() {
        let payload = WsMemberPayload {
            event: "joined".into(),
            guild_id: GuildId::new(),
            user_id: UserId::new(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["event"], "joined");
    }

    #[test]
    fn ws_error_payload_serializes() {
        let payload = WsErrorPayload {
            code: 4001,
            message: "permission denied".into(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["code"], 4001);
        assert_eq!(json["message"], "permission denied");
    }

    #[test]
    fn ws_message_updated_payload_serializes() {
        let payload = WsMessageUpdatedPayload {
            channel_id: ChannelId::new(),
            message_id: MessageId::new(),
            sender_id: UserId::new(),
            ciphertext: vec![4, 5, 6],
            message_type: "prekey".into(),
            edited_at: "2025-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["message_type"], "prekey");
        assert!(json.get("edited_at").is_some());
    }
}
