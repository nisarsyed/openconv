use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use base64::Engine;
use tower::ServiceExt;

use openconv_server::config::{JwtConfig, ServerConfig};
use openconv_server::email::MockEmailService;
use openconv_server::jwt::JwtService;
use openconv_server::redis::create_redis_pool;
use openconv_server::router::build_router;
use openconv_server::state::AppState;

const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIONXw0UoRsRapn/WATSl25Hsej6hTuwsf+olF9npjjSs\n-----END PRIVATE KEY-----";
const TEST_PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA9eB0735gPgffPc6aheXCqzsXb4ylG7Yi6I0yUIb2vZ4=\n-----END PUBLIC KEY-----";

fn test_jwt() -> Arc<JwtService> {
    let jwt_config = JwtConfig {
        private_key_pem: TEST_PRIVATE_KEY_PEM.to_string(),
        public_key_pem: TEST_PUBLIC_KEY_PEM.to_string(),
        ..Default::default()
    };
    Arc::new(JwtService::new(&jwt_config).unwrap())
}

async fn cleanup_redis_keys(redis: &fred::clients::Pool, patterns: &[&str]) {
    use fred::interfaces::KeysInterface;
    for key in patterns {
        let _: i64 = redis.del(*key).await.unwrap_or_default();
    }
}

async fn build_test_app(
    pool: sqlx::PgPool,
) -> (axum::Router, Arc<JwtService>, fred::clients::Pool) {
    let config = ServerConfig::default();
    let redis = create_redis_pool(&config.redis).await.unwrap();
    let jwt = test_jwt();

    cleanup_redis_keys(&redis, &["rl:ip:10.99.0.3:auth"]).await;

    let state = AppState {
        db: pool,
        config: Arc::new(config),
        redis: redis.clone(),
        jwt: jwt.clone(),
        email: Arc::new(MockEmailService::new()),
        object_store: Arc::new(object_store::memory::InMemory::new()),
    };
    (build_router(state), jwt, redis)
}

fn json_post(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("X-Forwarded-For", "10.99.0.3")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

fn authed_post(uri: &str, token: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Forwarded-For", "10.99.0.3")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

fn authed_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Forwarded-For", "10.99.0.3")
        .body(Body::empty())
        .unwrap()
}

fn authed_delete(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Forwarded-For", "10.99.0.3")
        .body(Body::empty())
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Create a user + device + refresh token in the DB and return (user_id, device_id, access_token, refresh_token, family).
async fn seed_user_with_session(
    pool: &sqlx::PgPool,
    jwt: &JwtService,
) -> (
    openconv_shared::ids::UserId,
    openconv_shared::ids::DeviceId,
    String,
    String,
    String,
) {
    let user_id = openconv_shared::ids::UserId::new();
    let device_id = openconv_shared::ids::DeviceId::new();

    // Create user
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id.0)
        .bind(format!("pk_{}", uuid::Uuid::new_v4()))
        .bind(format!("{}@example.com", uuid::Uuid::new_v4()))
        .bind("Test User")
        .execute(pool)
        .await
        .unwrap();

    // Create device
    sqlx::query(
        "INSERT INTO devices (id, user_id, device_name, last_active, created_at) VALUES ($1, $2, $3, NOW(), NOW())",
    )
    .bind(device_id.0)
    .bind(user_id.0)
    .bind("Test Device")
    .execute(pool)
    .await
    .unwrap();

    // Issue tokens
    let family = uuid::Uuid::now_v7().to_string();
    let access_token = jwt.issue_access_token(&user_id, &device_id).unwrap();
    let (refresh_token, jti_str) = jwt
        .issue_refresh_token(&user_id, &device_id, &family)
        .unwrap();

    // Store refresh token in DB
    let jti: uuid::Uuid = jti_str.parse().unwrap();
    let family_uuid: uuid::Uuid = family.parse().unwrap();
    let expires_at = chrono::Utc::now() + jwt.refresh_ttl();

    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at, is_used) VALUES ($1, $2, $3, $4, $5, false)",
    )
    .bind(jti)
    .bind(user_id.0)
    .bind(device_id.0)
    .bind(family_uuid)
    .bind(expires_at)
    .execute(pool)
    .await
    .unwrap();

    (user_id, device_id, access_token, refresh_token, family)
}

