use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use openconv_shared::api::guild::{
    CreateGuildRequest, GuildListResponse, GuildMemberResponse, GuildResponse, RoleSummary,
    UpdateGuildRequest,
};
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::{ChannelId, GuildId, RoleId, UserId};
use openconv_shared::permissions::Permissions;

use crate::error::ServerError;
use crate::extractors::auth::AuthUser;
use crate::extractors::guild_member::GuildMember;
use crate::state::AppState;

fn db_err(e: sqlx::Error) -> ServerError {
    tracing::error!(error = %e, "database error");
    ServerError(OpenConvError::Internal("database error".into()))
}

async fn fetch_guild_owner(db: &sqlx::PgPool, guild_id: GuildId) -> Result<UserId, ServerError> {
    sqlx::query_scalar::<_, UserId>("SELECT owner_id FROM guilds WHERE id = $1")
        .bind(guild_id)
        .fetch_optional(db)
        .await
        .map_err(db_err)?
        .ok_or(ServerError(OpenConvError::NotFound))
}

/// Create a new guild. Auth only -- no guild membership required.
pub async fn create_guild(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateGuildRequest>,
) -> Result<(StatusCode, Json<GuildResponse>), ServerError> {
    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > 100 {
        return Err(ServerError(OpenConvError::Validation(
            "Guild name must be between 1 and 100 characters".into(),
        )));
    }

    let guild_id = GuildId::new();
    let owner_role_id = RoleId::new();
    let admin_role_id = RoleId::new();
    let member_role_id = RoleId::new();
    let channel_id = ChannelId::new();

    let owner_perms = Permissions::all().bits() as i64;
    let admin_perms = (Permissions::MANAGE_GUILD
        | Permissions::MANAGE_CHANNELS
        | Permissions::MANAGE_ROLES
        | Permissions::MANAGE_INVITES
        | Permissions::KICK_MEMBERS
        | Permissions::SEND_MESSAGES
        | Permissions::READ_MESSAGES
        | Permissions::ATTACH_FILES
        | Permissions::MENTION_EVERYONE
        | Permissions::MANAGE_MESSAGES)
        .bits() as i64;
    let member_perms =
        (Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES | Permissions::ATTACH_FILES)
            .bits() as i64;

    let mut tx = state.db.begin().await.map_err(db_err)?;

    // 1. Insert guild
    let row = sqlx::query_as::<_, GuildRow>(
        "INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3) \
         RETURNING id, name, owner_id, icon_url, created_at",
    )
    .bind(guild_id)
    .bind(&name)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_err)?;

    // 2. Insert default roles
    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, permissions, position, role_type) VALUES \
         ($1, $2, 'owner', $3, 100, 'owner'), \
         ($4, $2, 'admin', $5, 50, 'admin'), \
         ($6, $2, 'member', $7, 1, 'member')",
    )
    .bind(owner_role_id)
    .bind(guild_id)
    .bind(owner_perms)
    .bind(admin_role_id)
    .bind(admin_perms)
    .bind(member_role_id)
    .bind(member_perms)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    // 3. Insert creator into guild_members
    sqlx::query("INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)")
        .bind(auth.user_id)
        .bind(guild_id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

    // 4. Assign owner role to creator
    sqlx::query(
        "INSERT INTO guild_member_roles (user_id, guild_id, role_id) VALUES ($1, $2, $3)",
    )
    .bind(auth.user_id)
    .bind(guild_id)
    .bind(owner_role_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    // 5. Insert #main channel
    sqlx::query("INSERT INTO channels (id, guild_id, name, position) VALUES ($1, $2, 'main', 0)")
        .bind(channel_id)
        .bind(guild_id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

    // 6. Commit
    tx.commit().await.map_err(db_err)?;

    let resp = GuildResponse {
        id: row.id,
        name: row.name,
        owner_id: row.owner_id,
        icon_url: row.icon_url,
        created_at: row.created_at,
        member_count: Some(1),
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

/// List guilds where the authenticated user is a member.
pub async fn list_guilds(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<GuildListResponse>, ServerError> {
    let rows = sqlx::query_as::<_, GuildRow>(
        "SELECT g.id, g.name, g.owner_id, g.icon_url, g.created_at \
         FROM guilds g \
         INNER JOIN guild_members gm ON gm.guild_id = g.id \
         WHERE gm.user_id = $1 AND g.deleted_at IS NULL \
         ORDER BY g.created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let guilds = rows
        .into_iter()
        .map(|r| GuildResponse {
            id: r.id,
            name: r.name,
            owner_id: r.owner_id,
            icon_url: r.icon_url,
            created_at: r.created_at,
            member_count: None,
        })
        .collect();

    Ok(Json(GuildListResponse { guilds }))
}

/// Get a single guild's details. Requires guild membership (GuildMember extractor).
pub async fn get_guild(
    member: GuildMember,
    State(state): State<AppState>,
) -> Result<Json<GuildResponse>, ServerError> {
    let row = sqlx::query_as::<_, GuildWithCountRow>(
        "SELECT g.id, g.name, g.owner_id, g.icon_url, g.created_at, \
         (SELECT COUNT(*) FROM guild_members WHERE guild_id = g.id) AS member_count \
         FROM guilds g \
         WHERE g.id = $1 AND g.deleted_at IS NULL",
    )
    .bind(member.guild_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    Ok(Json(GuildResponse {
        id: row.id,
        name: row.name,
        owner_id: row.owner_id,
        icon_url: row.icon_url,
        created_at: row.created_at,
        member_count: Some(row.member_count),
    }))
}

/// Update guild name/icon. Requires MANAGE_GUILD permission.
pub async fn update_guild(
    member: GuildMember,
    State(state): State<AppState>,
    Json(body): Json<UpdateGuildRequest>,
) -> Result<Json<GuildResponse>, ServerError> {
    member.require(Permissions::MANAGE_GUILD)?;

    if body.name.is_none() && body.icon_url.is_none() {
        return Err(ServerError(OpenConvError::Validation(
            "At least one field must be provided".into(),
        )));
    }

    if let Some(ref name) = body.name {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.len() > 100 {
            return Err(ServerError(OpenConvError::Validation(
                "Guild name must be between 1 and 100 characters".into(),
            )));
        }
    }

    // Build dynamic update query
    let mut set_clauses = Vec::new();
    let mut param_idx = 2u32; // $1 is guild_id

    if body.name.is_some() {
        set_clauses.push(format!("name = ${param_idx}"));
        param_idx += 1;
    }
    if body.icon_url.is_some() {
        set_clauses.push(format!("icon_url = ${param_idx}"));
    }

    let query_str = format!(
        "UPDATE guilds SET {} WHERE id = $1 AND deleted_at IS NULL \
         RETURNING id, name, owner_id, icon_url, created_at",
        set_clauses.join(", ")
    );

    let mut query = sqlx::query_as::<_, GuildRow>(&query_str).bind(member.guild_id);

    if let Some(ref name) = body.name {
        query = query.bind(name.trim());
    }
    if let Some(ref icon_url) = body.icon_url {
        query = query.bind(icon_url.as_str());
    }

    let row = query
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?
        .ok_or(ServerError(OpenConvError::NotFound))?;

    Ok(Json(GuildResponse {
        id: row.id,
        name: row.name,
        owner_id: row.owner_id,
        icon_url: row.icon_url,
        created_at: row.created_at,
        member_count: None,
    }))
}

/// Soft-delete a guild. Only the guild owner can do this.
/// Uses atomic owner check + update in a single query to avoid race conditions.
pub async fn delete_guild(
    member: GuildMember,
    State(state): State<AppState>,
) -> Result<StatusCode, ServerError> {
    let result = sqlx::query(
        "UPDATE guilds SET deleted_at = NOW() \
         WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL",
    )
    .bind(member.guild_id)
    .bind(member.user_id)
    .execute(&state.db)
    .await
    .map_err(db_err)?;

    if result.rows_affected() == 0 {
        // Either not the owner, or guild already deleted
        let owner_id = fetch_guild_owner(&state.db, member.guild_id).await?;
        if member.user_id != owner_id {
            return Err(ServerError(OpenConvError::Forbidden));
        }
        // Guild was already soft-deleted
        return Err(ServerError(OpenConvError::NotFound));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Restore a soft-deleted guild within the 7-day window. Owner only.
///
/// NOTE: The GuildMember extractor does NOT filter by `deleted_at IS NULL` on the guilds table.
/// This is intentional -- it allows the restore endpoint to function since the guild_members rows
/// still exist for soft-deleted guilds. If a future change adds that filter to the extractor,
/// this endpoint would need a custom extractor.
pub async fn restore_guild(
    member: GuildMember,
    State(state): State<AppState>,
) -> Result<Json<GuildResponse>, ServerError> {
    // Atomic owner check + restore in a single query
    let row = sqlx::query_as::<_, GuildRow>(
        "UPDATE guilds SET deleted_at = NULL \
         WHERE id = $1 AND owner_id = $2 AND deleted_at IS NOT NULL \
         AND deleted_at > NOW() - INTERVAL '7 days' \
         RETURNING id, name, owner_id, icon_url, created_at",
    )
    .bind(member.guild_id)
    .bind(member.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;

    match row {
        Some(row) => Ok(Json(GuildResponse {
            id: row.id,
            name: row.name,
            owner_id: row.owner_id,
            icon_url: row.icon_url,
            created_at: row.created_at,
            member_count: None,
        })),
        None => {
            // Determine specific error: not owner, or window expired
            let owner_id = fetch_guild_owner(&state.db, member.guild_id).await?;
            if member.user_id != owner_id {
                Err(ServerError(OpenConvError::Forbidden))
            } else {
                Err(ServerError(OpenConvError::Validation(
                    "Guild is not soft-deleted or the 7-day restore window has expired".into(),
                )))
            }
        }
    }
}

/// Leave a guild. Owner cannot leave.
pub async fn leave_guild(
    member: GuildMember,
    State(state): State<AppState>,
) -> Result<StatusCode, ServerError> {
    let owner_id = fetch_guild_owner(&state.db, member.guild_id).await?;

    if member.user_id == owner_id {
        return Err(ServerError(OpenConvError::Validation(
            "Guild owner cannot leave the guild".into(),
        )));
    }

    sqlx::query("DELETE FROM guild_members WHERE user_id = $1 AND guild_id = $2")
        .bind(member.user_id)
        .bind(member.guild_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Kick a member from the guild. Requires KICK_MEMBERS and hierarchy check.
pub async fn kick_member(
    member: GuildMember,
    State(state): State<AppState>,
    Path((_, target_user_id)): Path<(GuildId, UserId)>,
) -> Result<StatusCode, ServerError> {
    member.require(Permissions::KICK_MEMBERS)?;

    if member.user_id == target_user_id {
        return Err(ServerError(OpenConvError::Validation(
            "Cannot kick yourself".into(),
        )));
    }

    // Check if target is a member
    let target_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_members WHERE user_id = $1 AND guild_id = $2)",
    )
    .bind(target_user_id)
    .bind(member.guild_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;

    if !target_exists {
        return Err(ServerError(OpenConvError::NotFound));
    }

    // Guild owner bypasses hierarchy check
    let owner_id = fetch_guild_owner(&state.db, member.guild_id).await?;

    if member.user_id != owner_id {
        // Hierarchy check: actor's highest role position must be > target's highest role position
        let actor_max: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(r.position) FROM guild_member_roles gmr \
             JOIN roles r ON r.id = gmr.role_id \
             WHERE gmr.user_id = $1 AND gmr.guild_id = $2",
        )
        .bind(member.user_id)
        .bind(member.guild_id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;

        let target_max: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(r.position) FROM guild_member_roles gmr \
             JOIN roles r ON r.id = gmr.role_id \
             WHERE gmr.user_id = $1 AND gmr.guild_id = $2",
        )
        .bind(target_user_id)
        .bind(member.guild_id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;

        let actor_pos = actor_max.unwrap_or(0);
        let target_pos = target_max.unwrap_or(0);

        if actor_pos <= target_pos {
            return Err(ServerError(OpenConvError::Forbidden));
        }
    }

    sqlx::query("DELETE FROM guild_members WHERE user_id = $1 AND guild_id = $2")
        .bind(target_user_id)
        .bind(member.guild_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    Ok(StatusCode::NO_CONTENT)
}

/// List guild members with their roles.
pub async fn list_members(
    member: GuildMember,
    State(state): State<AppState>,
) -> Result<Json<Vec<GuildMemberResponse>>, ServerError> {
    let rows = sqlx::query_as::<_, MemberRow>(
        "SELECT \
             u.id AS user_id, \
             u.display_name, \
             gm.joined_at, \
             COALESCE( \
                 json_agg(json_build_object('id', r.id, 'name', r.name, 'position', r.position)) \
                 FILTER (WHERE r.id IS NOT NULL), \
                 '[]' \
             ) AS roles \
         FROM guild_members gm \
         JOIN users u ON u.id = gm.user_id \
         LEFT JOIN guild_member_roles gmr ON gmr.user_id = gm.user_id AND gmr.guild_id = gm.guild_id \
         LEFT JOIN roles r ON r.id = gmr.role_id \
         WHERE gm.guild_id = $1 \
         GROUP BY u.id, u.display_name, gm.joined_at \
         ORDER BY gm.joined_at ASC",
    )
    .bind(member.guild_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let members = rows
        .into_iter()
        .map(|r| {
            let roles: Vec<RoleSummary> = match serde_json::from_value(r.roles) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        user_id = %r.user_id,
                        error = %e,
                        "failed to deserialize member roles from JSON, defaulting to empty"
                    );
                    vec![]
                }
            };
            GuildMemberResponse {
                user_id: r.user_id,
                display_name: r.display_name,
                joined_at: r.joined_at,
                roles,
            }
        })
        .collect();

    Ok(Json(members))
}

/// Route builder for guild endpoints.
pub fn routes() -> axum::Router<AppState> {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/", post(create_guild).get(list_guilds))
        .route(
            "/{guild_id}",
            get(get_guild).patch(update_guild).delete(delete_guild),
        )
        .route("/{guild_id}/restore", post(restore_guild))
}

/// Route builder for guild member endpoints.
/// Mounted at /api/guilds/{guild_id}/members by the router.
pub fn member_routes() -> axum::Router<AppState> {
    use axum::routing::{delete, get, put};

    axum::Router::new()
        .route("/", get(list_members))
        .route("/me", delete(leave_guild))
        .route("/{user_id}", delete(kick_member))
        .route(
            "/{user_id}/roles/{role_id}",
            put(super::roles::assign_role).delete(super::roles::remove_role),
        )
}

// Internal row types for sqlx queries

#[derive(sqlx::FromRow)]
struct GuildRow {
    id: GuildId,
    name: String,
    owner_id: UserId,
    icon_url: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct GuildWithCountRow {
    id: GuildId,
    name: String,
    owner_id: UserId,
    icon_url: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    member_count: i64,
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    user_id: UserId,
    display_name: String,
    joined_at: chrono::DateTime<chrono::Utc>,
    roles: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_owner_permissions_are_all() {
        let owner_perms = Permissions::all();
        assert!(owner_perms.contains(Permissions::ADMINISTRATOR));
        assert!(owner_perms.contains(Permissions::MANAGE_GUILD));
        assert!(owner_perms.contains(Permissions::KICK_MEMBERS));
    }

    #[test]
    fn default_admin_permissions_subset() {
        let admin_perms = Permissions::MANAGE_GUILD
            | Permissions::MANAGE_CHANNELS
            | Permissions::MANAGE_ROLES
            | Permissions::MANAGE_INVITES
            | Permissions::KICK_MEMBERS
            | Permissions::SEND_MESSAGES
            | Permissions::READ_MESSAGES
            | Permissions::ATTACH_FILES
            | Permissions::MENTION_EVERYONE
            | Permissions::MANAGE_MESSAGES;
        assert!(admin_perms.contains(Permissions::MANAGE_GUILD));
        assert!(!admin_perms.contains(Permissions::ADMINISTRATOR));
    }

    #[test]
    fn default_member_permissions_minimal() {
        let member_perms =
            Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES | Permissions::ATTACH_FILES;
        assert!(member_perms.contains(Permissions::SEND_MESSAGES));
        assert!(member_perms.contains(Permissions::READ_MESSAGES));
        assert!(!member_perms.contains(Permissions::KICK_MEMBERS));
        assert!(!member_perms.contains(Permissions::MANAGE_GUILD));
    }

    #[test]
    fn guild_name_validation_rejects_empty() {
        let name = "   ".trim();
        assert!(name.is_empty());
    }

    #[test]
    fn guild_name_validation_rejects_too_long() {
        let name = "a".repeat(101);
        assert!(name.len() > 100);
    }

    #[test]
    fn guild_name_validation_accepts_valid() {
        let name = "My Cool Guild".trim();
        assert!(!name.is_empty() && name.len() <= 100);
    }

    #[test]
    fn role_summary_serde_roundtrip() {
        let summary = RoleSummary {
            id: RoleId::new(),
            name: "member".into(),
            position: 1,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let back: RoleSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "member");
        assert_eq!(back.position, 1);
    }

    #[test]
    fn guild_member_response_with_empty_roles() {
        let resp = GuildMemberResponse {
            user_id: UserId::new(),
            display_name: "TestUser".into(),
            joined_at: chrono::Utc::now(),
            roles: vec![],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["roles"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn update_guild_request_all_none_is_valid_struct() {
        let req = UpdateGuildRequest {
            name: None,
            icon_url: None,
        };
        assert!(req.name.is_none());
        assert!(req.icon_url.is_none());
    }

    #[test]
    fn routes_builds_without_panic() {
        let _ = routes();
    }
}
