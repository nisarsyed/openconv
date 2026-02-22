use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use openconv_shared::ids::{ChannelId, DeviceId, GuildId, UserId};
use openconv_shared::permissions::Permissions;
use std::collections::HashMap;

use crate::error::ServerError;
use crate::extractors::auth::AuthUser;
use crate::extractors::guild_member::{resolve_guild_membership, GuildMemberRejection};
use crate::state::AppState;

/// Extracted channel membership info with resolved permissions.
///
/// Resolves guild_id from channel_id, then checks guild membership.
#[derive(Debug, Clone)]
pub struct ChannelMember {
    pub user_id: UserId,
    pub device_id: DeviceId,
    pub channel_id: ChannelId,
    pub guild_id: GuildId,
    pub permissions: Permissions,
}

impl ChannelMember {
    /// Check that the resolved permissions contain the required set.
    /// Returns `Err(ServerError)` with 403 Forbidden if insufficient.
    pub fn require(&self, required: Permissions) -> Result<(), ServerError> {
        use openconv_shared::error::OpenConvError;
        if self.permissions.contains(required) {
            Ok(())
        } else {
            Err(ServerError(OpenConvError::Forbidden))
        }
    }
}

impl FromRequestParts<AppState> for ChannelMember {
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

        let channel_id: ChannelId = params
            .get("channel_id")
            .ok_or(GuildMemberRejection::NotFound)?
            .parse()
            .map_err(|_| GuildMemberRejection::NotFound)?;

        let guild_id: GuildId = sqlx::query_scalar("SELECT guild_id FROM channels WHERE id = $1")
            .bind(channel_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| GuildMemberRejection::Internal(e.to_string()))?
            .ok_or(GuildMemberRejection::NotFound)?;

        let permissions = resolve_guild_membership(&state.db, auth.user_id, guild_id).await?;

        let member = ChannelMember {
            user_id: auth.user_id,
            device_id: auth.device_id,
            channel_id,
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
    fn test_channel_member_require_passes_with_permission() {
        let member = ChannelMember {
            user_id: UserId::new(),
            device_id: DeviceId::new(),
            channel_id: ChannelId::new(),
            guild_id: GuildId::new(),
            permissions: Permissions::SEND_MESSAGES | Permissions::ATTACH_FILES,
        };
        assert!(member.require(Permissions::ATTACH_FILES).is_ok());
    }

    #[test]
    fn test_channel_member_require_fails_without_permission() {
        let member = ChannelMember {
            user_id: UserId::new(),
            device_id: DeviceId::new(),
            channel_id: ChannelId::new(),
            guild_id: GuildId::new(),
            permissions: Permissions::SEND_MESSAGES,
        };
        assert!(member.require(Permissions::MANAGE_CHANNELS).is_err());
    }
}
