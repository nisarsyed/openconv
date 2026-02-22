use crate::api::message::base64_serde;
use crate::ids::{ChannelId, DeviceId, DmChannelId, GuildId, MessageId, UserId};
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

/// Per-device encrypted payload for a single recipient.
/// Each device of each channel member receives its own Signal-encrypted ciphertext.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RecipientPayload {
    pub user_id: UserId,
    pub device_id: DeviceId,
    #[serde(with = "base64_serde")]
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub ciphertext: Vec<u8>,
    /// Either "prekey" (first message to a device) or "signal" (established session)
    pub message_type: String,
}

/// Messages sent from the client to the server over WebSocket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        /// Guild channel ID. Exactly one of channel_id or dm_channel_id must be Some.
        #[serde(skip_serializing_if = "Option::is_none")]
        channel_id: Option<ChannelId>,
        /// DM channel ID. Exactly one of channel_id or dm_channel_id must be Some.
        #[serde(skip_serializing_if = "Option::is_none")]
        dm_channel_id: Option<DmChannelId>,
        /// Per-device ciphertext for each recipient device
        recipients: Vec<RecipientPayload>,
    },
    EditMessage {
        channel_id: ChannelId,
        message_id: MessageId,
        /// Re-encrypted content per-device with current ratchet state
        recipients: Vec<RecipientPayload>,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        sender_id: UserId,
        /// The ciphertext encrypted specifically for this receiving device
        #[serde(with = "base64_serde")]
        #[cfg_attr(feature = "utoipa", schema(value_type = String))]
        ciphertext: Vec<u8>,
        /// "prekey" or "signal"
        message_type: String,
        created_at: chrono::DateTime<chrono::Utc>,
    },
    MessageUpdated {
        channel_id: ChannelId,
        message_id: MessageId,
        sender_id: UserId,
        #[serde(with = "base64_serde")]
        #[cfg_attr(feature = "utoipa", schema(value_type = String))]
        ciphertext: Vec<u8>,
        message_type: String,
        edited_at: chrono::DateTime<chrono::Utc>,
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
    fn recipient_payload_round_trip() {
        let payload = RecipientPayload {
            user_id: UserId::new(),
            device_id: DeviceId::new(),
            ciphertext: b"encrypted_for_device".to_vec(),
            message_type: "signal".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: RecipientPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, payload.user_id);
        assert_eq!(back.device_id, payload.device_id);
        assert_eq!(back.ciphertext, b"encrypted_for_device");
        assert_eq!(back.message_type, "signal");
    }

    #[test]
    fn recipient_payload_ciphertext_is_base64() {
        let payload = RecipientPayload {
            user_id: UserId::new(),
            device_id: DeviceId::new(),
            ciphertext: b"test_bytes".to_vec(),
            message_type: "prekey".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        use base64::Engine;
        let expected = base64::engine::general_purpose::STANDARD.encode(b"test_bytes");
        assert!(json.contains(&expected));
    }

    #[test]
    fn client_message_send_message_recipients_round_trip() {
        let msg = ClientMessage::SendMessage {
            channel_id: Some(ChannelId::new()),
            dm_channel_id: None,
            recipients: vec![
                RecipientPayload {
                    user_id: UserId::new(),
                    device_id: DeviceId::new(),
                    ciphertext: b"ct1".to_vec(),
                    message_type: "signal".to_string(),
                },
                RecipientPayload {
                    user_id: UserId::new(),
                    device_id: DeviceId::new(),
                    ciphertext: b"ct2".to_vec(),
                    message_type: "prekey".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"SendMessage""#));
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::SendMessage {
                recipients,
                channel_id,
                dm_channel_id,
            } => {
                assert_eq!(recipients.len(), 2);
                assert!(channel_id.is_some());
                assert!(dm_channel_id.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn client_message_send_message_empty_recipients() {
        let msg = ClientMessage::SendMessage {
            channel_id: Some(ChannelId::new()),
            dm_channel_id: None,
            recipients: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::SendMessage { recipients, .. } => {
                assert!(recipients.is_empty());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn client_message_send_message_channel_only() {
        let ch = ChannelId::new();
        let msg = ClientMessage::SendMessage {
            channel_id: Some(ch),
            dm_channel_id: None,
            recipients: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::SendMessage {
                channel_id,
                dm_channel_id,
                ..
            } => {
                assert_eq!(channel_id, Some(ch));
                assert!(dm_channel_id.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn client_message_send_message_dm_only() {
        let dm = DmChannelId::new();
        let msg = ClientMessage::SendMessage {
            channel_id: None,
            dm_channel_id: Some(dm),
            recipients: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::SendMessage {
                channel_id,
                dm_channel_id,
                ..
            } => {
                assert!(channel_id.is_none());
                assert_eq!(dm_channel_id, Some(dm));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn client_message_edit_message_recipients_round_trip() {
        let msg = ClientMessage::EditMessage {
            channel_id: ChannelId::new(),
            message_id: MessageId::new(),
            recipients: vec![RecipientPayload {
                user_id: UserId::new(),
                device_id: DeviceId::new(),
                ciphertext: b"edited_ct".to_vec(),
                message_type: "signal".to_string(),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"EditMessage""#));
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::EditMessage { recipients, .. } => {
                assert_eq!(recipients.len(), 1);
                assert_eq!(recipients[0].ciphertext, b"edited_ct");
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
    fn server_message_message_created_with_ciphertext_round_trip() {
        let now = chrono::Utc::now();
        let sender = UserId::new();
        let msg = ServerMessage::MessageCreated {
            channel_id: ChannelId::new(),
            message_id: MessageId::new(),
            sender_id: sender,
            ciphertext: b"device_specific_ct".to_vec(),
            message_type: "prekey".to_string(),
            created_at: now,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"MessageCreated""#));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::MessageCreated {
                sender_id,
                ciphertext,
                message_type,
                created_at,
                ..
            } => {
                assert_eq!(sender_id, sender);
                assert_eq!(ciphertext, b"device_specific_ct");
                assert_eq!(message_type, "prekey");
                assert!(created_at.timestamp() > 0);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_message_message_created_has_sender_and_timestamp() {
        let msg = ServerMessage::MessageCreated {
            channel_id: ChannelId::new(),
            message_id: MessageId::new(),
            sender_id: UserId::new(),
            ciphertext: b"ct".to_vec(),
            message_type: "signal".to_string(),
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert!(json.get("sender_id").is_some());
        assert!(json.get("created_at").is_some());
        assert!(json.get("ciphertext").is_some());
        assert!(json.get("message_type").is_some());
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

    #[test]
    fn server_message_message_updated_round_trip() {
        let now = chrono::Utc::now();
        let sender = UserId::new();
        let msg = ServerMessage::MessageUpdated {
            channel_id: ChannelId::new(),
            message_id: MessageId::new(),
            sender_id: sender,
            ciphertext: b"updated_ct".to_vec(),
            message_type: "signal".to_string(),
            edited_at: now,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"MessageUpdated""#));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::MessageUpdated {
                sender_id,
                ciphertext,
                message_type,
                edited_at,
                ..
            } => {
                assert_eq!(sender_id, sender);
                assert_eq!(ciphertext, b"updated_ct");
                assert_eq!(message_type, "signal");
                assert!(edited_at.timestamp() > 0);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn client_message_send_message_both_none() {
        let msg = ClientMessage::SendMessage {
            channel_id: None,
            dm_channel_id: None,
            recipients: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::SendMessage {
                channel_id,
                dm_channel_id,
                ..
            } => {
                assert!(channel_id.is_none());
                assert!(dm_channel_id.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }
}
