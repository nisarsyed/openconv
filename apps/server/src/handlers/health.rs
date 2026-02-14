use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::state::AppState;

/// GET /health/live — returns 200 unconditionally.
/// Used by load balancers to check if the process is alive.
pub async fn liveness() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// GET /health/ready — queries the database to verify connectivity.
/// Returns 200 on success, 503 on failure.
pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "unavailable" })),
        )
            .into_response(),
    }
}
