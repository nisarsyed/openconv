macro_rules! define_id {
    ($name:ident) => {
        /// Typed wrapper around UUID v7 for entity identification.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
        #[cfg_attr(feature = "sqlx", sqlx(transparent))]
        pub struct $name(pub uuid::Uuid);

        #[allow(clippy::new_without_default)]
        impl $name {
            /// Generate a new time-sortable UUID v7 identifier.
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(uuid::Uuid::parse_str(s)?))
            }
        }
    };
}

define_id!(UserId);
define_id!(GuildId);
define_id!(ChannelId);
define_id!(MessageId);
define_id!(RoleId);
define_id!(FileId);
define_id!(DmChannelId);

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn user_id_new_creates_valid_uuid() {
        let id = UserId::new();
        assert_eq!(id.0.get_version(), Some(uuid::Version::SortRand));
    }

    #[test]
    fn user_id_serializes_to_uuid_string() {
        let id = UserId::new();
        let json = serde_json::to_string(&id).unwrap();
        // Should be a quoted UUID string
        assert!(json.starts_with('"'));
        assert!(json.ends_with('"'));
        let inner = &json[1..json.len() - 1];
        uuid::Uuid::parse_str(inner).unwrap();
    }

    #[test]
    fn user_id_deserializes_from_uuid_string() {
        let id = UserId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn user_id_roundtrip_serde() {
        let id = UserId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn user_id_display_formats_as_uuid() {
        let id = UserId::new();
        let display = id.to_string();
        uuid::Uuid::parse_str(&display).unwrap();
    }

    #[test]
    fn user_id_from_str_valid() {
        let id = UserId::new();
        let s = id.to_string();
        let parsed = UserId::from_str(&s).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn user_id_from_str_invalid() {
        let result = UserId::from_str("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn user_id_new_produces_unique_ids() {
        let a = UserId::new();
        let b = UserId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn user_id_new_is_time_sortable() {
        let a = UserId::new();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = UserId::new();
        assert!(a.to_string() < b.to_string());
    }

    #[test]
    fn guild_id_roundtrip_serde() {
        let id = GuildId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: GuildId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn channel_id_roundtrip_serde() {
        let id = ChannelId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: ChannelId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn message_id_roundtrip_serde() {
        let id = MessageId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: MessageId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn role_id_roundtrip_serde() {
        let id = RoleId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: RoleId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn file_id_roundtrip_serde() {
        let id = FileId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: FileId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn dm_channel_id_roundtrip_serde() {
        let id = DmChannelId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: DmChannelId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
