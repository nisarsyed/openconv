use crate::ids::{DmChannelId, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Request to create a DM channel.
/// - For 1:1 DMs: provide a single user_id in `user_ids`
/// - For group DMs: provide 2+ user_ids (up to 24, since the creator is added automatically)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDmChannelRequest {
    pub user_ids: Vec<UserId>,
    pub name: Option<String>,
}

/// Response for a DM channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmChannelResponse {
    pub id: DmChannelId,
    pub name: Option<String>,
    pub creator_id: Option<UserId>,
    pub is_group: bool,
    pub members: Vec<UserId>,
    pub created_at: DateTime<Utc>,
}

/// Request to add a member to a group DM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddDmMemberRequest {
    pub user_id: UserId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_dm_channel_request_serde() {
        let req = CreateDmChannelRequest {
            user_ids: vec![UserId::new()],
            name: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreateDmChannelRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_ids.len(), 1);
        assert!(back.name.is_none());
    }

    #[test]
    fn dm_channel_response_serde() {
        let resp = DmChannelResponse {
            id: DmChannelId::new(),
            name: Some("Group Chat".into()),
            creator_id: Some(UserId::new()),
            is_group: true,
            members: vec![UserId::new(), UserId::new()],
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: DmChannelResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, Some("Group Chat".into()));
        assert_eq!(back.members.len(), 2);
    }

    #[test]
    fn add_dm_member_request_serde() {
        let req = AddDmMemberRequest {
            user_id: UserId::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: AddDmMemberRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, req.user_id);
    }
}
