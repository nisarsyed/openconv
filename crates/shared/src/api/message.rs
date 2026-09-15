use crate::ids::{ChannelId, DeviceId, DmChannelId, MessageId, UserId};
use serde::{Deserialize, Serialize};

/// Serde module for serializing `Vec<u8>` as base64 strings in JSON.
pub mod base64_serde {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        s.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(serde::de::Error::custom)
    }
}

/// Serde module for serializing `Option<Vec<u8>>` as optional base64 strings.
pub mod option_base64_serde {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(b) => {
                let encoded = base64::engine::general_purpose::STANDARD.encode(b);
                s.serialize_some(&encoded)
            }
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let opt: Option<String> = Option::deserialize(d)?;
        match opt {
            Some(s) => base64::engine::general_purpose::STANDARD
                .decode(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

/// Request to send an encrypted message to a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SendMessageRequest {
    #[serde(with = "base64_serde")]
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub encrypted_content: Vec<u8>,
    #[serde(with = "base64_serde")]
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub nonce: Vec<u8>,
}

/// Message details response with encrypted content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct MessageResponse {
    pub id: MessageId,
    pub channel_id: ChannelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_channel_id: Option<DmChannelId>,
    pub sender_id: UserId,
    /// UUID of the sending device. Absent for rows written before per-device
    /// encryption, or whose device has since been deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_device_id: Option<DeviceId>,
    /// Signal protocol device id of the sender — required to rebuild the
    /// decrypting `ProtocolAddress` when reading history over REST.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_signal_device_id: Option<u32>,
    /// Legacy single-blob encrypted content (nullable for per-recipient messages).
    #[serde(
        with = "option_base64_serde",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "utoipa", schema(value_type = Option<String>))]
    pub encrypted_content: Option<Vec<u8>>,
    /// Legacy nonce (nullable for per-recipient messages).
    #[serde(
        with = "option_base64_serde",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "utoipa", schema(value_type = Option<String>))]
    pub nonce: Option<Vec<u8>>,
    /// Per-device ciphertext from message_recipients table.
    #[serde(
        with = "option_base64_serde",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "utoipa", schema(value_type = Option<String>))]
    pub ciphertext: Option<Vec<u8>>,
    /// Message type for per-device ciphertext ("prekey" or "signal").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<String>,
    pub edited_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Query parameters for cursor-based message history.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema, utoipa::IntoParams))]
