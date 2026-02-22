use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
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

async fn build_test_app(
    pool: sqlx::PgPool,
) -> (axum::Router, Arc<JwtService>, fred::clients::Pool) {
    let config = ServerConfig::default();
    let redis = create_redis_pool(&config.redis).await.unwrap();

    // Cleanup rate-limit keys
    use fred::interfaces::KeysInterface;
    let _: i64 = redis.del("rl:ip:10.99.0.1:auth").await.unwrap_or_default();

    let jwt = test_jwt();
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

/// Create a user + device in the DB and return (user_id, device_id, access_token).
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

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

// ---------------------------------------------------------------------------
// Profile Endpoint Tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn get_me_returns_current_user_full_profile(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, _, token) = seed_user(&pool, &jwt, "TestUser", "test_getme@example.com").await;

    let req = authed_get("/api/users/me", &token);
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    assert_eq!(json["id"], user_id.0.to_string());
    assert_eq!(json["email"], "test_getme@example.com");
    assert_eq!(json["display_name"], "TestUser");
    assert!(json["created_at"].is_string());
    assert!(json["updated_at"].is_string());
}

#[sqlx::test]
async fn get_me_returns_401_without_auth(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/users/me")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

#[sqlx::test]
async fn patch_me_updates_display_name(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Original", "patch_name@example.com").await;

    let req = authed_patch(
        "/api/users/me",
        &token,
        serde_json::json!({ "display_name": "New Name" }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    assert_eq!(json["display_name"], "New Name");
}

#[sqlx::test]
async fn patch_me_updates_avatar_url(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "AvatarUser", "patch_avatar@example.com").await;

    let req = authed_patch(
        "/api/users/me",
        &token,
        serde_json::json!({ "avatar_url": "https://example.com/avatar.png" }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    assert_eq!(json["avatar_url"], "https://example.com/avatar.png");
}

#[sqlx::test]
async fn patch_me_ignores_unset_fields(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, _, token) = seed_user(&pool, &jwt, "Original", "patch_partial@example.com").await;

    // First set avatar
    sqlx::query("UPDATE users SET avatar_url = $1 WHERE id = $2")
        .bind("old.png")
        .bind(user_id.0)
        .execute(&pool)
        .await
        .unwrap();

    let req = authed_patch(
        "/api/users/me",
        &token,
        serde_json::json!({ "display_name": "Updated" }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    assert_eq!(json["display_name"], "Updated");
    assert_eq!(json["avatar_url"], "old.png");
}

#[sqlx::test]
async fn patch_me_rejects_long_display_name(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "User", "patch_long@example.com").await;

    let long_name = "a".repeat(65);
    let req = authed_patch(
        "/api/users/me",
        &token,
        serde_json::json!({ "display_name": long_name }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 400);
}

#[sqlx::test]
async fn get_user_returns_public_profile_no_email(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, viewer_token) = seed_user(&pool, &jwt, "Viewer", "viewer@example.com").await;
    let (target_id, _, _) = seed_user(&pool, &jwt, "Target", "target@example.com").await;

    let req = authed_get(&format!("/api/users/{}", target_id.0), &viewer_token);
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    assert_eq!(json["display_name"], "Target");
    assert!(
        json.get("email").is_none(),
        "email should not be in public profile"
    );
}

#[sqlx::test]
async fn get_user_returns_404_for_missing_user(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Viewer", "viewer404@example.com").await;

    let fake_id = uuid::Uuid::new_v4();
    let req = authed_get(&format!("/api/users/{fake_id}"), &token);
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 404);
}

#[sqlx::test]
async fn search_users_returns_matching_by_display_name(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Alice", "alice_search@example.com").await;
    seed_user(&pool, &jwt, "Alicia", "alicia_search@example.com").await;
    seed_user(&pool, &jwt, "Bob", "bob_search@example.com").await;

    let req = authed_get("/api/users/search?q=Ali", &token);
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    let users = json["users"].as_array().unwrap();
    assert_eq!(users.len(), 2);
    let names: Vec<&str> = users
        .iter()
        .map(|u| u["display_name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Alice"));
    assert!(names.contains(&"Alicia"));
}

#[sqlx::test]
async fn search_users_returns_empty_for_no_matches(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (_, _, token) = seed_user(&pool, &jwt, "Searcher", "searcher@example.com").await;

    let req = authed_get("/api/users/search?q=zzzznonexistent", &token);
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    let users = json["users"].as_array().unwrap();
    assert!(users.is_empty());
    assert_eq!(json["total"], 0);
}

// ---------------------------------------------------------------------------
// Pre-Key Bundle Endpoint Tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn get_prekeys_returns_one_bundle(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (target_id, _, _) = seed_user(&pool, &jwt, "Target", "prekey_target@example.com").await;
    let (_, _, requester_token) =
        seed_user(&pool, &jwt, "Requester", "prekey_requester@example.com").await;

    // Insert pre-key bundles for target
    for i in 0..3u8 {
        sqlx::query("INSERT INTO pre_key_bundles (id, user_id, key_data, is_used) VALUES ($1, $2, $3, false)")
            .bind(uuid::Uuid::now_v7())
            .bind(target_id.0)
            .bind(vec![i; 32])
            .execute(&pool)
            .await
            .unwrap();
    }

    let req = authed_get(
        &format!("/api/users/{}/prekeys", target_id.0),
        &requester_token,
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    let json = response_json(response).await;
    assert!(json["key_data"].is_array());
}

#[sqlx::test]
async fn get_prekeys_marks_bundle_as_used(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (target_id, _, _) = seed_user(&pool, &jwt, "Target", "prekey_used@example.com").await;
    let (_, _, requester_token) =
        seed_user(&pool, &jwt, "Requester", "prekey_req_used@example.com").await;

    let bundle_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO pre_key_bundles (id, user_id, key_data, is_used) VALUES ($1, $2, $3, false)",
    )
    .bind(bundle_id)
    .bind(target_id.0)
    .bind(vec![42u8; 32])
    .execute(&pool)
    .await
    .unwrap();

    let req = authed_get(
        &format!("/api/users/{}/prekeys", target_id.0),
        &requester_token,
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 200);

    // Verify bundle is marked used
    let is_used: bool = sqlx::query_scalar("SELECT is_used FROM pre_key_bundles WHERE id = $1")
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(is_used);
}

#[sqlx::test]
async fn get_prekeys_returns_404_when_none_available(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (target_id, _, _) = seed_user(&pool, &jwt, "Target", "prekey_empty@example.com").await;
    let (_, _, requester_token) =
        seed_user(&pool, &jwt, "Requester", "prekey_req_empty@example.com").await;

    let req = authed_get(
        &format!("/api/users/{}/prekeys", target_id.0),
        &requester_token,
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 404);
}

#[sqlx::test]
async fn post_prekeys_stores_new_bundles(pool: sqlx::PgPool) {
    let (app, jwt, _) = build_test_app(pool.clone()).await;
    let (user_id, _, token) = seed_user(&pool, &jwt, "Uploader", "prekey_upload@example.com").await;

    let bundles = vec![vec![1u8; 32], vec![2u8; 32], vec![3u8; 32]];
    let req = authed_post(
        "/api/users/me/prekeys",
        &token,
        serde_json::json!({ "pre_key_bundles": bundles }),
    );
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 201);

    // Verify bundles in DB
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pre_key_bundles WHERE user_id = $1")
        .bind(user_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3);
}

#[sqlx::test]
async fn post_prekeys_returns_401_without_auth(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/users/me/prekeys")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({ "pre_key_bundles": [[1, 2, 3]] })).unwrap(),
        ))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}

// ---------------------------------------------------------------------------
// Router Tests
// ---------------------------------------------------------------------------

#[sqlx::test]
async fn user_routes_mounted_at_api_users(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    // GET /api/users/me — should be 401 (not 404)
    let req = Request::builder()
        .method("GET")
        .uri("/api/users/me")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_ne!(response.status(), 404, "route should exist");

    // GET /api/users/search — should be 401 (not 404)
    let req = Request::builder()
        .method("GET")
        .uri("/api/users/search?q=test")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_ne!(response.status(), 404, "route should exist");
}

#[sqlx::test]
async fn auth_middleware_applies_to_user_routes(pool: sqlx::PgPool) {
    let (app, _, _) = build_test_app(pool).await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/users/me")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), 401);
}
