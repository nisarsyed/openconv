use crate::ids::{DeviceId, UserId};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Legacy type kept for backward compatibility (unused in new flows)
// ---------------------------------------------------------------------------

/// Registration request with public key, email, and display name.
#[deprecated(note = "Use RegisterStartRequest/RegisterCompleteRequest instead")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub public_key: String,
    pub email: String,
    pub display_name: String,
}

// ---------------------------------------------------------------------------
// Registration flow (three-phase: start → verify → complete)
// ---------------------------------------------------------------------------

/// POST /api/auth/register/start
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RegisterStartRequest {
    pub email: String,
    pub display_name: String,
}

/// Response for register/start (always the same, privacy-first).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RegisterStartResponse {
    pub message: String,
}

/// POST /api/auth/register/verify
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RegisterVerifyRequest {
    pub email: String,
    pub code: String,
}

/// Response for register/verify (contains registration token).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RegisterVerifyResponse {
    pub registration_token: String,
}

/// POST /api/auth/register/complete
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RegisterCompleteRequest {
    pub registration_token: String,
    pub public_key: String,
    /// Base64-encoded pre-key bundle bytes.
    pub pre_key_bundle: String,
    pub device_id: DeviceId,
    pub device_name: String,
}

/// Registration response with user ID, tokens, and device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RegisterResponse {
    pub user_id: UserId,
    pub access_token: String,
    pub refresh_token: String,
    pub device_id: DeviceId,
}

// ---------------------------------------------------------------------------
// Login flow (challenge → verify)
// ---------------------------------------------------------------------------

/// Login challenge request with public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct LoginChallengeRequest {
    pub public_key: String,
}

/// Login challenge response containing the challenge to sign.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct LoginChallengeResponse {
    pub challenge: String,
}

/// Login verification request with public key, signed challenge, and device info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct LoginVerifyRequest {
    pub public_key: String,
    pub signature: String,
    pub device_id: DeviceId,
    pub device_name: String,
}

/// Login verification response with tokens and identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct LoginVerifyResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: UserId,
    pub device_id: DeviceId,
}

// ---------------------------------------------------------------------------
// Token refresh
// ---------------------------------------------------------------------------

/// POST /api/auth/refresh
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// Response for token refresh.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
}

// ---------------------------------------------------------------------------
// Account recovery (three-phase: start → verify → complete)
// ---------------------------------------------------------------------------

/// POST /api/auth/recover/start
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RecoverStartRequest {
    pub email: String,
}

/// Response for recover/start (always the same, privacy-first).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RecoverStartResponse {
    pub message: String,
}

/// POST /api/auth/recover/verify
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RecoverVerifyRequest {
    pub email: String,
    pub code: String,
}

/// Response for recover/verify (contains recovery token).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RecoverVerifyResponse {
    pub recovery_token: String,
}

/// POST /api/auth/recover/complete
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RecoverCompleteRequest {
    pub recovery_token: String,
    pub new_public_key: String,
    /// Base64-encoded new pre-key bundle bytes.
    pub new_pre_key_bundle: String,
    pub device_id: DeviceId,
    pub device_name: String,
}

/// Response for recover/complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RecoverCompleteResponse {
    pub user_id: UserId,
    pub access_token: String,
    pub refresh_token: String,
    pub device_id: DeviceId,
}

// ---------------------------------------------------------------------------
// Device management
// ---------------------------------------------------------------------------

