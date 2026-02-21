use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use openconv_shared::api::role::{CreateRoleRequest, RoleResponse, UpdateRoleRequest};
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::{GuildId, RoleId, UserId};
use openconv_shared::permissions::Permissions;

use crate::error::ServerError;
use crate::extractors::guild_member::GuildMember;
use crate::state::AppState;

fn db_err(e: sqlx::Error) -> ServerError {
    tracing::error!(error = %e, "database error");
    ServerError(OpenConvError::Internal("database error".into()))
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(ref db_err) = e {
        db_err.code().as_deref() == Some("23505")
    } else {
        false
    }
}

/// Get the actor's highest role position in the guild.
async fn actor_highest_position(
    db: &sqlx::PgPool,
    user_id: UserId,
    guild_id: GuildId,
) -> Result<i32, ServerError> {
    let pos: Option<i32> = sqlx::query_scalar(
        "SELECT MAX(r.position) FROM guild_member_roles gmr \
         JOIN roles r ON r.id = gmr.role_id \
         WHERE gmr.user_id = $1 AND gmr.guild_id = $2",
    )
    .bind(user_id)
    .bind(guild_id)
    .fetch_one(db)
    .await
    .map_err(db_err)?;
    Ok(pos.unwrap_or(0))
}

/// Check if the actor is the guild owner.
async fn is_guild_owner(
    db: &sqlx::PgPool,
    user_id: UserId,
    guild_id: GuildId,
) -> Result<bool, ServerError> {
    let owner_id: Option<UserId> =
        sqlx::query_scalar("SELECT owner_id FROM guilds WHERE id = $1")
            .bind(guild_id)
            .fetch_optional(db)
            .await
            .map_err(db_err)?;
    Ok(owner_id == Some(user_id))
}

/// Validates that the actor can modify the target role.
fn check_role_hierarchy(
    actor_highest_position: i32,
    target_role_position: i32,
    is_owner: bool,
) -> Result<(), ServerError> {
    if is_owner {
        return Ok(());
    }
    if actor_highest_position <= target_role_position {
        return Err(ServerError(OpenConvError::Forbidden));
    }
    Ok(())
}

/// Validates that new_permissions does not contain bits the actor lacks.
fn check_privilege_escalation(
    new_permissions: Permissions,
    actor_permissions: Permissions,
    is_owner: bool,
) -> Result<(), ServerError> {
    if is_owner {
        return Ok(());
    }
    let escalation = new_permissions & !actor_permissions;
    if !escalation.is_empty() {
        return Err(ServerError(OpenConvError::Forbidden));
    }
    Ok(())
}

/// Create a new custom role in the guild.
pub async fn create_role(
    State(state): State<AppState>,
    guild_member: GuildMember,
    Path(guild_id): Path<GuildId>,
    Json(body): Json<CreateRoleRequest>,
) -> Result<(StatusCode, Json<RoleResponse>), ServerError> {
    guild_member.require(Permissions::MANAGE_ROLES)?;

    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > 100 {
        return Err(ServerError(OpenConvError::Validation(
            "Role name must be between 1 and 100 characters".into(),
        )));
    }

    let new_perms = Permissions::from_bits_truncate(body.permissions);
    let is_owner = is_guild_owner(&state.db, guild_member.user_id, guild_id).await?;
    check_privilege_escalation(new_perms, guild_member.permissions, is_owner)?;

    let mut tx = state.db.begin().await.map_err(db_err)?;

    // Shift existing custom roles at position >= 2 up by 1.
    // Two-step approach to avoid UNIQUE constraint violation:
    // 1. Negate positions (guaranteed unique since all originals are positive)
    // 2. Set to -(negated) + 1 = original + 1
    sqlx::query(
        "UPDATE roles SET position = -position \
         WHERE guild_id = $1 AND position >= 2 AND role_type = 'custom'",
    )
    .bind(guild_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    sqlx::query(
        "UPDATE roles SET position = -position + 1 \
         WHERE guild_id = $1 AND position < 0",
    )
    .bind(guild_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    // Store only known permission bits (truncated value)
    let stored_perms = new_perms.bits() as i64;

    // Insert new role at position 2
    let row = sqlx::query_as::<_, RoleRow>(
        "INSERT INTO roles (id, guild_id, name, permissions, position, role_type) \
         VALUES ($1, $2, $3, $4, 2, 'custom') \
         RETURNING id, guild_id, name, permissions, position, role_type, created_at",
    )
    .bind(RoleId::new())
    .bind(guild_id)
    .bind(&name)
    .bind(stored_perms)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            ServerError(OpenConvError::Conflict(
                "A role with that position already exists".into(),
            ))
        } else {
            db_err(e)
        }
    })?;

    tx.commit().await.map_err(db_err)?;

    Ok((StatusCode::CREATED, Json(row.into_response())))
}

