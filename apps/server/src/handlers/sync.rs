use axum::extract::{Query, State};
use axum::Json;
use openconv_shared::api::sync::{SyncEvent, SyncResponse};
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::ChannelId;

use crate::error::ServerError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;

#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct SyncQuery {
    pub after_sequence: i64,
    /// Comma-separated channel UUIDs to filter events.
    pub channel_ids: Option<String>,
    pub limit: Option<i64>,
}

fn db_err(e: sqlx::Error) -> ServerError {
    tracing::error!(error = %e, "database error");
    ServerError(OpenConvError::Internal("database error".into()))
}

#[utoipa::path(get, path = "/api/sync", tag = "Sync", security(("bearer_auth" = [])), params(SyncQuery), responses((status = 200, body = openconv_shared::api::sync::SyncResponse), (status = 401, body = crate::error::ErrorResponse)))]
/// GET /api/sync — fetch channel events after a given sequence for offline gap-fill.
pub async fn sync_events(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(params): Query<SyncQuery>,
) -> Result<Json<SyncResponse>, ServerError> {
    let limit = params.limit.unwrap_or(100).clamp(1, 500);

    let channel_ids: Option<Vec<ChannelId>> = params.channel_ids.as_deref().and_then(|s| {
        let ids: Vec<ChannelId> = s
            .split(',')
            .filter_map(|part| part.trim().parse().ok())
            .collect();
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    });

    let channel_id_array: Option<Vec<uuid::Uuid>> =
        channel_ids.map(|ids| ids.into_iter().map(|id| id.0).collect());

    let rows: Vec<SyncEventRow> = sqlx::query_as(
        "SELECT ce.sequence, ce.channel_id, ce.event_type, ce.message_id, ce.created_at \
         FROM channel_events ce \
         JOIN channels c ON c.id = ce.channel_id \
         JOIN guild_members gm ON gm.guild_id = c.guild_id AND gm.user_id = $1 \
         WHERE ce.sequence > $2 \
           AND ($3::uuid[] IS NULL OR ce.channel_id = ANY($3)) \
         ORDER BY ce.sequence ASC \
         LIMIT $4",
    )
    .bind(auth_user.user_id)
    .bind(params.after_sequence)
    .bind(&channel_id_array)
    .bind(limit + 1)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let has_more = rows.len() as i64 > limit;
    let events: Vec<SyncEventRow> = rows.into_iter().take(limit as usize).collect();

    let next_sequence = events
        .last()
        .map(|e| e.sequence as u64)
        .unwrap_or(params.after_sequence.max(0) as u64);

    Ok(Json(SyncResponse {
        events: events
            .into_iter()
            .map(|r| SyncEvent {
                sequence: r.sequence as u64,
                channel_id: r.channel_id,
                event_type: r.event_type,
                message_id: r.message_id,
                created_at: r.created_at,
            })
            .collect(),
        next_sequence,
        has_more,
    }))
}

/// Route builder for sync endpoints.
pub fn routes() -> axum::Router<AppState> {
    axum::Router::new().route("/", axum::routing::get(sync_events))
}

#[derive(sqlx::FromRow)]
struct SyncEventRow {
    sequence: i64,
    channel_id: ChannelId,
    event_type: String,
    message_id: openconv_shared::ids::MessageId,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_routes_build_without_panic() {
        let _ = routes();
    }

    #[test]
    fn sync_query_deserializes() {
        let json = r#"{"after_sequence": 42}"#;
        let query: SyncQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.after_sequence, 42);
        assert!(query.channel_ids.is_none());
        assert!(query.limit.is_none());
    }

    #[test]
    fn sync_query_with_channel_ids() {
        let json = r#"{"after_sequence": 0, "channel_ids": "abc,def", "limit": 50}"#;
        let query: SyncQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.after_sequence, 0);
        assert_eq!(query.channel_ids.as_deref(), Some("abc,def"));
        assert_eq!(query.limit, Some(50));
    }
}
