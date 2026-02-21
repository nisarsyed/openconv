use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Permissions: u64 {
        const ADMINISTRATOR    = 1 << 0;
        const MANAGE_GUILD     = 1 << 1;
        const MANAGE_CHANNELS  = 1 << 2;
        const MANAGE_ROLES     = 1 << 3;
        const MANAGE_INVITES   = 1 << 4;
        const KICK_MEMBERS     = 1 << 5;
        const SEND_MESSAGES    = 1 << 6;
        const READ_MESSAGES    = 1 << 7;
        const ATTACH_FILES     = 1 << 8;
        const MENTION_EVERYONE = 1 << 9;
        const MANAGE_MESSAGES  = 1 << 10;
    }
}

impl Default for Permissions {
    fn default() -> Self {
        Permissions::empty()
    }
}

impl serde::Serialize for Permissions {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.bits().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Permissions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bits = u64::deserialize(deserializer)?;
        Ok(Permissions::from_bits_truncate(bits))
    }
}

#[cfg(feature = "sqlx")]
mod sqlx_impls {
    use super::Permissions;
    use sqlx::encode::IsNull;
    use sqlx::postgres::{PgArgumentBuffer, PgTypeInfo, PgValueRef};
    use sqlx::{Decode, Encode, Postgres, Type};

    impl Type<Postgres> for Permissions {
        fn type_info() -> PgTypeInfo {
            <i64 as Type<Postgres>>::type_info()
        }

        fn compatible(ty: &PgTypeInfo) -> bool {
            <i64 as Type<Postgres>>::compatible(ty)
        }
    }

    impl Encode<'_, Postgres> for Permissions {
        fn encode_by_ref(
            &self,
            buf: &mut PgArgumentBuffer,
        ) -> Result<IsNull, Box<dyn std::error::Error + Send + Sync>> {
            <i64 as Encode<'_, Postgres>>::encode_by_ref(&(self.bits() as i64), buf)
        }
    }

    impl<'r> Decode<'r, Postgres> for Permissions {
        fn decode(
            value: PgValueRef<'r>,
        ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
            let val = <i64 as Decode<'r, Postgres>>::decode(value)?;
            Ok(Permissions::from_bits_truncate(val as u64))
        }
    }
}

/// Resolve effective permissions by ORing all role permission bitfields.
/// If ADMINISTRATOR is set in any role, returns `Permissions::all()` immediately.
pub fn resolve(role_permissions: &[Permissions]) -> Permissions {
    let mut result = Permissions::empty();
    for &perm in role_permissions {
        result |= perm;
        if result.contains(Permissions::ADMINISTRATOR) {
            return Permissions::all();
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permissions_serde_roundtrip() {
        let perms = Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES;
        let json = serde_json::to_value(perms).unwrap();
        assert!(json.is_number());
        let back: Permissions = serde_json::from_value(json).unwrap();
        assert_eq!(perms, back);
    }

    #[test]
    fn test_resolve_ors_all_role_permissions() {
        let role1 = Permissions::SEND_MESSAGES;
        let role2 = Permissions::READ_MESSAGES;
        let resolved = resolve(&[role1, role2]);
        assert_eq!(
            resolved,
            Permissions::SEND_MESSAGES | Permissions::READ_MESSAGES
        );
    }

    #[test]
    fn test_resolve_admin_gets_all() {
        let role1 = Permissions::ADMINISTRATOR;
        let role2 = Permissions::SEND_MESSAGES;
        let resolved = resolve(&[role1, role2]);
        assert_eq!(resolved, Permissions::all());
    }

    #[test]
    fn test_resolve_empty_roles_returns_empty() {
        let perms = resolve(&[]);
        assert!(perms.is_empty());
    }

    #[test]
    fn test_manage_messages_bit_position() {
        assert_eq!(Permissions::MANAGE_MESSAGES.bits(), 1 << 10);
    }

    #[test]
    fn test_all_flags_unique_bits() {
        let flags = [
            Permissions::ADMINISTRATOR,
            Permissions::MANAGE_GUILD,
            Permissions::MANAGE_CHANNELS,
            Permissions::MANAGE_ROLES,
            Permissions::MANAGE_INVITES,
            Permissions::KICK_MEMBERS,
            Permissions::SEND_MESSAGES,
            Permissions::READ_MESSAGES,
            Permissions::ATTACH_FILES,
            Permissions::MENTION_EVERYONE,
            Permissions::MANAGE_MESSAGES,
        ];
        for (i, a) in flags.iter().enumerate() {
            for (j, b) in flags.iter().enumerate() {
                if i != j {
                    assert!(
                        (*a & *b).is_empty(),
                        "flags at positions {i} and {j} share bits",
                    );
                }
            }
        }
    }

    #[test]
    fn test_serialize_as_u64_number() {
        let perms = Permissions::ADMINISTRATOR;
        let json = serde_json::to_string(&perms).unwrap();
        assert_eq!(json, "1");
    }

    #[test]
    fn test_deserialize_truncates_unknown_bits() {
        let json = serde_json::json!(u64::MAX);
        let perms: Permissions = serde_json::from_value(json).unwrap();
        assert_eq!(perms, Permissions::all());
    }
}
