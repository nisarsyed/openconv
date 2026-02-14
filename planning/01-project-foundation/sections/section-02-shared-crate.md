Now I have all the context needed. Let me produce the section content.

# Section 02: Shared Crate (openconv-shared)

## Overview

This section implements the `openconv-shared` crate, a library crate at `/Users/nisar/personal/projects/openconv/crates/shared/` that provides shared types used by both the Axum server and the Tauri desktop client. It contains:

1. A `define_id!` macro and typed ID newtypes for every entity (UserId, GuildId, etc.)
2. API request/response types grouped by domain
3. A shared `OpenConvError` enum using `thiserror`
4. Shared constants (size limits, name lengths)
5. Conditional SQLx integration via a Cargo feature flag

## Dependencies

**Depends on:** section-01-monorepo-setup (the workspace root `Cargo.toml` and directory structure must exist)

**Blocks:** section-03-server-scaffold, section-04-postgres-migrations, section-05-tauri-scaffold, section-06-sqlite-migrations

## Files to Create/Modify

| File | Action |
|------|--------|
| `/Users/nisar/personal/projects/openconv/crates/shared/Cargo.toml` | Create |
| `/Users/nisar/personal/projects/openconv/crates/shared/src/lib.rs` | Create |
| `/Users/nisar/personal/projects/openconv/crates/shared/src/ids.rs` | Create |
| `/Users/nisar/personal/projects/openconv/crates/shared/src/api/mod.rs` | Create |
| `/Users/nisar/personal/projects/openconv/crates/shared/src/api/auth.rs` | Create |
| `/Users/nisar/personal/projects/openconv/crates/shared/src/api/guild.rs` | Create |
| `/Users/nisar/personal/projects/openconv/crates/shared/src/api/channel.rs` | Create |
| `/Users/nisar/personal/projects/openconv/crates/shared/src/api/message.rs` | Create |
| `/Users/nisar/personal/projects/openconv/crates/shared/src/api/user.rs` | Create |
| `/Users/nisar/personal/projects/openconv/crates/shared/src/error.rs` | Create |
| `/Users/nisar/personal/projects/openconv/crates/shared/src/constants.rs` | Create |

---

## Tests (Write First)

All tests live in `/Users/nisar/personal/projects/openconv/crates/shared/src/` as inline `#[cfg(test)]` modules within each source file. These tests should be written before the corresponding implementations.

### 3.1 Typed ID Tests

Place in `/Users/nisar/personal/projects/openconv/crates/shared/src/ids.rs` as a `#[cfg(test)] mod tests` block.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // Test: UserId::new() generates a valid UUID v7
    #[test]
    fn user_id_new_creates_valid_uuid() { todo!() }

    // Test: UserId serializes to a UUID string via serde_json
    #[test]
    fn user_id_serializes_to_uuid_string() { todo!() }

    // Test: UserId deserializes from a UUID string via serde_json
    #[test]
    fn user_id_deserializes_from_uuid_string() { todo!() }

    // Test: UserId roundtrip: serialize then deserialize produces the same value
    #[test]
    fn user_id_roundtrip_serde() { todo!() }

    // Test: UserId Display formats as UUID string
    #[test]
    fn user_id_display_formats_as_uuid() { todo!() }

    // Test: UserId FromStr parses a valid UUID string
    #[test]
    fn user_id_from_str_valid() { todo!() }

    // Test: UserId FromStr rejects an invalid string
    #[test]
    fn user_id_from_str_invalid() { todo!() }

    // Test: Two calls to UserId::new() produce different IDs
    #[test]
    fn user_id_new_produces_unique_ids() { todo!() }

    // Test: UserId::new() produces time-sortable IDs (second > first lexicographically)
    #[test]
    fn user_id_new_is_time_sortable() { todo!() }

    // Repeat the same pattern for all ID types. A helper macro or parameterized
    // approach is acceptable since all IDs share the same behavior via define_id!.
    // At minimum, test one representative non-UserId type (e.g., GuildId) to verify
    // the macro works for all variants:

    #[test]
    fn guild_id_roundtrip_serde() { todo!() }

    #[test]
    fn channel_id_roundtrip_serde() { todo!() }

    #[test]
    fn message_id_roundtrip_serde() { todo!() }

    #[test]
    fn role_id_roundtrip_serde() { todo!() }

    #[test]
    fn file_id_roundtrip_serde() { todo!() }

    #[test]
    fn dm_channel_id_roundtrip_serde() { todo!() }
}
```

### 3.2 API Type Tests

Place in `/Users/nisar/personal/projects/openconv/crates/shared/src/api/mod.rs` or as separate test modules in each API submodule.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test: RegisterRequest serializes to JSON with expected field names
    #[test]
    fn register_request_serializes() { todo!() }

    // Test: RegisterRequest deserializes from JSON
    #[test]
    fn register_request_deserializes() { todo!() }

    // Test: GuildResponse roundtrip serialization
    #[test]
    fn guild_response_roundtrip() { todo!() }

    // Test: MessageResponse includes all fields in JSON output
    #[test]
    fn message_response_includes_all_fields() { todo!() }

    // Test: CreateGuildRequest with minimal fields serializes correctly
    #[test]
    fn create_guild_request_minimal() { todo!() }

    // Test: ChannelResponse deserializes channel_type as expected type
    #[test]
    fn channel_response_channel_type() { todo!() }
}
```