/// List all roles in the guild, ordered by position ascending.
pub async fn list_roles(
    State(state): State<AppState>,
    _guild_member: GuildMember,
    Path(guild_id): Path<GuildId>,
) -> Result<Json<Vec<RoleResponse>>, ServerError> {
    let rows = sqlx::query_as::<_, RoleRow>(
        "SELECT id, guild_id, name, permissions, position, role_type, created_at \
         FROM roles WHERE guild_id = $1 ORDER BY position ASC",
    )
    .bind(guild_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    Ok(Json(rows.into_iter().map(|r| r.into_response()).collect()))
}

/// Update a role's name, permissions, and/or position.
pub async fn update_role(
    State(state): State<AppState>,
    guild_member: GuildMember,
    Path((guild_id, role_id)): Path<(GuildId, RoleId)>,
    Json(body): Json<UpdateRoleRequest>,
) -> Result<Json<RoleResponse>, ServerError> {
    guild_member.require(Permissions::MANAGE_ROLES)?;

    if body.name.is_none() && body.permissions.is_none() && body.position.is_none() {
        return Err(ServerError(OpenConvError::Validation(
            "At least one field must be provided".into(),
        )));
    }

    if let Some(ref name) = body.name {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.len() > 100 {
            return Err(ServerError(OpenConvError::Validation(
                "Role name must be between 1 and 100 characters".into(),
            )));
        }
    }

    // Fetch the target role
    let target = sqlx::query_as::<_, RoleRow>(
        "SELECT id, guild_id, name, permissions, position, role_type, created_at \
         FROM roles WHERE id = $1 AND guild_id = $2",
    )
    .bind(role_id)
    .bind(guild_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    // Only custom roles can be updated
    if target.role_type != "custom" {
        return Err(ServerError(OpenConvError::Validation(
            "Built-in roles cannot be modified".into(),
        )));
    }

    // Hierarchy check
    let is_owner = is_guild_owner(&state.db, guild_member.user_id, guild_id).await?;
    let actor_pos = actor_highest_position(&state.db, guild_member.user_id, guild_id).await?;
    check_role_hierarchy(actor_pos, target.position, is_owner)?;

    // Privilege escalation check if updating permissions
    if let Some(new_perms_bits) = body.permissions {
        let new_perms = Permissions::from_bits_truncate(new_perms_bits);
        check_privilege_escalation(new_perms, guild_member.permissions, is_owner)?;
    }

    // Build dynamic update
    let mut set_clauses = Vec::new();
    let mut param_idx = 3u32; // $1 = role_id, $2 = guild_id

    if body.name.is_some() {
        set_clauses.push(format!("name = ${param_idx}"));
        param_idx += 1;
    }
    if body.permissions.is_some() {
        set_clauses.push(format!("permissions = ${param_idx}"));
        param_idx += 1;
    }
    if body.position.is_some() {
        set_clauses.push(format!("position = ${param_idx}"));
    }

    let query_str = format!(
        "UPDATE roles SET {} WHERE id = $1 AND guild_id = $2 \
         RETURNING id, guild_id, name, permissions, position, role_type, created_at",
        set_clauses.join(", ")
    );

    let mut query = sqlx::query_as::<_, RoleRow>(&query_str)
        .bind(role_id)
        .bind(guild_id);

    if let Some(ref name) = body.name {
        query = query.bind(name.trim());
    }
    if let Some(perms) = body.permissions {
        let truncated = Permissions::from_bits_truncate(perms).bits() as i64;
        query = query.bind(truncated);
    }
    if let Some(pos) = body.position {
        query = query.bind(pos);
    }

    let row = query
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                ServerError(OpenConvError::Conflict(
                    "A role with that position already exists in this guild".into(),
                ))
            } else {
                db_err(e)
            }
        })?
        .ok_or(ServerError(OpenConvError::NotFound))?;

    Ok(Json(row.into_response()))
}