/// A device record returned in device listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub device_name: String,
    pub last_active: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Response body for GET /api/auth/devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct DevicesListResponse {
    pub devices: Vec<DeviceInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[allow(deprecated)]
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

    #[allow(deprecated)]
    #[test]
    fn register_request_deserializes() {
        let json = r#"{"public_key":"pk","email":"a@b.com","display_name":"A"}"#;
        let req: RegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.public_key, "pk");
        assert_eq!(req.email, "a@b.com");
        assert_eq!(req.display_name, "A");
    }

    #[test]
    fn login_verify_request_with_device_fields_roundtrip() {
        let req = LoginVerifyRequest {
            public_key: "pk".into(),
            signature: "sig".into(),
            device_id: DeviceId::new(),
            device_name: "MacBook Pro".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: LoginVerifyRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.device_name, "MacBook Pro");
    }

    #[test]
    fn login_verify_response_with_tokens_roundtrip() {
        let resp = LoginVerifyResponse {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            user_id: UserId::new(),
            device_id: DeviceId::new(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: LoginVerifyResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_token, "at");
        assert_eq!(back.refresh_token, "rt");
    }

    #[test]
    fn all_new_request_types_serde_roundtrip() {
        // RegisterStartRequest
        let req = RegisterStartRequest {
            email: "a@b.com".into(),
            display_name: "Alice".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: RegisterStartRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.email, "a@b.com");

        // RegisterVerifyRequest
        let req = RegisterVerifyRequest {
            email: "a@b.com".into(),
            code: "123456".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: RegisterVerifyRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.code, "123456");

        // RegisterCompleteRequest
        let bundle_b64 = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]);
        let req = RegisterCompleteRequest {
            registration_token: "tok".into(),
            public_key: "pk".into(),
            pre_key_bundle: bundle_b64.clone(),
            device_id: DeviceId::new(),
            device_name: "Phone".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: RegisterCompleteRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.registration_token, "tok");
        assert_eq!(back.pre_key_bundle, bundle_b64);

        // RefreshRequest
        let req = RefreshRequest {
            refresh_token: "rt".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: RefreshRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.refresh_token, "rt");

        // RecoverStartRequest
        let req = RecoverStartRequest {
            email: "a@b.com".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: RecoverStartRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.email, "a@b.com");

        // RecoverVerifyRequest
        let req = RecoverVerifyRequest {
            email: "a@b.com".into(),
            code: "654321".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: RecoverVerifyRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.code, "654321");

        // RecoverCompleteRequest
        let new_bundle_b64 = base64::engine::general_purpose::STANDARD.encode([4u8, 5, 6]);
        let req = RecoverCompleteRequest {
            recovery_token: "rtok".into(),
            new_public_key: "npk".into(),
            new_pre_key_bundle: new_bundle_b64.clone(),
            device_id: DeviceId::new(),
            device_name: "Tablet".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: RecoverCompleteRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.recovery_token, "rtok");
        assert_eq!(back.new_pre_key_bundle, new_bundle_b64);
    }

    #[test]
    fn all_new_response_types_serde_roundtrip() {
        // RegisterStartResponse
        let resp = RegisterStartResponse {
            message: "check email".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: RegisterStartResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.message, "check email");

        // RegisterVerifyResponse
        let resp = RegisterVerifyResponse {
            registration_token: "tok".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: RegisterVerifyResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.registration_token, "tok");

        // RegisterResponse (updated)
        let resp = RegisterResponse {
            user_id: UserId::new(),
            access_token: "at".into(),
            refresh_token: "rt".into(),
            device_id: DeviceId::new(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: RegisterResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_token, "at");

        // RefreshResponse
        let resp = RefreshResponse {
            access_token: "at2".into(),
            refresh_token: "rt2".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: RefreshResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_token, "at2");

        // RecoverStartResponse
        let resp = RecoverStartResponse {
            message: "check email".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: RecoverStartResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.message, "check email");

        // RecoverVerifyResponse
        let resp = RecoverVerifyResponse {
            recovery_token: "rtok".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: RecoverVerifyResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.recovery_token, "rtok");

        // RecoverCompleteResponse
        let resp = RecoverCompleteResponse {
            user_id: UserId::new(),
            access_token: "at3".into(),
            refresh_token: "rt3".into(),
            device_id: DeviceId::new(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: RecoverCompleteResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_token, "at3");
    }

    #[test]
    fn device_info_serde_roundtrip() {
        let now = chrono::Utc::now();
        let info = DeviceInfo {
            id: DeviceId::new(),
            device_name: "iPhone".into(),
            last_active: Some(now),
            created_at: now,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: DeviceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.device_name, "iPhone");
    }
}