// ---------------------------------------------------------------------------
// 6.1 Token Refresh Tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn refresh_valid_unused_token_returns_new_pair(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, _, refresh_token, _) = seed_user_with_session(&pool, &jwt).await;

    let req = json_post(
        "/api/auth/refresh",
        serde_json::json!({ "refresh_token": refresh_token }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    assert!(json["access_token"].as_str().is_some());
    assert!(json["refresh_token"].as_str().is_some());
}

#[sqlx::test]
async fn refresh_marks_old_token_as_used(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, _, refresh_token, _) = seed_user_with_session(&pool, &jwt).await;

    let old_claims = jwt.validate_refresh_token(&refresh_token).unwrap();
    let old_jti: uuid::Uuid = old_claims.jti.parse().unwrap();

    let req = json_post(
        "/api/auth/refresh",
        serde_json::json!({ "refresh_token": refresh_token }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let (is_used, used_at): (bool, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT is_used, used_at FROM refresh_tokens WHERE jti = $1")
            .bind(old_jti)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(is_used);
    assert!(used_at.is_some());
}

#[sqlx::test]
async fn refresh_issues_new_token_in_same_family(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, _, refresh_token, family) = seed_user_with_session(&pool, &jwt).await;

    let req = json_post(
        "/api/auth/refresh",
        serde_json::json!({ "refresh_token": refresh_token }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    let new_refresh = json["refresh_token"].as_str().unwrap();
    let new_claims = jwt.validate_refresh_token(new_refresh).unwrap();
    assert_eq!(new_claims.family, family);

    // New token should exist in DB in same family
    let family_uuid: uuid::Uuid = family.parse().unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE family = $1")
        .bind(family_uuid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2); // old + new
}

#[sqlx::test]
async fn refresh_reused_token_invalidates_entire_family(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, device_id, _, refresh_token, family) = seed_user_with_session(&pool, &jwt).await;

    // Mark the token as already used (simulating prior refresh)
    let claims = jwt.validate_refresh_token(&refresh_token).unwrap();
    let jti: uuid::Uuid = claims.jti.parse().unwrap();
    sqlx::query("UPDATE refresh_tokens SET is_used = true, used_at = NOW() WHERE jti = $1")
        .bind(jti)
        .execute(&pool)
        .await
        .unwrap();

    // Also add another token in the same family (simulating new token from prior refresh)
    let family_uuid: uuid::Uuid = family.parse().unwrap();
    let other_jti = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at, is_used) VALUES ($1, $2, $3, $4, NOW() + INTERVAL '7 days', false)",
    )
    .bind(other_jti)
    .bind(user_id.0)
    .bind(device_id.0)
    .bind(family_uuid)
    .execute(&pool)
    .await
    .unwrap();

    // Try to reuse the old token
    let req = json_post(
        "/api/auth/refresh",
        serde_json::json!({ "refresh_token": refresh_token }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);

    // All tokens in the family should now be invalidated
    let unused_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens WHERE family = $1 AND is_used = false",
    )
    .bind(family_uuid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unused_count, 0);
}

#[sqlx::test]
async fn refresh_reused_token_returns_session_compromised(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, _, refresh_token, _) = seed_user_with_session(&pool, &jwt).await;

    // Mark token as used
    let claims = jwt.validate_refresh_token(&refresh_token).unwrap();
    let jti: uuid::Uuid = claims.jti.parse().unwrap();
    sqlx::query("UPDATE refresh_tokens SET is_used = true, used_at = NOW() WHERE jti = $1")
        .bind(jti)
        .execute(&pool)
        .await
        .unwrap();

    let req = json_post(
        "/api/auth/refresh",
        serde_json::json!({ "refresh_token": refresh_token }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);

    let json = response_json(response).await;
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("session compromised"),
        "expected 'session compromised', got: {:?}",
        json["error"]
    );
}

