use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use openconv_shared::api::dm_channel::{
    AddDmMemberRequest, CreateDmChannelRequest, DmChannelResponse,
};
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::{DmChannelId, UserId};

use crate::error::ServerError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;

fn db_err(e: sqlx::Error) -> ServerError {
    tracing::error!(error = %e, "database error");
    ServerError(OpenConvError::Internal("database error".into()))
}

/// Verify the user is a member of the DM channel. Returns 403 if not.
async fn require_dm_membership(
    db: &sqlx::PgPool,
    dm_channel_id: DmChannelId,
    user_id: UserId,
) -> Result<(), ServerError> {
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dm_channel_members WHERE dm_channel_id = $1 AND user_id = $2)",
    )
    .bind(dm_channel_id)
    .bind(user_id)
    .fetch_one(db)
    .await
    .map_err(db_err)?;

    if !is_member {
        return Err(ServerError(OpenConvError::Forbidden));
    }
    Ok(())
}

/// Fetch the member list for a DM channel.
async fn fetch_members(
    db: &sqlx::PgPool,
    dm_channel_id: DmChannelId,
) -> Result<Vec<UserId>, ServerError> {
    let members: Vec<UserId> = sqlx::query_scalar(
        "SELECT user_id FROM dm_channel_members WHERE dm_channel_id = $1 ORDER BY user_id",
    )
    .bind(dm_channel_id)
    .fetch_all(db)
    .await
    .map_err(db_err)?;
    Ok(members)
}

/// Build a DmChannelResponse from a row + members.
fn build_response(row: DmChannelRow, members: Vec<UserId>) -> DmChannelResponse {
    DmChannelResponse {
        id: row.id,
        name: row.name,
        creator_id: row.creator_id,
        is_group: row.is_group,
        members,
        created_at: row.created_at,
    }
}

#[utoipa::path(post, path = "/api/dm-channels", tag = "DM Channels", security(("bearer_auth" = [])), request_body = openconv_shared::api::dm_channel::CreateDmChannelRequest, responses((status = 201, body = openconv_shared::api::dm_channel::DmChannelResponse), (status = 200, body = openconv_shared::api::dm_channel::DmChannelResponse), (status = 400, body = crate::error::ErrorResponse)))]
/// POST /api/dm-channels
/// Create a DM channel (1:1 or group).
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateDmChannelRequest>,
) -> Result<(StatusCode, Json<DmChannelResponse>), ServerError> {
    if body.user_ids.is_empty() {
        return Err(ServerError(OpenConvError::Validation(
            "user_ids must not be empty".into(),
        )));
    }

    // Prevent creating a DM with yourself only
    if body.user_ids.len() == 1 && body.user_ids[0] == auth.user_id {
        return Err(ServerError(OpenConvError::Validation(
            "cannot create a DM with yourself".into(),
        )));
    }

    if body.user_ids.len() == 1 {
        // 1:1 DM
        create_one_to_one(&state, &auth, body.user_ids[0]).await
    } else {
        // Group DM
        create_group(&state, &auth, &body).await
    }
}

