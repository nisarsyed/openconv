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

fn authed_put(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("PUT")
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

use openconv_shared::permissions::Permissions;

// ─── Role Creation ─────────────────────────────────────────

#[sqlx::test]
async fn create_custom_role(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let perms = (Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES).bits();
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token,
        serde_json::json!({ "name": "Moderator", "permissions": perms }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    assert_eq!(json["name"], "Moderator");
    assert_eq!(json["role_type"], "custom");
    assert_eq!(json["position"], 2);
}

#[sqlx::test]
async fn create_role_requires_manage_roles(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_b, _, token_b) = seed_user(&pool, &jwt, "Member", "member@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    add_member(&pool, user_b, guild_uuid).await;

    // Member without MANAGE_ROLES tries to create role -> 403
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token_b,
        serde_json::json!({ "name": "Hacker", "permissions": 0 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ─── Role Listing ──────────────────────────────────────────

#[sqlx::test]
async fn list_roles_returns_ordered_by_position(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    // Create 2 custom roles
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token,
        serde_json::json!({ "name": "Mod1", "permissions": 0 }),
    );
    app.clone().oneshot(req).await.unwrap();

    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token,
        serde_json::json!({ "name": "Mod2", "permissions": 0 }),
    );
    app.clone().oneshot(req).await.unwrap();

    let req = authed_get(&format!("/api/guilds/{guild_id}/roles"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let roles = json.as_array().unwrap();
    // 3 default + 2 custom = 5
    assert_eq!(roles.len(), 5);

    // Verify sorted by position
    let positions: Vec<i64> = roles.iter().map(|r| r["position"].as_i64().unwrap()).collect();
    let mut sorted = positions.clone();
    sorted.sort();
    assert_eq!(positions, sorted);
}

// ─── Role Update ───────────────────────────────────────────

#[sqlx::test]
async fn update_role_name_and_permissions(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    // Create custom role
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token,
        serde_json::json!({ "name": "OldName", "permissions": 0 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let role = body_json(resp).await;
    let role_id = role["id"].as_str().unwrap();

    let new_perms = Permissions::SEND_MESSAGES.bits();
    let req = authed_patch(
        &format!("/api/guilds/{guild_id}/roles/{role_id}"),
        &token,
        serde_json::json!({ "name": "NewName", "permissions": new_perms }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["name"], "NewName");
    assert_eq!(json["permissions"], new_perms);
}

// ─── Role Deletion ─────────────────────────────────────────

#[sqlx::test]
async fn delete_custom_role(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    // Create custom role
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token,
        serde_json::json!({ "name": "ToDelete", "permissions": 0 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let role = body_json(resp).await;
    let role_id = role["id"].as_str().unwrap();

    let req = authed_delete(&format!("/api/guilds/{guild_id}/roles/{role_id}"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[sqlx::test]
async fn cannot_delete_builtin_role(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Get the admin role ID
    let admin_role_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM roles WHERE guild_id = $1 AND role_type = 'admin'",
    )
    .bind(guild_uuid)
    .fetch_one(&pool)
    .await
    .unwrap();

    let req = authed_delete(
        &format!("/api/guilds/{guild_id}/roles/{admin_role_id}"),
        &token,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Privilege Escalation ──────────────────────────────────

#[sqlx::test]
async fn cannot_create_role_with_perms_actor_lacks(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_admin, _, token_admin) = seed_user(&pool, &jwt, "Admin", "admin@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Add user_admin with admin role (has MANAGE_ROLES but not ADMINISTRATOR)
    sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
        .bind(user_admin.0)
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
        .bind(user_admin.0)
        .bind(guild_uuid)
        .bind(admin_role_id)
        .execute(&pool)
        .await
        .unwrap();

    // Admin tries to create role with ADMINISTRATOR -> 403
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token_admin,
        serde_json::json!({ "name": "SuperRole", "permissions": Permissions::ADMINISTRATOR.bits() }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn owner_can_create_role_with_any_perms(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token,
        serde_json::json!({ "name": "GodRole", "permissions": Permissions::ADMINISTRATOR.bits() }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

// ─── Hierarchy Enforcement ─────────────────────────────────

#[sqlx::test]
async fn cannot_modify_role_at_or_above_actor_position(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token_owner) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_admin, _, token_admin) = seed_user(&pool, &jwt, "Admin", "admin@test.com").await;

    let guild = create_guild_via_api(&app, &token_owner, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    // Add user_admin with admin role
    sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
        .bind(user_admin.0)
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
        .bind(user_admin.0)
        .bind(guild_uuid)
        .bind(admin_role_id)
        .execute(&pool)
        .await
        .unwrap();

    // Owner creates a custom role (gets position 2 after shift)
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token_owner,
        serde_json::json!({ "name": "HighRole", "permissions": 0 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let high_role: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap(),
    )
    .unwrap();
    let high_role_id = high_role["id"].as_str().unwrap();

    // Move custom role to position 51 (above admin's 50) via owner
    let req = authed_patch(
        &format!("/api/guilds/{guild_id}/roles/{high_role_id}"),
        &token_owner,
        serde_json::json!({ "position": 51 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Admin tries to modify custom role at position 51 (above actor's 50) -> 403
    let req = authed_patch(
        &format!("/api/guilds/{guild_id}/roles/{high_role_id}"),
        &token_admin,
        serde_json::json!({ "name": "Hacked" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Also verify built-in roles are rejected as immutable (400, not hierarchy 403)
    let req = authed_patch(
        &format!("/api/guilds/{guild_id}/roles/{admin_role_id}"),
        &token_admin,
        serde_json::json!({ "name": "SelfModified" }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Role Assignment ───────────────────────────────────────

#[sqlx::test]
async fn assign_and_remove_role(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_b, _, _) = seed_user(&pool, &jwt, "Member", "member@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    add_member(&pool, user_b, guild_uuid).await;

    // Create a custom role
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token,
        serde_json::json!({ "name": "VIP", "permissions": 0 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let role = body_json(resp).await;
    let role_id = role["id"].as_str().unwrap();
    let user_b_id = user_b.0.to_string();

    // Assign role to member
    let req = authed_put(
        &format!("/api/guilds/{guild_id}/members/{user_b_id}/roles/{role_id}"),
        &token,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify assignment
    let has_role: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_member_roles WHERE user_id = $1 AND guild_id = $2 AND role_id = $3)",
    )
    .bind(user_b.0)
    .bind(guild_uuid)
    .bind(role_id.parse::<uuid::Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(has_role);

    // Remove role from member
    let req = authed_delete(
        &format!("/api/guilds/{guild_id}/members/{user_b_id}/roles/{role_id}"),
        &token,
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify removal
    let has_role: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_member_roles WHERE user_id = $1 AND guild_id = $2 AND role_id = $3)",
    )
    .bind(user_b.0)
    .bind(guild_uuid)
    .bind(role_id.parse::<uuid::Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!has_role);
}

// ─── Position Shift on Create ──────────────────────────────

#[sqlx::test]
async fn custom_roles_shift_positions_on_create(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();

    // Create first custom role -> position 2
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token,
        serde_json::json!({ "name": "First", "permissions": 0 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let first = body_json(resp).await;
    assert_eq!(first["position"], 2);

    // Create second custom role -> position 2, first shifts to 3
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token,
        serde_json::json!({ "name": "Second", "permissions": 0 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let second = body_json(resp).await;
    assert_eq!(second["position"], 2);

    // Verify listing: Second should be at 2, First at 3
    let req = authed_get(&format!("/api/guilds/{guild_id}/roles"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    let roles = json.as_array().unwrap();

    let second_role = roles.iter().find(|r| r["name"] == "Second").unwrap();
    let first_role = roles.iter().find(|r| r["name"] == "First").unwrap();
    assert_eq!(second_role["position"], 2);
    assert_eq!(first_role["position"], 3);
}

// ─── Role Deletion Cascades ────────────────────────────────

#[sqlx::test]
async fn role_deletion_cascades_to_member_roles(pool: sqlx::PgPool) {
    let (app, jwt) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Owner", "owner@test.com").await;
    let (user_b, _, _) = seed_user(&pool, &jwt, "Member", "member@test.com").await;

    let guild = create_guild_via_api(&app, &token, "Test Guild").await;
    let guild_id = guild["id"].as_str().unwrap();
    let guild_uuid: uuid::Uuid = guild_id.parse().unwrap();

    add_member(&pool, user_b, guild_uuid).await;

    // Create and assign custom role
    let req = authed_post(
        &format!("/api/guilds/{guild_id}/roles"),
        &token,
        serde_json::json!({ "name": "Temp", "permissions": 0 }),
    );
    let resp = app.clone().oneshot(req).await.unwrap();
    let role = body_json(resp).await;
    let role_id = role["id"].as_str().unwrap();
    let user_b_id = user_b.0.to_string();

    // Assign to member
    let req = authed_put(
        &format!("/api/guilds/{guild_id}/members/{user_b_id}/roles/{role_id}"),
        &token,
    );
    app.clone().oneshot(req).await.unwrap();

    // Delete the role
    let req = authed_delete(&format!("/api/guilds/{guild_id}/roles/{role_id}"), &token);
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify cascade: member_role entry should be gone
    let has_role: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_member_roles WHERE user_id = $1 AND role_id = $2)",
    )
    .bind(user_b.0)
    .bind(role_id.parse::<uuid::Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!has_role);
}
