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
    let mut config = ServerConfig::default();
    // Raise rate limit for parallel test execution
    config.rate_limit.auth_per_ip_per_minute = 300;
    let redis = create_redis_pool(&config.redis).await.unwrap();
    let jwt = test_jwt();

    cleanup_redis_keys(&redis, &["rl:ip:10.99.0.4:auth"]).await;

    let state = AppState {
        db: pool,
        config: Arc::new(config),
        redis: redis.clone(),
        jwt: jwt.clone(),
        email: Arc::new(MockEmailService::new()),
        object_store: Arc::new(object_store::memory::InMemory::new()),
        ws: Arc::new(openconv_server::ws::state::WsState::new()),
    };
    (build_router(state), jwt, redis)
}

fn json_post(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("X-Forwarded-For", "10.99.0.4")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn generate_test_keypair() -> (String, Vec<u8>) {
    use libsignal_protocol::IdentityKeyPair;

    let identity = IdentityKeyPair::generate(&mut rand::rng());
    let public_key_b64 =
        base64::engine::general_purpose::STANDARD.encode(identity.public_key().serialize());
    let pre_key_bundle = vec![1u8, 2, 3, 4, 5];
    (public_key_b64, pre_key_bundle)
}

/// Create a user in the DB and return (user_id_uuid, email).
async fn seed_user(pool: &sqlx::PgPool) -> (uuid::Uuid, String) {
    let user_id = uuid::Uuid::now_v7();
    let email = format!("recover_{}@example.com", uuid::Uuid::new_v4());
    let (public_key, _) = generate_test_keypair();

    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind(&public_key)
        .bind(&email)
        .bind("Recovery Test User")
        .execute(pool)
        .await
        .unwrap();

    (user_id, email)
}

/// Create a user with multiple devices, refresh tokens, and pre-key bundles.
async fn seed_user_with_devices(
    pool: &sqlx::PgPool,
    jwt: &JwtService,
    num_devices: usize,
) -> (uuid::Uuid, String) {
    let (user_id, email) = seed_user(pool).await;

    for i in 0..num_devices {
        let device_id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO devices (id, user_id, device_name, last_active, created_at) VALUES ($1, $2, $3, NOW(), NOW())",
        )
        .bind(device_id)
        .bind(user_id)
        .bind(format!("Device {i}"))
        .execute(pool)
        .await
        .unwrap();

        // Add a refresh token for each device
        let uid = openconv_shared::ids::UserId(user_id);
        let did = openconv_shared::ids::DeviceId(device_id);
        let family = uuid::Uuid::now_v7().to_string();
        let (_rt, jti_str) = jwt.issue_refresh_token(&uid, &did, &family).unwrap();
        let jti: uuid::Uuid = jti_str.parse().unwrap();
        let family_uuid: uuid::Uuid = family.parse().unwrap();
        let expires_at = chrono::Utc::now() + jwt.refresh_ttl();

        sqlx::query(
            "INSERT INTO refresh_tokens (jti, user_id, device_id, family, expires_at, is_used) VALUES ($1, $2, $3, $4, $5, false)",
        )
        .bind(jti)
        .bind(user_id)
        .bind(device_id)
        .bind(family_uuid)
        .bind(expires_at)
        .execute(pool)
        .await
        .unwrap();

        // Add a pre-key bundle for each device
        sqlx::query(
            "INSERT INTO pre_key_bundles (user_id, device_id, key_data, is_used) VALUES ($1, $2, $3, false)",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(&[10u8, 20, 30] as &[u8])
        .execute(pool)
        .await
        .unwrap();
    }

    (user_id, email)
}

async fn seed_recovery_code(redis: &fred::clients::Pool, email: &str, code: &str, attempts: u32) {
    use fred::interfaces::KeysInterface;
    let data = serde_json::json!({
        "code": code,
        "attempts_remaining": attempts
    });
    let key = format!("recover:{email}");
    redis
        .set::<(), _, _>(
            &key,
            serde_json::to_string(&data).unwrap().as_str(),
            Some(fred::types::Expiration::EX(600)),
            None,
            false,
        )
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// recover/start tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn recover_start_existing_email_returns_200_and_stores_code(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, email) = seed_user(&pool).await;
    cleanup_redis_keys(
        &redis,
        &[&format!("recover:{email}"), &format!("rl:email:{email}")],
    )
    .await;

    let req = json_post(
        "/api/auth/recover/start",
        serde_json::json!({ "email": &email }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;
    assert_eq!(json["message"], "Recovery code sent");

    // Verify Redis key exists with correct structure
    use fred::interfaces::KeysInterface;
    let stored: Option<String> = redis.get(&format!("recover:{email}")).await.unwrap();
    assert!(stored.is_some(), "recovery code should be stored in Redis");

    let data: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
    assert_eq!(data["attempts_remaining"], 5);
    assert_eq!(data["code"].as_str().unwrap().len(), 6);

    let ttl: i64 = redis.ttl(&format!("recover:{email}")).await.unwrap();
    assert!(
        ttl > 0 && ttl <= 600,
        "TTL should be between 1 and 600, got {ttl}"
    );

    cleanup_redis_keys(
        &redis,
        &[&format!("recover:{email}"), &format!("rl:email:{email}")],
    )
    .await;
}

#[sqlx::test]
async fn recover_start_nonexistent_email_returns_same_200(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool).await;
    let email = "nonexistent_recover@example.com";
    cleanup_redis_keys(
        &redis,
        &[&format!("recover:{email}"), &format!("rl:email:{email}")],
    )
    .await;

    let req = json_post(
        "/api/auth/recover/start",
        serde_json::json!({ "email": email }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;
    assert_eq!(json["message"], "Recovery code sent");

    // A Redis key IS stored even for non-existent emails (timing equalization),
    // but the email itself is never sent. Verify the key exists to confirm
    // the code path is identical.
    use fred::interfaces::KeysInterface;
    let stored: Option<String> = redis.get(&format!("recover:{email}")).await.unwrap();
    assert!(
        stored.is_some(),
        "recovery code should be stored even for non-existent email (timing equalization)"
    );

    cleanup_redis_keys(
        &redis,
        &[&format!("recover:{email}"), &format!("rl:email:{email}")],
    )
    .await;
}

// ---------------------------------------------------------------------------
// recover/verify tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn recover_verify_correct_code_returns_recovery_token(pool: sqlx::PgPool) {
    let (app, jwt, redis) = build_test_app(pool.clone()).await;
    let (_, email) = seed_user(&pool).await;
    cleanup_redis_keys(&redis, &[&format!("recover:{email}")]).await;
    seed_recovery_code(&redis, &email, "123456", 5).await;

    let req = json_post(
        "/api/auth/recover/verify",
        serde_json::json!({ "email": &email, "code": "123456" }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    let token = json["recovery_token"].as_str().unwrap();
    assert!(!token.is_empty());

    let claims = jwt.validate_recovery_token(token).unwrap();
    assert_eq!(claims.purpose, "recovery");
    assert_eq!(claims.email, email);

    // Redis key should be deleted after successful verification
    use fred::interfaces::KeysInterface;
    let stored: Option<String> = redis.get(&format!("recover:{email}")).await.unwrap();
    assert!(
        stored.is_none(),
        "Redis key should be deleted after verification"
    );
}

#[sqlx::test]
async fn recover_verify_enforces_5_attempt_cap(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, email) = seed_user(&pool).await;
    cleanup_redis_keys(&redis, &[&format!("recover:{email}")]).await;
    // Seed with 2 attempts to keep the test fast
    seed_recovery_code(&redis, &email, "123456", 2).await;

    use fred::interfaces::KeysInterface;

    // 1st wrong attempt — decrements to 1
    // Clean rate limit before each request to avoid parallel test interference
    cleanup_redis_keys(&redis, &["rl:ip:10.99.0.4:auth"]).await;
    let req = json_post(
        "/api/auth/recover/verify",
        serde_json::json!({ "email": &email, "code": "999999" }),
    );
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        400,
        "1st wrong attempt should return 400"
    );

    let stored: Option<String> = redis.get(&format!("recover:{email}")).await.unwrap();
    assert!(
        stored.is_some(),
        "key should still exist after 1st wrong attempt"
    );
    let data: serde_json::Value = serde_json::from_str(stored.as_ref().unwrap()).unwrap();
    assert_eq!(data["attempts_remaining"], 1);

    // 2nd wrong attempt — decrements to 0, key deleted
    cleanup_redis_keys(&redis, &["rl:ip:10.99.0.4:auth"]).await;
    let req = json_post(
        "/api/auth/recover/verify",
        serde_json::json!({ "email": &email, "code": "999999" }),
    );
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        400,
        "2nd wrong attempt should return 400"
    );

    let stored: Option<String> = redis.get(&format!("recover:{email}")).await.unwrap();
    assert!(
        stored.is_none(),
        "key should be deleted after attempts exhausted"
    );

    // 3rd attempt — no key found, still returns 400
    cleanup_redis_keys(&redis, &["rl:ip:10.99.0.4:auth"]).await;
    let req = json_post(
        "/api/auth/recover/verify",
        serde_json::json!({ "email": &email, "code": "123456" }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        400,
        "attempt after exhaustion should return 400"
    );
}

#[sqlx::test]
async fn recover_verify_wrong_code_returns_400(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, email) = seed_user(&pool).await;
    cleanup_redis_keys(&redis, &[&format!("recover:{email}")]).await;
    seed_recovery_code(&redis, &email, "123456", 5).await;

    let req = json_post(
        "/api/auth/recover/verify",
        serde_json::json!({ "email": &email, "code": "654321" }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 400);

    // Attempts should be decremented
    use fred::interfaces::KeysInterface;
    let stored: String = redis.get(&format!("recover:{email}")).await.unwrap();
    let data: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(data["attempts_remaining"], 4);

    cleanup_redis_keys(&redis, &[&format!("recover:{email}")]).await;
}

// ---------------------------------------------------------------------------
// recover/complete tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn recover_complete_updates_public_key(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, email) = seed_user(&pool).await;
    let uid = openconv_shared::ids::UserId(user_id);
    let recovery_token = jwt.issue_recovery_token(&email, &uid).unwrap();

    let (new_public_key, new_pre_key_bundle) = generate_test_keypair();
    let device_id = uuid::Uuid::now_v7();

    let req = json_post(
        "/api/auth/recover/complete",
        serde_json::json!({
            "recovery_token": recovery_token,
            "new_public_key": new_public_key,
            "new_pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&new_pre_key_bundle),
            "device_id": device_id.to_string(),
            "device_name": "Recovery Device"
        }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let (db_pk,): (String,) = sqlx::query_as("SELECT public_key FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(db_pk, new_public_key);
}

#[sqlx::test]
async fn recover_complete_sets_public_key_changed_at(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, email) = seed_user(&pool).await;
    let uid = openconv_shared::ids::UserId(user_id);
    let recovery_token = jwt.issue_recovery_token(&email, &uid).unwrap();

    let (new_public_key, new_pre_key_bundle) = generate_test_keypair();
    let device_id = uuid::Uuid::now_v7();

    let req = json_post(
        "/api/auth/recover/complete",
        serde_json::json!({
            "recovery_token": recovery_token,
            "new_public_key": new_public_key,
            "new_pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&new_pre_key_bundle),
            "device_id": device_id.to_string(),
            "device_name": "Recovery Device"
        }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let (changed_at,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT public_key_changed_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(changed_at.is_some(), "public_key_changed_at should be set");
    let elapsed = chrono::Utc::now() - changed_at.unwrap();
    assert!(
        elapsed.num_seconds() < 10,
        "public_key_changed_at should be recent"
    );
}

#[sqlx::test]
async fn recover_complete_deletes_all_existing_devices(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, email) = seed_user_with_devices(&pool, &jwt, 3).await;
    let uid = openconv_shared::ids::UserId(user_id);
    let recovery_token = jwt.issue_recovery_token(&email, &uid).unwrap();

    let (new_public_key, new_pre_key_bundle) = generate_test_keypair();
    let new_device_id = uuid::Uuid::now_v7();

    let req = json_post(
        "/api/auth/recover/complete",
        serde_json::json!({
            "recovery_token": recovery_token,
            "new_public_key": new_public_key,
            "new_pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&new_pre_key_bundle),
            "device_id": new_device_id.to_string(),
            "device_name": "New Recovery Device"
        }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "should have exactly 1 device (the new one)");

    let (dev_id,): (uuid::Uuid,) = sqlx::query_as("SELECT id FROM devices WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(dev_id, new_device_id);
}

#[sqlx::test]
async fn recover_complete_deletes_all_existing_refresh_tokens(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, email) = seed_user_with_devices(&pool, &jwt, 2).await;
    let uid = openconv_shared::ids::UserId(user_id);
    let recovery_token = jwt.issue_recovery_token(&email, &uid).unwrap();

    let (new_public_key, new_pre_key_bundle) = generate_test_keypair();
    let device_id = uuid::Uuid::now_v7();

    let req = json_post(
        "/api/auth/recover/complete",
        serde_json::json!({
            "recovery_token": recovery_token,
            "new_public_key": new_public_key,
            "new_pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&new_pre_key_bundle),
            "device_id": device_id.to_string(),
            "device_name": "Recovery Device"
        }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "should have exactly 1 refresh token (the new one)"
    );
}

#[sqlx::test]
async fn recover_complete_deletes_all_existing_pre_key_bundles(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, email) = seed_user_with_devices(&pool, &jwt, 3).await;
    let uid = openconv_shared::ids::UserId(user_id);
    let recovery_token = jwt.issue_recovery_token(&email, &uid).unwrap();

    let (new_public_key, new_pre_key_bundle) = generate_test_keypair();
    let device_id = uuid::Uuid::now_v7();

    let req = json_post(
        "/api/auth/recover/complete",
        serde_json::json!({
            "recovery_token": recovery_token,
            "new_public_key": new_public_key,
            "new_pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&new_pre_key_bundle),
            "device_id": device_id.to_string(),
            "device_name": "Recovery Device"
        }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pre_key_bundles WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "should have exactly 1 pre-key bundle (the new one)"
    );
}

#[sqlx::test]
async fn recover_complete_creates_new_device_and_bundle(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, email) = seed_user(&pool).await;
    let uid = openconv_shared::ids::UserId(user_id);
    let recovery_token = jwt.issue_recovery_token(&email, &uid).unwrap();

    let (new_public_key, new_pre_key_bundle) = generate_test_keypair();
    let device_id = uuid::Uuid::now_v7();

    let req = json_post(
        "/api/auth/recover/complete",
        serde_json::json!({
            "recovery_token": recovery_token,
            "new_public_key": new_public_key,
            "new_pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&new_pre_key_bundle),
            "device_id": device_id.to_string(),
            "device_name": "Fresh Device"
        }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    // Check device exists
    let (dev_name,): (String,) =
        sqlx::query_as("SELECT device_name FROM devices WHERE id = $1 AND user_id = $2")
            .bind(device_id)
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(dev_name, "Fresh Device");

    // Check pre-key bundle linked to new device
    let (bundle_device_id,): (uuid::Uuid,) = sqlx::query_as(
        "SELECT device_id FROM pre_key_bundles WHERE user_id = $1 AND device_id = $2",
    )
    .bind(user_id)
    .bind(device_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(bundle_device_id, device_id);
}

#[sqlx::test]
async fn recover_complete_returns_new_tokens(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, email) = seed_user(&pool).await;
    let uid = openconv_shared::ids::UserId(user_id);
    let recovery_token = jwt.issue_recovery_token(&email, &uid).unwrap();

    let (new_public_key, new_pre_key_bundle) = generate_test_keypair();
    let device_id = uuid::Uuid::now_v7();

    let req = json_post(
        "/api/auth/recover/complete",
        serde_json::json!({
            "recovery_token": recovery_token,
            "new_public_key": new_public_key,
            "new_pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&new_pre_key_bundle),
            "device_id": device_id.to_string(),
            "device_name": "Token Device"
        }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    assert!(json["user_id"].as_str().is_some());
    assert!(json["access_token"].as_str().is_some());
    assert!(json["refresh_token"].as_str().is_some());
    assert_eq!(json["device_id"].as_str().unwrap(), device_id.to_string());

    // Validate access token
    let access_claims = jwt
        .validate_access_token(json["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(access_claims.purpose, "access");
    assert_eq!(access_claims.sub, user_id.to_string());

    // Validate refresh token
    let refresh_claims = jwt
        .validate_refresh_token(json["refresh_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(refresh_claims.purpose, "refresh");
    assert_eq!(refresh_claims.sub, user_id.to_string());
}

#[sqlx::test]
async fn recover_complete_rejects_wrong_purpose_token(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool).await;

    // Use a registration token instead of recovery token
    let wrong_token = jwt
        .issue_registration_token("test@example.com", "Test")
        .unwrap();

    let (new_public_key, new_pre_key_bundle) = generate_test_keypair();

    let req = json_post(
        "/api/auth/recover/complete",
        serde_json::json!({
            "recovery_token": wrong_token,
            "new_public_key": new_public_key,
            "new_pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&new_pre_key_bundle),
            "device_id": uuid::Uuid::now_v7().to_string(),
            "device_name": "Test"
        }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}
