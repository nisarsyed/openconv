use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::{DeviceId, UserId};
use serde::{Deserialize, Serialize};

use crate::config::JwtConfig;

pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    access_ttl: std::time::Duration,
    refresh_ttl: std::time::Duration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccessClaims {
    pub sub: String,
    pub device_id: String,
    pub purpose: String,
    pub exp: usize,
    pub iat: usize,
    pub jti: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshClaims {
    pub sub: String,
    pub device_id: String,
    pub purpose: String,
    pub exp: usize,
    pub iat: usize,
    pub jti: String,
    pub family: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegistrationClaims {
    pub email: String,
    pub display_name: String,
    pub purpose: String,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecoveryClaims {
    pub email: String,
    pub user_id: String,
    pub purpose: String,
    pub exp: usize,
    pub iat: usize,
}

fn now_epoch() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before UNIX epoch")
        .as_secs() as usize
}

impl JwtService {
    pub fn new(config: &JwtConfig) -> Result<Self, OpenConvError> {
        let encoding_key = EncodingKey::from_ed_pem(config.private_key_pem.as_bytes())
            .map_err(|e| OpenConvError::Internal(format!("invalid JWT private key: {e}")))?;
        let decoding_key = DecodingKey::from_ed_pem(config.public_key_pem.as_bytes())
            .map_err(|e| OpenConvError::Internal(format!("invalid JWT public key: {e}")))?;

        Ok(Self {
            encoding_key,
            decoding_key,
            access_ttl: std::time::Duration::from_secs(config.access_token_ttl_seconds),
            refresh_ttl: std::time::Duration::from_secs(config.refresh_token_ttl_seconds),
        })
    }

    pub fn issue_access_token(
        &self,
        user_id: &UserId,
        device_id: &DeviceId,
    ) -> Result<String, OpenConvError> {
        let now = now_epoch();
        let claims = AccessClaims {
            sub: user_id.to_string(),
            device_id: device_id.to_string(),
            purpose: "access".to_string(),
            exp: now + self.access_ttl.as_secs() as usize,
            iat: now,
            jti: uuid::Uuid::new_v4().to_string(),
        };
        jsonwebtoken::encode(&Header::new(Algorithm::EdDSA), &claims, &self.encoding_key)
            .map_err(|e| OpenConvError::Internal(format!("JWT encode error: {e}")))
    }

    /// Issue a refresh token. Returns `(token_string, jti)` so the caller
    /// can store the jti in the database without re-validating the token.
    pub fn issue_refresh_token(
        &self,
        user_id: &UserId,
        device_id: &DeviceId,
        family: &str,
    ) -> Result<(String, String), OpenConvError> {
        let now = now_epoch();
        let jti = uuid::Uuid::new_v4().to_string();
        let claims = RefreshClaims {
            sub: user_id.to_string(),
            device_id: device_id.to_string(),
            purpose: "refresh".to_string(),
            exp: now + self.refresh_ttl.as_secs() as usize,
            iat: now,
            jti: jti.clone(),
            family: family.to_string(),
        };
        let token =
            jsonwebtoken::encode(&Header::new(Algorithm::EdDSA), &claims, &self.encoding_key)
                .map_err(|e| OpenConvError::Internal(format!("JWT encode error: {e}")))?;
        Ok((token, jti))
    }

    /// Returns the refresh token TTL for computing expires_at timestamps.
    pub fn refresh_ttl(&self) -> std::time::Duration {
        self.refresh_ttl
    }

    pub fn issue_registration_token(
        &self,
        email: &str,
        display_name: &str,
    ) -> Result<String, OpenConvError> {
        let now = now_epoch();
        let claims = RegistrationClaims {
            email: email.to_string(),
            display_name: display_name.to_string(),
            purpose: "registration".to_string(),
            exp: now + self.access_ttl.as_secs() as usize,
            iat: now,
        };
        jsonwebtoken::encode(&Header::new(Algorithm::EdDSA), &claims, &self.encoding_key)
            .map_err(|e| OpenConvError::Internal(format!("JWT encode error: {e}")))
    }

    pub fn issue_recovery_token(
        &self,
        email: &str,
        user_id: &UserId,
    ) -> Result<String, OpenConvError> {
        let now = now_epoch();
        let claims = RecoveryClaims {
            email: email.to_string(),
            user_id: user_id.to_string(),
            purpose: "recovery".to_string(),
            exp: now + self.access_ttl.as_secs() as usize,
            iat: now,
        };
        jsonwebtoken::encode(&Header::new(Algorithm::EdDSA), &claims, &self.encoding_key)
            .map_err(|e| OpenConvError::Internal(format!("JWT encode error: {e}")))
    }

    pub fn validate_access_token(&self, token: &str) -> Result<AccessClaims, OpenConvError> {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.required_spec_claims.clear();
        validation.set_required_spec_claims(&["exp"]);
        let data = jsonwebtoken::decode::<AccessClaims>(token, &self.decoding_key, &validation)
            .map_err(|_| OpenConvError::Unauthorized)?;
        if data.claims.purpose != "access" {
            return Err(OpenConvError::Unauthorized);
        }
        Ok(data.claims)
    }

    pub fn validate_refresh_token(&self, token: &str) -> Result<RefreshClaims, OpenConvError> {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.required_spec_claims.clear();
        validation.set_required_spec_claims(&["exp"]);
        let data = jsonwebtoken::decode::<RefreshClaims>(token, &self.decoding_key, &validation)
            .map_err(|_| OpenConvError::Unauthorized)?;
        if data.claims.purpose != "refresh" {
            return Err(OpenConvError::Unauthorized);
        }
        Ok(data.claims)
    }

    pub fn validate_registration_token(
        &self,
        token: &str,
    ) -> Result<RegistrationClaims, OpenConvError> {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.required_spec_claims.clear();
        validation.set_required_spec_claims(&["exp"]);
        let data =
            jsonwebtoken::decode::<RegistrationClaims>(token, &self.decoding_key, &validation)
                .map_err(|_| OpenConvError::Unauthorized)?;
        if data.claims.purpose != "registration" {
            return Err(OpenConvError::Unauthorized);
        }
        Ok(data.claims)
    }

    pub fn validate_recovery_token(&self, token: &str) -> Result<RecoveryClaims, OpenConvError> {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.required_spec_claims.clear();
        validation.set_required_spec_claims(&["exp"]);
        let data = jsonwebtoken::decode::<RecoveryClaims>(token, &self.decoding_key, &validation)
            .map_err(|_| OpenConvError::Unauthorized)?;
        if data.claims.purpose != "recovery" {
            return Err(OpenConvError::Unauthorized);
        }
        Ok(data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test Ed25519 keypair in PKCS#8 PEM format (generated for testing only)
    const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIONXw0UoRsRapn/WATSl25Hsej6hTuwsf+olF9npjjSs\n-----END PRIVATE KEY-----";
    const TEST_PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA9eB0735gPgffPc6aheXCqzsXb4ylG7Yi6I0yUIb2vZ4=\n-----END PUBLIC KEY-----";

    fn test_jwt_config() -> JwtConfig {
        JwtConfig {
            private_key_pem: TEST_PRIVATE_KEY_PEM.to_string(),
            public_key_pem: TEST_PUBLIC_KEY_PEM.to_string(),
            access_token_ttl_seconds: 300,
            refresh_token_ttl_seconds: 604800,
        }
    }

    fn test_jwt_service() -> JwtService {
        JwtService::new(&test_jwt_config()).unwrap()
    }

    #[test]
    fn jwt_service_initializes_from_pem_strings() {
        let _svc = test_jwt_service();
    }

    #[test]
    fn issue_access_token_has_correct_claims() {
        let svc = test_jwt_service();
        let uid = UserId::new();
        let did = DeviceId::new();
        let token = svc.issue_access_token(&uid, &did).unwrap();
        let claims = svc.validate_access_token(&token).unwrap();
        assert_eq!(claims.sub, uid.to_string());
        assert_eq!(claims.device_id, did.to_string());
        assert_eq!(claims.purpose, "access");
        assert!(!claims.jti.is_empty());
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn issue_refresh_token_has_purpose_refresh_and_family() {
        let svc = test_jwt_service();
        let uid = UserId::new();
        let did = DeviceId::new();
        let family = uuid::Uuid::new_v4().to_string();
        let (token, jti) = svc.issue_refresh_token(&uid, &did, &family).unwrap();
        assert!(!jti.is_empty());
        let claims = svc.validate_refresh_token(&token).unwrap();
        assert_eq!(claims.purpose, "refresh");
        assert_eq!(claims.family, family);
        assert_eq!(claims.jti, jti);
    }

    #[test]
    fn issue_registration_token_has_purpose_registration() {
        let svc = test_jwt_service();
        let token = svc
            .issue_registration_token("test@example.com", "Test User")
            .unwrap();
        let claims = svc.validate_registration_token(&token).unwrap();
        assert_eq!(claims.purpose, "registration");
        assert_eq!(claims.email, "test@example.com");
        assert_eq!(claims.display_name, "Test User");
    }

    #[test]
    fn issue_recovery_token_has_purpose_recovery() {
        let svc = test_jwt_service();
        let uid = UserId::new();
        let token = svc.issue_recovery_token("test@example.com", &uid).unwrap();
        let claims = svc.validate_recovery_token(&token).unwrap();
        assert_eq!(claims.purpose, "recovery");
        assert_eq!(claims.email, "test@example.com");
        assert_eq!(claims.user_id, uid.to_string());
    }

    #[test]
    fn validate_access_token_accepts_fresh_token() {
        let svc = test_jwt_service();
        let uid = UserId::new();
        let did = DeviceId::new();
        let token = svc.issue_access_token(&uid, &did).unwrap();
        assert!(svc.validate_access_token(&token).is_ok());
    }

    #[test]
    fn validate_access_token_rejects_refresh_purpose() {
        let svc = test_jwt_service();
        let uid = UserId::new();
        let did = DeviceId::new();
        let (token, _) = svc.issue_refresh_token(&uid, &did, "fam").unwrap();
        assert!(svc.validate_access_token(&token).is_err());
    }

    #[test]
    fn validate_refresh_token_rejects_access_purpose() {
        let svc = test_jwt_service();
        let uid = UserId::new();
        let did = DeviceId::new();
        let token = svc.issue_access_token(&uid, &did).unwrap();
        assert!(svc.validate_refresh_token(&token).is_err());
    }

    #[test]
    fn validate_registration_token_rejects_recovery_purpose() {
        let svc = test_jwt_service();
        let uid = UserId::new();
        let token = svc.issue_recovery_token("a@b.com", &uid).unwrap();
        assert!(svc.validate_registration_token(&token).is_err());
    }

    #[test]
    fn validate_access_token_rejects_different_signing_key() {
        let svc = test_jwt_service();
        let uid = UserId::new();
        let did = DeviceId::new();
        let token = svc.issue_access_token(&uid, &did).unwrap();

        // Create a different service with different keys
        let other_config = JwtConfig {
            private_key_pem: "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIAFXmYu06aQMi0Iz79P0v/MjTJIorT6e+65IWYb45JkJ\n-----END PRIVATE KEY-----".to_string(),
            public_key_pem: "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAJTLN/8jd2ZjaZOmpuzvHQGOA5mAdyvuO8kwS2CSoUXc=\n-----END PUBLIC KEY-----".to_string(),
            access_token_ttl_seconds: 300,
            refresh_token_ttl_seconds: 604800,
        };
        let other_svc = JwtService::new(&other_config).unwrap();
        assert!(other_svc.validate_access_token(&token).is_err());
    }

    #[test]
    fn validate_access_token_rejects_expired_token() {
        let svc = test_jwt_service();
        // Manually encode an already-expired token
        let claims = AccessClaims {
            sub: UserId::new().to_string(),
            device_id: DeviceId::new().to_string(),
            purpose: "access".to_string(),
            exp: 1000, // epoch + 1000s, long in the past
            iat: 900,
            jti: uuid::Uuid::new_v4().to_string(),
        };
        let token =
            jsonwebtoken::encode(&Header::new(Algorithm::EdDSA), &claims, &svc.encoding_key)
                .unwrap();
        assert!(svc.validate_access_token(&token).is_err());
    }

    #[test]
    fn access_token_default_ttl_is_5_minutes() {
        let svc = test_jwt_service();
        assert_eq!(svc.access_ttl.as_secs(), 300);
    }

    #[test]
    fn refresh_token_default_ttl_is_7_days() {
        let svc = test_jwt_service();
        assert_eq!(svc.refresh_ttl.as_secs(), 604800);
    }
}
