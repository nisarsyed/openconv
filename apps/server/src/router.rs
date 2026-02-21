use axum::extract::DefaultBodyLimit;
use axum::http::HeaderValue;
use axum::middleware;
use axum::routing::{get, post};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::state::AppState;

/// Builds the application router with all middleware and routes.
pub fn build_router(state: AppState) -> axum::Router {
    let origins: Vec<HeaderValue> = state
        .config
        .cors_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ]);

    let auth_routes = axum::Router::new()
        .route("/register/start", post(handlers::auth::register_start))
        .route("/register/verify", post(handlers::auth::register_verify))
        .route(
            "/register/complete",
            post(handlers::auth::register_complete),
        )
        .layer(crate::middleware::rate_limit::RateLimitLayer::new(
            state.redis.clone(),
            state.config.rate_limit.auth_per_ip_per_minute,
            60,
            "auth".to_string(),
        ));

    axum::Router::new()
        .route("/health/live", get(handlers::health::liveness))
        .route("/health/ready", get(handlers::health::readiness))
        .nest("/api/auth", auth_routes)
        .layer(middleware::from_fn(request_id_middleware))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn request_id_middleware(
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    tracing::Span::current().record("request_id", request_id.as_str());
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("x-request-id", HeaderValue::from_str(&request_id).unwrap());
    response
}
