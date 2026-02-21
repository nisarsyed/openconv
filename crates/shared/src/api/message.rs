use crate::ids::{ChannelId, DmChannelId, MessageId, UserId};
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

/// Request to send an encrypted message to a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    #[serde(with = "base64_serde")]
    pub encrypted_content: Vec<u8>,
    #[serde(with = "base64_serde")]
    pub nonce: Vec<u8>,
}

/// Message details response with encrypted content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResponse {
    pub id: MessageId,
    pub channel_id: ChannelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_channel_id: Option<DmChannelId>,
    pub sender_id: UserId,
    #[serde(with = "base64_serde")]
    pub encrypted_content: Vec<u8>,
    #[serde(with = "base64_serde")]
    pub nonce: Vec<u8>,
    pub edited_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Query parameters for cursor-based message history.
#[derive(Debug, Deserialize)]
pub struct MessageHistoryQuery {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// Paginated message history response.
#[derive(Debug, Serialize)]
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
            encrypted_content: b"encrypted_data".to_vec(),
            nonce: b"nonce_bytes".to_vec(),
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
            encrypted_content: b"data".to_vec(),
            nonce: b"nonce".to_vec(),
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
            encrypted_content: b"data".to_vec(),
            nonce: b"nonce".to_vec(),
            edited_at: Some(now),
            created_at: now,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(!json["edited_at"].is_null());
    }

    #[test]
    fn vec_u8_fields_serialize_as_base64_in_json() {
        let content = b"hello encrypted world".to_vec();
        let nonce = b"random_nonce_12".to_vec();
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: None,
            sender_id: UserId::new(),
            encrypted_content: content.clone(),
            nonce: nonce.clone(),
            edited_at: None,
            created_at: chrono::Utc::now(),
        };

        let json_str = serde_json::to_string(&resp).unwrap();

        // Verify the JSON contains base64 strings, not raw bytes
        use base64::Engine;
        let expected_content =
            base64::engine::general_purpose::STANDARD.encode(&content);
        let expected_nonce =
            base64::engine::general_purpose::STANDARD.encode(&nonce);
        assert!(json_str.contains(&expected_content));
        assert!(json_str.contains(&expected_nonce));
    }

    #[test]
    fn vec_u8_fields_roundtrip_via_json() {
        let content = b"test content bytes".to_vec();
        let nonce = b"test nonce bytes".to_vec();
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            dm_channel_id: None,
            sender_id: UserId::new(),
            encrypted_content: content.clone(),
            nonce: nonce.clone(),
            edited_at: None,
            created_at: chrono::Utc::now(),
        };

        let json_str = serde_json::to_string(&resp).unwrap();
        let deserialized: MessageResponse = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.encrypted_content, content);
        assert_eq!(deserialized.nonce, nonce);
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
            encrypted_content: b"data".to_vec(),
            nonce: b"nonce".to_vec(),
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
            encrypted_content: b"data".to_vec(),
            nonce: b"nonce".to_vec(),
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
