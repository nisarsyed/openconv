use axum::extract::{Query, State};
use axum::Json;
use base64::Engine;
use openconv_shared::api::message::{MessageHistoryQuery, MessageHistoryResponse, MessageResponse};
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::{ChannelId, MessageId, UserId};
use openconv_shared::permissions::Permissions;

use crate::error::ServerError;
use crate::extractors::channel_member::ChannelMember;
use crate::state::AppState;

fn db_err(e: sqlx::Error) -> ServerError {
    tracing::error!(error = %e, "database error");
    ServerError(OpenConvError::Internal("database error".into()))
}

// ─── Cursor helpers ─────────────────────────────────────────

struct CursorData {
    created_at: chrono::DateTime<chrono::Utc>,
    id: MessageId,
}

fn encode_cursor(created_at: chrono::DateTime<chrono::Utc>, id: MessageId) -> String {
    let raw = format!("{}|{}", created_at.timestamp_micros(), id);
    base64::engine::general_purpose::STANDARD.encode(raw)
}

fn decode_cursor(cursor: &str) -> Result<CursorData, ServerError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cursor)
        .map_err(|_| ServerError(OpenConvError::Validation("invalid cursor".into())))?;
    let raw = String::from_utf8(bytes)
        .map_err(|_| ServerError(OpenConvError::Validation("invalid cursor".into())))?;
    let (ts_str, id_str) = raw
        .split_once('|')
        .ok_or_else(|| ServerError(OpenConvError::Validation("invalid cursor format".into())))?;
    let micros: i64 = ts_str
        .parse()
        .map_err(|_| ServerError(OpenConvError::Validation("invalid cursor timestamp".into())))?;
    let created_at = chrono::DateTime::from_timestamp_micros(micros)
        .ok_or_else(|| ServerError(OpenConvError::Validation("invalid cursor timestamp".into())))?;
    let id: MessageId = id_str
        .parse()
        .map_err(|_| ServerError(OpenConvError::Validation("invalid cursor id".into())))?;
    Ok(CursorData { created_at, id })
}

// ─── Guild channel message history ─────────────────────────

#[utoipa::path(get, path = "/api/channels/{channel_id}/messages", tag = "Messages", security(("bearer_auth" = [])), params(("channel_id" = openconv_shared::ids::ChannelId, Path, description = "Channel ID"), openconv_shared::api::message::MessageHistoryQuery), responses((status = 200, body = openconv_shared::api::message::MessageHistoryResponse), (status = 403, body = crate::error::ErrorResponse)))]
/// GET /api/channels/:channel_id/messages
/// Cursor-paginated message history for a guild channel.
pub async fn guild_messages(
    State(state): State<AppState>,
    channel_member: ChannelMember,
    Query(params): Query<MessageHistoryQuery>,
) -> Result<Json<MessageHistoryResponse>, ServerError> {
    channel_member.require(Permissions::READ_MESSAGES)?;

    let limit = params.limit.unwrap_or(50).clamp(1, 100) as i64;

    let rows = if let Some(ref cursor) = params.cursor {
        let decoded = decode_cursor(cursor)?;
        sqlx::query_as::<_, MessageRow>(
            "SELECT id, channel_id, sender_id, encrypted_content, nonce, edited_at, created_at \
             FROM messages \
             WHERE channel_id = $1 AND deleted = false \
               AND (created_at, id) < ($2, $3) \
             ORDER BY created_at DESC, id DESC \
             LIMIT $4",
        )
        .bind(channel_member.channel_id)
        .bind(decoded.created_at)
        .bind(decoded.id)
        .bind(limit + 1)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?
    } else {
        sqlx::query_as::<_, MessageRow>(
            "SELECT id, channel_id, sender_id, encrypted_content, nonce, edited_at, created_at \
             FROM messages \
             WHERE channel_id = $1 AND deleted = false \
             ORDER BY created_at DESC, id DESC \
             LIMIT $2",
        )
        .bind(channel_member.channel_id)
        .bind(limit + 1)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?
    };

    let has_more = rows.len() as i64 > limit;
    let msgs: Vec<MessageRow> = rows.into_iter().take(limit as usize).collect();

    let next_cursor = if has_more {
        msgs.last().map(|m| encode_cursor(m.created_at, m.id))
    } else {
        None
    };

    Ok(Json(MessageHistoryResponse {
        messages: msgs.into_iter().map(|m| m.into_response()).collect(),
        next_cursor,
        has_more,
    }))
}

// ─── Message edit/delete (WebSocket operation helpers) ───────

