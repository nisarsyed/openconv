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

fn authed_patch(uri: &str, token: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PATCH")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Forwarded-For", "10.99.0.1")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
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

/// Create a guild via the API and return the response JSON.
async fn create_guild_via_api(
    app: &axum::Router,
    token: &str,
    name: &str,
) -> serde_json::Value {
    let req = authed_post("/api/guilds", token, serde_json::json!({ "name": name }));
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    body_json(resp).await
}

/// Add a user as a guild member with the "member" role.
async fn add_member(pool: &sqlx::PgPool, user_id: openconv_shared::ids::UserId, guild_uuid: uuid::Uuid) {
    sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
        .bind(user_id.0)
        .bind(guild_uuid)
        .execute(pool)
        .await
        .unwrap();

    let member_role_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM roles WHERE guild_id = $1 AND role_type = 'member'",
    )
    .bind(guild_uuid)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO guild_member_roles (user_id, guild_id, role_id) VALUES ($1, $2, $3)")
        .bind(user_id.0)
        .bind(guild_uuid)
        .bind(member_role_id)
        .execute(pool)
        .await
        .unwrap();
}

/// Create a channel via the API and return the response JSON.
async fn create_channel_via_api(
    app: &axum::Router,
    token: &str,
    guild_id: &str,
    name: &str,
) -> serde_json::Value {
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/channels"),
        token,
        serde_json::json!({ "name": name, "channel_type": "text" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    body_json(resp).await
}

// ─── Channel Creation ──────────────────────────────────────

#[sqlx::test]
async fn create_channel_requires_manage_channels(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_b, _, token_b) = seed_user(&pool, &jwt, "Member", "member@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Add user_b as basic member (no MANAGE_CHANNELS)
    add_member(&pool, user_b, guild_uuid).await;

    // Member tries to create channel -> 403
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/channels"),
        &token_b,
        serde_json::json!({ "name": "dev-chat", "channel_type": "text" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn create_channel_with_correct_guild_and_position(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    // Guild already has "main" at position 0, new channel should be at position 1
    let channel = create_channel_via_api(&app, &token, guild_id, "dev-chat").await;

    assert_eq!(channel["name"], "dev-chat");
    assert_eq!(channel["guild_id"], guild_id);
    assert_eq!(channel["position"], 1);
}

// ─── Channel Listing ───────────────────────────────────────

#[sqlx::test]
async fn list_channels_returns_ordered_by_position(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    // Create additional channels (guild already has "main" at position 0)
    create_channel_via_api(&app, &token, guild_id, "dev").await;
    create_channel_via_api(&app, &token, guild_id, "design").await;

    let req = authed_get(&format!("/api/guilds/{guild_id}/channels"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let channels = json.as_array().unwrap();
    assert_eq!(channels.len(), 3);

    // Verify order by position
    assert_eq!(channels[0]["position"], 0);
    assert_eq!(channels[1]["position"], 1);
    assert_eq!(channels[2]["position"], 2);
}

// ─── Get Channel ───────────────────────────────────────────

#[sqlx::test]
async fn get_channel_returns_details(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let channel = create_channel_via_api(&app, &token, guild_id, "dev-chat").await;
    let channel_id = channel["id"].as_str().unwrap();

    let req = authed_get(&format!("/api/channels/{channel_id}"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["name"], "dev-chat");
    assert_eq!(json["guild_id"], guild_id);
}

#[sqlx::test]
async fn get_channel_returns_403_for_non_member(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (_, _, token_b) = seed_user(&pool, &jwt, "Outsider", "outsider@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let channel = create_channel_via_api(&app, &token_owner, guild_id, "private").await;
    let channel_id = channel["id"].as_str().unwrap();

    // Non-member tries to get channel -> 403
    let req = authed_get(&format!("/api/channels/{channel_id}"), &token_b);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ─── Update Channel ────────────────────────────────────────

#[sqlx::test]
async fn update_channel_name_and_topic(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let channel = create_channel_via_api(&app, &token, guild_id, "dev-chat").await;
    let channel_id = channel["id"].as_str().unwrap();

    let req = authed_patch(
        &format!("/api/channels/{channel_id}"),
        &token,
        serde_json::json!({ "name": "announcements", "topic": "Guild announcements" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["name"], "announcements");
    assert_eq!(json["topic"], "Guild announcements");
}

// ─── Delete Channel ────────────────────────────────────────

#[sqlx::test]
async fn delete_channel_succeeds_with_multiple_channels(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    // Guild has "main" plus we add one more
    let channel = create_channel_via_api(&app, &token, guild_id, "temp").await;
    let channel_id = channel["id"].as_str().unwrap();

    let req = authed_delete(&format!("/api/channels/{channel_id}"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify channel is gone
    let req = authed_get(&format!("/api/guilds/{guild_id}/channels"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    assert_eq!(json.as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn cannot_delete_last_channel(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Get the default "main" channel id
    let main_channel_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM channels WHERE guild_id = $1 AND name = 'main'")
            .bind(guild_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();

    let req = authed_delete(
        &format!("/api/channels/{main_channel_id}"),
        &token,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Channel Reorder ───────────────────────────────────────

#[sqlx::test]
async fn reorder_channels_updates_positions(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Create 2 more channels (main already exists at pos 0)
    let ch_b = create_channel_via_api(&app, &token, guild_id, "beta").await;
    let ch_c = create_channel_via_api(&app, &token, guild_id, "gamma").await;

    // Get main channel ID
    let main_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM channels WHERE guild_id = $1 AND name = 'main'")
            .bind(guild_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();

    let main_id_str = main_id.to_string();
    let beta_id = ch_b["id"].as_str().unwrap();
    let gamma_id = ch_c["id"].as_str().unwrap();

    // Reorder: main->2, beta->0, gamma->1
    let req = authed_patch(
        &format!("/api/guilds/{guild_id}/channels/reorder"),
        &token,
        serde_json::json!({
            "channels": [
                { "channel_id": main_id_str, "position": 2 },
                { "channel_id": beta_id, "position": 0 },
                { "channel_id": gamma_id, "position": 1 }
            ]
        }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify new ordering
    let req = authed_get(&format!("/api/guilds/{guild_id}/channels"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    let channels = json.as_array().unwrap();

    assert_eq!(channels[0]["name"], "beta");
    assert_eq!(channels[0]["position"], 0);
    assert_eq!(channels[1]["name"], "gamma");
    assert_eq!(channels[1]["position"], 1);
    assert_eq!(channels[2]["name"], "main");
    assert_eq!(channels[2]["position"], 2);
}

#[sqlx::test]
async fn reorder_rejects_foreign_channel_ids(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild_a = create_guild_via_api(&app, &token, "Guild A").await;
    let guild_a_id = guild_a["id"].as_str().unwrap();

    let guild_b = create_guild_via_api(&app, &token, "Guild B").await;
    let guild_b_id = guild_b["id"].as_str().unwrap();
    let guild_b_uuid: uuid::Uuid = guild_b_id.parse().unwrap();

    // Get a channel from guild B
    let foreign_channel_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM channels WHERE guild_id = $1 LIMIT 1")
            .bind(guild_b_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();

    // Try to reorder guild A's channels with a foreign channel ID
    let req = authed_patch(
        &format!("/api/guilds/{guild_a_id}/channels/reorder"),
        &token,
        serde_json::json!({
            "channels": [
                { "channel_id": foreign_channel_id.to_string(), "position": 0 }
            ]
        }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Channel Name Validation (via API) ─────────────────────

#[sqlx::test]
async fn duplicate_channel_name_returns_409(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    // "main" already exists from guild creation
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/channels"),
        &token,
        serde_json::json!({ "name": "main", "channel_type": "text" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[sqlx::test]
async fn invalid_channel_name_returns_400(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    // Uppercase should fail
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/channels"),
        &token,
        serde_json::json!({ "name": "General", "channel_type": "text" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