### 3.3 Error Type Tests

Place in `/Users/nisar/personal/projects/openconv/crates/shared/src/error.rs`.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test: OpenConvError::NotFound displays correctly
    #[test]
    fn not_found_display() { todo!() }

    // Test: OpenConvError::Validation contains the message
    #[test]
    fn validation_contains_message() { todo!() }

    // Test: OpenConvError::Internal contains the message
    #[test]
    fn internal_contains_message() { todo!() }

    // Test: All error variants implement std::error::Error
    #[test]
    fn all_variants_impl_error() {
        // Verify that each variant can be used as &dyn std::error::Error
        todo!()
    }
}
```

### 3.5 Constants Tests

Place in `/Users/nisar/personal/projects/openconv/crates/shared/src/constants.rs`.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test: MAX_FILE_SIZE_BYTES equals 25 * 1024 * 1024
    #[test]
    fn max_file_size_is_25mb() { todo!() }

    // Test: All length constants are > 0
    #[test]
    fn all_length_constants_positive() { todo!() }
}
```

### 3.4 SQLx Feature Flag

No dedicated unit tests. Verification is structural:
- The server crate (section-03) enables `features = ["sqlx"]` and compiles successfully with typed IDs in SQL queries.
- The desktop crate (section-05) uses the shared crate without the `sqlx` feature and compiles successfully.
- Both crates building in the same workspace confirms the feature flag works correctly.

---

## Implementation Details

### Crate Cargo.toml

File: `/Users/nisar/personal/projects/openconv/crates/shared/Cargo.toml`

```toml
[package]
name = "openconv-shared"
version = "0.1.0"
edition = "2021"

[features]
default = []
sqlx = ["dep:sqlx"]

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }

# Optional: only pulled in when the "sqlx" feature is enabled
sqlx = { workspace = true, optional = true }
```

Key points:
- The `sqlx` feature is opt-in. The server crate enables it; the desktop crate does not.
- All other dependencies use `workspace = true` to inherit versions from the root `Cargo.toml` (set up in section-01).

### Module Structure

File: `/Users/nisar/personal/projects/openconv/crates/shared/src/lib.rs`

This is the crate root. It re-exports all public modules:

```rust
pub mod ids;
pub mod api;
pub mod error;
pub mod constants;
```

### Typed IDs (`ids.rs`)

File: `/Users/nisar/personal/projects/openconv/crates/shared/src/ids.rs`

Define a `define_id!` declarative macro that generates a newtype wrapper around `uuid::Uuid` for each entity. The macro should produce:

- A tuple struct wrapping `Uuid`
- Derives: `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`
- Conditional derive: `#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]` -- this makes the type transparent to SQLx when the feature is enabled
- A `new()` constructor that calls `uuid::Uuid::now_v7()` to generate time-sortable UUIDs
- `Display` impl that formats as the UUID string
- `FromStr` impl that parses a UUID string

Invoke the macro for each entity type:

```rust
define_id!(UserId);
define_id!(GuildId);
define_id!(ChannelId);
define_id!(MessageId);
define_id!(RoleId);
define_id!(FileId);
define_id!(DmChannelId);
```

The macro signature should look approximately like:

```rust
macro_rules! define_id {
    ($name:ident) => {
        /// Typed wrapper around UUID v7 for $name.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
        #[cfg_attr(feature = "sqlx", sqlx(transparent))]
        pub struct $name(pub uuid::Uuid);

        impl $name {
            /// Generate a new time-sortable UUID v7 identifier.
            pub fn new() -> Self { /* ... */ }
        }

        impl std::fmt::Display for $name { /* format as UUID string */ }
        impl std::str::FromStr for $name { /* parse UUID string */ }
    };
}
```

The `#[sqlx(transparent)]` attribute tells SQLx to treat the newtype as its inner `Uuid` type when encoding/decoding database values.

### API Types

These are data-only structs (no methods beyond serde). Each domain gets its own submodule under `/Users/nisar/personal/projects/openconv/crates/shared/src/api/`.

File: `/Users/nisar/personal/projects/openconv/crates/shared/src/api/mod.rs`

```rust
pub mod auth;
pub mod guild;
pub mod channel;
pub mod message;
pub mod user;
```

**Auth types** -- File: `/Users/nisar/personal/projects/openconv/crates/shared/src/api/auth.rs`

All structs derive `Debug, Clone, Serialize, Deserialize`.

```rust
/// Registration request with public key, email, and display name.
pub struct RegisterRequest {
    pub public_key: String,
    pub email: String,
    pub display_name: String,
}

/// Registration response with new user ID and auth token.
pub struct RegisterResponse {
    pub user_id: UserId,
    pub token: String,
}

/// Login challenge request with public key.
pub struct LoginChallengeRequest {
    pub public_key: String,
}

/// Login challenge response containing the challenge to sign.
pub struct LoginChallengeResponse {
    pub challenge: String,
}

/// Login verification request with public key and signed challenge.
pub struct LoginVerifyRequest {
    pub public_key: String,
    pub signature: String,
}

/// Login verification response with auth token.
pub struct LoginVerifyResponse {
    pub token: String,
}
```

