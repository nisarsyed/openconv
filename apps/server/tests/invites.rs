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

/// Create a guild via the API and return the guild JSON.
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

/// Create an invite via the API and return the invite JSON.
async fn create_invite_via_api(
    app: &axum::Router,
    token: &str,
    guild_id: &str,
    body: serde_json::Value,
) -> serde_json::Value {
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/invites"),
        token,
        body,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    body_json(resp).await
}

/// Add a user as a basic member of a guild (for permission testing).
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

// ─── Create Invite ──────────────────────────────────────────

#[sqlx::test]
async fn create_invite_requires_manage_invites_permission(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_b, _, token_b) = seed_user(&pool, &jwt, "Member", "member@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Add user_b as member (no MANAGE_INVITES)
    add_member(&pool, user_b, guild_uuid).await;

    // Member without MANAGE_INVITES -> 403
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/invites"),
        &token_b,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Owner (has all permissions) -> 201
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/invites"),
        &token_owner,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn create_invite_returns_8_char_base62_code(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let invite = create_invite_via_api(
        &app,
        &token,
        guild_id,
        serde_json::json!({ "max_uses": 10 }),
    )
    .await;

    let code = invite["code"].as_str().unwrap();
    assert_eq!(code.len(), 8);
    assert!(code.chars().all(|c| c.is_ascii_alphanumeric()));
    assert_eq!(invite["guild_id"], guild_id);
    assert_eq!(invite["max_uses"], 10);
    assert_eq!(invite["use_count"], 0);
}

// ─── List Invites ───────────────────────────────────────────

#[sqlx::test]
async fn list_invites_returns_guild_scoped_invites(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild1 = create_guild_via_api(&app, &token, "Guild 1").await;
    let guild2 = create_guild_via_api(&app, &token, "Guild 2").await;
    let g1_id = guild1["id"].as_str().unwrap();
    let g2_id = guild2["id"].as_str().unwrap();

    // Create 3 invites for guild1
    for _ in 0..3 {
        create_invite_via_api(&app, &token, g1_id, serde_json::json!({})).await;
    }
    // Create 1 invite for guild2
    create_invite_via_api(&app, &token, g2_id, serde_json::json!({})).await;

    // List guild1's invites
    let req = authed_get(&format!("/api/guilds/{g1_id}/invites"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json.as_array().unwrap().len(), 3);

    // List guild2's invites
    let req = authed_get(&format!("/api/guilds/{g2_id}/invites"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    assert_eq!(json.as_array().unwrap().len(), 1);
}

// ─── Revoke Invite ──────────────────────────────────────────

#[sqlx::test]
async fn revoke_invite_removes_it(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let invite = create_invite_via_api(&app, &token, guild_id, serde_json::json!({})).await;
    let code = invite["code"].as_str().unwrap();

    // Delete the invite
    let req = authed_delete(
        &format!("/api/guilds/{guild_id}/invites/{code}"),
        &token,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify it's gone from the list
    let req = authed_get(&format!("/api/guilds/{guild_id}/invites"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    assert_eq!(json.as_array().unwrap().len(), 0);
}

// ─── Get Invite Info ────────────────────────────────────────

#[sqlx::test]
async fn get_invite_info_returns_guild_details(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (_, _, token_other) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Cool Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let invite = create_invite_via_api(&app, &token_owner, guild_id, serde_json::json!({})).await;
    let code = invite["code"].as_str().unwrap();

    // Any authenticated user can look up the invite
    let req = authed_get(&format!("/api/invites/{code}"), &token_other);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["guild_name"], "Cool Guild");
    assert_eq!(json["guild_id"], guild_id);
    assert_eq!(json["member_count"], 1);
    assert_eq!(json["inviter_display_name"], "Alice");
}

#[sqlx::test]
async fn get_invite_info_returns_404_for_nonexistent_code(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let req = authed_get("/api/invites/NOTACODE", &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── Accept Invite ──────────────────────────────────────────

#[sqlx::test]
async fn accept_invite_adds_user_to_guild(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_b, _, token_b) = seed_user(&pool, &jwt, "Joiner", "joiner@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    let invite = create_invite_via_api(&app, &token_owner, guild_id, serde_json::json!({})).await;
    let code = invite["code"].as_str().unwrap();

    // Accept the invite
    let req = authed_post(
        &format!("/api/invites/{code}/accept"),
        &token_b,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify user is now a member
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_members WHERE user_id = $1 AND guild_id = $2)",
    )
    .bind(user_b.0)
    .bind(guild_uuid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(is_member);

    // Verify user has the member role
    let role_type: String = sqlx::query_scalar(
        "SELECT r.role_type FROM guild_member_roles gmr \
         JOIN roles r ON r.id = gmr.role_id \
         WHERE gmr.user_id = $1 AND gmr.guild_id = $2",
    )
    .bind(user_b.0)
    .bind(guild_uuid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(role_type, "member");
}

#[sqlx::test]
async fn accept_invite_increments_use_count(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (_, _, token_b) = seed_user(&pool, &jwt, "Joiner", "joiner@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let invite = create_invite_via_api(&app, &token_owner, guild_id, serde_json::json!({})).await;
    let code = invite["code"].as_str().unwrap();

    // Accept
    let req = authed_post(
        &format!("/api/invites/{code}/accept"),
        &token_b,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify use_count incremented
    let use_count: i32 =
        sqlx::query_scalar("SELECT use_count FROM guild_invites WHERE code = $1")
            .bind(code)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(use_count, 1);
}

#[sqlx::test]
async fn accept_expired_invite_returns_400(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (_, _, token_b) = seed_user(&pool, &jwt, "Joiner", "joiner@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Insert expired invite directly
    sqlx::query(
        "INSERT INTO guild_invites (code, guild_id, inviter_id, expires_at) \
         VALUES ($1, $2, $3, NOW() - INTERVAL '1 hour')",
    )
    .bind("EXPIRED1")
    .bind(guild_uuid)
    .bind(
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT owner_id FROM guilds WHERE id = $1")
            .bind(guild_uuid)
            .fetch_one(&pool)
            .await
            .unwrap(),
    )
    .execute(&pool)
    .await
    .unwrap();

    let req = authed_post(
        "/api/invites/EXPIRED1/accept",
        &token_b,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn accept_maxed_out_invite_returns_400(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (_, _, token_b) = seed_user(&pool, &jwt, "User1", "user1@test.com").await;
    let (_, _, token_c) = seed_user(&pool, &jwt, "User2", "user2@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let invite = create_invite_via_api(
        &app,
        &token_owner,
        guild_id,
        serde_json::json!({ "max_uses": 1 }),
    )
    .await;
    let code = invite["code"].as_str().unwrap();

    // First accept succeeds
    let req = authed_post(
        &format!("/api/invites/{code}/accept"),
        &token_b,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Second accept fails (maxed out)
    let req = authed_post(
        &format!("/api/invites/{code}/accept"),
        &token_c,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn accept_invite_already_member_returns_409(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let invite = create_invite_via_api(&app, &token_owner, guild_id, serde_json::json!({})).await;
    let code = invite["code"].as_str().unwrap();

    // Owner is already a member, try to accept -> 409
    let req = authed_post(
        &format!("/api/invites/{code}/accept"),
        &token_owner,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[sqlx::test]
async fn accept_invite_for_soft_deleted_guild_returns_404(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (_, _, token_b) = seed_user(&pool, &jwt, "Joiner", "joiner@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    let invite = create_invite_via_api(&app, &token_owner, guild_id, serde_json::json!({})).await;
    let code = invite["code"].as_str().unwrap();

    // Soft-delete the guild
    sqlx::query("UPDATE guilds SET deleted_at = NOW() WHERE id = $1")
        .bind(guild_uuid)
        .execute(&pool)
        .await
        .unwrap();

    let req = authed_post(
        &format!("/api/invites/{code}/accept"),
        &token_b,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn accept_nonexistent_invite_returns_404(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let req = authed_post(
        "/api/invites/NOTREAL1/accept",
        &token,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── Cascade Delete ─────────────────────────────────────────

#[sqlx::test]
async fn guild_hard_delete_cascades_to_invites(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    create_invite_via_api(&app, &token, guild_id, serde_json::json!({})).await;

    // Verify invite exists
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM guild_invites WHERE guild_id = $1")
            .bind(guild_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);

    // Hard-delete the guild (bypassing soft-delete for this test)
    sqlx::query("DELETE FROM guilds WHERE id = $1")
        .bind(guild_uuid)
        .execute(&pool)
        .await
        .unwrap();

    // Verify invites are gone (ON DELETE CASCADE)
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM guild_invites WHERE guild_id = $1")
            .bind(guild_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

// ─── Concurrent Acceptance ──────────────────────────────────

#[sqlx::test]
async fn concurrent_accepts_cannot_exceed_max_uses(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let invite = create_invite_via_api(
        &app,
        &token_owner,
        guild_id,
        serde_json::json!({ "max_uses": 1 }),
    )
    .await;
    let code = invite["code"].as_str().unwrap().to_string();

    // Create 10 different users
    let mut tokens = Vec::new();
    for i in 0..10 {
        let (_, _, token) = seed_user(
            &pool,
            &jwt,
            &format!("User{i}"),
            &format!("user{i}@test.com"),
        )
        .await;
        tokens.push(token);
    }

    // Spawn concurrent accept requests
    let mut handles = Vec::new();
    for token in tokens {
        let app = app.clone();
        let code = code.clone();
        handles.push(tokio::spawn(async move {
            let req = authed_post(
                &format!("/api/invites/{code}/accept"),
                &token,
                serde_json::json!({}),
            );
            app.oneshot(req).await.unwrap().status()
        }));
    }

    let mut success_count = 0;
    for handle in handles {
        if handle.await.unwrap() == StatusCode::OK {
            success_count += 1;
        }
    }

    assert_eq!(success_count, 1, "exactly 1 concurrent accept should succeed");

    // Verify use_count == 1 in database
    let use_count: i32 =
        sqlx::query_scalar("SELECT use_count FROM guild_invites WHERE code = $1")
            .bind(&code)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(use_count, 1);
}
