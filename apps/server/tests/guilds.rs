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

/// Create a guild via the API and return the guild ID string.
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

// ─── Guild Creation ─────────────────────────────────────────

#[sqlx::test]
async fn create_guild_returns_201_with_guild_details(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;

    assert_eq!(guild["name"], "Test Guild");
    assert!(guild["id"].is_string());
    assert_eq!(guild["member_count"], 1);
}

#[sqlx::test]
async fn create_guild_creates_default_roles(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id: uuid::Uuid = guild["id"].as_str().unwrap().parse().unwrap();

    // Check that 3 default roles were created
    let role_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM roles WHERE guild_id = $1")
            .bind(guild_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(role_count, 3);

    // Check role types
    let role_types: Vec<String> = sqlx::query_scalar(
        "SELECT role_type FROM roles WHERE guild_id = $1 ORDER BY position DESC",
    )
    .bind(guild_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(role_types, vec!["owner", "admin", "member"]);
}

#[sqlx::test]
async fn create_guild_creates_default_channel(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id: uuid::Uuid = guild["id"].as_str().unwrap().parse().unwrap();

    let channel_name: String =
        sqlx::query_scalar("SELECT name FROM channels WHERE guild_id = $1")
            .bind(guild_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(channel_name, "main");
}

#[sqlx::test]
async fn create_guild_assigns_owner_role_to_creator(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (user_id, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id: uuid::Uuid = guild["id"].as_str().unwrap().parse().unwrap();

    // Check creator is in guild_members
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_members WHERE user_id = $1 AND guild_id = $2)",
    )
    .bind(user_id.0)
    .bind(guild_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(is_member);

    // Check creator has the owner role
    let role_type: String = sqlx::query_scalar(
        "SELECT r.role_type FROM guild_member_roles gmr \
         JOIN roles r ON r.id = gmr.role_id \
         WHERE gmr.user_id = $1 AND gmr.guild_id = $2",
    )
    .bind(user_id.0)
    .bind(guild_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(role_type, "owner");
}

// ─── Guild Listing ──────────────────────────────────────────

#[sqlx::test]
async fn list_guilds_returns_only_member_guilds(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (_, _, token_b) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;

    // Alice creates 2 guilds, Bob creates 1
    create_guild_via_api(&app, &token_a, "Alice Guild 1").await;
    create_guild_via_api(&app, &token_a, "Alice Guild 2").await;
    create_guild_via_api(&app, &token_b, "Bob Guild").await;

    // Alice should see 2 guilds
    let req = authed_get("/api/guilds", &token_a);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["guilds"].as_array().unwrap().len(), 2);

    // Bob should see 1 guild
    let req = authed_get("/api/guilds", &token_b);
    let resp = app.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["guilds"].as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn list_guilds_excludes_soft_deleted(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "To Delete").await;
    let guild_id: uuid::Uuid = guild["id"].as_str().unwrap().parse().unwrap();

    // Soft-delete directly in DB
    sqlx::query("UPDATE guilds SET deleted_at = NOW() WHERE id = $1")
        .bind(guild_id)
        .execute(&pool)
        .await
        .unwrap();

    let req = authed_get("/api/guilds", &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    assert_eq!(json["guilds"].as_array().unwrap().len(), 0);
}

// ─── Get Guild ──────────────────────────────────────────────

#[sqlx::test]
async fn get_guild_returns_details_with_member_count(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "My Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let req = authed_get(&format!("/api/guilds/{guild_id}"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["name"], "My Guild");
    assert_eq!(json["member_count"], 1);
}

#[sqlx::test]
async fn get_guild_returns_403_for_non_members(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_a) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;
    let (_, _, token_b) = seed_user(&pool, &jwt, "Bob", "bob@test.com").await;

    let guild = create_guild_via_api(&app, &token_a, "Alice Only").await;
    let guild_id = guild["id"].as_str().unwrap();

    let req = authed_get(&format!("/api/guilds/{guild_id}"), &token_b);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ─── Update Guild ───────────────────────────────────────────

#[sqlx::test]
async fn update_guild_requires_manage_guild_permission(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_b, _, token_b) = seed_user(&pool, &jwt, "Member", "member@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "My Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Add user_b as a basic member (no MANAGE_GUILD)
    sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
        .bind(user_b.0)
        .bind(guild_uuid)
        .execute(&pool)
        .await
        .unwrap();

    let member_role_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM roles WHERE guild_id = $1 AND role_type = 'member'",
    )
    .bind(guild_uuid)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO guild_member_roles (user_id, guild_id, role_id) VALUES ($1, $2, $3)")
        .bind(user_b.0)
        .bind(guild_uuid)
        .bind(member_role_id)
        .execute(&pool)
        .await
        .unwrap();

    // Member tries to update -> 403
    let req = authed_patch(
        &format!("/api/guilds/{guild_id}"),
        &token_b,
        serde_json::json!({ "name": "Hacked" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Owner updates -> 200
    let req = authed_patch(
        &format!("/api/guilds/{guild_id}"),
        &token_owner,
        serde_json::json!({ "name": "Renamed" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["name"], "Renamed");
}

// ─── Delete & Restore ───────────────────────────────────────

#[sqlx::test]
async fn delete_guild_requires_owner(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_b, _, token_b) = seed_user(&pool, &jwt, "Admin", "admin@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "My Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Add user_b with admin role
    sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
        .bind(user_b.0)
        .bind(guild_uuid)
        .execute(&pool)
        .await
        .unwrap();

    let admin_role_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM roles WHERE guild_id = $1 AND role_type = 'admin'",
    )
    .bind(guild_uuid)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO guild_member_roles (user_id, guild_id, role_id) VALUES ($1, $2, $3)")
        .bind(user_b.0)
        .bind(guild_uuid)
        .bind(admin_role_id)
        .execute(&pool)
        .await
        .unwrap();

    // Admin tries to delete -> should fail (not 204)
    let req = authed_delete(&format!("/api/guilds/{guild_id}"), &token_b);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::NO_CONTENT);

    // Owner deletes -> 204
    let req = authed_delete(&format!("/api/guilds/{guild_id}"), &token_owner);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[sqlx::test]
async fn delete_guild_sets_deleted_at(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "To Delete").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    let req = authed_delete(&format!("/api/guilds/{guild_id}"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify deleted_at is set
    let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM guilds WHERE id = $1")
            .bind(guild_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(deleted_at.is_some());
}

#[sqlx::test]
async fn restore_guild_within_7_day_window(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Restore Me").await;
    let guild_id = guild["id"].as_str().unwrap();

    // Soft-delete
    let req = authed_delete(&format!("/api/guilds/{guild_id}"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Restore
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/restore"),
        &token,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["name"], "Restore Me");
}

#[sqlx::test]
async fn restore_guild_fails_after_7_day_window(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Old Delete").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Set deleted_at to 8 days ago
    sqlx::query("UPDATE guilds SET deleted_at = NOW() - INTERVAL '8 days' WHERE id = $1")
        .bind(guild_uuid)
        .execute(&pool)
        .await
        .unwrap();

    let req = authed_post(
        &format!("/api/guilds/{guild_id}/restore"),
        &token,
        serde_json::json!({}),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Member Management ──────────────────────────────────────

#[sqlx::test]
async fn owner_cannot_leave_guild(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "My Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let req = authed_delete(&format!("/api/guilds/{guild_id}/members/me"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn member_can_leave_guild(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_b, _, token_b) = seed_user(&pool, &jwt, "Member", "member@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "My Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Add user_b as member
    sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
        .bind(user_b.0)
        .bind(guild_uuid)
        .execute(&pool)
        .await
        .unwrap();

    let member_role_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM roles WHERE guild_id = $1 AND role_type = 'member'",
    )
    .bind(guild_uuid)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO guild_member_roles (user_id, guild_id, role_id) VALUES ($1, $2, $3)")
        .bind(user_b.0)
        .bind(guild_uuid)
        .bind(member_role_id)
        .execute(&pool)
        .await
        .unwrap();

    // Member leaves
    let req = authed_delete(&format!("/api/guilds/{guild_id}/members/me"), &token_b);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify no longer a member
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_members WHERE user_id = $1 AND guild_id = $2)",
    )
    .bind(user_b.0)
    .bind(guild_uuid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!is_member);
}

#[sqlx::test]
async fn list_members_returns_members_with_roles(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "My Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let req = authed_get(&format!("/api/guilds/{guild_id}/members"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;

    let members = json.as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["display_name"], "Alice");
    assert!(!members[0]["roles"].as_array().unwrap().is_empty());
}

// ─── Guild Cleanup ──────────────────────────────────────────

#[sqlx::test]
async fn cleanup_deletes_expired_guilds(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Old Guild").await;
    let guild_uuid: uuid::Uuid = guild["id"].as_str().unwrap().parse().unwrap();

    // Set deleted_at to 8 days ago
    sqlx::query("UPDATE guilds SET deleted_at = NOW() - INTERVAL '8 days' WHERE id = $1")
        .bind(guild_uuid)
        .execute(&pool)
        .await
        .unwrap();

    let count = openconv_server::tasks::guild_cleanup::cleanup_expired_guilds(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // Verify guild is gone
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM guilds WHERE id = $1)")
            .bind(guild_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!exists);
}

#[sqlx::test]
async fn cleanup_does_not_delete_within_7_day_window(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Recent Delete").await;
    let guild_uuid: uuid::Uuid = guild["id"].as_str().unwrap().parse().unwrap();

    // Set deleted_at to 3 days ago (within window)
    sqlx::query("UPDATE guilds SET deleted_at = NOW() - INTERVAL '3 days' WHERE id = $1")
        .bind(guild_uuid)
        .execute(&pool)
        .await
        .unwrap();

    let count = openconv_server::tasks::guild_cleanup::cleanup_expired_guilds(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // Guild still exists
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM guilds WHERE id = $1)")
            .bind(guild_uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(exists);
}
