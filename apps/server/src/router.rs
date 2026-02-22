use axum::extract::DefaultBodyLimit;
use axum::http::HeaderValue;
use axum::middleware;
use axum::routing::{delete, get, post};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::handlers;
use crate::middleware::rate_limit::UserRateLimitLayer;
use crate::openapi::ApiDoc;
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
            axum::http::Method::PATCH,
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
        .route("/challenge", post(handlers::auth::challenge))
        .route("/verify", post(handlers::auth::login_verify))
        .route("/refresh", post(handlers::auth::refresh))
        .route("/logout", post(handlers::auth::logout))
        .route("/logout-all", post(handlers::auth::logout_all))
        .route("/devices", get(handlers::auth::list_devices))
        .route(
            "/devices/{device_id}",
            delete(handlers::auth::revoke_device),
        )
        .route("/recover/start", post(handlers::auth::recover_start))
        .route("/recover/verify", post(handlers::auth::recover_verify))
        .route("/recover/complete", post(handlers::auth::recover_complete))
        .layer(crate::middleware::rate_limit::RateLimitLayer::new(
            state.redis.clone(),
            state.config.rate_limit.auth_per_ip_per_minute,
            60,
            "auth".to_string(),
        ));

    let user_routes = axum::Router::new()
        .route(
            "/me",
            get(handlers::users::get_me).patch(handlers::users::update_me),
        )
        .route("/me/prekeys", post(handlers::users::upload_prekeys))
        .route("/search", get(handlers::users::search_users))
        .route("/{user_id}", get(handlers::users::get_user))
        .route("/{user_id}/prekeys", get(handlers::users::get_prekeys))
        .layer(crate::middleware::rate_limit::RateLimitLayer::new(
            state.redis.clone(),
            state.config.rate_limit.auth_per_ip_per_minute,
            60,
            "users".to_string(),
        ));

    let rl = &state.config.rate_limit;

    let guild_routes = handlers::guilds::routes().layer(UserRateLimitLayer::new(
        state.redis.clone(),
        state.jwt.clone(),
        rl.guild_per_user_per_minute,
        60,
        "guilds".to_string(),
    ));

    let channel_routes = handlers::channels::routes().layer(UserRateLimitLayer::new(
        state.redis.clone(),
        state.jwt.clone(),
        rl.channel_per_user_per_minute,
        60,
        "channels".to_string(),
    ));

    let channel_detail_routes = handlers::channels::detail_routes();
    let role_routes = handlers::roles::routes();
    let member_routes = handlers::guilds::member_routes();

    let invite_guild_routes = handlers::invites::guild_routes().layer(UserRateLimitLayer::new(
        state.redis.clone(),
        state.jwt.clone(),
        rl.invite_per_user_per_hour,
        3600,
        "invites".to_string(),
    ));

    let invite_public_routes = handlers::invites::public_routes();
    let dm_routes = handlers::dm_channels::routes();
    let message_routes = handlers::messages::guild_message_routes();

    // File upload routes get a higher body limit (25MB) and per-user rate limiting
    let guild_file_routes = handlers::files::guild_file_routes()
        .layer(DefaultBodyLimit::max(26_214_400))
        .layer(UserRateLimitLayer::new(
            state.redis.clone(),
            state.jwt.clone(),
            rl.file_per_user_per_minute,
            60,
            "files".to_string(),
        ));

    let dm_file_routes = handlers::files::dm_file_routes()
        .layer(DefaultBodyLimit::max(26_214_400))
        .layer(UserRateLimitLayer::new(
            state.redis.clone(),
            state.jwt.clone(),
            rl.file_per_user_per_minute,
            60,
            "files".to_string(),
        ));

    let file_routes = handlers::files::file_routes();

    let ws_ticket_routes = axum::Router::new()
        .route("/", post(handlers::ws::create_ws_ticket))
        .layer(UserRateLimitLayer::new(
            state.redis.clone(),
            state.jwt.clone(),
            10,
            60,
            "ws_ticket".to_string(),
        ));

    axum::Router::new()
        .merge(Scalar::with_url("/docs", ApiDoc::openapi()))
        .route("/health/live", get(handlers::health::liveness))
        .route("/health/ready", get(handlers::health::readiness))
        .nest("/api/auth", auth_routes)
        .nest("/api/users", user_routes)
        .nest("/api/guilds", guild_routes)
        .nest("/api/guilds/{guild_id}/channels", channel_routes)
        .nest("/api/channels/{channel_id}/messages", message_routes)
        .nest("/api/channels/{channel_id}/files", guild_file_routes)
        .nest("/api/channels", channel_detail_routes)
        .nest("/api/guilds/{guild_id}/roles", role_routes)
        .nest("/api/guilds/{guild_id}/members", member_routes)
        .nest("/api/guilds/{guild_id}/invites", invite_guild_routes)
        .nest("/api/invites", invite_public_routes)
        .nest("/api/dm-channels", dm_routes)
        .nest("/api/dm-channels/{dm_channel_id}/files", dm_file_routes)
        .nest("/api/files", file_routes)
        .nest("/api/ws/ticket", ws_ticket_routes)
        .route("/ws", get(handlers::ws::ws_upgrade))
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
