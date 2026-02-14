use crate::ids::{ChannelId, MessageId, UserId};
use serde::{Deserialize, Serialize};

/// Request to send an encrypted message to a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub encrypted_content: String,
    pub nonce: String,
}

/// Message details response with encrypted content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResponse {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub sender_id: UserId,
    pub encrypted_content: String,
    pub nonce: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_response_includes_all_fields() {
        let resp = MessageResponse {
            id: MessageId::new(),
            channel_id: ChannelId::new(),
            sender_id: UserId::new(),
            encrypted_content: "enc_data".into(),
            nonce: "nonce123".into(),
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("id").is_some());
        assert!(json.get("channel_id").is_some());
        assert!(json.get("sender_id").is_some());
        assert!(json.get("encrypted_content").is_some());
        assert!(json.get("nonce").is_some());
        assert!(json.get("created_at").is_some());
    }
}
