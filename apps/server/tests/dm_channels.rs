use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
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

async fn build_test_app(pool: sqlx::PgPool) -> (axum::Router, Arc<JwtService>) {
    let config = ServerConfig::default();
    let redis = create_redis_pool(&config.redis).await.unwrap();
    let jwt = test_jwt();
    let state = AppState {
        db: pool,
        config: Arc::new(config),
        redis,
        jwt: jwt.clone(),
        email: Arc::new(MockEmailService::new()),
        object_store: Arc::new(object_store::memory::InMemory::new()),
    };
    (build_router(state), jwt)
}

async fn seed_user(
    pool: &sqlx::PgPool,
    jwt: &JwtService,
    display_name: &str,
    email: &str,
) -> (
    openconv_shared::ids::UserId,
    openconv_shared::ids::DeviceId,
    String,
) {
    let user_id = openconv_shared::ids::UserId::new();
    let device_id = openconv_shared::ids::DeviceId::new();

    sqlx::query("INSERT INTO users (id, public_key, email, display_name) VALUES ($1, $2, $3, $4)")
        .bind(user_id.0)
        .bind(format!("pk_{}", uuid::Uuid::new_v4()))
        .bind(email)
        .bind(display_name)
        .execute(pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO devices (id, user_id, device_name, last_active, created_at) VALUES ($1, $2, $3, NOW(), NOW())",
    )
    .bind(device_id.0)
    .bind(user_id.0)
    .bind("Test Device")
    .execute(pool)
    .await
    .unwrap();

    let access_token = jwt.issue_access_token(&user_id, &device_id).unwrap();
    (user_id, device_id, access_token)
}

fn authed_post(uri: &str, token: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Forwarded-For", "10.99.0.1")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

fn authed_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Forwarded-For", "10.99.0.1")
        .body(Body::empty())
        .unwrap()
}

fn authed_delete(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Forwarded-For", "10.99.0.1")
        .body(Body::empty())
        .unwrap()
}

async fn body_json(response: axum::http::Response<Body>) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

// ─── 1:1 DM Creation ───────────────────────────────────────

