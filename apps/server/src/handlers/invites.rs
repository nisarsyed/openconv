use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use openconv_shared::api::invite::{CreateInviteRequest, InviteInfoResponse, InviteResponse};
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::GuildId;
use openconv_shared::permissions::Permissions;
use rand::Rng;

use crate::error::ServerError;
use crate::extractors::auth::AuthUser;
use crate::extractors::guild_member::GuildMember;
use crate::state::AppState;

fn db_err(e: sqlx::Error) -> ServerError {
    tracing::error!(error = %e, "database error");
    ServerError(OpenConvError::Internal("database error".into()))
}

const BASE62_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn generate_invite_code() -> String {
    let mut rng = rand::rng();
    (0..8)
        .map(|_| BASE62_CHARS[rng.random_range(0..62)] as char)
        .collect()
}

/// POST /api/guilds/:guild_id/invites
/// Requires MANAGE_INVITES permission.
pub async fn create_invite(
    State(state): State<AppState>,
    guild_member: GuildMember,
    Path(guild_id): Path<GuildId>,
    Json(body): Json<CreateInviteRequest>,
) -> Result<(StatusCode, Json<InviteResponse>), ServerError> {
    guild_member.require(Permissions::MANAGE_INVITES)?;

    if let Some(max_uses) = body.max_uses {
        if max_uses < 1 {
            return Err(ServerError(OpenConvError::Validation(
                "max_uses must be at least 1".into(),
            )));
        }
    }

    // Retry up to 5 times in case of code collision
    for _ in 0..5 {
        let code = generate_invite_code();
        let result = sqlx::query_as::<_, InviteRow>(
            "INSERT INTO guild_invites (code, guild_id, inviter_id, max_uses, expires_at) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (code) DO NOTHING \
             RETURNING code, guild_id, inviter_id, max_uses, use_count, expires_at, created_at",
        )
        .bind(&code)
        .bind(guild_id)
        .bind(guild_member.user_id)
        .bind(body.max_uses)
        .bind(body.expires_at)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?;

        if let Some(row) = result {
            return Ok((StatusCode::CREATED, Json(row.into_response())));
        }
    }

    Err(ServerError(OpenConvError::Internal(
        "failed to generate unique invite code".into(),
    )))
}