**Guild types** -- File: `/Users/nisar/personal/projects/openconv/crates/shared/src/api/guild.rs`

```rust
pub struct CreateGuildRequest {
    pub name: String,
}

pub struct GuildResponse {
    pub id: GuildId,
    pub name: String,
    pub owner_id: UserId,
    pub icon_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct GuildListResponse {
    pub guilds: Vec<GuildResponse>,
}
```

**Channel types** -- File: `/Users/nisar/personal/projects/openconv/crates/shared/src/api/channel.rs`

```rust
pub struct CreateChannelRequest {
    pub name: String,
    pub channel_type: String,
}

pub struct ChannelResponse {
    pub id: ChannelId,
    pub guild_id: GuildId,
    pub name: String,
    pub channel_type: String,
    pub position: i32,
}
```

**Message types** -- File: `/Users/nisar/personal/projects/openconv/crates/shared/src/api/message.rs`

```rust
pub struct SendMessageRequest {
    pub encrypted_content: String,
    pub nonce: String,
}

pub struct MessageResponse {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub sender_id: UserId,
    pub encrypted_content: String,
    pub nonce: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

**User types** -- File: `/Users/nisar/personal/projects/openconv/crates/shared/src/api/user.rs`

```rust
pub struct UserProfileResponse {
    pub id: UserId,
    pub display_name: String,
    pub avatar_url: Option<String>,
}
```

### Error Types

File: `/Users/nisar/personal/projects/openconv/crates/shared/src/error.rs`

Use `thiserror` v2 to derive `Display` and `Error`. The enum has five variants:

```rust
/// Shared error type used across server and client.
#[derive(Debug, thiserror::Error)]
pub enum OpenConvError {
    #[error("not found")]
    NotFound,

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("validation error: {0}")]
    Validation(String),

    #[error("internal error: {0}")]
    Internal(String),
}
```

The server (section-03) will implement `axum::response::IntoResponse` for this type, mapping variants to HTTP status codes. The desktop client will use these variants for its own error handling. The `IntoResponse` implementation does NOT belong in the shared crate since it would create an Axum dependency; it belongs in the server crate.

### Constants

File: `/Users/nisar/personal/projects/openconv/crates/shared/src/constants.rs`

```rust
/// Maximum file upload size: 25 MB
pub const MAX_FILE_SIZE_BYTES: usize = 25 * 1024 * 1024;

/// Maximum length for user display names
pub const MAX_DISPLAY_NAME_LENGTH: usize = 64;

/// Maximum length for channel names
pub const MAX_CHANNEL_NAME_LENGTH: usize = 100;

/// Maximum length for guild names
pub const MAX_GUILD_NAME_LENGTH: usize = 100;

/// Maximum size for a single message in bytes
pub const MAX_MESSAGE_SIZE_BYTES: usize = 8 * 1024;
```

The exact values for `MAX_DISPLAY_NAME_LENGTH`, `MAX_CHANNEL_NAME_LENGTH`, `MAX_GUILD_NAME_LENGTH`, and `MAX_MESSAGE_SIZE_BYTES` are reasonable defaults. The critical one is `MAX_FILE_SIZE_BYTES` which must equal `25 * 1024 * 1024`.

### SQLx Feature Flag Integration

The feature flag is wired through three locations:

1. **Shared crate `Cargo.toml`** (shown above): declares `sqlx` as an optional dependency behind a feature flag.

2. **`define_id!` macro** (in `ids.rs`): uses `#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]` and `#[cfg_attr(feature = "sqlx", sqlx(transparent))]` so that when the feature is active, the ID types automatically implement SQLx's `Type`, `Encode`, and `Decode` traits via the transparent wrapper pattern.

3. **Consumer crates** (implemented in later sections):
   - Server (`apps/server/Cargo.toml`): `openconv-shared = { path = "../../crates/shared", features = ["sqlx"] }`
   - Desktop (`apps/desktop/src-tauri/Cargo.toml`): `openconv-shared = { path = "../../../crates/shared" }` (no `sqlx` feature)

---

## Verification

After implementation, verify:

1. `cargo build -p openconv-shared` compiles with no errors or warnings
2. `cargo build -p openconv-shared --features sqlx` compiles with no errors or warnings
3. `cargo test -p openconv-shared` passes all tests
4. `cargo clippy -p openconv-shared -- -D warnings` produces no warnings

---

## Implementation Notes

**Deviation from plan:** The `define_id!` macro uses `#[allow(clippy::new_without_default)]` instead of implementing `Default`. A `Default` that generates random UUIDs is semantically misleading; opted to suppress the clippy lint instead.

**Code review additions:**
- Doc comments added to all public items (constants, error enum, API structs, macro-generated types)
- Roundtrip serialization test added for `UserProfileResponse` in `api/user.rs`
- 28 total tests passing, clippy clean