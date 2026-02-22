use crate::api::message::base64_serde;
use crate::ids::{ChannelId, GuildId, MessageId, UserId};
use serde::{Deserialize, Serialize};

/// Presence status for a user connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum PresenceStatus {
    Online,
    Idle,
    Dnd,
    Offline,
}

/// Messages sent from the client to the server over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(tag = "type")]
pub enum ClientMessage {
    Subscribe {
        channel_id: ChannelId,
    },
    Unsubscribe {
        channel_id: ChannelId,
    },
    SendMessage {
        channel_id: ChannelId,
        #[serde(with = "base64_serde")]
        encrypted_content: Vec<u8>,
        #[serde(with = "base64_serde")]
        nonce: Vec<u8>,
    },
    EditMessage {
        channel_id: ChannelId,
        message_id: MessageId,
        #[serde(with = "base64_serde")]
        encrypted_content: Vec<u8>,
        #[serde(with = "base64_serde")]
        nonce: Vec<u8>,
    },
    DeleteMessage {
        channel_id: ChannelId,
        message_id: MessageId,
    },
    StartTyping {
        channel_id: ChannelId,
    },
    StopTyping {
        channel_id: ChannelId,
    },
    SetPresence {
        status: PresenceStatus,
    },
    Ping {
        ts: u64,
    },
}

/// Messages sent from the server to the client over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(tag = "type")]
pub enum ServerMessage {
    Ready {
        user_id: UserId,
        guild_ids: Vec<GuildId>,
    },
    MessageCreated {
        channel_id: ChannelId,
        message_id: MessageId,
    },
    MessageUpdated {
        channel_id: ChannelId,
        message_id: MessageId,
    },
    MessageDeleted {
        channel_id: ChannelId,
        message_id: MessageId,
    },
    TypingStarted {
        channel_id: ChannelId,
        user_id: UserId,
    },
    PresenceUpdate {
        user_id: UserId,
        status: PresenceStatus,
    },
    MemberJoined {
        guild_id: GuildId,
        user_id: UserId,
    },
    MemberLeft {
        guild_id: GuildId,
        user_id: UserId,
    },
    Pong {
        ts: u64,
    },
    Error {
        code: u32,
        message: String,
    },
    ReplayComplete {
        channel_id: ChannelId,
    },
}

/// WebSocket error codes.
pub mod error_codes {
    pub const PERMISSION_DENIED: u32 = 4001;
    pub const NOT_FOUND: u32 = 4002;
    pub const RATE_LIMITED: u32 = 4003;
    pub const INVALID_MESSAGE_FORMAT: u32 = 4004;
    pub const CHANNEL_NOT_SUBSCRIBED: u32 = 4005;
    pub const LAGGED: u32 = 4006;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_subscribe_round_trip() {
        let msg = ClientMessage::Subscribe {
            channel_id: ChannelId::new(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"Subscribe""#));
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::Subscribe { .. } => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn client_message_ping_round_trip() {
        let msg = ClientMessage::Ping { ts: 1234567890 };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::Ping { ts } => assert_eq!(ts, 1234567890),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn client_message_send_message_round_trip() {
        let content = b"hello encrypted".to_vec();
        let nonce_bytes = b"random_nonce".to_vec();
        let msg = ClientMessage::SendMessage {
            channel_id: ChannelId::new(),
            encrypted_content: content.clone(),
            nonce: nonce_bytes.clone(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        // Verify base64 encoding in JSON
        use base64::Engine;
        let expected = base64::engine::general_purpose::STANDARD.encode(&content);
        assert!(json.contains(&expected));
        // Verify round-trip
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::SendMessage {
                encrypted_content,
                nonce,
                ..
            } => {
                assert_eq!(encrypted_content, content);
                assert_eq!(nonce, nonce_bytes);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_message_error_round_trip() {
        let msg = ServerMessage::Error {
            code: 4004,
            message: "invalid message format".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Error { code, message } => {
                assert_eq!(code, 4004);
                assert_eq!(message, "invalid message format");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_message_pong_round_trip() {
        let msg = ServerMessage::Pong { ts: 42 };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Pong { ts } => assert_eq!(ts, 42),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_message_ready_round_trip() {
        let msg = ServerMessage::Ready {
            user_id: UserId::new(),
            guild_ids: vec![GuildId::new(), GuildId::new()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"Ready""#));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::Ready { guild_ids, .. } => assert_eq!(guild_ids.len(), 2),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn presence_status_all_variants_round_trip() {
        for status in [
            PresenceStatus::Online,
            PresenceStatus::Idle,
            PresenceStatus::Dnd,
            PresenceStatus::Offline,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: PresenceStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn unknown_client_message_type_fails_deserialization() {
        let json = r#"{"type": "UnknownThing"}"#;
        let result = serde_json::from_str::<ClientMessage>(json);
        assert!(result.is_err());
    }

    #[test]
    fn server_message_message_created_round_trip() {
        let msg = ServerMessage::MessageCreated {
            channel_id: ChannelId::new(),
            message_id: MessageId::new(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"MessageCreated""#));
        let _back: ServerMessage = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn server_message_typing_started_round_trip() {
        let msg = ServerMessage::TypingStarted {
            channel_id: ChannelId::new(),
            user_id: UserId::new(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::TypingStarted { .. } => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_message_presence_update_round_trip() {
        let msg = ServerMessage::PresenceUpdate {
            user_id: UserId::new(),
            status: PresenceStatus::Dnd,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::PresenceUpdate { status, .. } => {
                assert_eq!(status, PresenceStatus::Dnd);
            }
            _ => panic!("wrong variant"),
        }
    }
}
