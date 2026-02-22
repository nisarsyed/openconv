use std::collections::HashSet;
use std::sync::LazyLock;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use openconv_shared::api::channel::{
    ChannelResponse, CreateChannelRequest, ReorderChannelsRequest, UpdateChannelRequest,
};
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::{ChannelId, GuildId};
use openconv_shared::permissions::Permissions;
use regex::Regex;

use crate::error::ServerError;
use crate::extractors::channel_member::ChannelMember;
use crate::extractors::guild_member::GuildMember;
use crate::state::AppState;

const ALLOWED_CHANNEL_TYPES: &[&str] = &["text"];
const MAX_TOPIC_LENGTH: usize = 1024;

fn db_err(e: sqlx::Error) -> ServerError {
    tracing::error!(error = %e, "database error");
    ServerError(OpenConvError::Internal("database error".into()))
}

static CHANNEL_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9]([a-z0-9-]*[a-z0-9])?$").unwrap());

fn validate_channel_name(name: &str) -> Result<(), ServerError> {
    if name.is_empty() || name.len() > 100 {
        return Err(ServerError(OpenConvError::Validation(
            "Channel name must be 1-100 characters".into(),
        )));
    }
    if !CHANNEL_NAME_RE.is_match(name) {
        return Err(ServerError(OpenConvError::Validation(
            "Channel name must be lowercase alphanumeric with hyphens, no leading/trailing hyphens"
                .into(),
        )));
    }
    Ok(())
}

#[utoipa::path(post, path = "/api/guilds/{guild_id}/channels", tag = "Channels", security(("bearer_auth" = [])), params(("guild_id" = openconv_shared::ids::GuildId, Path, description = "Guild ID")), request_body = openconv_shared::api::channel::CreateChannelRequest, responses((status = 201, body = openconv_shared::api::channel::ChannelResponse), (status = 400, body = crate::error::ErrorResponse), (status = 403, body = crate::error::ErrorResponse)))]
/// Create a new channel in a guild.
pub async fn create_channel(
    State(state): State<AppState>,
    guild_member: GuildMember,
    Path(guild_id): Path<GuildId>,
    Json(body): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<ChannelResponse>), ServerError> {
    guild_member.require(Permissions::MANAGE_CHANNELS)?;

    validate_channel_name(&body.name)?;

    if !ALLOWED_CHANNEL_TYPES.contains(&body.channel_type.as_str()) {
        return Err(ServerError(OpenConvError::Validation(format!(
            "Invalid channel type. Allowed: {}",
            ALLOWED_CHANNEL_TYPES.join(", ")
        ))));
    }

    // Atomic INSERT with position calculation in a single statement
    let row = sqlx::query_as::<_, ChannelRow>(
        "INSERT INTO channels (id, guild_id, name, channel_type, position) \
         VALUES ($1, $2, $3, $4, COALESCE((SELECT MAX(position) + 1 FROM channels WHERE guild_id = $2), 0)) \
         RETURNING id, guild_id, name, channel_type, position, topic",
    )
    .bind(ChannelId::new())
    .bind(guild_id)
    .bind(&body.name)
    .bind(&body.channel_type)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            ServerError(OpenConvError::Conflict(
                "A channel with that name already exists in this guild".into(),
            ))
        } else {
            db_err(e)
        }
    })?;

    Ok((StatusCode::CREATED, Json(row.into_response())))
}