/// GET /api/guilds/:guild_id/invites
/// Requires MANAGE_INVITES permission.
pub async fn list_invites(
    State(state): State<AppState>,
    guild_member: GuildMember,
    Path(guild_id): Path<GuildId>,
) -> Result<Json<Vec<InviteResponse>>, ServerError> {
    guild_member.require(Permissions::MANAGE_INVITES)?;

    let rows = sqlx::query_as::<_, InviteRow>(
        "SELECT code, guild_id, inviter_id, max_uses, use_count, expires_at, created_at \
         FROM guild_invites WHERE guild_id = $1 ORDER BY created_at DESC",
    )
    .bind(guild_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    Ok(Json(rows.into_iter().map(|r| r.into_response()).collect()))
}

/// DELETE /api/guilds/:guild_id/invites/:code
/// Requires MANAGE_INVITES permission.
pub async fn revoke_invite(
    State(state): State<AppState>,
    guild_member: GuildMember,
    Path((guild_id, code)): Path<(GuildId, String)>,
) -> Result<StatusCode, ServerError> {
    guild_member.require(Permissions::MANAGE_INVITES)?;

    let result = sqlx::query(
        "DELETE FROM guild_invites WHERE code = $1 AND guild_id = $2",
    )
    .bind(&code)
    .bind(guild_id)
    .execute(&state.db)
    .await
    .map_err(db_err)?;

    if result.rows_affected() == 0 {
        return Err(ServerError(OpenConvError::NotFound));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/invites/:code
/// Auth only -- any authenticated user can look up an invite.
pub async fn get_invite_info(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(code): Path<String>,
) -> Result<Json<InviteInfoResponse>, ServerError> {
    let row = sqlx::query_as::<_, InviteInfoRow>(
        "SELECT \
            gi.code, \
            g.name AS guild_name, \
            g.id AS guild_id, \
            (SELECT COUNT(*) FROM guild_members WHERE guild_id = g.id) AS member_count, \
            u.display_name AS inviter_display_name \
         FROM guild_invites gi \
         JOIN guilds g ON g.id = gi.guild_id AND g.deleted_at IS NULL \
         LEFT JOIN users u ON u.id = gi.inviter_id \
         WHERE gi.code = $1 \
           AND (gi.expires_at IS NULL OR gi.expires_at > NOW()) \
           AND (gi.max_uses IS NULL OR gi.use_count < gi.max_uses)",
    )
    .bind(&code)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    Ok(Json(InviteInfoResponse {
        code: row.code,
        guild_name: row.guild_name,
        guild_id: row.guild_id,
        member_count: row.member_count,
        inviter_display_name: row.inviter_display_name,
    }))
}

/// POST /api/invites/:code/accept
/// Auth only -- any authenticated user can accept an invite.
pub async fn accept_invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(code): Path<String>,
) -> Result<StatusCode, ServerError> {
    let mut tx = state.db.begin().await.map_err(db_err)?;

    // Step 1: Atomically validate and claim the invite
    let invite = sqlx::query_as::<_, InviteRow>(
        "UPDATE guild_invites \
         SET use_count = use_count + 1 \
         WHERE code = $1 \
           AND (max_uses IS NULL OR use_count < max_uses) \
           AND (expires_at IS NULL OR expires_at > NOW()) \
         RETURNING code, guild_id, inviter_id, max_uses, use_count, expires_at, created_at",
    )
    .bind(&code)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;

    let invite = match invite {
        Some(inv) => inv,
        None => {
            // Distinguish: does the invite exist at all?
            let exists: Option<bool> = sqlx::query_scalar(
                "SELECT true FROM guild_invites WHERE code = $1",
            )
            .bind(&code)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?;

            return if exists.is_some() {
                Err(ServerError(OpenConvError::Validation(
                    "invite is expired or at max uses".into(),
                )))
            } else {
                Err(ServerError(OpenConvError::NotFound))
            };
        }
    };

    // Step 2: Verify guild is not soft-deleted
    sqlx::query_scalar::<_, GuildId>(
        "SELECT id FROM guilds WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(invite.guild_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?
    .ok_or(ServerError(OpenConvError::NotFound))?;

    // Step 3: Check existing membership
    let existing: Option<bool> = sqlx::query_scalar(
        "SELECT true FROM guild_members WHERE user_id = $1 AND guild_id = $2",
    )
    .bind(auth.user_id)
    .bind(invite.guild_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;

    if existing.is_some() {
        return Err(ServerError(OpenConvError::Conflict(
            "already a member of this guild".into(),
        )));
    }

    // Step 4a: Add user to guild_members
    sqlx::query(
        "INSERT INTO guild_members (user_id, guild_id) VALUES ($1, $2)",
    )
    .bind(auth.user_id)
    .bind(invite.guild_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    // Step 4b: Find the default 'member' role and assign it
    let member_role_id: openconv_shared::ids::RoleId = sqlx::query_scalar(
        "SELECT id FROM roles WHERE guild_id = $1 AND role_type = 'member'",
    )
    .bind(invite.guild_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_err)?;

    sqlx::query(
        "INSERT INTO guild_member_roles (user_id, guild_id, role_id) VALUES ($1, $2, $3)",
    )
    .bind(auth.user_id)
    .bind(invite.guild_id)
    .bind(member_role_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    tx.commit().await.map_err(db_err)?;

    Ok(StatusCode::OK)
}

/// Route builder for guild-scoped invite endpoints.
pub fn guild_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/", axum::routing::post(create_invite).get(list_invites))
        .route("/{code}", axum::routing::delete(revoke_invite))
}

/// Route builder for public invite endpoints (auth only, no guild membership required).
pub fn public_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/{code}", axum::routing::get(get_invite_info))
        .route("/{code}/accept", axum::routing::post(accept_invite))
}

#[derive(sqlx::FromRow)]
struct InviteRow {
    code: String,
    guild_id: GuildId,
    inviter_id: openconv_shared::ids::UserId,
    max_uses: Option<i32>,
    use_count: i32,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl InviteRow {
    fn into_response(self) -> InviteResponse {
        InviteResponse {
            code: self.code,
            guild_id: self.guild_id,
            inviter_id: self.inviter_id,
            max_uses: self.max_uses,
            use_count: self.use_count,
            expires_at: self.expires_at,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct InviteInfoRow {
    code: String,
    guild_name: String,
    guild_id: GuildId,
    member_count: i64,
    inviter_display_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn generated_codes_are_8_chars_base62() {
        for _ in 0..100 {
            let code = generate_invite_code();
            assert_eq!(code.len(), 8, "code length should be 8, got {}", code.len());
            assert!(
                code.chars().all(|c| c.is_ascii_alphanumeric()),
                "code should be base62, got: {}",
                code
            );
        }
    }

    #[test]
    fn different_invites_get_different_codes() {
        let codes: HashSet<String> = (0..100).map(|_| generate_invite_code()).collect();
        assert_eq!(codes.len(), 100, "all 100 codes should be unique");
    }

    #[test]
    fn routes_build_without_panic() {
        let _ = guild_routes();
        let _ = public_routes();
    }
}