/// Edit a message. Validates sender ownership and channel association.
/// Called by WebSocket dispatch (section-09).
pub async fn handle_edit_message(
    db: &sqlx::PgPool,
    user_id: UserId,
    channel_id: ChannelId,
    message_id: MessageId,
    encrypted_content: Vec<u8>,
    nonce: Vec<u8>,
) -> Result<MessageResponse, ServerError> {
    // Verify the message exists, belongs to this channel, and is owned by the user
    let existing = sqlx::query_as::<_, MessageOwnerRow>(
        "SELECT id, channel_id, sender_id FROM messages \
         WHERE id = $1 AND deleted = false",
    )
    .bind(message_id)
    .fetch_optional(db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    if existing.channel_id != Some(channel_id) {
        return Err(ServerError(OpenConvError::NotFound));
    }

    if existing.sender_id != user_id {
        return Err(ServerError(OpenConvError::Forbidden));
    }

    // Update atomically
    let row = sqlx::query_as::<_, MessageRow>(
        "UPDATE messages \
         SET encrypted_content = $1, nonce = $2, edited_at = NOW() \
         WHERE id = $3 AND deleted = false \
         RETURNING id, channel_id, sender_id, encrypted_content, nonce, edited_at, created_at",
    )
    .bind(&encrypted_content)
    .bind(&nonce)
    .bind(message_id)
    .fetch_one(db)
    .await
    .map_err(db_err)?;

    Ok(row.into_response())
}

/// Delete a message (soft-delete with cryptographic erasure).
/// Validates sender ownership or MANAGE_MESSAGES permission.
/// Called by WebSocket dispatch (section-09).
pub async fn handle_delete_message(
    db: &sqlx::PgPool,
    user_id: UserId,
    user_permissions: Permissions,
    channel_id: ChannelId,
    message_id: MessageId,
) -> Result<(ChannelId, MessageId), ServerError> {
    // Fetch message to check ownership and channel association
    let msg = sqlx::query_as::<_, MessageOwnerRow>(
        "SELECT id, channel_id, sender_id FROM messages \
         WHERE id = $1 AND deleted = false",
    )
    .bind(message_id)
    .fetch_optional(db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    // Verify message belongs to the expected channel
    if msg.channel_id != Some(channel_id) {
        return Err(ServerError(OpenConvError::NotFound));
    }

    // Authorization: sender or MANAGE_MESSAGES
    if msg.sender_id != user_id && !user_permissions.contains(Permissions::MANAGE_MESSAGES) {
        return Err(ServerError(OpenConvError::Forbidden));
    }

    // Soft-delete with cryptographic erasure: zero out content
    let empty: Vec<u8> = Vec::new();
    sqlx::query(
        "UPDATE messages SET deleted = true, encrypted_content = $1, nonce = $2 WHERE id = $3",
    )
    .bind(&empty)
    .bind(&empty)
    .bind(message_id)
    .execute(db)
    .await
    .map_err(db_err)?;

    Ok((channel_id, message_id))
}

// ─── Route builder ──────────────────────────────────────────

/// Routes for guild channel messages.
/// Mounted at /api/channels/:channel_id/messages by section-13 router.
pub fn guild_message_routes() -> axum::Router<AppState> {
    axum::Router::new().route("/", axum::routing::get(guild_messages))
}

// ─── Internal row types ─────────────────────────────────────

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: MessageId,
    channel_id: ChannelId,
    sender_id: UserId,
    encrypted_content: Vec<u8>,
    nonce: Vec<u8>,
    edited_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl MessageRow {
    fn into_response(self) -> MessageResponse {
        MessageResponse {
            id: self.id,
            channel_id: self.channel_id,
            dm_channel_id: None,
            sender_id: self.sender_id,
            encrypted_content: self.encrypted_content,
            nonce: self.nonce,
            edited_at: self.edited_at,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MessageOwnerRow {
    #[allow(dead_code)]
    id: MessageId,
    channel_id: Option<ChannelId>,
    sender_id: UserId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_roundtrip() {
        let now = chrono::Utc::now();
        let id = MessageId::new();
        let encoded = encode_cursor(now, id);
        let decoded = decode_cursor(&encoded).unwrap();
        assert_eq!(decoded.id, id);
    }

    #[test]
    fn cursor_roundtrip_preserves_timestamp_micros() {
        let now = chrono::Utc::now();
        let id = MessageId::new();
        let encoded = encode_cursor(now, id);
        let decoded = decode_cursor(&encoded).unwrap();
        assert_eq!(
            decoded.created_at.timestamp_micros(),
            now.timestamp_micros()
        );
    }

    #[test]
    fn invalid_cursor_returns_error() {
        assert!(decode_cursor("not-base64!@#$").is_err());
        assert!(decode_cursor("").is_err());
    }

    #[test]
    fn cursor_with_bad_format_returns_error() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("nodivider");
        assert!(decode_cursor(&encoded).is_err());
    }

    #[test]
    fn cursor_with_bad_timestamp_returns_error() {
        let encoded = base64::engine::general_purpose::STANDARD
            .encode("not_a_number|00000000-0000-0000-0000-000000000000");
        assert!(decode_cursor(&encoded).is_err());
    }

    #[test]
    fn cursor_with_bad_uuid_returns_error() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("12345|not-a-uuid");
        assert!(decode_cursor(&encoded).is_err());
    }

    #[test]
    fn guild_message_routes_build_without_panic() {
        let _ = guild_message_routes();
    }
}
