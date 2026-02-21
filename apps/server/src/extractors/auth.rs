use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use openconv_shared::ids::{DeviceId, UserId};

use crate::state::AppState;

/// Authenticated user information extracted from a valid access JWT.
///
/// Use this as a handler parameter to require authentication:
/// ```ignore
/// async fn my_handler(auth: AuthUser) -> impl IntoResponse { ... }
/// ```
#[derive(Debug)]
pub struct AuthUser {
    pub user_id: UserId,
    pub device_id: DeviceId,
}

#[derive(Debug)]
pub struct AuthRejection;

impl IntoResponse for AuthRejection {
    fn into_response(self) -> Response {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response()
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                tracing::debug!("auth: missing or non-ASCII Authorization header");
                AuthRejection
            })?;

        let token = header.strip_prefix("Bearer ").ok_or_else(|| {
            tracing::debug!("auth: Authorization header missing Bearer prefix");
            AuthRejection
        })?;

        let claims = state.jwt.validate_access_token(token).map_err(|e| {
            tracing::debug!(error = %e, "auth: token validation failed");
            AuthRejection
        })?;

        let user_id: UserId = claims.sub.parse().map_err(|_| AuthRejection)?;
        let device_id: DeviceId = claims.device_id.parse().map_err(|_| AuthRejection)?;

        Ok(AuthUser { user_id, device_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::config::{JwtConfig, ServerConfig};
    use crate::email::MockEmailService;
    use crate::jwt::JwtService;

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

    fn test_app_state() -> AppState {
        let jwt = Arc::new(test_jwt_service());
        let db = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/openconv_test")
            .unwrap();
        let redis_config = fred::types::config::Config::from_url("redis://localhost:6379").unwrap();
        let redis = fred::clients::Pool::new(redis_config, None, None, None, 1).unwrap();
        let config = Arc::new(ServerConfig {
            database_url: "postgres://localhost/openconv_test".to_string(),
            ..ServerConfig::default()
        });
        let email: Arc<dyn crate::email::EmailService> = Arc::new(MockEmailService::new());
        AppState {
            db,
            config,
            redis,
            jwt,
            email,
            object_store: std::sync::Arc::new(object_store::memory::InMemory::new()),
            ws: std::sync::Arc::new(crate::ws::state::WsState::new()),
        }
    }

    #[tokio::test]
    async fn auth_user_extractor_returns_user_id_and_device_id_from_valid_token() {
        let state = test_app_state();
        let uid = UserId::new();
        let did = DeviceId::new();
        let token = state.jwt.issue_access_token(&uid, &did).unwrap();

        let request = axum::http::Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();

        let auth = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        assert_eq!(auth.user_id, uid);
        assert_eq!(auth.device_id, did);
    }

    #[tokio::test]
    async fn auth_user_extractor_returns_401_when_header_missing() {
        let state = test_app_state();

        let request = axum::http::Request::builder().body(()).unwrap();
        let (mut parts, _) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &state).await;
        assert!(result.is_err());
        let response = result.unwrap_err().into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_user_extractor_returns_401_when_token_expired() {
        use crate::jwt::AccessClaims;
        use jsonwebtoken::{Algorithm, EncodingKey, Header};

        let state = test_app_state();
        // Manually create a token with exp far in the past (beyond any leeway)
        let claims = AccessClaims {
            sub: UserId::new().to_string(),
            device_id: DeviceId::new().to_string(),
            purpose: "access".to_string(),
            exp: 1000, // epoch + 1000s, clearly expired
            iat: 900,
            jti: uuid::Uuid::new_v4().to_string(),
        };
        let encoding_key = EncodingKey::from_ed_pem(TEST_PRIVATE_KEY_PEM.as_bytes()).unwrap();
        let token =
            jsonwebtoken::encode(&Header::new(Algorithm::EdDSA), &claims, &encoding_key).unwrap();

        let request = axum::http::Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &state).await;
        assert!(result.is_err());
        let response = result.unwrap_err().into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_user_extractor_returns_401_when_token_has_wrong_purpose() {
        let state = test_app_state();
        let uid = UserId::new();
        let did = DeviceId::new();
        // Issue a refresh token instead of access token
        let (token, _) = state
            .jwt
            .issue_refresh_token(&uid, &did, "family123")
            .unwrap();

        let request = axum::http::Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &state).await;
        assert!(result.is_err());
        let response = result.unwrap_err().into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_user_extractor_returns_401_when_token_is_malformed() {
        let state = test_app_state();

        let request = axum::http::Request::builder()
            .header("Authorization", "Bearer not-a-jwt")
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &state).await;
        assert!(result.is_err());
        let response = result.unwrap_err().into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
