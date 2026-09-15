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
    pub plaintext: Option<String>,
    /// "delivered", "pending", "decrypt_failed"
    pub status: String,
    pub created_at: String,
}

/// Payload emitted for ws:message_updated events.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsMessageUpdatedPayload {
    pub channel_id: ChannelId,
    pub message_id: MessageId,
    pub sender_id: UserId,
    pub plaintext: Option<String>,
    /// "delivered", "decrypt_failed"
    pub status: String,
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

/// Payload emitted for ws:ready_data events (guild/channel data after login).
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsReadyDataPayload {
    pub user_id: UserId,
    pub display_name: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub guilds: Vec<GuildPayload>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GuildPayload {
    pub id: GuildId,
    pub name: String,
    pub owner_id: UserId,
    pub icon_url: Option<String>,
    pub channels: Vec<ChannelPayload>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChannelPayload {
    pub id: ChannelId,
    pub guild_id: GuildId,
    pub name: String,
    pub channel_type: String,
    pub position: i32,
}

/// Event name constants.
pub const EVENT_READY_DATA: &str = "ws:ready_data";
pub const EVENT_STATE: &str = "ws:state";
pub const EVENT_MESSAGE: &str = "ws:message";
pub const EVENT_MESSAGE_UPDATED: &str = "ws:message_updated";
pub const EVENT_MESSAGE_DELETED: &str = "ws:message_deleted";
pub const EVENT_TYPING: &str = "ws:typing";
pub const EVENT_PRESENCE: &str = "ws:presence";
pub const EVENT_MEMBER: &str = "ws:member";
pub const EVENT_REPLAY_COMPLETE: &str = "ws:replay_complete";
pub const EVENT_ERROR: &str = "ws:error";

/// Emit a ws:ready_data event with guild/channel data after authentication.
pub fn emit_ready_data(app: &AppHandle, payload: &WsReadyDataPayload) {
    let _ = app.emit(EVENT_READY_DATA, payload);
}

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
///
/// Note: MessageCreated, MessageUpdated, and MessageDeleted are handled by
/// the messaging pipeline in handlers.rs (decrypt → cache → emit), not here.
/// Returns true if the message was handled.
pub fn dispatch_server_message(app: &AppHandle, msg: &ServerMessage) -> bool {
    match msg {
        // MessageCreated/Updated/Deleted are handled by the decrypt-store-emit
        // pipeline in handlers.rs -- they should not reach here.
        ServerMessage::MessageCreated { .. }
        | ServerMessage::MessageUpdated { .. }
        | ServerMessage::MessageDeleted { .. } => {
            tracing::warn!(
                "dispatch_server_message: message variant should be handled by pipeline"
            );
            false
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
            EVENT_READY_DATA,
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
            plaintext: Some("hello world".into()),
            status: "delivered".into(),
            created_at: "2025-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert!(json.get("channel_id").is_some());
        assert!(json.get("message_id").is_some());
        assert!(json.get("sender_id").is_some());
        assert_eq!(json["status"], "delivered");
        assert_eq!(json["plaintext"], "hello world");
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
    fn ws_ready_data_payload_serializes() {
        let payload = WsReadyDataPayload {
            user_id: UserId::new(),
            display_name: "Alice".into(),
            email: "alice@example.com".into(),
            avatar_url: None,
            guilds: vec![GuildPayload {
                id: GuildId::new(),
                name: "Test Guild".into(),
                owner_id: UserId::new(),
                icon_url: None,
                channels: vec![ChannelPayload {
                    id: ChannelId::new(),
                    guild_id: GuildId::new(),
                    name: "main".into(),
                    channel_type: "text".into(),
                    position: 0,
                }],
            }],
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert!(json.get("user_id").is_some());
        assert_eq!(json["display_name"], "Alice");
        assert_eq!(json["guilds"].as_array().unwrap().len(), 1);
        assert_eq!(json["guilds"][0]["channels"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn ws_message_updated_payload_serializes() {
        let payload = WsMessageUpdatedPayload {
            channel_id: ChannelId::new(),
            message_id: MessageId::new(),
            sender_id: UserId::new(),
            plaintext: Some("updated content".into()),
            status: "delivered".into(),
            edited_at: "2025-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["status"], "delivered");
        assert!(json.get("edited_at").is_some());
    }
}
