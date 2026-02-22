use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use openconv_shared::error::OpenConvError;
use openconv_shared::ids::{DeviceId, GuildId, UserId};
use openconv_shared::permissions::{self, Permissions};
use std::collections::HashMap;

use crate::error::ServerError;
use crate::extractors::auth::AuthUser;
use crate::state::AppState;

/// Extracted guild membership info with resolved permissions.
#[derive(Debug, Clone)]
pub struct GuildMember {
    pub user_id: UserId,
    pub device_id: DeviceId,
    pub guild_id: GuildId,
    pub permissions: Permissions,
}

impl GuildMember {
    /// Check that the resolved permissions contain the required set.
    /// Returns `Err(ServerError)` with 403 Forbidden if insufficient.
    pub fn require(&self, required: Permissions) -> Result<(), ServerError> {
        if self.permissions.contains(required) {
            Ok(())
        } else {
            Err(ServerError(OpenConvError::Forbidden))
        }
    }
}

#[derive(Debug)]
pub enum GuildMemberRejection {
    Unauthenticated,
    Forbidden,
    NotFound,
    Internal(String),
}

impl IntoResponse for GuildMemberRejection {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::Unauthenticated => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found"),
            Self::Internal(e) => {
                tracing::error!(error = %e, "guild member extractor error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// Resolve guild membership and permissions for a user.
///
/// Runs a single JOIN query across `guild_members`, `guild_member_roles`, and `roles`
/// to fetch all role permissions, then ORs them together via `resolve()`.
///
/// Returns `Forbidden` if the user is not a guild member (or has no roles).
pub(crate) async fn resolve_guild_membership(
    db: &sqlx::PgPool,
    user_id: UserId,
    guild_id: GuildId,
) -> Result<Permissions, GuildMemberRejection> {
    let role_perms: Vec<i64> = sqlx::query_scalar(
        "SELECT r.permissions \
         FROM guild_members gm \
         JOIN guild_member_roles gmr ON gmr.user_id = gm.user_id AND gmr.guild_id = gm.guild_id \
         JOIN roles r ON r.id = gmr.role_id \
         WHERE gm.user_id = $1 AND gm.guild_id = $2",
    )
    .bind(user_id)
    .bind(guild_id)
    .fetch_all(db)
    .await
    .map_err(|e| GuildMemberRejection::Internal(e.to_string()))?;

    if role_perms.is_empty() {
        return Err(GuildMemberRejection::Forbidden);
    }

    let perms: Vec<Permissions> = role_perms
        .into_iter()
        .map(|bits| Permissions::from_bits_truncate(bits as u64))
        .collect();

    Ok(permissions::resolve(&perms))
}

impl FromRequestParts<AppState> for GuildMember {
    type Rejection = GuildMemberRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state)
            .await
            .map_err(|_| GuildMemberRejection::Unauthenticated)?;

        let Path(params): Path<HashMap<String, String>> = Path::from_request_parts(parts, state)
            .await
            .map_err(|_| GuildMemberRejection::NotFound)?;

        let guild_id: GuildId = params
            .get("guild_id")
            .ok_or(GuildMemberRejection::NotFound)?
            .parse()
            .map_err(|_| GuildMemberRejection::NotFound)?;

        let permissions = resolve_guild_membership(&state.db, auth.user_id, guild_id).await?;

        let member = GuildMember {
            user_id: auth.user_id,
            device_id: auth.device_id,
            guild_id,
            permissions,
        };

        parts.extensions.insert(member.clone());

        Ok(member)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_require_permission_returns_403_missing() {
        let member = GuildMember {
            user_id: UserId::new(),
            device_id: DeviceId::new(),
            guild_id: GuildId::new(),
            permissions: Permissions::SEND_MESSAGES,
        };
        let result = member.require(Permissions::MANAGE_CHANNELS);
        assert!(result.is_err());
    }

    #[test]
    fn test_require_permission_passes_with_permission() {
        let member = GuildMember {
            user_id: UserId::new(),
            device_id: DeviceId::new(),
            guild_id: GuildId::new(),
            permissions: Permissions::SEND_MESSAGES | Permissions::MANAGE_CHANNELS,
        };
        assert!(member.require(Permissions::MANAGE_CHANNELS).is_ok());
    }

    #[test]
    fn test_require_permission_passes_for_admin() {
        let member = GuildMember {
            user_id: UserId::new(),
            device_id: DeviceId::new(),
            guild_id: GuildId::new(),
            permissions: Permissions::all(),
        };
        assert!(member.require(Permissions::MANAGE_CHANNELS).is_ok());
        assert!(member.require(Permissions::ADMINISTRATOR).is_ok());
    }

    #[test]
    fn test_require_multiple_permissions() {
        let member = GuildMember {
            user_id: UserId::new(),
            device_id: DeviceId::new(),
            guild_id: GuildId::new(),
            permissions: Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES,
        };
        // Requiring both at once
        assert!(member
            .require(Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES)
            .is_ok());
        // Requiring one we don't have
        assert!(member
            .require(Permissions::SEND_MESSAGES | Permissions::MANAGE_CHANNELS)
            .is_err());
    }

    #[tokio::test]
    async fn test_guild_member_rejection_unauthenticated_is_401() {
        let rejection = GuildMemberRejection::Unauthenticated;
        let response = rejection.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_guild_member_rejection_forbidden_is_403() {
        let rejection = GuildMemberRejection::Forbidden;
        let response = rejection.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_guild_member_rejection_not_found_is_404() {
        let rejection = GuildMemberRejection::NotFound;
        let response = rejection.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
