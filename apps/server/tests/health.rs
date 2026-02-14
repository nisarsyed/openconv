use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for `oneshot`

use openconv_server::config::ServerConfig;
use openconv_server::router::build_router;
use openconv_server::state::AppState;

/// Helper to build a test router without a real database.
/// Uses a pool that points to an invalid URL — fine for liveness checks
/// and middleware tests that don't hit the DB.
fn test_app() -> axum::Router {
    // Create a PgPool with a dummy URL — it won't actually connect
    // since liveness doesn't query the DB.
    let pool = sqlx::PgPool::connect_lazy("postgresql://fake@localhost/fake").unwrap();
    let config = ServerConfig {
        database_url: "postgresql://fake@localhost/fake".to_string(),
        ..Default::default()
    };
    let state = AppState {
        db: pool,
        config: std::sync::Arc::new(config),
    };
    build_router(state)
}

#[tokio::test]
async fn test_health_live_returns_200_with_status_ok() {
    let app = test_app();
    let request = Request::builder()
        .uri("/health/live")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[sqlx::test]
async fn test_health_ready_returns_200_when_db_connected(pool: sqlx::PgPool) {
    let config = ServerConfig {
        database_url: String::new(),
        ..Default::default()
    };
    let state = AppState {
        db: pool,
        config: std::sync::Arc::new(config),
    };
    let app = build_router(state);
    let request = Request::builder()
        .uri("/health/ready")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_health_ready_returns_503_when_db_unreachable() {
    let app = test_app();
    let request = Request::builder()
        .uri("/health/ready")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_requests_include_x_request_id_header() {
    let app = test_app();
    let request = Request::builder()
        .uri("/health/live")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let request_id = response.headers().get("x-request-id");
    assert!(
        request_id.is_some(),
        "Response should include x-request-id header"
    );
    // Verify the value is a valid UUID
    let id_str = request_id.unwrap().to_str().unwrap();
    uuid::Uuid::parse_str(id_str).expect("x-request-id should be a valid UUID");
}

#[tokio::test]
async fn test_cors_headers_present() {
    let app = test_app();
    let request = Request::builder()
        .method("OPTIONS")
        .uri("/health/live")
        .header("Origin", "http://localhost:1420")
        .header("Access-Control-Request-Method", "GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_some(),
        "Response should include Access-Control-Allow-Origin header"
    );
}

#[tokio::test]
async fn test_unknown_routes_return_404() {
    let app = test_app();
    let request = Request::builder()
        .uri("/nonexistent")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
