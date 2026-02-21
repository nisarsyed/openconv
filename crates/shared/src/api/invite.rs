use crate::ids::{GuildId, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Request body for POST /api/guilds/:guild_id/invites.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInviteRequest {
    /// Maximum number of uses. None = unlimited.
    pub max_uses: Option<i32>,
    /// When the invite expires. None = never.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Response for invite CRUD operations (guild-scoped).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteResponse {
    pub code: String,
    pub guild_id: GuildId,
    pub inviter_id: UserId,
    pub max_uses: Option<i32>,
    pub use_count: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Response for GET /api/invites/:code (public invite lookup).
/// Contains enough info for the user to decide whether to join.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteInfoResponse {
    pub code: String,
    pub guild_name: String,
    pub guild_id: GuildId,
    pub member_count: i64,
    pub inviter_display_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_invite_request_serde() {
        let req = CreateInviteRequest {
            max_uses: Some(10),
            expires_at: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreateInviteRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.max_uses, Some(10));
        assert!(back.expires_at.is_none());
    }

    #[test]
    fn invite_response_serde() {
        let resp = InviteResponse {
            code: "AbCd1234".into(),
            guild_id: GuildId::new(),
            inviter_id: UserId::new(),
            max_uses: Some(5),
            use_count: 0,
            expires_at: None,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: InviteResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.code, "AbCd1234");
    }

    #[test]
    fn invite_info_response_serde() {
        let resp = InviteInfoResponse {
            code: "XyZ98765".into(),
            guild_name: "Test Guild".into(),
            guild_id: GuildId::new(),
            member_count: 42,
            inviter_display_name: Some("Alice".into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: InviteInfoResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.guild_name, "Test Guild");
        assert_eq!(back.member_count, 42);
    }
}