async fn create_one_to_one(
    state: &AppState,
    auth: &AuthUser,
    target_user_id: UserId,
) -> Result<(StatusCode, Json<DmChannelResponse>), ServerError> {
    // Validate target user exists
    let user_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(target_user_id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;

    if !user_exists {
        return Err(ServerError(OpenConvError::NotFound));
    }

    // Use a transaction for the entire dedup check + insert to prevent races
    let mut tx = state.db.begin().await.map_err(db_err)?;

    // Check for existing 1:1 DM between these two users (inside transaction)
    let existing = sqlx::query_as::<_, DmChannelRow>(
        "SELECT dc.id, dc.name, dc.creator_id, dc.is_group, dc.created_at \
         FROM dm_channels dc \
         WHERE dc.is_group = false \
         AND EXISTS (SELECT 1 FROM dm_channel_members WHERE dm_channel_id = dc.id AND user_id = $1) \
         AND EXISTS (SELECT 1 FROM dm_channel_members WHERE dm_channel_id = dc.id AND user_id = $2) \
         LIMIT 1 \
         FOR UPDATE",
    )
    .bind(auth.user_id)
    .bind(target_user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;

    if let Some(row) = existing {
        let members = fetch_members(&state.db, row.id).await?;
        tx.commit().await.map_err(db_err)?;
        return Ok((StatusCode::OK, Json(build_response(row, members))));
    }

    // Create new 1:1 DM
    let row = sqlx::query_as::<_, DmChannelRow>(
        "INSERT INTO dm_channels (creator_id, is_group) VALUES ($1, false) \
         RETURNING id, name, creator_id, is_group, created_at",
    )
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_err)?;

    sqlx::query(
        "INSERT INTO dm_channel_members (dm_channel_id, user_id) VALUES ($1, $2), ($1, $3)",
    )
    .bind(row.id)
    .bind(auth.user_id)
    .bind(target_user_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    tx.commit().await.map_err(db_err)?;

    let members = vec![auth.user_id, target_user_id];
    Ok((StatusCode::CREATED, Json(build_response(row, members))))
}

async fn create_group(
    state: &AppState,
    auth: &AuthUser,
    body: &CreateDmChannelRequest,
) -> Result<(StatusCode, Json<DmChannelResponse>), ServerError> {
    // Validate name length
    if let Some(ref name) = body.name {
        if name.len() > 100 {
            return Err(ServerError(OpenConvError::Validation(
                "group DM name must be 100 characters or fewer".into(),
            )));
        }
    }

    // Build participant list (creator + provided user_ids, deduplicated)
    let mut participants: Vec<UserId> = vec![auth.user_id];
    for &uid in &body.user_ids {
        if !participants.contains(&uid) {
            participants.push(uid);
        }
    }

    let total = participants.len();
    if !(2..=25).contains(&total) {
        return Err(ServerError(OpenConvError::Validation(
            "group DMs must have between 2 and 25 participants".into(),
        )));
    }

    // Validate all user IDs exist using ANY($1)
    let participant_uuids: Vec<uuid::Uuid> = participants.iter().map(|u| u.0).collect();
    let existing_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = ANY($1)")
        .bind(&participant_uuids)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;

    if existing_count != participants.len() as i64 {
        return Err(ServerError(OpenConvError::Validation(
            "one or more user_ids do not exist".into(),
        )));
    }

    let mut tx = state.db.begin().await.map_err(db_err)?;

    let row = sqlx::query_as::<_, DmChannelRow>(
        "INSERT INTO dm_channels (name, creator_id, is_group) VALUES ($1, $2, true) \
         RETURNING id, name, creator_id, is_group, created_at",
    )
    .bind(&body.name)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_err)?;

    for &uid in &participants {
        sqlx::query("INSERT INTO dm_channel_members (dm_channel_id, user_id) VALUES ($1, $2)")
            .bind(row.id)
            .bind(uid)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
    }

    tx.commit().await.map_err(db_err)?;

    Ok((StatusCode::CREATED, Json(build_response(row, participants))))
}

#[utoipa::path(get, path = "/api/dm-channels", tag = "DM Channels", security(("bearer_auth" = [])), responses((status = 200, body = Vec<openconv_shared::api::dm_channel::DmChannelResponse>)))]
/// GET /api/dm-channels
/// List the authenticated user's DM channels.
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<DmChannelResponse>>, ServerError> {
    let rows = sqlx::query_as::<_, DmChannelRow>(
        "SELECT dc.id, dc.name, dc.creator_id, dc.is_group, dc.created_at \
         FROM dm_channels dc \
         JOIN dm_channel_members dcm ON dcm.dm_channel_id = dc.id \
         WHERE dcm.user_id = $1 \
         ORDER BY dc.created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    if rows.is_empty() {
        return Ok(Json(vec![]));
    }

    // Batch-fetch all members in one query
    let channel_ids: Vec<DmChannelId> = rows.iter().map(|r| r.id).collect();
    let member_rows = sqlx::query_as::<_, MemberRow>(
        "SELECT dm_channel_id, user_id FROM dm_channel_members \
         WHERE dm_channel_id = ANY($1) \
         ORDER BY user_id",
    )
    .bind(&channel_ids)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    // Group members by channel
    let mut members_map: std::collections::HashMap<DmChannelId, Vec<UserId>> =
        std::collections::HashMap::new();
    for mr in member_rows {
        members_map
            .entry(mr.dm_channel_id)
            .or_default()
            .push(mr.user_id);
    }

    let result = rows
        .into_iter()
        .map(|row| {
            let members = members_map.remove(&row.id).unwrap_or_default();
            build_response(row, members)
        })
        .collect();

    Ok(Json(result))
}

#[utoipa::path(get, path = "/api/dm-channels/{id}", tag = "DM Channels", security(("bearer_auth" = [])), params(("id" = openconv_shared::ids::DmChannelId, Path, description = "DM channel ID")), responses((status = 200, body = openconv_shared::api::dm_channel::DmChannelResponse), (status = 403, body = crate::error::ErrorResponse)))]
/// GET /api/dm-channels/:id
/// Get DM channel details.
pub async fn get_one(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<DmChannelId>,
) -> Result<Json<DmChannelResponse>, ServerError> {
    require_dm_membership(&state.db, id, auth.user_id).await?;

    let row = sqlx::query_as::<_, DmChannelRow>(
        "SELECT id, name, creator_id, is_group, created_at FROM dm_channels WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    let members = fetch_members(&state.db, id).await?;
    Ok(Json(build_response(row, members)))
}

#[utoipa::path(post, path = "/api/dm-channels/{id}/members", tag = "DM Channels", security(("bearer_auth" = [])), params(("id" = openconv_shared::ids::DmChannelId, Path, description = "DM channel ID")), request_body = openconv_shared::api::dm_channel::AddDmMemberRequest, responses((status = 200, body = openconv_shared::api::dm_channel::DmChannelResponse), (status = 400, body = crate::error::ErrorResponse), (status = 403, body = crate::error::ErrorResponse)))]
/// POST /api/dm-channels/:id/members
/// Add a member to a group DM.
pub async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<DmChannelId>,
    Json(body): Json<AddDmMemberRequest>,
) -> Result<Json<DmChannelResponse>, ServerError> {
    require_dm_membership(&state.db, id, auth.user_id).await?;

    // Use is_group column for definitive 1:1 vs group check
    let is_group: bool = sqlx::query_scalar("SELECT is_group FROM dm_channels WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;

    if !is_group {
        return Err(ServerError(OpenConvError::Validation(
            "cannot add members to a 1:1 DM".into(),
        )));
    }

    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM dm_channel_members WHERE dm_channel_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(db_err)?;

    if member_count >= 25 {
        return Err(ServerError(OpenConvError::Validation(
            "group DM is at maximum capacity (25 members)".into(),
        )));
    }

    // Validate target user exists
    let user_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(body.user_id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;

    if !user_exists {
        return Err(ServerError(OpenConvError::NotFound));
    }

    // Check not already a member
    let already_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dm_channel_members WHERE dm_channel_id = $1 AND user_id = $2)",
    )
    .bind(id)
    .bind(body.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;

    if already_member {
        return Err(ServerError(OpenConvError::Conflict(
            "user is already a member of this DM channel".into(),
        )));
    }

    sqlx::query("INSERT INTO dm_channel_members (dm_channel_id, user_id) VALUES ($1, $2)")
        .bind(id)
        .bind(body.user_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    // Return updated channel
    let row = sqlx::query_as::<_, DmChannelRow>(
        "SELECT id, name, creator_id, is_group, created_at FROM dm_channels WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;

    let members = fetch_members(&state.db, id).await?;
    Ok(Json(build_response(row, members)))
}

#[utoipa::path(delete, path = "/api/dm-channels/{id}/members/me", tag = "DM Channels", security(("bearer_auth" = [])), params(("id" = openconv_shared::ids::DmChannelId, Path, description = "DM channel ID")), responses((status = 204), (status = 400, body = crate::error::ErrorResponse), (status = 403, body = crate::error::ErrorResponse)))]
/// DELETE /api/dm-channels/:id/members/me
/// Leave a group DM.
pub async fn leave(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<DmChannelId>,
) -> Result<StatusCode, ServerError> {
    require_dm_membership(&state.db, id, auth.user_id).await?;

    // Use is_group column for definitive check
    let is_group: bool = sqlx::query_scalar("SELECT is_group FROM dm_channels WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;

    if !is_group {
        return Err(ServerError(OpenConvError::Validation(
            "cannot leave a 1:1 DM".into(),
        )));
    }

    sqlx::query("DELETE FROM dm_channel_members WHERE dm_channel_id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    // If channel is now empty, clean it up
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM dm_channel_members WHERE dm_channel_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(db_err)?;

    if remaining == 0 {
        sqlx::query("DELETE FROM dm_channels WHERE id = $1")
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(db_err)?;
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/api/dm-channels/{id}/messages", tag = "DM Channels", security(("bearer_auth" = [])), params(("id" = openconv_shared::ids::DmChannelId, Path, description = "DM channel ID"), crate::handlers::dm_channels::MessageQuery), responses((status = 200, body = crate::handlers::dm_channels::MessagePage), (status = 403, body = crate::error::ErrorResponse)))]
/// GET /api/dm-channels/:id/messages
/// Cursor-paginated message history for a DM channel.
pub async fn messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<DmChannelId>,
    Query(params): Query<MessageQuery>,
) -> Result<Json<MessagePage>, ServerError> {
    require_dm_membership(&state.db, id, auth.user_id).await?;

    let limit = params.limit.unwrap_or(50).clamp(1, 100) as i64;

    let rows = if let Some(ref cursor) = params.cursor {
        let decoded = base64_decode_cursor(cursor)?;
        sqlx::query_as::<_, MessageRow>(
            "SELECT id, dm_channel_id, sender_id, encrypted_content, nonce, edited_at, created_at \
             FROM messages \
             WHERE dm_channel_id = $1 AND deleted = false \
               AND (created_at, id) < ($2, $3) \
             ORDER BY created_at DESC, id DESC \
             LIMIT $4",
        )
        .bind(id)
        .bind(decoded.created_at)
        .bind(decoded.id)
        .bind(limit + 1)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?
    } else {
        sqlx::query_as::<_, MessageRow>(
            "SELECT id, dm_channel_id, sender_id, encrypted_content, nonce, edited_at, created_at \
             FROM messages \
             WHERE dm_channel_id = $1 AND deleted = false \
             ORDER BY created_at DESC, id DESC \
             LIMIT $2",
        )
        .bind(id)
        .bind(limit + 1)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?
    };

    let has_more = rows.len() as i64 > limit;
    let msgs: Vec<MessageRow> = rows.into_iter().take(limit as usize).collect();

    let next_cursor = if has_more {
        msgs.last()
            .map(|m| base64_encode_cursor(m.created_at, m.id))
    } else {
        None
    };

    Ok(Json(MessagePage {
        messages: msgs.into_iter().map(|m| m.into_response()).collect(),
        next_cursor,
        has_more,
    }))
}

/// Route builder for DM channel endpoints.
pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/", axum::routing::post(create).get(list))
        .route("/{id}", axum::routing::get(get_one))
        .route("/{id}/members", axum::routing::post(add_member))
        .route("/{id}/members/me", axum::routing::delete(leave))
        .route("/{id}/messages", axum::routing::get(messages))
}

// ─── Internal types ─────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct DmChannelRow {
    id: DmChannelId,
    name: Option<String>,
    creator_id: Option<UserId>,
    is_group: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    dm_channel_id: DmChannelId,
    user_id: UserId,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct MessageQuery {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MessagePage {
    pub messages: Vec<MessageResponse>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MessageResponse {
    pub id: uuid::Uuid,
    pub dm_channel_id: Option<DmChannelId>,
    pub sender_id: UserId,
    pub encrypted_content: String,
    pub nonce: String,
    pub edited_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: uuid::Uuid,
    dm_channel_id: Option<DmChannelId>,
    sender_id: UserId,
    encrypted_content: String,
    nonce: String,
    edited_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl MessageRow {
    fn into_response(self) -> MessageResponse {
        MessageResponse {
            id: self.id,
            dm_channel_id: self.dm_channel_id,
            sender_id: self.sender_id,
            encrypted_content: self.encrypted_content,
            nonce: self.nonce,
            edited_at: self.edited_at,
            created_at: self.created_at,
        }
    }
}

struct CursorData {
    created_at: chrono::DateTime<chrono::Utc>,
    id: uuid::Uuid,
}

fn base64_encode_cursor(created_at: chrono::DateTime<chrono::Utc>, id: uuid::Uuid) -> String {
    use base64::Engine;
    // Use timestamp micros + uuid for reliable roundtrip
    let raw = format!("{}|{}", created_at.timestamp_micros(), id);
    base64::engine::general_purpose::STANDARD.encode(raw)
}

fn base64_decode_cursor(cursor: &str) -> Result<CursorData, ServerError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cursor)
        .map_err(|_| ServerError(OpenConvError::Validation("invalid cursor".into())))?;
    let raw = String::from_utf8(bytes)
        .map_err(|_| ServerError(OpenConvError::Validation("invalid cursor".into())))?;
    let parts: Vec<&str> = raw.splitn(2, '|').collect();
    if parts.len() != 2 {
        return Err(ServerError(OpenConvError::Validation(
            "invalid cursor format".into(),
        )));
    }
    let micros: i64 = parts[0]
        .parse()
        .map_err(|_| ServerError(OpenConvError::Validation("invalid cursor timestamp".into())))?;
    let created_at = chrono::DateTime::from_timestamp_micros(micros)
        .ok_or_else(|| ServerError(OpenConvError::Validation("invalid cursor timestamp".into())))?;
    let id = parts[1]
        .parse::<uuid::Uuid>()
        .map_err(|_| ServerError(OpenConvError::Validation("invalid cursor id".into())))?;
    Ok(CursorData { created_at, id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_roundtrip() {
        let now = chrono::Utc::now();
        let id = uuid::Uuid::new_v4();
        let encoded = base64_encode_cursor(now, id);
        let decoded = base64_decode_cursor(&encoded).unwrap();
        assert_eq!(decoded.id, id);
    }

    #[test]
    fn invalid_cursor_returns_error() {
        assert!(base64_decode_cursor("not-base64!@#$").is_err());
        assert!(base64_decode_cursor("").is_err());
    }

    #[test]
    fn routes_build_without_panic() {
        let _ = routes();
    }
}
