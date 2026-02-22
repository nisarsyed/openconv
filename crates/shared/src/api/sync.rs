//! Sync protocol types for offline gap-fill and read-state tracking.

use crate::ids::{ChannelId, MessageId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Query parameters for the sync endpoint: GET /api/sync
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema, utoipa::IntoParams))]
pub struct SyncRequest {
    /// Fetch events with sequence numbers strictly greater than this value.
    /// Use 0 on first sync.
    pub after_sequence: u64,
    /// Only return events for these channels. Empty means all accessible channels.
    pub channel_ids: Vec<ChannelId>,
    /// Maximum number of events to return. Server may impose its own cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// A single event in the sync event stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SyncEvent {
    /// Monotonically increasing sequence number (globally unique per event).
    pub sequence: u64,
    pub channel_id: ChannelId,
    /// One of: "message_created", "message_updated", "message_deleted"
    pub event_type: String,
    pub message_id: MessageId,
    pub created_at: DateTime<Utc>,
}

/// Response from the sync endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SyncResponse {
    pub events: Vec<SyncEvent>,
    /// Use this value as `after_sequence` in the next request to continue pagination.
    pub next_sequence: u64,
    /// True if there are more events beyond this page.
    pub has_more: bool,
}

/// A single read-state entry returned from the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ReadStateEntry {
    pub channel_id: ChannelId,
    pub last_read_message_id: MessageId,
    pub updated_at: DateTime<Utc>,
}

/// A single update in a batch read-state request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ReadStateUpdate {
    pub channel_id: ChannelId,
    pub last_read_message_id: MessageId,
}

/// Request body for POST /api/users/me/read-state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct BatchUpdateReadStateRequest {
    pub entries: Vec<ReadStateUpdate>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_request_round_trip() {
        let req = SyncRequest {
            after_sequence: 42,
            channel_ids: vec![ChannelId::new(), ChannelId::new()],
            limit: Some(100),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: SyncRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.after_sequence, 42);
        assert_eq!(back.channel_ids.len(), 2);
        assert_eq!(back.limit, Some(100));
    }

    #[test]
    fn sync_request_no_limit() {
        let req = SyncRequest {
            after_sequence: 0,
            channel_ids: vec![],
            limit: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: SyncRequest = serde_json::from_str(&json).unwrap();
        assert!(back.limit.is_none());
    }

    #[test]
    fn sync_event_round_trip() {
        let evt = SyncEvent {
            sequence: 99,
            channel_id: ChannelId::new(),
            event_type: "message_created".to_string(),
            message_id: MessageId::new(),
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        let back: SyncEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sequence, 99);
        assert_eq!(back.event_type, "message_created");
    }

    #[test]
    fn sync_response_round_trip() {
        let resp = SyncResponse {
            events: vec![],
            next_sequence: 100,
            has_more: false,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: SyncResponse = serde_json::from_str(&json).unwrap();
        assert!(back.events.is_empty());
        assert_eq!(back.next_sequence, 100);
        assert!(!back.has_more);
    }

    #[test]
    fn sync_response_with_events() {
        let resp = SyncResponse {
            events: vec![SyncEvent {
                sequence: 1,
                channel_id: ChannelId::new(),
                event_type: "message_deleted".to_string(),
                message_id: MessageId::new(),
                created_at: chrono::Utc::now(),
            }],
            next_sequence: 2,
            has_more: true,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: SyncResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.events.len(), 1);
        assert!(back.has_more);
    }

    #[test]
    fn read_state_entry_round_trip() {
        let entry = ReadStateEntry {
            channel_id: ChannelId::new(),
            last_read_message_id: MessageId::new(),
            updated_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: ReadStateEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.channel_id, entry.channel_id);
        assert_eq!(back.last_read_message_id, entry.last_read_message_id);
    }

    #[test]
    fn batch_update_read_state_round_trip() {
        let req = BatchUpdateReadStateRequest {
            entries: vec![
                ReadStateUpdate {
                    channel_id: ChannelId::new(),
                    last_read_message_id: MessageId::new(),
                },
                ReadStateUpdate {
                    channel_id: ChannelId::new(),
                    last_read_message_id: MessageId::new(),
                },
            ],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: BatchUpdateReadStateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 2);
    }
}