#[utoipa::path(get, path = "/api/guilds/{guild_id}/channels", tag = "Channels", security(("bearer_auth" = [])), params(("guild_id" = openconv_shared::ids::GuildId, Path, description = "Guild ID")), responses((status = 200, body = Vec<openconv_shared::api::channel::ChannelResponse>)))]
/// List all channels in a guild, ordered by position.
pub async fn list_channels(
    State(state): State<AppState>,
    _guild_member: GuildMember,
    Path(guild_id): Path<GuildId>,
) -> Result<Json<Vec<ChannelResponse>>, ServerError> {
    let rows = sqlx::query_as::<_, ChannelRow>(
        "SELECT id, guild_id, name, channel_type, position, topic \
         FROM channels WHERE guild_id = $1 ORDER BY position ASC",
    )
    .bind(guild_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    Ok(Json(rows.into_iter().map(|r| r.into_response()).collect()))
}

#[utoipa::path(get, path = "/api/channels/{channel_id}", tag = "Channels", security(("bearer_auth" = [])), params(("channel_id" = openconv_shared::ids::ChannelId, Path, description = "Channel ID")), responses((status = 200, body = openconv_shared::api::channel::ChannelResponse), (status = 404, body = crate::error::ErrorResponse)))]
/// Get a single channel by ID.
pub async fn get_channel(
    State(state): State<AppState>,
    channel_member: ChannelMember,
    Path(_channel_id): Path<ChannelId>,
) -> Result<Json<ChannelResponse>, ServerError> {
    let row = sqlx::query_as::<_, ChannelRow>(
        "SELECT id, guild_id, name, channel_type, position, topic \
         FROM channels WHERE id = $1",
    )
    .bind(channel_member.channel_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    Ok(Json(row.into_response()))
}

#[utoipa::path(patch, path = "/api/channels/{channel_id}", tag = "Channels", security(("bearer_auth" = [])), params(("channel_id" = openconv_shared::ids::ChannelId, Path, description = "Channel ID")), request_body = openconv_shared::api::channel::UpdateChannelRequest, responses((status = 200, body = openconv_shared::api::channel::ChannelResponse), (status = 400, body = crate::error::ErrorResponse), (status = 403, body = crate::error::ErrorResponse)))]
/// Update a channel's name and/or topic.
pub async fn update_channel(
    State(state): State<AppState>,
    channel_member: ChannelMember,
    Path(_channel_id): Path<ChannelId>,
    Json(body): Json<UpdateChannelRequest>,
) -> Result<Json<ChannelResponse>, ServerError> {
    channel_member.require(Permissions::MANAGE_CHANNELS)?;

    if body.name.is_none() && body.topic.is_none() {
        return Err(ServerError(OpenConvError::Validation(
            "At least one field must be provided".into(),
        )));
    }

    if let Some(ref name) = body.name {
        validate_channel_name(name)?;
    }

    if let Some(ref topic) = body.topic {
        if topic.len() > MAX_TOPIC_LENGTH {
            return Err(ServerError(OpenConvError::Validation(format!(
                "Topic must be at most {MAX_TOPIC_LENGTH} characters"
            ))));
        }
    }

    // Build dynamic update query
    let mut set_clauses = Vec::new();
    let mut param_idx = 2u32; // $1 is channel_id

    if body.name.is_some() {
        set_clauses.push(format!("name = ${param_idx}"));
        param_idx += 1;
    }
    if body.topic.is_some() {
        set_clauses.push(format!("topic = ${param_idx}"));
    }

    let query_str = format!(
        "UPDATE channels SET {} WHERE id = $1 \
         RETURNING id, guild_id, name, channel_type, position, topic",
        set_clauses.join(", ")
    );

    let mut query = sqlx::query_as::<_, ChannelRow>(&query_str).bind(channel_member.channel_id);

    if let Some(ref name) = body.name {
        query = query.bind(name.as_str());
    }
    if let Some(ref topic) = body.topic {
        query = query.bind(topic.as_str());
    }

    let row = query
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                ServerError(OpenConvError::Conflict(
                    "A channel with that name already exists in this guild".into(),
                ))
            } else {
                db_err(e)
            }
        })?
        .ok_or(ServerError(OpenConvError::NotFound))?;

    Ok(Json(row.into_response()))
}

