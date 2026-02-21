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
    let state = AppState {
        db: pool,
        config: Arc::new(config),
        redis: redis.clone(),
        jwt: jwt.clone(),
        email: Arc::new(MockEmailService::new()),
    };
    (build_router(state), jwt, redis)
}

fn json_request(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("X-Forwarded-For", "10.99.0.1")
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
    use base64::Engine;
    use libsignal_protocol::IdentityKeyPair;

    let identity = IdentityKeyPair::generate(&mut rand::rng());
    let public_key_b64 =
        base64::engine::general_purpose::STANDARD.encode(identity.public_key().serialize());
    let pre_key_bundle = vec![1u8, 2, 3, 4, 5]; // Dummy pre-key data
    (public_key_b64, pre_key_bundle)
}

// ---------------------------------------------------------------------------
// register/start tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn register_start_new_email_returns_200_generic_message(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool).await;
    cleanup_redis_keys(
        &redis,
        &["verify:newuser@example.com", "rl:email:newuser@example.com"],
    )
    .await;

    let req = json_request(
        "/api/auth/register/start",
        serde_json::json!({
            "email": "newuser@example.com",
            "display_name": "New User"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;
    assert_eq!(json["message"], "Verification code sent");

    cleanup_redis_keys(
        &redis,
        &["verify:newuser@example.com", "rl:email:newuser@example.com"],
    )
    .await;
}

#[sqlx::test]
async fn register_start_existing_email_returns_same_200(pool: sqlx::PgPool) {
    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(uuid::Uuid::now_v7())
        .bind("existingpk_unique_1")
        .bind("existing@example.com")
        .bind("Existing User")
        .execute(&pool)
        .await
        .unwrap();

    let (app, _, redis) = build_test_app(pool).await;
    cleanup_redis_keys(&redis, &["rl:email:existing@example.com"]).await;

    let req = json_request(
        "/api/auth/register/start",
        serde_json::json!({
            "email": "existing@example.com",
            "display_name": "Existing User"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;
    assert_eq!(json["message"], "Verification code sent");

    cleanup_redis_keys(&redis, &["rl:email:existing@example.com"]).await;
}

#[sqlx::test]
async fn register_start_stores_code_in_redis_with_ttl(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool).await;
    let email = "redischeck@example.com";
    cleanup_redis_keys(
        &redis,
        &[&format!("verify:{email}"), &format!("rl:email:{email}")],
    )
    .await;

    let req = json_request(
        "/api/auth/register/start",
        serde_json::json!({
            "email": email,
            "display_name": "Redis Check"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    use fred::interfaces::KeysInterface;
    let stored: Option<String> = redis.get(&format!("verify:{email}")).await.unwrap();
    assert!(stored.is_some(), "verification data should be in Redis");

    let data: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
    assert_eq!(data["attempts_remaining"], 5);
    assert_eq!(data["display_name"], "Redis Check");
    assert_eq!(data["code"].as_str().unwrap().len(), 6);

    let ttl: i64 = redis.ttl(&format!("verify:{email}")).await.unwrap();
    assert!(
        ttl > 0 && ttl <= 600,
        "TTL should be between 1 and 600 seconds, got {ttl}"
    );

    cleanup_redis_keys(
        &redis,
        &[&format!("verify:{email}"), &format!("rl:email:{email}")],
    )
    .await;
}

#[sqlx::test]
async fn register_start_rejects_invalid_email(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    let req = json_request(
        "/api/auth/register/start",
        serde_json::json!({
            "email": "not-an-email",
            "display_name": "Test"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 400);
}

#[sqlx::test]
async fn register_start_rejects_empty_display_name(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    let req = json_request(
        "/api/auth/register/start",
        serde_json::json!({
            "email": "test@example.com",
            "display_name": ""
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 400);
}

#[sqlx::test]
async fn register_start_rejects_display_name_over_64_chars(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    let req = json_request(
        "/api/auth/register/start",
        serde_json::json!({
            "email": "test@example.com",
            "display_name": "a".repeat(65)
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 400);
}

// ---------------------------------------------------------------------------
// register/verify tests
// ---------------------------------------------------------------------------

async fn seed_verification_code(
    redis: &fred::clients::Pool,
    email: &str,
    code: &str,
    attempts: u32,
) {
    use fred::interfaces::KeysInterface;
    let data = serde_json::json!({
        "code": code,
        "display_name": "Test User",
        "attempts_remaining": attempts
    });
    let key = format!("verify:{email}");
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

#[sqlx::test]
async fn register_verify_correct_code_returns_registration_token(pool: sqlx::PgPool) {
    let (app, jwt, redis) = build_test_app(pool).await;
    let email = "verify_ok@example.com";
    cleanup_redis_keys(&redis, &[&format!("verify:{email}")]).await;
    seed_verification_code(&redis, email, "123456", 5).await;

    let req = json_request(
        "/api/auth/register/verify",
        serde_json::json!({
            "email": email,
            "code": "123456"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;
    let token = json["registration_token"].as_str().unwrap();
    assert!(!token.is_empty());

    let claims = jwt.validate_registration_token(token).unwrap();
    assert_eq!(claims.purpose, "registration");
    assert_eq!(claims.email, email);

    cleanup_redis_keys(&redis, &[&format!("verify:{email}")]).await;
}

#[sqlx::test]
async fn register_verify_wrong_code_returns_400(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool).await;
    let email = "verify_bad@example.com";
    cleanup_redis_keys(&redis, &[&format!("verify:{email}")]).await;
    seed_verification_code(&redis, email, "123456", 5).await;

    let req = json_request(
        "/api/auth/register/verify",
        serde_json::json!({
            "email": email,
            "code": "999999"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 400);

    // Verify attempts decremented (via Lua script)
    use fred::interfaces::KeysInterface;
    let stored: String = redis.get(&format!("verify:{email}")).await.unwrap();
    let data: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(data["attempts_remaining"], 4);

    cleanup_redis_keys(&redis, &[&format!("verify:{email}")]).await;
}

#[sqlx::test]
async fn register_verify_expired_code_returns_400(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool).await;
    let email = "verify_expired@example.com";
    cleanup_redis_keys(&redis, &[&format!("verify:{email}")]).await;

    let req = json_request(
        "/api/auth/register/verify",
        serde_json::json!({
            "email": email,
            "code": "123456"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 400);
    let json = response_json(response).await;
    assert!(json["error"].as_str().unwrap().contains("expired"));
}

#[sqlx::test]
async fn register_verify_correct_code_deletes_redis_key(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool).await;
    let email = "verify_del@example.com";
    cleanup_redis_keys(&redis, &[&format!("verify:{email}")]).await;
    seed_verification_code(&redis, email, "654321", 5).await;

    let req = json_request(
        "/api/auth/register/verify",
        serde_json::json!({
            "email": email,
            "code": "654321"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    use fred::interfaces::KeysInterface;
    let stored: Option<String> = redis.get(&format!("verify:{email}")).await.unwrap();
    assert!(
        stored.is_none(),
        "Redis key should be deleted after successful verification"
    );
}

// ---------------------------------------------------------------------------
// register/complete tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn register_complete_creates_user_in_db(pool: sqlx::PgPool) {
    let (app, jwt, redis) = build_test_app(pool.clone()).await;
    let email = "complete_user@example.com";
    cleanup_redis_keys(&redis, &[&format!("rl:email:{email}")]).await;

    let token = jwt
        .issue_registration_token(email, "Complete User")
        .unwrap();
    let (public_key, pre_key_bundle) = generate_test_keypair();
    let device_id = uuid::Uuid::now_v7();

    let req = json_request(
        "/api/auth/register/complete",
        serde_json::json!({
            "registration_token": token,
            "public_key": public_key,
            "pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&pre_key_bundle),
            "device_id": device_id.to_string(),
            "device_name": "Test Device"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let user: (String, String) =
        sqlx::query_as("SELECT email, display_name FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(user.0, email);
    assert_eq!(user.1, "Complete User");
}

#[sqlx::test]
async fn register_complete_returns_tokens_and_ids(pool: sqlx::PgPool) {
    let (app, jwt, redis) = build_test_app(pool).await;
    let email = "complete_tokens@example.com";
    cleanup_redis_keys(&redis, &[&format!("rl:email:{email}")]).await;

    let token = jwt.issue_registration_token(email, "Token User").unwrap();
    let (public_key, pre_key_bundle) = generate_test_keypair();
    let device_id = uuid::Uuid::now_v7();

    let req = json_request(
        "/api/auth/register/complete",
        serde_json::json!({
            "registration_token": token,
            "public_key": public_key,
            "pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&pre_key_bundle),
            "device_id": device_id.to_string(),
            "device_name": "Test Device"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;

    assert!(json["user_id"].as_str().is_some());
    assert!(json["access_token"].as_str().is_some());
    assert!(json["refresh_token"].as_str().is_some());
    assert_eq!(json["device_id"].as_str().unwrap(), device_id.to_string());

    let access_claims = jwt
        .validate_access_token(json["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(access_claims.purpose, "access");
}

#[sqlx::test]
async fn register_complete_stores_prekey_bundle_with_device(pool: sqlx::PgPool) {
    let (app, jwt, redis) = build_test_app(pool.clone()).await;
    let email = "complete_prekey@example.com";
    cleanup_redis_keys(&redis, &[&format!("rl:email:{email}")]).await;

    let token = jwt.issue_registration_token(email, "Prekey User").unwrap();
    let (public_key, pre_key_bundle) = generate_test_keypair();
    let device_id = uuid::Uuid::now_v7();

    let req = json_request(
        "/api/auth/register/complete",
        serde_json::json!({
            "registration_token": token,
            "public_key": public_key,
            "pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&pre_key_bundle),
            "device_id": device_id.to_string(),
            "device_name": "Test Device"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let row: (uuid::Uuid,) =
        sqlx::query_as("SELECT device_id FROM pre_key_bundles WHERE device_id = $1")
            .bind(device_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, device_id);
}

#[sqlx::test]
async fn register_complete_expired_token_returns_401(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    let req = json_request(
        "/api/auth/register/complete",
        serde_json::json!({
            "registration_token": "expired.jwt.token",
            "public_key": "aaa",
            "pre_key_bundle": "bbb",
            "device_id": uuid::Uuid::now_v7().to_string(),
            "device_name": "Test"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

#[sqlx::test]
async fn register_complete_wrong_purpose_token_returns_401(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool).await;

    let uid = openconv_shared::ids::UserId::new();
    let did = openconv_shared::ids::DeviceId::new();
    let wrong_token = jwt.issue_access_token(&uid, &did).unwrap();

    let req = json_request(
        "/api/auth/register/complete",
        serde_json::json!({
            "registration_token": wrong_token,
            "public_key": "aaa",
            "pre_key_bundle": "bbb",
            "device_id": uuid::Uuid::now_v7().to_string(),
            "device_name": "Test"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

#[sqlx::test]
async fn register_complete_invalid_public_key_returns_400(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool).await;

    let token = jwt
        .issue_registration_token("pk_test@example.com", "Test")
        .unwrap();

    let bad_key = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);

    let req = json_request(
        "/api/auth/register/complete",
        serde_json::json!({
            "registration_token": token,
            "public_key": bad_key,
            "pre_key_bundle": base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]),
            "device_id": uuid::Uuid::now_v7().to_string(),
            "device_name": "Test"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 400);
}

#[sqlx::test]
async fn register_complete_duplicate_email_returns_conflict(pool: sqlx::PgPool) {
    let (public_key, pre_key_bundle) = generate_test_keypair();
    let email = "dup@example.com";

    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(uuid::Uuid::now_v7())
        .bind("unique_pk_for_dup_test")
        .bind(email)
        .bind("Existing")
        .execute(&pool)
        .await
        .unwrap();

    let (app, jwt, _) = build_test_app(pool).await;
    let token = jwt.issue_registration_token(email, "Duplicate").unwrap();

    let req = json_request(
        "/api/auth/register/complete",
        serde_json::json!({
            "registration_token": token,
            "public_key": public_key,
            "pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&pre_key_bundle),
            "device_id": uuid::Uuid::now_v7().to_string(),
            "device_name": "Test"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 409);
}

#[sqlx::test]
async fn register_complete_stores_refresh_token_in_db(pool: sqlx::PgPool) {
    let (app, jwt, redis) = build_test_app(pool.clone()).await;
    let email = "rt_store@example.com";
    cleanup_redis_keys(&redis, &[&format!("rl:email:{email}")]).await;

    let token = jwt.issue_registration_token(email, "RT User").unwrap();
    let (public_key, pre_key_bundle) = generate_test_keypair();
    let device_id = uuid::Uuid::now_v7();

    let req = json_request(
        "/api/auth/register/complete",
        serde_json::json!({
            "registration_token": token,
            "public_key": public_key,
            "pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&pre_key_bundle),
            "device_id": device_id.to_string(),
            "device_name": "Test Device"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE device_id = $1")
        .bind(device_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
async fn register_complete_user_id_is_uuid_v7(pool: sqlx::PgPool) {
    let (app, jwt, redis) = build_test_app(pool).await;
    let email = "v7check@example.com";
    cleanup_redis_keys(&redis, &[&format!("rl:email:{email}")]).await;

    let token = jwt.issue_registration_token(email, "V7 User").unwrap();
    let (public_key, pre_key_bundle) = generate_test_keypair();

    let req = json_request(
        "/api/auth/register/complete",
        serde_json::json!({
            "registration_token": token,
            "public_key": public_key,
            "pre_key_bundle": base64::engine::general_purpose::STANDARD.encode(&pre_key_bundle),
            "device_id": uuid::Uuid::now_v7().to_string(),
            "device_name": "Test"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;

    let user_id: uuid::Uuid = json["user_id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        user_id.get_version(),
        Some(uuid::Version::SortRand),
        "user_id should be UUID v7"
    );
}

// ---------------------------------------------------------------------------
// Router wiring tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn register_routes_do_not_require_auth(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool).await;
    cleanup_redis_keys(&redis, &["rl:email:noauth@example.com"]).await;

    let req = json_request(
        "/api/auth/register/start",
        serde_json::json!({
            "email": "noauth@example.com",
            "display_name": "No Auth"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    cleanup_redis_keys(
        &redis,
        &["verify:noauth@example.com", "rl:email:noauth@example.com"],
    )
    .await;
}

#[sqlx::test]
async fn auth_routes_mounted_at_api_auth(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    let req = json_request(
        "/register/start",
        serde_json::json!({
            "email": "test@example.com",
            "display_name": "Test"
        }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 404);
}