#[sqlx::test]
async fn create_one_to_one_dm(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (user_a, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (user_b, _, _token_b) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;

    let req = authed_post(
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [user_b.0] }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let json = body_json(resp).await;
    assert!(json["id"].is_string());
    assert!(json["name"].is_null());
    assert_eq!(json["is_group"], false);
    let members = json["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    // Both users should be in the member list
    let member_strs: Vec<&str> = members.iter().map(|m| m.as_str().unwrap()).collect();
    assert!(member_strs.contains(&user_a.0.to_string().as_str()));
    assert!(member_strs.contains(&user_b.0.to_string().as_str()));
}

#[sqlx::test]
async fn create_one_to_one_dm_deduplicates(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (user_b, _, token_b) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;

    // Alice creates DM with Bob
    let req = authed_post(
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [user_b.0] }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let first = body_json(resp).await;
    let first_id = first["id"].as_str().unwrap();

    // Alice creates DM with Bob again -> should return existing (200, not 201)
    let req = authed_post(
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [user_b.0] }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let second = body_json(resp).await;
    assert_eq!(second["id"], first_id);

    // Bob creates DM with Alice -> should also return existing
    let alice_id = first["creator_id"].as_str().unwrap();
    let req = authed_post(
        "/api/dm-channels",
        &token_b,
        serde_json::json!({ "user_ids": [alice_id] }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let third = body_json(resp).await;
    assert_eq!(third["id"], first_id);
}

// ─── Group DM Creation ─────────────────────────────────────

#[sqlx::test]
async fn create_group_dm(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (user_b, _, _) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;
    let (user_c, _, _) = seed_user(&pool, &jwt, "Charlie", "charlie@test.com").await;

    let req = authed_post(
        "/api/dm-channels",
        &token_a,
        serde_json::json!({
            "user_ids": [user_b.0, user_c.0],
            "name": "Project Chat"
        }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let json = body_json(resp).await;
    assert_eq!(json["name"], "Project Chat");
    assert_eq!(json["is_group"], true);
    assert_eq!(json["members"].as_array().unwrap().len(), 3);
}

#[sqlx::test]
async fn group_dm_not_deduplicated(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (user_b, _, _) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;
    let (user_c, _, _) = seed_user(&pool, &jwt, "Charlie", "charlie@test.com").await;

    // Create group DM twice with same participants
    let body = serde_json::json!({ "user_ids": [user_b.0, user_c.0] });

    let req = authed_post("/api/dm-channels", &token_a, body.clone());
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let first = body_json(resp).await;

    let req = authed_post("/api/dm-channels", &token_a, body);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let second = body_json(resp).await;

    // Different channel IDs
    assert_ne!(first["id"], second["id"]);
}

#[sqlx::test]
async fn group_dm_validates_participant_count(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    // Empty user_ids -> 400
    let req = authed_post(
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [] }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── List DM Channels ──────────────────────────────────────

#[sqlx::test]
async fn list_dm_channels(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (user_b, _, _) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;
    let (user_c, _, _) = seed_user(&pool, &jwt, "Charlie", "charlie@test.com").await;

    // Create two DMs
    authed_post_expect(
        &app,
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [user_b.0] }),
        StatusCode::CREATED,
    )
    .await;
    authed_post_expect(
        &app,
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [user_c.0] }),
        StatusCode::CREATED,
    )
    .await;

    let req = authed_get("/api/dm-channels", &token_a);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json.as_array().unwrap().len(), 2);
}

// ─── Add Member ─────────────────────────────────────────────

#[sqlx::test]
async fn add_member_to_group_dm(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (user_b, _, _) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;
    let (user_c, _, _) = seed_user(&pool, &jwt, "Charlie", "charlie@test.com").await;
    let (user_d, _, _) = seed_user(&pool, &jwt, "Dave", "dave@test.com").await;

    // Create group DM
    let req = authed_post(
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [user_b.0, user_c.0], "name": "Team" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let dm = body_json(resp).await;
    let dm_id = dm["id"].as_str().unwrap();

    // Add Dave
    let req = authed_post(
        &format!("/api/dm-channels/{dm_id}/members"),
        &token_a,
        serde_json::json!({ "user_id": user_d.0 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let updated = body_json(resp).await;
    assert_eq!(updated["members"].as_array().unwrap().len(), 4);
}

#[sqlx::test]
async fn cannot_add_member_to_1_1_dm(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (user_b, _, _) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;
    let (user_c, _, _) = seed_user(&pool, &jwt, "Charlie", "charlie@test.com").await;

    // Create 1:1 DM
    let req = authed_post(
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [user_b.0] }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let dm = body_json(resp).await;
    let dm_id = dm["id"].as_str().unwrap();

    // Try to add Charlie -> 400
    let req = authed_post(
        &format!("/api/dm-channels/{dm_id}/members"),
        &token_a,
        serde_json::json!({ "user_id": user_c.0 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Leave DM ───────────────────────────────────────────────

#[sqlx::test]
async fn cannot_leave_1_1_dm(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (user_b, _, _) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;

    let req = authed_post(
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [user_b.0] }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let dm = body_json(resp).await;
    let dm_id = dm["id"].as_str().unwrap();

    let req = authed_delete(&format!("/api/dm-channels/{dm_id}/members/me"), &token_a);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn leave_group_dm(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (user_b, _, _) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;
    let (user_c, _, _) = seed_user(&pool, &jwt, "Charlie", "charlie@test.com").await;

    let req = authed_post(
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [user_b.0, user_c.0], "name": "Group" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let dm = body_json(resp).await;
    let dm_id = dm["id"].as_str().unwrap();

    // Leave the group
    let req = authed_delete(&format!("/api/dm-channels/{dm_id}/members/me"), &token_a);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify no longer listed for Alice
    let req = authed_get("/api/dm-channels", &token_a);
    let resp = app.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    let dm_ids: Vec<&str> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert!(!dm_ids.contains(&dm_id));
}

// ─── Non-member Access ──────────────────────────────────────

#[sqlx::test]
async fn non_member_cannot_access_dm(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (user_b, _, _) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;
    let (_, _, token_c) = seed_user(&pool, &jwt, "Charlie", "charlie@test.com").await;

    let req = authed_post(
        "/api/dm-channels",
        &token_a,
        serde_json::json!({ "user_ids": [user_b.0] }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let dm = body_json(resp).await;
    let dm_id = dm["id"].as_str().unwrap();

    // Charlie tries to access -> 403
    let req = authed_get(&format!("/api/dm-channels/{dm_id}"), &token_c);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ─── Helper ─────────────────────────────────────────────────

async fn authed_post_expect(
    app: &axum::Router,
    uri: &str,
    token: &str,
    body: serde_json::Value,
    expected: StatusCode,
) -> serde_json::Value {
    let req = authed_post(uri, token, body);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), expected);
    body_json(resp).await
}