/// Delete a custom role. Built-in roles cannot be deleted.
pub async fn delete_role(
    State(state): State<AppState>,
    guild_member: GuildMember,
    Path((guild_id, role_id)): Path<(GuildId, RoleId)>,
) -> Result<StatusCode, ServerError> {
    guild_member.require(Permissions::MANAGE_ROLES)?;

    // Fetch the target role
    let target = sqlx::query_as::<_, RoleRow>(
        "SELECT id, guild_id, name, permissions, position, role_type, created_at \
         FROM roles WHERE id = $1 AND guild_id = $2",
    )
    .bind(role_id)
    .bind(guild_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    if target.role_type != "custom" {
        return Err(ServerError(OpenConvError::Validation(
            "Built-in roles cannot be deleted".into(),
        )));
    }

    // Hierarchy check
    let is_owner = is_guild_owner(&state.db, guild_member.user_id, guild_id).await?;
    let actor_pos = actor_highest_position(&state.db, guild_member.user_id, guild_id).await?;
    check_role_hierarchy(actor_pos, target.position, is_owner)?;

    sqlx::query("DELETE FROM roles WHERE id = $1 AND guild_id = $2")
        .bind(role_id)
        .bind(guild_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Assign a role to a guild member.
pub async fn assign_role(
    State(state): State<AppState>,
    guild_member: GuildMember,
    Path((guild_id, user_id, role_id)): Path<(GuildId, UserId, RoleId)>,
) -> Result<StatusCode, ServerError> {
    guild_member.require(Permissions::MANAGE_ROLES)?;

    // Fetch the target role
    let target = sqlx::query_as::<_, RoleRow>(
        "SELECT id, guild_id, name, permissions, position, role_type, created_at \
         FROM roles WHERE id = $1 AND guild_id = $2",
    )
    .bind(role_id)
    .bind(guild_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    // Hierarchy check
    let is_owner = is_guild_owner(&state.db, guild_member.user_id, guild_id).await?;
    let actor_pos = actor_highest_position(&state.db, guild_member.user_id, guild_id).await?;
    check_role_hierarchy(actor_pos, target.position, is_owner)?;

    // Verify target user is a guild member
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_members WHERE user_id = $1 AND guild_id = $2)",
    )
    .bind(user_id)
    .bind(guild_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;

    if !is_member {
        return Err(ServerError(OpenConvError::NotFound));
    }

    sqlx::query(
        "INSERT INTO guild_member_roles (user_id, guild_id, role_id) \
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(guild_id)
    .bind(role_id)
    .execute(&state.db)
    .await
    .map_err(db_err)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Remove a role from a guild member.
pub async fn remove_role(
    State(state): State<AppState>,
    guild_member: GuildMember,
    Path((guild_id, user_id, role_id)): Path<(GuildId, UserId, RoleId)>,
) -> Result<StatusCode, ServerError> {
    guild_member.require(Permissions::MANAGE_ROLES)?;

    // Fetch the target role
    let target = sqlx::query_as::<_, RoleRow>(
        "SELECT id, guild_id, name, permissions, position, role_type, created_at \
         FROM roles WHERE id = $1 AND guild_id = $2",
    )
    .bind(role_id)
    .bind(guild_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    // Hierarchy check
    let is_owner = is_guild_owner(&state.db, guild_member.user_id, guild_id).await?;
    let actor_pos = actor_highest_position(&state.db, guild_member.user_id, guild_id).await?;
    check_role_hierarchy(actor_pos, target.position, is_owner)?;

    sqlx::query(
        "DELETE FROM guild_member_roles WHERE user_id = $1 AND guild_id = $2 AND role_id = $3",
    )
    .bind(user_id)
    .bind(guild_id)
    .bind(role_id)
    .execute(&state.db)
    .await
    .map_err(db_err)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Route builder for role CRUD endpoints.
pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/", axum::routing::post(create_role).get(list_roles))
        .route(
            "/{role_id}",
            axum::routing::patch(update_role).delete(delete_role),
        )
}

/// Route builder for role assignment endpoints (nested under members).
pub fn assignment_routes() -> axum::Router<AppState> {
    axum::Router::new().route(
        "/{user_id}/roles/{role_id}",
        axum::routing::put(assign_role).delete(remove_role),
    )
}

#[derive(sqlx::FromRow)]
struct RoleRow {
    id: RoleId,
    guild_id: GuildId,
    name: String,
    permissions: i64,
    position: i32,
    role_type: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl RoleRow {
    fn into_response(self) -> RoleResponse {
        RoleResponse {
            id: self.id,
            guild_id: self.guild_id,
            name: self.name,
            permissions: self.permissions as u64,
            position: self.position,
            role_type: self.role_type,
            created_at: self.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_check_allows_owner() {
        assert!(check_role_hierarchy(1, 100, true).is_ok());
    }

    #[test]
    fn hierarchy_check_blocks_lower_position() {
        assert!(check_role_hierarchy(50, 100, false).is_err());
    }

    #[test]
    fn hierarchy_check_blocks_equal_position() {
        assert!(check_role_hierarchy(50, 50, false).is_err());
    }

    #[test]
    fn hierarchy_check_allows_higher_position() {
        assert!(check_role_hierarchy(100, 50, false).is_ok());
    }

    #[test]
    fn privilege_escalation_allows_subset() {
        let actor = Permissions::MANAGE_ROLES | Permissions::SEND_MESSAGES;
        let new = Permissions::SEND_MESSAGES;
        assert!(check_privilege_escalation(new, actor, false).is_ok());
    }

    #[test]
    fn privilege_escalation_blocks_superset() {
        let actor = Permissions::SEND_MESSAGES;
        let new = Permissions::ADMINISTRATOR;
        assert!(check_privilege_escalation(new, actor, false).is_err());
    }

    #[test]
    fn privilege_escalation_allows_owner() {
        let actor = Permissions::SEND_MESSAGES;
        let new = Permissions::ADMINISTRATOR;
        assert!(check_privilege_escalation(new, actor, true).is_ok());
    }

    #[test]
    fn routes_builds_without_panic() {
        let _ = routes();
        let _ = assignment_routes();
    }
}
