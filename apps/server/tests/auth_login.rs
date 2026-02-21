use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use base64::Engine;
use chrono::Datelike;
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

    // Clear IP-based rate limit counter for our test IP
    cleanup_redis_keys(&redis, &["rl:ip:10.99.0.2:auth"]).await;

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

fn json_request(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("X-Forwarded-For", "10.99.0.2")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Generate a libsignal identity keypair and return (public_key_b64, IdentityKeyPair).
fn generate_identity() -> (String, libsignal_protocol::IdentityKeyPair) {
    let identity = libsignal_protocol::IdentityKeyPair::generate(&mut rand::rng());
    let public_key_b64 =
        base64::engine::general_purpose::STANDARD.encode(identity.public_key().serialize());
    (public_key_b64, identity)
}

/// Seed a user directly in the database with a known identity keypair.
async fn seed_test_user(
    pool: &sqlx::PgPool,
) -> (
    openconv_shared::ids::UserId,
    String,
    libsignal_protocol::IdentityKeyPair,
) {
    let (public_key_b64, identity) = generate_identity();
    let user_id = openconv_shared::ids::UserId::new();

    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id.0)
        .bind(&public_key_b64)
        .bind(format!("{}@example.com", uuid::Uuid::new_v4()))
        .bind("Test User")
        .execute(pool)
        .await
        .unwrap();

    (user_id, public_key_b64, identity)
}

/// Sign a challenge using the identity keypair's private key.
fn sign_challenge(
    identity: &libsignal_protocol::IdentityKeyPair,
    challenge_bytes: &[u8],
) -> String {
    let signature = identity
        .private_key()
        .calculate_signature(challenge_bytes, &mut rand::rng())
        .unwrap();
    base64::engine::general_purpose::STANDARD.encode(&signature)
}

// ---------------------------------------------------------------------------
// POST /api/auth/challenge tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn challenge_returns_base64_encoded_challenge(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, public_key_b64, _) = seed_test_user(&pool).await;
    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{public_key_b64}"),
            &format!("rl:pk:{public_key_b64}:challenge"),
        ],
    )
    .await;

    let req = json_request(
        "/api/auth/challenge",
        serde_json::json!({ "public_key": public_key_b64 }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;

    let challenge = json["challenge"].as_str().unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(challenge)
        .unwrap();
    assert_eq!(decoded.len(), 32);

    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{public_key_b64}"),
            &format!("rl:pk:{public_key_b64}:challenge"),
        ],
    )
    .await;
}

#[sqlx::test]
async fn challenge_nonexistent_key_still_returns_200(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool).await;
    let (fake_key_b64, _) = generate_identity();
    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{fake_key_b64}"),
            &format!("rl:pk:{fake_key_b64}:challenge"),
        ],
    )
    .await;

    let req = json_request(
        "/api/auth/challenge",
        serde_json::json!({ "public_key": fake_key_b64 }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;
    assert!(json["challenge"].as_str().is_some());

    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{fake_key_b64}"),
            &format!("rl:pk:{fake_key_b64}:challenge"),
        ],
    )
    .await;
}

#[sqlx::test]
async fn challenge_stores_exists_true_for_known_user(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, public_key_b64, _) = seed_test_user(&pool).await;
    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{public_key_b64}"),
            &format!("rl:pk:{public_key_b64}:challenge"),
        ],
    )
    .await;

    let req = json_request(
        "/api/auth/challenge",
        serde_json::json!({ "public_key": public_key_b64 }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    use fred::interfaces::KeysInterface;
    let stored: String = redis
        .get(&format!("challenge:{public_key_b64}"))
        .await
        .unwrap();
    let data: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(data["exists"], true);

    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{public_key_b64}"),
            &format!("rl:pk:{public_key_b64}:challenge"),
        ],
    )
    .await;
}

#[sqlx::test]
async fn challenge_stores_exists_false_for_unknown_user(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool).await;
    let (fake_key_b64, _) = generate_identity();
    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{fake_key_b64}"),
            &format!("rl:pk:{fake_key_b64}:challenge"),
        ],
    )
    .await;

    let req = json_request(
        "/api/auth/challenge",
        serde_json::json!({ "public_key": fake_key_b64 }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    use fred::interfaces::KeysInterface;
    let stored: String = redis
        .get(&format!("challenge:{fake_key_b64}"))
        .await
        .unwrap();
    let data: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(data["exists"], false);

    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{fake_key_b64}"),
            &format!("rl:pk:{fake_key_b64}:challenge"),
        ],
    )
    .await;
}

