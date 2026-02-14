use crate::ids::UserId;
use serde::{Deserialize, Serialize};

/// Registration request with public key, email, and display name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub public_key: String,
    pub email: String,
    pub display_name: String,
}

/// Registration response with new user ID and auth token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub user_id: UserId,
    pub token: String,
}

/// Login challenge request with public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginChallengeRequest {
    pub public_key: String,
}

/// Login challenge response containing the challenge to sign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginChallengeResponse {
    pub challenge: String,
}

/// Login verification request with public key and signed challenge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginVerifyRequest {
    pub public_key: String,
    pub signature: String,
}

/// Login verification response with auth token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginVerifyResponse {
    pub token: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_request_serializes() {
        let req = RegisterRequest {
            public_key: "pk_test".into(),
            email: "test@example.com".into(),
            display_name: "Test User".into(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["public_key"], "pk_test");
        assert_eq!(json["email"], "test@example.com");
        assert_eq!(json["display_name"], "Test User");
    }

    #[test]
    fn register_request_deserializes() {
        let json = r#"{"public_key":"pk","email":"a@b.com","display_name":"A"}"#;
        let req: RegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.public_key, "pk");
        assert_eq!(req.email, "a@b.com");
        assert_eq!(req.display_name, "A");
    }
}
