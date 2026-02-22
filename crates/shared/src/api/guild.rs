use crate::ids::{GuildId, RoleId, UserId};
use serde::{Deserialize, Serialize};

/// Request to create a new guild.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CreateGuildRequest {
    pub name: String,
}

/// Request to update guild properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct UpdateGuildRequest {
    pub name: Option<String>,
    pub icon_url: Option<String>,
}

/// Guild details response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct GuildResponse {
    pub id: GuildId,
    pub name: String,
    pub owner_id: UserId,
    pub icon_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<i64>,
}

/// List of guilds response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct GuildListResponse {
    pub guilds: Vec<GuildResponse>,
}

/// Response for a guild member with role information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct GuildMemberResponse {
    pub user_id: UserId,
    pub display_name: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub roles: Vec<RoleSummary>,
}

/// Minimal role info included in member listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RoleSummary {
    pub id: RoleId,
    pub name: String,
    pub position: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guild_response_roundtrip() {
        let resp = GuildResponse {
            id: GuildId::new(),
            name: "Test Guild".into(),
            owner_id: UserId::new(),
            icon_url: None,
            created_at: chrono::Utc::now(),
            member_count: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: GuildResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, resp.id);
        assert_eq!(back.name, "Test Guild");
    }

    #[test]
    fn guild_response_member_count_omitted_when_none() {
        let resp = GuildResponse {
            id: GuildId::new(),
            name: "Test".into(),
            owner_id: UserId::new(),
            icon_url: None,
            created_at: chrono::Utc::now(),
            member_count: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("member_count").is_none());
    }

    #[test]
    fn guild_response_member_count_present_when_some() {
        let resp = GuildResponse {
            id: GuildId::new(),
            name: "Test".into(),
            owner_id: UserId::new(),
            icon_url: None,
            created_at: chrono::Utc::now(),
            member_count: Some(42),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["member_count"], 42);
    }

    #[test]
    fn create_guild_request_minimal() {
        let req = CreateGuildRequest {
            name: "My Guild".into(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["name"], "My Guild");
    }

    #[test]
    fn update_guild_request_roundtrip() {
        let req = UpdateGuildRequest {
            name: Some("New Name".into()),
            icon_url: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: UpdateGuildRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, Some("New Name".into()));
    }

    #[test]
    fn guild_member_response_roundtrip() {
        let resp = GuildMemberResponse {
            user_id: UserId::new(),
            display_name: "Alice".into(),
            joined_at: chrono::Utc::now(),
            roles: vec![RoleSummary {
                id: RoleId::new(),
                name: "member".into(),
                position: 1,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: GuildMemberResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.display_name, "Alice");
        assert_eq!(back.roles.len(), 1);
    }
}