pub struct MessageHistoryQuery {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// Paginated message history response.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct MessageHistoryResponse {
    pub messages: Vec<MessageResponse>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_response_includes_all_fields() {
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: None,
            sender_id: UserId::new(),
            sender_device_id: None,
            sender_signal_device_id: None,
            encrypted_content: Some(b"encrypted_data".to_vec()),
            nonce: Some(b"nonce_bytes".to_vec()),
            ciphertext: None,
            message_type: None,
            edited_at: None,
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("id").is_some());
        assert!(json.get("channel_id").is_some());
        assert!(json.get("sender_id").is_some());
        assert!(json.get("encrypted_content").is_some());
        assert!(json.get("nonce").is_some());
        assert!(json.get("edited_at").is_some());
        assert!(json.get("created_at").is_some());
    }

    #[test]
    fn message_response_edited_at_serializes_as_null_when_none() {
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: None,
            sender_id: UserId::new(),
            sender_device_id: None,
            sender_signal_device_id: None,
            encrypted_content: Some(b"data".to_vec()),
            nonce: Some(b"nonce".to_vec()),
            ciphertext: None,
            message_type: None,
            edited_at: None,
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["edited_at"].is_null());
    }

    #[test]
    fn message_response_edited_at_serializes_when_some() {
        let now = chrono::Utc::now();
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: None,
            sender_id: UserId::new(),
            sender_device_id: None,
            sender_signal_device_id: None,
            encrypted_content: Some(b"data".to_vec()),
            nonce: Some(b"nonce".to_vec()),
            ciphertext: None,
            message_type: None,
            edited_at: Some(now),
            created_at: now,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(!json["edited_at"].is_null());
    }

    #[test]
    fn optional_base64_fields_serialize_as_base64_in_json() {
        let content = b"hello encrypted world".to_vec();
        let nonce = b"random_nonce_12".to_vec();
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: None,
            sender_id: UserId::new(),
            sender_device_id: None,
            sender_signal_device_id: None,
            encrypted_content: Some(content.clone()),
            nonce: Some(nonce.clone()),
            ciphertext: None,
            message_type: None,
            edited_at: None,
            created_at: chrono::Utc::now(),
        };

        let json_str = serde_json::to_string(&resp).unwrap();

        use base64::Engine;
        let expected_content = base64::engine::general_purpose::STANDARD.encode(&content);
        let expected_nonce = base64::engine::general_purpose::STANDARD.encode(&nonce);
        assert!(json_str.contains(&expected_content));
        assert!(json_str.contains(&expected_nonce));
    }

    #[test]
    fn optional_base64_fields_roundtrip_via_json() {
        let content = b"test content bytes".to_vec();
        let nonce = b"test nonce bytes".to_vec();
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: None,
            sender_id: UserId::new(),
            sender_device_id: None,
            sender_signal_device_id: None,
            encrypted_content: Some(content.clone()),
            nonce: Some(nonce.clone()),
            ciphertext: None,
            message_type: None,
            edited_at: None,
            created_at: chrono::Utc::now(),
        };

        let json_str = serde_json::to_string(&resp).unwrap();
        let deserialized: MessageResponse = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.encrypted_content, Some(content));
        assert_eq!(deserialized.nonce, Some(nonce));
    }

    #[test]
    fn per_device_ciphertext_fields_roundtrip() {
        let ct = b"per-device encrypted data".to_vec();
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: None,
            sender_id: UserId::new(),
            sender_device_id: None,
            sender_signal_device_id: None,
            encrypted_content: None,
            nonce: None,
            ciphertext: Some(ct.clone()),
            message_type: Some("signal".to_string()),
            edited_at: None,
            created_at: chrono::Utc::now(),
        };

        let json_str = serde_json::to_string(&resp).unwrap();
        let deserialized: MessageResponse = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.ciphertext, Some(ct));
        assert_eq!(deserialized.message_type.as_deref(), Some("signal"));
        assert!(deserialized.encrypted_content.is_none());
        assert!(deserialized.nonce.is_none());
    }

    #[test]
    fn null_optional_fields_omitted_from_json() {
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: None,
            sender_id: UserId::new(),
            sender_device_id: None,
            sender_signal_device_id: None,
            encrypted_content: None,
            nonce: None,
            ciphertext: None,
            message_type: None,
            edited_at: None,
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("encrypted_content").is_none());
        assert!(json.get("nonce").is_none());
        assert!(json.get("ciphertext").is_none());
        assert!(json.get("message_type").is_none());
    }

    #[test]
    fn send_message_request_base64_roundtrip() {
        let req = SendMessageRequest {
            encrypted_content: b"message payload".to_vec(),
            nonce: b"msg_nonce".to_vec(),
        };

        let json_str = serde_json::to_string(&req).unwrap();
        let deserialized: SendMessageRequest = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.encrypted_content, b"message payload");
        assert_eq!(deserialized.nonce, b"msg_nonce");
    }

    #[test]
    fn message_history_response_serializes() {
        let resp = MessageHistoryResponse {
            messages: vec![],
            next_cursor: Some("abc123".into()),
            has_more: true,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["has_more"], true);
        assert_eq!(json["next_cursor"], "abc123");
        assert!(json["messages"].as_array().unwrap().is_empty());
    }

    #[test]
    fn message_history_query_deserializes_with_defaults() {
        let json = r#"{}"#;
        let query: MessageHistoryQuery = serde_json::from_str(json).unwrap();
        assert!(query.cursor.is_none());
        assert!(query.limit.is_none());
    }

    #[test]
    fn message_history_query_deserializes_with_values() {
        let json = r#"{"cursor": "abc", "limit": 25}"#;
        let query: MessageHistoryQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.cursor.as_deref(), Some("abc"));
        assert_eq!(query.limit, Some(25));
    }

    #[test]
    fn dm_channel_id_omitted_when_none() {
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: None,
            sender_id: UserId::new(),
            sender_device_id: None,
            sender_signal_device_id: None,
            encrypted_content: Some(b"data".to_vec()),
            nonce: Some(b"nonce".to_vec()),
            ciphertext: None,
            message_type: None,
            edited_at: None,
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("dm_channel_id").is_none());
    }

    #[test]
    fn dm_channel_id_present_when_some() {
        let dm_id = DmChannelId::new();
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: Some(dm_id),
            sender_id: UserId::new(),
            sender_device_id: None,
            sender_signal_device_id: None,
            encrypted_content: Some(b"data".to_vec()),
            nonce: Some(b"nonce".to_vec()),
            ciphertext: None,
            message_type: None,
            edited_at: None,
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("dm_channel_id").is_some());
        // Round-trip
        let json_str = serde_json::to_string(&resp).unwrap();
        let back: MessageResponse = serde_json::from_str(&json_str).unwrap();
        assert_eq!(back.dm_channel_id, Some(dm_id));
    }
}
