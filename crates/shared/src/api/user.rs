use crate::ids::UserId;
use serde::{Deserialize, Serialize};

/// User profile details response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfileResponse {
    pub id: UserId,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_profile_response_roundtrip() {
        let resp = UserProfileResponse {
            id: UserId::new(),
            display_name: "Test User".into(),
            avatar_url: Some("https://example.com/avatar.png".into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: UserProfileResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, resp.id);
        assert_eq!(back.display_name, "Test User");
    }
}
