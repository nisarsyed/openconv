diff --git a/crates/shared/src/api/auth.rs b/crates/shared/src/api/auth.rs
new file mode 100644
index 0000000..1274dd0
--- /dev/null
+++ b/crates/shared/src/api/auth.rs
@@ -0,0 +1,63 @@
+use crate::ids::UserId;
+use serde::{Deserialize, Serialize};
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct RegisterRequest {
+    pub public_key: String,
+    pub email: String,
+    pub display_name: String,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct RegisterResponse {
+    pub user_id: UserId,
+    pub token: String,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct LoginChallengeRequest {
+    pub public_key: String,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct LoginChallengeResponse {
+    pub challenge: String,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct LoginVerifyRequest {
+    pub public_key: String,
+    pub signature: String,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct LoginVerifyResponse {
+    pub token: String,
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn register_request_serializes() {
+        let req = RegisterRequest {
+            public_key: "pk_test".into(),
+            email: "test@example.com".into(),
+            display_name: "Test User".into(),
+        };
+        let json = serde_json::to_value(&req).unwrap();
+        assert_eq!(json["public_key"], "pk_test");
+        assert_eq!(json["email"], "test@example.com");
+        assert_eq!(json["display_name"], "Test User");
+    }
+
+    #[test]
+    fn register_request_deserializes() {
+        let json = r#"{"public_key":"pk","email":"a@b.com","display_name":"A"}"#;
+        let req: RegisterRequest = serde_json::from_str(json).unwrap();
+        assert_eq!(req.public_key, "pk");
+        assert_eq!(req.email, "a@b.com");
+        assert_eq!(req.display_name, "A");
+    }
+}
diff --git a/crates/shared/src/api/channel.rs b/crates/shared/src/api/channel.rs
new file mode 100644
index 0000000..17a2037
--- /dev/null
+++ b/crates/shared/src/api/channel.rs
@@ -0,0 +1,36 @@
+use crate::ids::{ChannelId, GuildId};
+use serde::{Deserialize, Serialize};
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct CreateChannelRequest {
+    pub name: String,
+    pub channel_type: String,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct ChannelResponse {
+    pub id: ChannelId,
+    pub guild_id: GuildId,
+    pub name: String,
+    pub channel_type: String,
+    pub position: i32,
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn channel_response_channel_type() {
+        let resp = ChannelResponse {
+            id: ChannelId::new(),
+            guild_id: GuildId::new(),
+            name: "general".into(),
+            channel_type: "text".into(),
+            position: 0,
+        };
+        let json = serde_json::to_string(&resp).unwrap();
+        let back: ChannelResponse = serde_json::from_str(&json).unwrap();
+        assert_eq!(back.channel_type, "text");
+    }
+}
diff --git a/crates/shared/src/api/guild.rs b/crates/shared/src/api/guild.rs
new file mode 100644
index 0000000..6a25564
--- /dev/null
+++ b/crates/shared/src/api/guild.rs
@@ -0,0 +1,50 @@
+use crate::ids::{GuildId, UserId};
+use serde::{Deserialize, Serialize};
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct CreateGuildRequest {
+    pub name: String,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct GuildResponse {
+    pub id: GuildId,
+    pub name: String,
+    pub owner_id: UserId,
+    pub icon_url: Option<String>,
+    pub created_at: chrono::DateTime<chrono::Utc>,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct GuildListResponse {
+    pub guilds: Vec<GuildResponse>,
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn guild_response_roundtrip() {
+        let resp = GuildResponse {
+            id: GuildId::new(),
+            name: "Test Guild".into(),
+            owner_id: UserId::new(),
+            icon_url: None,
+            created_at: chrono::Utc::now(),
+        };
+        let json = serde_json::to_string(&resp).unwrap();
+        let back: GuildResponse = serde_json::from_str(&json).unwrap();
+        assert_eq!(back.id, resp.id);
+        assert_eq!(back.name, "Test Guild");
+    }
+
+    #[test]
+    fn create_guild_request_minimal() {
+        let req = CreateGuildRequest {
+            name: "My Guild".into(),
+        };
+        let json = serde_json::to_value(&req).unwrap();
+        assert_eq!(json["name"], "My Guild");
+    }
+}
diff --git a/crates/shared/src/api/message.rs b/crates/shared/src/api/message.rs
new file mode 100644
index 0000000..8832d21
--- /dev/null
+++ b/crates/shared/src/api/message.rs
@@ -0,0 +1,42 @@
+use crate::ids::{ChannelId, MessageId, UserId};
+use serde::{Deserialize, Serialize};
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct SendMessageRequest {
+    pub encrypted_content: String,
+    pub nonce: String,
+}
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct MessageResponse {
+    pub id: MessageId,
+    pub channel_id: ChannelId,
+    pub sender_id: UserId,
+    pub encrypted_content: String,
+    pub nonce: String,
+    pub created_at: chrono::DateTime<chrono::Utc>,
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn message_response_includes_all_fields() {
+        let resp = MessageResponse {
+            id: MessageId::new(),
+            channel_id: ChannelId::new(),
+            sender_id: UserId::new(),
+            encrypted_content: "enc_data".into(),
+            nonce: "nonce123".into(),
+            created_at: chrono::Utc::now(),
+        };
+        let json = serde_json::to_value(&resp).unwrap();
+        assert!(json.get("id").is_some());
+        assert!(json.get("channel_id").is_some());
+        assert!(json.get("sender_id").is_some());
+        assert!(json.get("encrypted_content").is_some());
+        assert!(json.get("nonce").is_some());
+        assert!(json.get("created_at").is_some());
+    }
+}
diff --git a/crates/shared/src/api/mod.rs b/crates/shared/src/api/mod.rs
new file mode 100644
index 0000000..24b48e1
--- /dev/null
+++ b/crates/shared/src/api/mod.rs
@@ -0,0 +1,5 @@
+pub mod auth;
+pub mod guild;
+pub mod channel;
+pub mod message;
+pub mod user;
diff --git a/crates/shared/src/api/user.rs b/crates/shared/src/api/user.rs
new file mode 100644
index 0000000..ff31531
--- /dev/null
+++ b/crates/shared/src/api/user.rs
@@ -0,0 +1,9 @@
+use crate::ids::UserId;
+use serde::{Deserialize, Serialize};
+
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct UserProfileResponse {
+    pub id: UserId,
+    pub display_name: String,
+    pub avatar_url: Option<String>,
+}
diff --git a/crates/shared/src/constants.rs b/crates/shared/src/constants.rs
new file mode 100644
index 0000000..6d44fb0
--- /dev/null
+++ b/crates/shared/src/constants.rs
@@ -0,0 +1,24 @@
+pub const MAX_FILE_SIZE_BYTES: usize = 25 * 1024 * 1024;
+pub const MAX_DISPLAY_NAME_LENGTH: usize = 64;
+pub const MAX_CHANNEL_NAME_LENGTH: usize = 100;
+pub const MAX_GUILD_NAME_LENGTH: usize = 100;
+pub const MAX_MESSAGE_SIZE_BYTES: usize = 8 * 1024;
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn max_file_size_is_25mb() {
+        assert_eq!(MAX_FILE_SIZE_BYTES, 25 * 1024 * 1024);
+    }
+
+    #[test]
+    fn all_length_constants_positive() {
+        assert!(MAX_FILE_SIZE_BYTES > 0);
+        assert!(MAX_DISPLAY_NAME_LENGTH > 0);
+        assert!(MAX_CHANNEL_NAME_LENGTH > 0);
+        assert!(MAX_GUILD_NAME_LENGTH > 0);
+        assert!(MAX_MESSAGE_SIZE_BYTES > 0);
+    }
+}
diff --git a/crates/shared/src/error.rs b/crates/shared/src/error.rs
new file mode 100644
index 0000000..15d7818
--- /dev/null
+++ b/crates/shared/src/error.rs
@@ -0,0 +1,54 @@
+#[derive(Debug, thiserror::Error)]
+pub enum OpenConvError {
+    #[error("not found")]
+    NotFound,
+
+    #[error("unauthorized")]
+    Unauthorized,
+
+    #[error("forbidden")]
+    Forbidden,
+
+    #[error("validation error: {0}")]
+    Validation(String),
+
+    #[error("internal error: {0}")]
+    Internal(String),
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn not_found_display() {
+        let err = OpenConvError::NotFound;
+        assert_eq!(err.to_string(), "not found");
+    }
+
+    #[test]
+    fn validation_contains_message() {
+        let err = OpenConvError::Validation("bad input".into());
+        assert_eq!(err.to_string(), "validation error: bad input");
+    }
+
+    #[test]
+    fn internal_contains_message() {
+        let err = OpenConvError::Internal("db down".into());
+        assert_eq!(err.to_string(), "internal error: db down");
+    }
+
+    #[test]
+    fn all_variants_impl_error() {
+        let errors: Vec<Box<dyn std::error::Error>> = vec![
+            Box::new(OpenConvError::NotFound),
+            Box::new(OpenConvError::Unauthorized),
+            Box::new(OpenConvError::Forbidden),
+            Box::new(OpenConvError::Validation("x".into())),
+            Box::new(OpenConvError::Internal("y".into())),
+        ];
+        for e in &errors {
+            let _ = e.to_string();
+        }
+    }
+}
diff --git a/crates/shared/src/ids.rs b/crates/shared/src/ids.rs
new file mode 100644
index 0000000..40af2c3
--- /dev/null
+++ b/crates/shared/src/ids.rs
@@ -0,0 +1,165 @@
+macro_rules! define_id {
+    ($name:ident) => {
+        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
+        #[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
+        #[cfg_attr(feature = "sqlx", sqlx(transparent))]
+        pub struct $name(pub uuid::Uuid);
+
+        impl Default for $name {
+            fn default() -> Self {
+                Self::new()
+            }
+        }
+
+        impl $name {
+            pub fn new() -> Self {
+                Self(uuid::Uuid::now_v7())
+            }
+        }
+
+        impl std::fmt::Display for $name {
+            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
+                write!(f, "{}", self.0)
+            }
+        }
+
+        impl std::str::FromStr for $name {
+            type Err = uuid::Error;
+
+            fn from_str(s: &str) -> Result<Self, Self::Err> {
+                Ok(Self(uuid::Uuid::parse_str(s)?))
+            }
+        }
+    };
+}
+
+define_id!(UserId);
+define_id!(GuildId);
+define_id!(ChannelId);
+define_id!(MessageId);
+define_id!(RoleId);
+define_id!(FileId);
+define_id!(DmChannelId);
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+    use std::str::FromStr;
+
+    #[test]
+    fn user_id_new_creates_valid_uuid() {
+        let id = UserId::new();
+        assert_eq!(id.0.get_version(), Some(uuid::Version::SortRand));
+    }
+
+    #[test]
+    fn user_id_serializes_to_uuid_string() {
+        let id = UserId::new();
+        let json = serde_json::to_string(&id).unwrap();
+        // Should be a quoted UUID string
+        assert!(json.starts_with('"'));
+        assert!(json.ends_with('"'));
+        let inner = &json[1..json.len() - 1];
+        uuid::Uuid::parse_str(inner).unwrap();
+    }
+
+    #[test]
+    fn user_id_deserializes_from_uuid_string() {
+        let id = UserId::new();
+        let json = serde_json::to_string(&id).unwrap();
+        let deserialized: UserId = serde_json::from_str(&json).unwrap();
+        assert_eq!(id, deserialized);
+    }
+
+    #[test]
+    fn user_id_roundtrip_serde() {
+        let id = UserId::new();
+        let json = serde_json::to_string(&id).unwrap();
+        let back: UserId = serde_json::from_str(&json).unwrap();
+        assert_eq!(id, back);
+    }
+
+    #[test]
+    fn user_id_display_formats_as_uuid() {
+        let id = UserId::new();
+        let display = id.to_string();
+        uuid::Uuid::parse_str(&display).unwrap();
+    }
+
+    #[test]
+    fn user_id_from_str_valid() {
+        let id = UserId::new();
+        let s = id.to_string();
+        let parsed = UserId::from_str(&s).unwrap();
+        assert_eq!(id, parsed);
+    }
+
+    #[test]
+    fn user_id_from_str_invalid() {
+        let result = UserId::from_str("not-a-uuid");
+        assert!(result.is_err());
+    }
+
+    #[test]
+    fn user_id_new_produces_unique_ids() {
+        let a = UserId::new();
+        let b = UserId::new();
+        assert_ne!(a, b);
+    }
+
+    #[test]
+    fn user_id_new_is_time_sortable() {
+        let a = UserId::new();
+        std::thread::sleep(std::time::Duration::from_millis(2));
+        let b = UserId::new();
+        assert!(a.to_string() < b.to_string());
+    }
+
+    #[test]
+    fn guild_id_roundtrip_serde() {
+        let id = GuildId::new();
+        let json = serde_json::to_string(&id).unwrap();
+        let back: GuildId = serde_json::from_str(&json).unwrap();
+        assert_eq!(id, back);
+    }
+
+    #[test]
+    fn channel_id_roundtrip_serde() {
+        let id = ChannelId::new();
+        let json = serde_json::to_string(&id).unwrap();
+        let back: ChannelId = serde_json::from_str(&json).unwrap();
+        assert_eq!(id, back);
+    }
+
+    #[test]
+    fn message_id_roundtrip_serde() {
+        let id = MessageId::new();
+        let json = serde_json::to_string(&id).unwrap();
+        let back: MessageId = serde_json::from_str(&json).unwrap();
+        assert_eq!(id, back);
+    }
+
+    #[test]
+    fn role_id_roundtrip_serde() {
+        let id = RoleId::new();
+        let json = serde_json::to_string(&id).unwrap();
+        let back: RoleId = serde_json::from_str(&json).unwrap();
+        assert_eq!(id, back);
+    }
+
+    #[test]
+    fn file_id_roundtrip_serde() {
+        let id = FileId::new();
+        let json = serde_json::to_string(&id).unwrap();
+        let back: FileId = serde_json::from_str(&json).unwrap();
+        assert_eq!(id, back);
+    }
+
+    #[test]
+    fn dm_channel_id_roundtrip_serde() {
+        let id = DmChannelId::new();
+        let json = serde_json::to_string(&id).unwrap();
+        let back: DmChannelId = serde_json::from_str(&json).unwrap();
+        assert_eq!(id, back);
+    }
+}
diff --git a/crates/shared/src/lib.rs b/crates/shared/src/lib.rs
index a57683e..6c7d94e 100644
--- a/crates/shared/src/lib.rs
+++ b/crates/shared/src/lib.rs
@@ -1 +1,6 @@
 //! OpenConv shared library — types, IDs, and API contracts shared between server and client.
+
+pub mod ids;
+pub mod api;
+pub mod error;
+pub mod constants;
