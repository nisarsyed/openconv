use crate::ids::{GuildId, RoleId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Request to create a new custom role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoleRequest {
    pub name: String,
    pub permissions: u64,
}

/// Request to update an existing role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRoleRequest {
    pub name: Option<String>,
    pub permissions: Option<u64>,
    pub position: Option<i32>,
}

/// Role details response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleResponse {
    pub id: RoleId,
    pub guild_id: GuildId,
    pub name: String,
    pub permissions: u64,
    pub position: i32,
    pub role_type: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_role_request_serde() {
        let req = CreateRoleRequest {
            name: "Moderator".into(),
            permissions: 0xFF,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreateRoleRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Moderator");
        assert_eq!(back.permissions, 0xFF);
    }

    #[test]
    fn role_response_serde() {
        let resp = RoleResponse {
            id: RoleId::new(),
            guild_id: GuildId::new(),
            name: "admin".into(),
            permissions: 123,
            position: 50,
            role_type: "admin".into(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: RoleResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "admin");
    }
}
