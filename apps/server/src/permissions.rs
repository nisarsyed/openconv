use openconv_shared::error::OpenConvError;
use openconv_shared::permissions::Permissions;

use crate::error::ServerError;

/// Check whether the actor can modify/delete the target role.
///
/// Rules:
/// - Guild owner (matched by `is_guild_owner`) bypasses all hierarchy checks
/// - Actor's highest role position must be strictly greater than the target role's position
pub fn check_role_hierarchy(
    actor_highest_position: i32,
    target_role_position: i32,
    is_guild_owner: bool,
) -> Result<(), ServerError> {
    if is_guild_owner {
        return Ok(());
    }
    if actor_highest_position <= target_role_position {
        return Err(ServerError(OpenConvError::Forbidden));
    }
    Ok(())
}

/// Check whether a built-in role can be deleted.
/// Only `'custom'` roles can be deleted.
pub fn can_delete_role(role_type: &str) -> bool {
    role_type == "custom"
}

/// Privilege escalation guard: verify that `new_permissions` is a subset of `actor_permissions`.
/// The actor cannot grant permissions they do not themselves possess.
/// Guild owner bypasses this check.
pub fn check_privilege_escalation(
    new_permissions: Permissions,
    actor_permissions: Permissions,
    is_guild_owner: bool,
) -> Result<(), ServerError> {
    if is_guild_owner {
        return Ok(());
    }
    if !actor_permissions.contains(new_permissions) {
        return Err(ServerError(OpenConvError::Forbidden));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cannot_modify_role_at_or_above_own_position() {
        // Same position
        assert!(check_role_hierarchy(5, 5, false).is_err());
        // Above own position
        assert!(check_role_hierarchy(5, 10, false).is_err());
    }

    #[test]
    fn test_can_modify_role_below_own_position() {
        assert!(check_role_hierarchy(10, 5, false).is_ok());
    }

    #[test]
    fn test_owner_bypasses_hierarchy() {
        assert!(check_role_hierarchy(1, 100, true).is_ok());
    }

    #[test]
    fn test_builtin_roles_not_deletable() {
        assert!(!can_delete_role("owner"));
        assert!(!can_delete_role("admin"));
        assert!(!can_delete_role("member"));
        assert!(can_delete_role("custom"));
    }

    #[test]
    fn test_privilege_escalation_blocked() {
        let actor_perms = Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES;
        let new_perms = Permissions::ADMINISTRATOR;
        assert!(check_privilege_escalation(new_perms, actor_perms, false).is_err());
    }

    #[test]
    fn test_privilege_escalation_allowed_subset() {
        let actor_perms =
            Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES | Permissions::MANAGE_CHANNELS;
        let new_perms = Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES;
        assert!(check_privilege_escalation(new_perms, actor_perms, false).is_ok());
    }

    #[test]
    fn test_privilege_escalation_owner_bypass() {
        let actor_perms = Permissions::SEND_MESSAGES;
        let new_perms = Permissions::ADMINISTRATOR;
        assert!(check_privilege_escalation(new_perms, actor_perms, true).is_ok());
    }

    #[test]
    fn test_privilege_escalation_exact_match() {
        let perms = Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES;
        assert!(check_privilege_escalation(perms, perms, false).is_ok());
    }

    #[test]
    fn test_privilege_escalation_empty_new_perms() {
        let actor_perms = Permissions::SEND_MESSAGES;
        let new_perms = Permissions::empty();
        assert!(check_privilege_escalation(new_perms, actor_perms, false).is_ok());
    }
}
