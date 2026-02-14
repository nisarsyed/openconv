use crate::ids::{GuildId, UserId};
use serde::{Deserialize, Serialize};

/// Request to create a new guild.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGuildRequest {
    pub name: String,
}

/// Guild details response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildResponse {
    pub id: GuildId,
    pub name: String,
    pub owner_id: UserId,
    pub icon_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// List of guilds response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildListResponse {
    pub guilds: Vec<GuildResponse>,
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
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: GuildResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, resp.id);
        assert_eq!(back.name, "Test Guild");
    }

    #[test]
    fn create_guild_request_minimal() {
        let req = CreateGuildRequest {
            name: "My Guild".into(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["name"], "My Guild");
    }
}