#[utoipa::path(delete, path = "/api/channels/{channel_id}", tag = "Channels", security(("bearer_auth" = [])), params(("channel_id" = openconv_shared::ids::ChannelId, Path, description = "Channel ID")), responses((status = 200), (status = 400, body = crate::error::ErrorResponse), (status = 403, body = crate::error::ErrorResponse)))]
/// Delete a channel. Cannot delete the last channel in a guild.
/// Uses SELECT FOR UPDATE within a transaction for true atomicity.
pub async fn delete_channel(
    State(state): State<AppState>,
    channel_member: ChannelMember,
    Path(_channel_id): Path<ChannelId>,
) -> Result<StatusCode, ServerError> {
    channel_member.require(Permissions::MANAGE_CHANNELS)?;

    let mut tx = state.db.begin().await.map_err(db_err)?;

    // Lock all channels in this guild to prevent concurrent deletes
    let locked_ids: Vec<ChannelId> =
        sqlx::query_scalar("SELECT id FROM channels WHERE guild_id = $1 FOR UPDATE")
            .bind(channel_member.guild_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(db_err)?;

    if locked_ids.len() <= 1 {
        return Err(ServerError(OpenConvError::Validation(
            "Cannot delete the last channel in a guild".into(),
        )));
    }

    sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(channel_member.channel_id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

    tx.commit().await.map_err(db_err)?;

    Ok(StatusCode::OK)
}

#[utoipa::path(patch, path = "/api/guilds/{guild_id}/channels/reorder", tag = "Channels", security(("bearer_auth" = [])), params(("guild_id" = openconv_shared::ids::GuildId, Path, description = "Guild ID")), request_body = openconv_shared::api::channel::ReorderChannelsRequest, responses((status = 204), (status = 400, body = crate::error::ErrorResponse), (status = 403, body = crate::error::ErrorResponse)))]
/// Reorder channels within a guild.
pub async fn reorder_channels(
    State(state): State<AppState>,
    guild_member: GuildMember,
    Path(guild_id): Path<GuildId>,
    Json(body): Json<ReorderChannelsRequest>,
) -> Result<StatusCode, ServerError> {
    guild_member.require(Permissions::MANAGE_CHANNELS)?;

    if body.channels.is_empty() {
        return Err(ServerError(OpenConvError::Validation(
            "Channels list cannot be empty".into(),
        )));
    }

    // Check for duplicate channel_ids
    let channel_ids: Vec<ChannelId> = body.channels.iter().map(|c| c.channel_id).collect();
    let unique_ids: HashSet<ChannelId> = channel_ids.iter().copied().collect();
    if unique_ids.len() != channel_ids.len() {
        return Err(ServerError(OpenConvError::Validation(
            "Duplicate channel IDs in reorder request".into(),
        )));
    }

    // All validation and updates in a single transaction
    let mut tx = state.db.begin().await.map_err(db_err)?;

    // Validate all channel_ids belong to this guild (inside transaction)
    let valid_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM channels WHERE guild_id = $1 AND id = ANY($2)")
            .bind(guild_id)
            .bind(&channel_ids)
            .fetch_one(&mut *tx)
            .await
            .map_err(db_err)?;

    if valid_count != channel_ids.len() as i64 {
        return Err(ServerError(OpenConvError::Validation(
            "All channel IDs must belong to the specified guild".into(),
        )));
    }

    // Update positions
    for entry in &body.channels {
        sqlx::query("UPDATE channels SET position = $1 WHERE id = $2")
            .bind(entry.position)
            .bind(entry.channel_id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
    }

    tx.commit().await.map_err(db_err)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Route builder for channel endpoints.
pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/", axum::routing::post(create_channel).get(list_channels))
        .route("/reorder", axum::routing::patch(reorder_channels))
}

/// Route builder for channel detail endpoints (keyed by channel_id).
pub fn detail_routes() -> axum::Router<AppState> {
    axum::Router::new().route(
        "/{channel_id}",
        axum::routing::get(get_channel)
            .patch(update_channel)
            .delete(delete_channel),
    )
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(ref db_err) = e {
        db_err.code().as_deref() == Some("23505")
    } else {
        false
    }
}

#[derive(sqlx::FromRow)]
struct ChannelRow {
    id: ChannelId,
    guild_id: GuildId,
    name: String,
    channel_type: String,
    position: i32,
    topic: Option<String>,
}

impl ChannelRow {
    fn into_response(self) -> ChannelResponse {
        ChannelResponse {
            id: self.id,
            guild_id: self.guild_id,
            name: self.name,
            channel_type: self.channel_type,
            position: self.position,
            topic: self.topic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_channel_names() {
        assert!(validate_channel_name("general").is_ok());
        assert!(validate_channel_name("dev-chat").is_ok());
        assert!(validate_channel_name("a").is_ok());
        assert!(validate_channel_name("a1b2c3").is_ok());
        assert!(validate_channel_name("ab").is_ok());
        assert!(validate_channel_name("a-b").is_ok());
    }

    #[test]
    fn invalid_channel_names() {
        assert!(validate_channel_name("General").is_err()); // uppercase
        assert!(validate_channel_name("--general--").is_err()); // leading/trailing hyphens
        assert!(validate_channel_name("").is_err()); // empty
        assert!(validate_channel_name("-").is_err()); // single hyphen
        assert!(validate_channel_name("a-").is_err()); // trailing hyphen
        assert!(validate_channel_name("-a").is_err()); // leading hyphen
    }

    #[test]
    fn channel_name_length_validation() {
        let long_name = "a".repeat(100);
        assert!(validate_channel_name(&long_name).is_ok());

        let too_long = "a".repeat(101);
        assert!(validate_channel_name(&too_long).is_err());
    }

    #[test]
    fn routes_builds_without_panic() {
        let _ = routes();
        let _ = detail_routes();
    }
}