#[sqlx::test]
async fn refresh_expired_token_returns_401(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    // Construct a properly expired refresh JWT
    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use openconv_server::jwt::RefreshClaims;

    let encoding_key = EncodingKey::from_ed_pem(TEST_PRIVATE_KEY_PEM.as_bytes()).unwrap();
    let claims = RefreshClaims {
        sub: openconv_shared::ids::UserId::new().to_string(),
        device_id: openconv_shared::ids::DeviceId::new().to_string(),
        purpose: "refresh".to_string(),
        exp: 1000, // epoch + 1000s, clearly expired
        iat: 900,
        jti: uuid::Uuid::new_v4().to_string(),
        family: uuid::Uuid::new_v4().to_string(),
    };
    let expired_token =
        jsonwebtoken::encode(&Header::new(Algorithm::EdDSA), &claims, &encoding_key).unwrap();

    let req = json_post(
        "/api/auth/refresh",
        serde_json::json!({ "refresh_token": expired_token }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

#[sqlx::test]
async fn refresh_wrong_purpose_returns_401(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, access_token, _, _) = seed_user_with_session(&pool, &jwt).await;

    // Send the access token as a refresh token
    let req = json_post(
        "/api/auth/refresh",
        serde_json::json!({ "refresh_token": access_token }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

// ---------------------------------------------------------------------------
// 6.2 Logout Tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn logout_invalidates_current_device_tokens_only(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, device_id1, access_token1, _, _) = seed_user_with_session(&pool, &jwt).await;

    // Create second device with its own refresh token
    let device_id2 = openconv_shared::ids::DeviceId::new();
    sqlx::query(
        "INSERT INTO devices (id, user_id, device_name, last_active, created_at) VALUES ($1, $2, $3, NOW(), NOW())",
    )
    .bind(device_id2.0)
    .bind(user_id.0)
    .bind("Device 2")
    .execute(&pool)
    .await
    .unwrap();

    let family2 = uuid::Uuid::now_v7().to_string();
    let (rt2, jti2_str) = jwt
        .issue_refresh_token(&user_id, &device_id2, &family2)
        .unwrap();
    let jti2: uuid::Uuid = jti2_str.parse().unwrap();
    let family2_uuid: uuid::Uuid = family2.parse().unwrap();
    let exp2 = chrono::Utc::now() + jwt.refresh_ttl();

    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at, is_used) VALUES ($1, $2, $3, $4, $5, false)",
    )
    .bind(jti2)
    .bind(user_id.0)
    .bind(device_id2.0)
    .bind(family2_uuid)
    .bind(exp2)
    .execute(&pool)
    .await
    .unwrap();

    // Logout device 1
    let req = authed_post("/api/auth/logout", &access_token1, serde_json::json!({}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    // Device 1 tokens should be invalidated
    let d1_unused: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens WHERE device_id = $1 AND is_used = false",
    )
    .bind(device_id1.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(d1_unused, 0);

    // Device 2 tokens should be untouched
    let d2_unused: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens WHERE device_id = $1 AND is_used = false",
    )
    .bind(device_id2.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(d2_unused, 1);
}

#[sqlx::test]
async fn logout_returns_200(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, access_token, _, _) = seed_user_with_session(&pool, &jwt).await;

    let req = authed_post("/api/auth/logout", &access_token, serde_json::json!({}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
}

#[sqlx::test]
async fn logout_all_invalidates_all_user_tokens(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, _, access_token, _, _) = seed_user_with_session(&pool, &jwt).await;

    // Add 2 more devices with tokens
    for i in 0..2 {
        let did = openconv_shared::ids::DeviceId::new();
        sqlx::query(
            "INSERT INTO devices (id, user_id, device_name, last_active, created_at) VALUES ($1, $2, $3, NOW(), NOW())",
        )
        .bind(did.0)
        .bind(user_id.0)
        .bind(format!("Extra Device {i}"))
        .execute(&pool)
        .await
        .unwrap();

        let fam = uuid::Uuid::now_v7().to_string();
        let (_rt, jti_str) = jwt.issue_refresh_token(&user_id, &did, &fam).unwrap();
        let jti: uuid::Uuid = jti_str.parse().unwrap();
        let fam_uuid: uuid::Uuid = fam.parse().unwrap();
        let exp = chrono::Utc::now() + jwt.refresh_ttl();

        sqlx::query(
            "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at, is_used) VALUES ($1, $2, $3, $4, $5, false)",
        )
        .bind(jti)
        .bind(user_id.0)
        .bind(did.0)
        .bind(fam_uuid)
        .bind(exp)
        .execute(&pool)
        .await
        .unwrap();
    }

    let req = authed_post("/api/auth/logout-all", &access_token, serde_json::json!({}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    // ALL tokens for this user should be invalidated
    let unused: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens WHERE user_id = $1 AND is_used = false",
    )
    .bind(user_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unused, 0);
}

#[sqlx::test]
async fn logout_without_auth_returns_401(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    let req = json_post("/api/auth/logout", serde_json::json!({}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

#[sqlx::test]
async fn logout_all_without_auth_returns_401(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    let req = json_post("/api/auth/logout-all", serde_json::json!({}));
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

// ---------------------------------------------------------------------------
// 6.3 Device Management Tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn get_devices_returns_all_user_devices(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, _, access_token, _, _) = seed_user_with_session(&pool, &jwt).await;

    // Add a second device
    let did2 = openconv_shared::ids::DeviceId::new();
    sqlx::query(
        "INSERT INTO devices (id, user_id, device_name, last_active, created_at) VALUES ($1, $2, $3, NOW(), NOW())",
    )
    .bind(did2.0)
    .bind(user_id.0)
    .bind("Second Device")
    .execute(&pool)
    .await
    .unwrap();

    let req = authed_get("/api/auth/devices", &access_token);
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    let devices = json["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 2);
}

#[sqlx::test]
async fn get_devices_excludes_other_users_devices(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, access_token1, _, _) = seed_user_with_session(&pool, &jwt).await;
    // Create another user with a device
    let _ = seed_user_with_session(&pool, &jwt).await;

    let req = authed_get("/api/auth/devices", &access_token1);
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    let devices = json["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
}

#[sqlx::test]
async fn delete_device_removes_device_and_tokens(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, device_id1, access_token, _, _) = seed_user_with_session(&pool, &jwt).await;

    // Add second device with its own token (access_token is from device_id1)
    let device_id2 = openconv_shared::ids::DeviceId::new();
    sqlx::query(
        "INSERT INTO devices (id, user_id, device_name, last_active, created_at) VALUES ($1, $2, $3, NOW(), NOW())",
    )
    .bind(device_id2.0)
    .bind(user_id.0)
    .bind("Device To Delete")
    .execute(&pool)
    .await
    .unwrap();

    let family2 = uuid::Uuid::now_v7().to_string();
    let (_rt2, jti2_str) = jwt
        .issue_refresh_token(&user_id, &device_id2, &family2)
        .unwrap();
    let jti2: uuid::Uuid = jti2_str.parse().unwrap();
    let fam2_uuid: uuid::Uuid = family2.parse().unwrap();
    let exp2 = chrono::Utc::now() + jwt.refresh_ttl();

    sqlx::query(
        "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at, is_used) VALUES ($1, $2, $3, $4, $5, false)",
    )
    .bind(jti2)
    .bind(user_id.0)
    .bind(device_id2.0)
    .bind(fam2_uuid)
    .bind(exp2)
    .execute(&pool)
    .await
    .unwrap();

    // Delete device 2
    let req = authed_delete(
        &format!("/api/auth/devices/{}", device_id2.0),
        &access_token,
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    // Device 2 should be gone
    let dev_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE id = $1")
        .bind(device_id2.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(dev_count, 0);

    // Device 2 tokens should be gone (CASCADE)
    let tok_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE device_id = $1")
            .bind(device_id2.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tok_count, 0);

    // Device 1 should be unaffected
    let d1_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE id = $1")
        .bind(device_id1.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(d1_count, 1);
}

#[sqlx::test]
async fn delete_nonexistent_device_returns_404(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, access_token, _, _) = seed_user_with_session(&pool, &jwt).await;

    let fake_id = uuid::Uuid::now_v7();
    let req = authed_delete(&format!("/api/auth/devices/{fake_id}"), &access_token);
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 404);
}

#[sqlx::test]
async fn delete_other_users_device_returns_403(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;

    // User A owns device_a
    let (_, device_a, _, _, _) = seed_user_with_session(&pool, &jwt).await;

    // User B with their own access token
    let (_, _, access_token_b, _, _) = seed_user_with_session(&pool, &jwt).await;

    // User B tries to delete User A's device
    let req = authed_delete(
        &format!("/api/auth/devices/{}", device_a.0),
        &access_token_b,
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 403);
}
