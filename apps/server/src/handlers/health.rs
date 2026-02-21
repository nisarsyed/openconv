use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use fred::interfaces::ClientLike;

use crate::state::AppState;

/// GET /health/live — returns 200 unconditionally.
/// Used by load balancers to check if the process is alive.
pub async fn liveness() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// GET /health/ready — queries database and Redis to verify connectivity.
/// Returns 200 on success, 503 on failure.
pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
    let redis_ok: bool = state.redis.ping::<()>(None).await.is_ok();

    if db_ok && redis_ok {
        (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "unavailable",
                "db": db_ok,
                "redis": redis_ok,
            })),
        )
            .into_response()
    }
}