#[sqlx::test]
async fn challenge_has_60s_ttl_in_redis(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, public_key_b64, _) = seed_test_user(&pool).await;
    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{public_key_b64}"),
            &format!("rl:pk:{public_key_b64}:challenge"),
        ],
    )
    .await;

    let req = json_request(
        "/api/auth/challenge",
        serde_json::json!({ "public_key": public_key_b64 }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    use fred::interfaces::KeysInterface;
    let ttl: i64 = redis
        .ttl(&format!("challenge:{public_key_b64}"))
        .await
        .unwrap();
    assert!(
        ttl > 0 && ttl <= 60,
        "TTL should be between 1 and 60, got {ttl}"
    );

    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{public_key_b64}"),
            &format!("rl:pk:{public_key_b64}:challenge"),
        ],
    )
    .await;
}

// ---------------------------------------------------------------------------
// POST /api/auth/verify tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn verify_valid_signature_returns_tokens(pool: sqlx::PgPool) {
    let (app, jwt, redis) = build_test_app(pool.clone()).await;
    let (user_id, public_key_b64, identity) = seed_test_user(&pool).await;

    // Seed a challenge in Redis with exists=true
    let challenge_bytes: [u8; 32] = rand::Rng::random(&mut rand::rng());
    let challenge_b64 = base64::engine::general_purpose::STANDARD.encode(challenge_bytes);
    let stored = serde_json::json!({
        "challenge": challenge_b64,
        "exists": true,
    });
    let key = format!("challenge:{public_key_b64}");
    cleanup_redis_keys(
        &redis,
        &[&key, &format!("rl:pk:{public_key_b64}:challenge")],
    )
    .await;
    {
        use fred::interfaces::KeysInterface;
        redis
            .set::<(), _, _>(
                &key,
                serde_json::to_string(&stored).unwrap().as_str(),
                Some(fred::types::Expiration::EX(60)),
                None,
                false,
            )
            .await
            .unwrap();
    }

    let signature_b64 = sign_challenge(&identity, &challenge_bytes);
    let device_id = uuid::Uuid::now_v7();

    let req = json_request(
        "/api/auth/verify",
        serde_json::json!({
            "public_key": public_key_b64,
            "signature": signature_b64,
            "device_id": device_id.to_string(),
            "device_name": "Test Device"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;

    assert!(json["access_token"].as_str().is_some());
    assert!(json["refresh_token"].as_str().is_some());
    assert_eq!(json["user_id"].as_str().unwrap(), user_id.0.to_string());
    assert_eq!(json["device_id"].as_str().unwrap(), device_id.to_string());

    let access_claims = jwt
        .validate_access_token(json["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(access_claims.purpose, "access");
    assert_eq!(access_claims.sub, user_id.0.to_string());

    cleanup_redis_keys(&redis, &[&format!("rl:pk:{public_key_b64}:challenge")]).await;
}

#[sqlx::test]
async fn verify_invalid_signature_returns_401(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, public_key_b64, _) = seed_test_user(&pool).await;

    let challenge_bytes: [u8; 32] = rand::Rng::random(&mut rand::rng());
    let challenge_b64 = base64::engine::general_purpose::STANDARD.encode(challenge_bytes);
    let stored = serde_json::json!({
        "challenge": challenge_b64,
        "exists": true,
    });
    let key = format!("challenge:{public_key_b64}");
    cleanup_redis_keys(&redis, &[&key]).await;
    {
        use fred::interfaces::KeysInterface;
        redis
            .set::<(), _, _>(
                &key,
                serde_json::to_string(&stored).unwrap().as_str(),
                Some(fred::types::Expiration::EX(60)),
                None,
                false,
            )
            .await
            .unwrap();
    }

    let garbage_sig = base64::engine::general_purpose::STANDARD.encode([0u8; 64]);

    let req = json_request(
        "/api/auth/verify",
        serde_json::json!({
            "public_key": public_key_b64,
            "signature": garbage_sig,
            "device_id": uuid::Uuid::now_v7().to_string(),
            "device_name": "Test Device"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

#[sqlx::test]
async fn verify_nonexistent_user_returns_401(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool).await;
    let (fake_key_b64, identity) = generate_identity();

    let challenge_bytes: [u8; 32] = rand::Rng::random(&mut rand::rng());
    let challenge_b64 = base64::engine::general_purpose::STANDARD.encode(challenge_bytes);
    let stored = serde_json::json!({
        "challenge": challenge_b64,
        "exists": false,
    });
    let key = format!("challenge:{fake_key_b64}");
    cleanup_redis_keys(&redis, &[&key]).await;
    {
        use fred::interfaces::KeysInterface;
        redis
            .set::<(), _, _>(
                &key,
                serde_json::to_string(&stored).unwrap().as_str(),
                Some(fred::types::Expiration::EX(60)),
                None,
                false,
            )
            .await
            .unwrap();
    }

    let signature_b64 = sign_challenge(&identity, &challenge_bytes);

    let req = json_request(
        "/api/auth/verify",
        serde_json::json!({
            "public_key": fake_key_b64,
            "signature": signature_b64,
            "device_id": uuid::Uuid::now_v7().to_string(),
            "device_name": "Test Device"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

#[sqlx::test]
async fn verify_atomically_deletes_challenge(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, public_key_b64, identity) = seed_test_user(&pool).await;

    let challenge_bytes: [u8; 32] = rand::Rng::random(&mut rand::rng());
    let challenge_b64 = base64::engine::general_purpose::STANDARD.encode(challenge_bytes);
    let stored = serde_json::json!({
        "challenge": challenge_b64,
        "exists": true,
    });
    let key = format!("challenge:{public_key_b64}");
    cleanup_redis_keys(&redis, &[&key]).await;
    {
        use fred::interfaces::KeysInterface;
        redis
            .set::<(), _, _>(
                &key,
                serde_json::to_string(&stored).unwrap().as_str(),
                Some(fred::types::Expiration::EX(60)),
                None,
                false,
            )
            .await
            .unwrap();
    }

    let signature_b64 = sign_challenge(&identity, &challenge_bytes);
    let device_id = uuid::Uuid::now_v7();

    // First verify succeeds
    let req = json_request(
        "/api/auth/verify",
        serde_json::json!({
            "public_key": public_key_b64,
            "signature": signature_b64,
            "device_id": device_id.to_string(),
            "device_name": "Test Device"
        }),
    );

    let (app2, _, _) = build_test_app(pool.clone()).await;
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    // Second verify with same challenge fails (challenge consumed)
    let req2 = json_request(
        "/api/auth/verify",
        serde_json::json!({
            "public_key": public_key_b64,
            "signature": signature_b64,
            "device_id": device_id.to_string(),
            "device_name": "Test Device"
        }),
    );

    let response2 = app2.oneshot(req2).await.unwrap();
    assert_eq!(response2.status(), 401);
}

#[sqlx::test]
async fn verify_expired_challenge_returns_401(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, public_key_b64, identity) = seed_test_user(&pool).await;

    // No challenge seeded — simulates expired/missing
    cleanup_redis_keys(&redis, &[&format!("challenge:{public_key_b64}")]).await;

    let challenge_bytes: [u8; 32] = rand::Rng::random(&mut rand::rng());
    let signature_b64 = sign_challenge(&identity, &challenge_bytes);

    let req = json_request(
        "/api/auth/verify",
        serde_json::json!({
            "public_key": public_key_b64,
            "signature": signature_b64,
            "device_id": uuid::Uuid::now_v7().to_string(),
            "device_name": "Test Device"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

#[sqlx::test]
async fn verify_creates_device_record(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (user_id, public_key_b64, identity) = seed_test_user(&pool).await;

    let challenge_bytes: [u8; 32] = rand::Rng::random(&mut rand::rng());
    let challenge_b64 = base64::engine::general_purpose::STANDARD.encode(challenge_bytes);
    let stored = serde_json::json!({
        "challenge": challenge_b64,
        "exists": true,
    });
    let key = format!("challenge:{public_key_b64}");
    cleanup_redis_keys(&redis, &[&key]).await;
    {
        use fred::interfaces::KeysInterface;
        redis
            .set::<(), _, _>(
                &key,
                serde_json::to_string(&stored).unwrap().as_str(),
                Some(fred::types::Expiration::EX(60)),
                None,
                false,
            )
            .await
            .unwrap();
    }

    let signature_b64 = sign_challenge(&identity, &challenge_bytes);
    let device_id = uuid::Uuid::now_v7();

    let req = json_request(
        "/api/auth/verify",
        serde_json::json!({
            "public_key": public_key_b64,
            "signature": signature_b64,
            "device_id": device_id.to_string(),
            "device_name": "Login Device"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let (dev_user_id, dev_name): (uuid::Uuid, String) =
        sqlx::query_as("SELECT user_id, device_name FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(dev_user_id, user_id.0);
    assert_eq!(dev_name, "Login Device");
}

#[sqlx::test]
async fn verify_upserts_existing_device(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (user_id, public_key_b64, identity) = seed_test_user(&pool).await;
    let device_id = uuid::Uuid::now_v7();

    // Create device first
    sqlx::query(
        "INSERT INTO devices (id, user_id, device_name, last_active, created_at) \
         VALUES ($1, $2, $3, '2020-01-01'::timestamptz, NOW())",
    )
    .bind(device_id)
    .bind(user_id.0)
    .bind("Old Name")
    .execute(&pool)
    .await
    .unwrap();

    let challenge_bytes: [u8; 32] = rand::Rng::random(&mut rand::rng());
    let challenge_b64 = base64::engine::general_purpose::STANDARD.encode(challenge_bytes);
    let stored = serde_json::json!({
        "challenge": challenge_b64,
        "exists": true,
    });
    let key = format!("challenge:{public_key_b64}");
    cleanup_redis_keys(&redis, &[&key]).await;
    {
        use fred::interfaces::KeysInterface;
        redis
            .set::<(), _, _>(
                &key,
                serde_json::to_string(&stored).unwrap().as_str(),
                Some(fred::types::Expiration::EX(60)),
                None,
                false,
            )
            .await
            .unwrap();
    }

    let signature_b64 = sign_challenge(&identity, &challenge_bytes);

    let req = json_request(
        "/api/auth/verify",
        serde_json::json!({
            "public_key": public_key_b64,
            "signature": signature_b64,
            "device_id": device_id.to_string(),
            "device_name": "New Name"
        }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let (dev_name, last_active): (String, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT device_name, last_active FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(dev_name, "New Name");
    assert!(last_active.year() >= 2025);
}

#[sqlx::test]
async fn verify_stores_refresh_token_in_db(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, public_key_b64, identity) = seed_test_user(&pool).await;

    let challenge_bytes: [u8; 32] = rand::Rng::random(&mut rand::rng());
    let challenge_b64 = base64::engine::general_purpose::STANDARD.encode(challenge_bytes);
    let stored = serde_json::json!({
        "challenge": challenge_b64,
        "exists": true,
    });
    let key = format!("challenge:{public_key_b64}");
    cleanup_redis_keys(&redis, &[&key]).await;
    {
        use fred::interfaces::KeysInterface;
        redis
            .set::<(), _, _>(
                &key,
                serde_json::to_string(&stored).unwrap().as_str(),
                Some(fred::types::Expiration::EX(60)),
                None,
                false,
            )
            .await
            .unwrap();
    }

    let signature_b64 = sign_challenge(&identity, &challenge_bytes);
    let device_id = uuid::Uuid::now_v7();

    let req = json_request(
        "/api/auth/verify",
        serde_json::json!({
            "public_key": public_key_b64,
            "signature": signature_b64,
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
async fn challenge_rate_limits_per_public_key(pool: sqlx::PgPool) {
    let (app, _, redis) = build_test_app(pool.clone()).await;
    let (_, public_key_b64, _) = seed_test_user(&pool).await;
    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{public_key_b64}"),
            &format!("rl:pk:{public_key_b64}:challenge"),
        ],
    )
    .await;

    // Default limit is 5 per key per minute — send 5 and expect 200
    for i in 0..5 {
        let req = json_request(
            "/api/auth/challenge",
            serde_json::json!({ "public_key": public_key_b64 }),
        );
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), 200, "request {i} should succeed");
    }

    // 6th request should be rate-limited
    let req = json_request(
        "/api/auth/challenge",
        serde_json::json!({ "public_key": public_key_b64 }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 429, "6th request should be rate-limited");

    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{public_key_b64}"),
            &format!("rl:pk:{public_key_b64}:challenge"),
        ],
    )
    .await;
}

#[sqlx::test]
async fn full_challenge_verify_flow(pool: sqlx::PgPool) {
    let (app, jwt, redis) = build_test_app(pool.clone()).await;
    let (user_id, public_key_b64, identity) = seed_test_user(&pool).await;
    cleanup_redis_keys(
        &redis,
        &[
            &format!("challenge:{public_key_b64}"),
            &format!("rl:pk:{public_key_b64}:challenge"),
        ],
    )
    .await;

    // Step 1: Request challenge
    let req = json_request(
        "/api/auth/challenge",
        serde_json::json!({ "public_key": public_key_b64 }),
    );

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;
    let challenge_b64 = json["challenge"].as_str().unwrap();
    let challenge_bytes = base64::engine::general_purpose::STANDARD
        .decode(challenge_b64)
        .unwrap();

    // Step 2: Sign challenge
    let signature_b64 = sign_challenge(&identity, &challenge_bytes);
    let device_id = uuid::Uuid::now_v7();

    // Step 3: Verify
    let (app2, _, _) = build_test_app(pool.clone()).await;
    let req = json_request(
        "/api/auth/verify",
        serde_json::json!({
            "public_key": public_key_b64,
            "signature": signature_b64,
            "device_id": device_id.to_string(),
            "device_name": "End-to-End Device"
        }),
    );

    let response = app2.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);
    let json = response_json(response).await;

    assert_eq!(json["user_id"].as_str().unwrap(), user_id.0.to_string());
    assert!(json["access_token"].as_str().is_some());
    assert!(json["refresh_token"].as_str().is_some());

    let access_claims = jwt
        .validate_access_token(json["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(access_claims.sub, user_id.0.to_string());
    assert_eq!(access_claims.device_id, device_id.to_string());

    cleanup_redis_keys(&redis, &[&format!("rl:pk:{public_key_b64}:challenge")]).await;
}
