No earlier sections have been written yet. Now I have everything I need to generate the section content.

# Section 4: PostgreSQL Migrations

## Overview

This section creates all 9 SQLx migration files for the server's PostgreSQL database. Migrations live in `/Users/nisar/personal/projects/openconv/apps/server/migrations/` and are executed sequentially by SQLx's built-in migration runner (`sqlx::migrate!()`), which the server scaffold (section 03) invokes at startup. Migrations are forward-only (no down files). Recovery from a bad migration is handled by dropping and recreating the database via `just db-reset`.

## Dependencies

- **section-01-monorepo-setup**: Directory structure and workspace configuration must exist.
- **section-03-server-scaffold**: The Axum server must be configured to call `sqlx::migrate!()` at startup, and the `PgPool` must be initialized in `AppState`. The server's `Cargo.toml` must include `sqlx` with `runtime-tokio, postgres, uuid, chrono` features.

## Tests (Write First)

All migration tests use the `sqlx::test` attribute macro, which automatically creates a temporary test database, runs all pending migrations, and tears down the database after each test. These tests belong in the server crate at `/Users/nisar/personal/projects/openconv/apps/server/tests/migrations.rs`.

**Important:** To use `sqlx::test`, the test file needs `sqlx::PgPool` as the function parameter. SQLx automatically runs all migrations from the configured migrations directory before each test.

```rust
// File: /Users/nisar/personal/projects/openconv/apps/server/tests/migrations.rs

use sqlx::PgPool;

/// All migrations apply successfully to a fresh database.
/// The sqlx::test macro itself validates this — if any migration SQL
/// is invalid, the test will fail before the function body runs.
#[sqlx::test]
async fn all_migrations_apply_successfully(pool: PgPool) {
    // If we reach here, all migrations ran without error.
    let row: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, 1);
}

/// Users table has expected columns and accepts a valid insert.
#[sqlx::test]
async fn users_table_insert_and_select(pool: PgPool) {
    // Insert a row with all required columns (id, public_key, email, display_name)
    // Select it back and verify fields match
}

/// Users table enforces UNIQUE constraint on public_key.
#[sqlx::test]
async fn users_table_unique_public_key(pool: PgPool) {
    // Insert two users with the same public_key
    // Assert the second insert fails with a unique violation
}

/// Users table enforces UNIQUE constraint on email.
#[sqlx::test]
async fn users_table_unique_email(pool: PgPool) {
    // Insert two users with the same email
    // Assert the second insert fails with a unique violation
}

/// The updated_at trigger automatically updates on user row modification.
#[sqlx::test]
async fn users_updated_at_trigger(pool: PgPool) {
    // Insert a user, record created_at/updated_at
    // Sleep briefly or use clock_timestamp() trick
    // Update the user's display_name
    // Assert updated_at has changed and is > the original value
}

/// Guilds table FK on owner_id rejects a nonexistent user.
#[sqlx::test]
async fn guilds_fk_owner_id_rejects_nonexistent_user(pool: PgPool) {
    // Attempt to insert a guild with a random UUID as owner_id
    // Assert it fails with a foreign key violation
}

/// The updated_at trigger automatically updates on guild row modification.
#[sqlx::test]
async fn guilds_updated_at_trigger(pool: PgPool) {
    // Create a user, then a guild owned by that user
    // Update the guild name
    // Assert updated_at changed
}

/// Channels table CASCADE deletes when guild is deleted.
#[sqlx::test]
async fn channels_cascade_delete_on_guild_delete(pool: PgPool) {
    // Create a user, guild, and channel
    // Delete the guild
    // Assert the channel no longer exists
}

/// Channels table enforces UNIQUE on (guild_id, name).
#[sqlx::test]
async fn channels_unique_guild_id_name(pool: PgPool) {
    // Create a user, guild, and channel with name "general"
    // Attempt to create another channel with the same guild_id and name "general"
    // Assert the second insert fails with a unique violation
}

/// Messages table CHECK constraint rejects null channel_id AND null dm_channel_id.
#[sqlx::test]
async fn messages_check_rejects_both_null(pool: PgPool) {
    // Create a user
    // Attempt to insert a message with both channel_id and dm_channel_id as NULL
    // Assert it fails with a check constraint violation
}

/// Messages table CHECK constraint rejects both channel_id AND dm_channel_id set.
#[sqlx::test]
async fn messages_check_rejects_both_set(pool: PgPool) {
    // Create a user, guild, channel, and dm_channel
    // Attempt to insert a message with both channel_id and dm_channel_id set
    // Assert it fails with a check constraint violation
}

/// Messages table accepts channel_id set with dm_channel_id null.
#[sqlx::test]
async fn messages_accepts_channel_id_only(pool: PgPool) {
    // Create a user, guild, channel
    // Insert a message with channel_id set and dm_channel_id NULL
    // Assert insert succeeds
}

/// Messages table accepts dm_channel_id set with channel_id null.
#[sqlx::test]
async fn messages_accepts_dm_channel_id_only(pool: PgPool) {
    // Create a user and dm_channel (with membership)
    // Insert a message with dm_channel_id set and channel_id NULL
    // Assert insert succeeds
}

/// Guild members composite PK prevents duplicate membership.
#[sqlx::test]
async fn guild_members_no_duplicate_membership(pool: PgPool) {
    // Create a user and guild
    // Insert user into guild_members
    // Attempt to insert the same user+guild again
    // Assert it fails with a primary key violation
}

/// DM channel members composite PK prevents duplicate membership.
#[sqlx::test]
async fn dm_channel_members_no_duplicate_membership(pool: PgPool) {
    // Create a user and dm_channel
    // Insert user into dm_channel_members
    // Attempt to insert the same user+dm_channel again
    // Assert it fails with a primary key violation
}

/// Files table allows null message_id.
#[sqlx::test]
async fn files_allows_null_message_id(pool: PgPool) {
    // Create a user
    // Insert a file row with message_id = NULL
    // Assert insert succeeds
}
```

### Test Configuration

The `sqlx::test` macro requires:
1. A `DATABASE_URL` environment variable pointing to a PostgreSQL instance (the test runner creates temporary databases from it).
2. The `.env` file or test environment must contain: `DATABASE_URL=postgres://openconv:openconv@localhost:5432/openconv`
3. The SQLx offline data in `.sqlx/` must be up to date (run `just sqlx-prepare` after writing queries).

## Migration Files

All files are created in `/Users/nisar/personal/projects/openconv/apps/server/migrations/`. SQLx migration filenames follow the pattern `YYYYMMDDHHMMSS_description.sql`. Use sequential timestamps to guarantee ordering. The exact timestamps are not critical, but they must be strictly increasing.

### Migration 1: Utility functions and users table

**File:** `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000001_users.sql`

This migration creates two things:

**1. The `set_updated_at()` trigger function** -- a reusable PostgreSQL trigger function that automatically sets `updated_at = NOW()` whenever a row is modified. This function is applied to every table that has an `updated_at` column.

```sql
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

**2. The `users` table** with the following columns:

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | `UUID` | PRIMARY KEY, DEFAULT `gen_random_uuid()` |
| `public_key` | `TEXT` | NOT NULL, UNIQUE |
| `email` | `TEXT` | NOT NULL, UNIQUE |
| `display_name` | `TEXT` | NOT NULL |
| `avatar_url` | `TEXT` | nullable |
| `created_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |
| `updated_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |

Additional elements:
- Index on `email`: `CREATE INDEX idx_users_email ON users (email);`
- Apply the `set_updated_at` trigger: `CREATE TRIGGER trigger_users_updated_at BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION set_updated_at();`

**Design note on IDs:** The `id` column uses `UUID` with `gen_random_uuid()` as a default. Application code should generate UUID v7 (time-sortable) IDs using the shared crate's `UserId::new()` and pass them explicitly on insert. The `gen_random_uuid()` default is a safety net only.

### Migration 2: Pre-key bundles table

**File:** `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000002_pre_key_bundles.sql`

The pre-key bundles table stores Signal protocol pre-key bundles uploaded by clients for X3DH key exchange.

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | `UUID` | PRIMARY KEY, DEFAULT `gen_random_uuid()` |
| `user_id` | `UUID` | NOT NULL, FK -> `users(id)` ON DELETE CASCADE |
| `key_data` | `BYTEA` | NOT NULL |
| `is_used` | `BOOLEAN` | NOT NULL, DEFAULT `false` |
| `created_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |

Additional elements:
- Index on `user_id`: `CREATE INDEX idx_pre_key_bundles_user_id ON pre_key_bundles (user_id);`

### Migration 3: Guilds table

**File:** `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000003_guilds.sql`

Guilds are the top-level organizational unit (equivalent to Discord "servers").

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | `UUID` | PRIMARY KEY, DEFAULT `gen_random_uuid()` |
| `name` | `TEXT` | NOT NULL |
| `owner_id` | `UUID` | NOT NULL, FK -> `users(id)` |
| `icon_url` | `TEXT` | nullable |
| `created_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |
| `updated_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |

Additional elements:
- Index on `owner_id`: `CREATE INDEX idx_guilds_owner_id ON guilds (owner_id);`
- Apply the `set_updated_at` trigger: `CREATE TRIGGER trigger_guilds_updated_at BEFORE UPDATE ON guilds FOR EACH ROW EXECUTE FUNCTION set_updated_at();`

**Note:** The FK on `owner_id` does NOT cascade delete -- deleting a user should not automatically delete all guilds they own. Guild deletion should be an explicit, intentional action handled by application logic (e.g., ownership transfer or confirmation).

### Migration 4: Channels table

**File:** `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000004_channels.sql`

Channels belong to guilds and are where messages are posted.

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | `UUID` | PRIMARY KEY, DEFAULT `gen_random_uuid()` |
| `guild_id` | `UUID` | NOT NULL, FK -> `guilds(id)` ON DELETE CASCADE |
| `name` | `TEXT` | NOT NULL |
| `channel_type` | `TEXT` | NOT NULL, DEFAULT `'text'` |
| `position` | `INTEGER` | NOT NULL, DEFAULT `0` |
| `created_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |
| `updated_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |

Additional elements:
- Unique constraint on `(guild_id, name)`: `ALTER TABLE channels ADD CONSTRAINT uq_channels_guild_name UNIQUE (guild_id, name);`
- Index on `guild_id`: `CREATE INDEX idx_channels_guild_id ON channels (guild_id);`
- Apply the `set_updated_at` trigger: `CREATE TRIGGER trigger_channels_updated_at BEFORE UPDATE ON channels FOR EACH ROW EXECUTE FUNCTION set_updated_at();`

The CASCADE DELETE on `guild_id` means deleting a guild automatically removes all its channels.

### Migration 5: Roles table

**File:** `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000005_roles.sql`

Roles define permission sets within a guild. Three default roles per guild (e.g., owner, moderator, member) will be inserted by application code when a guild is created -- not by this migration.

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | `UUID` | PRIMARY KEY, DEFAULT `gen_random_uuid()` |
| `guild_id` | `UUID` | NOT NULL, FK -> `guilds(id)` ON DELETE CASCADE |
| `name` | `TEXT` | NOT NULL |
| `permissions` | `BIGINT` | NOT NULL, DEFAULT `0` |
| `position` | `INTEGER` | NOT NULL, DEFAULT `0` |
| `created_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |

The CASCADE DELETE on `guild_id` means deleting a guild removes all its roles.

### Migration 6: Guild members and guild member roles tables

**File:** `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000006_guild_members.sql`

Two junction tables for the guild membership and role assignment many-to-many relationships.

**`guild_members` table:**

| Column | Type | Constraints |
|--------|------|-------------|
| `user_id` | `UUID` | NOT NULL, FK -> `users(id)` ON DELETE CASCADE |
| `guild_id` | `UUID` | NOT NULL, FK -> `guilds(id)` ON DELETE CASCADE |
| `joined_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |

- Composite primary key: `PRIMARY KEY (user_id, guild_id)`

**`guild_member_roles` table:**

| Column | Type | Constraints |
|--------|------|-------------|
| `user_id` | `UUID` | NOT NULL |
| `guild_id` | `UUID` | NOT NULL |
| `role_id` | `UUID` | NOT NULL, FK -> `roles(id)` ON DELETE CASCADE |

- Triple composite primary key: `PRIMARY KEY (user_id, guild_id, role_id)`
- Foreign key on `(user_id, guild_id)` referencing `guild_members(user_id, guild_id)` ON DELETE CASCADE -- removing a member from a guild automatically removes their role assignments.

### Migration 7: DM tables

**File:** `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000007_dm_channels.sql`

Direct message channels are separate from guild channels. A DM channel is a container that holds one or more members and can have messages.

**`dm_channels` table:**

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | `UUID` | PRIMARY KEY, DEFAULT `gen_random_uuid()` |
| `created_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |

**`dm_channel_members` table:**

| Column | Type | Constraints |
|--------|------|-------------|
| `dm_channel_id` | `UUID` | NOT NULL, FK -> `dm_channels(id)` ON DELETE CASCADE |
| `user_id` | `UUID` | NOT NULL, FK -> `users(id)` ON DELETE CASCADE |

- Composite primary key: `PRIMARY KEY (dm_channel_id, user_id)`

### Migration 8: Messages table (unified)

**File:** `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000008_messages.sql`

A single unified messages table serves both guild channel messages and DM messages. A message belongs to exactly one of `channel_id` (guild channel) or `dm_channel_id` (DM channel), never both and never neither. A CHECK constraint enforces this invariant at the database level.

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | `UUID` | PRIMARY KEY, DEFAULT `gen_random_uuid()` |
| `channel_id` | `UUID` | nullable, FK -> `channels(id)` ON DELETE CASCADE |
| `dm_channel_id` | `UUID` | nullable, FK -> `dm_channels(id)` ON DELETE CASCADE |
| `sender_id` | `UUID` | NOT NULL, FK -> `users(id)` |
| `encrypted_content` | `TEXT` | NOT NULL |
| `nonce` | `TEXT` | NOT NULL |
| `edited_at` | `TIMESTAMPTZ` | nullable |
| `deleted` | `BOOLEAN` | NOT NULL, DEFAULT `false` |
| `created_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |

Additional elements:
- CHECK constraint: `ALTER TABLE messages ADD CONSTRAINT chk_messages_channel_xor CHECK ((channel_id IS NOT NULL AND dm_channel_id IS NULL) OR (channel_id IS NULL AND dm_channel_id IS NOT NULL));`
- Composite index for guild message history: `CREATE INDEX idx_messages_channel_created ON messages (channel_id, created_at) WHERE channel_id IS NOT NULL;`
- Composite index for DM message history: `CREATE INDEX idx_messages_dm_channel_created ON messages (dm_channel_id, created_at) WHERE dm_channel_id IS NOT NULL;`

**Design rationale:** Unifying guild and DM messages into one table simplifies the query layer -- message retrieval, pagination, and search all use the same table. The CHECK constraint guarantees data integrity. The partial indexes (with `WHERE ... IS NOT NULL`) keep index size small and lookups fast since each index only covers the relevant subset of rows.

**Soft delete:** The `deleted` boolean supports soft delete -- messages marked as deleted are retained in the database but hidden from the UI. The `encrypted_content` of deleted messages may be cleared by application logic.

**FK on sender_id:** Does NOT cascade delete. Deleting a user should not delete their messages -- the display might show "Deleted User" instead.

### Migration 9: Files table

**File:** `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000009_files.sql`

Files are encrypted blobs uploaded by users, optionally attached to messages.

| Column | Type | Constraints |
|--------|------|-------------|
| `id` | `UUID` | PRIMARY KEY, DEFAULT `gen_random_uuid()` |
| `uploader_id` | `UUID` | NOT NULL, FK -> `users(id)` |
| `message_id` | `UUID` | nullable, FK -> `messages(id)` ON DELETE SET NULL |
| `file_name` | `TEXT` | NOT NULL |
| `mime_type` | `TEXT` | NOT NULL |
| `size_bytes` | `BIGINT` | NOT NULL |
| `storage_path` | `TEXT` | NOT NULL |
| `encrypted_blob_key` | `TEXT` | NOT NULL |
| `created_at` | `TIMESTAMPTZ` | NOT NULL, DEFAULT `NOW()` |

Additional elements:
- Index on `message_id`: `CREATE INDEX idx_files_message_id ON files (message_id) WHERE message_id IS NOT NULL;`

**Nullable message_id:** Files can exist without being attached to a message. This supports the upload flow where a file is uploaded first (getting a file ID back), then attached to a message when the message is sent. If a message is deleted, the file's `message_id` is set to NULL (ON DELETE SET NULL) rather than deleting the file.

## Implementation Checklist

1. Create the migrations directory: `mkdir -p /Users/nisar/personal/projects/openconv/apps/server/migrations/`
2. Write all 9 migration SQL files in the order listed above.
3. Write the migration integration tests at `/Users/nisar/personal/projects/openconv/apps/server/tests/migrations.rs`.
4. Ensure Docker Compose PostgreSQL is running (`just db-up`).
5. Run migrations against the local database (`just db-migrate`) and verify they apply cleanly.
6. Run `just sqlx-prepare` to generate/update the `.sqlx/` offline query data.
7. Run the test suite (`cargo test --workspace`) and verify all migration tests pass.

## Code Review Changes

- Removed redundant `idx_users_email` index (UNIQUE constraint on email already creates an automatic index)
- Strengthened all constraint-violation test assertions to check specific PostgreSQL error codes (23505, 23503, 23514) instead of generic `is_err()`
- Added `chrono` as dev-dependency for timestamp assertions in trigger tests
- Created `.env` file with `DATABASE_URL` for `sqlx::test` (gitignored, not committed)
- PostgreSQL started via `docker run` for testing (docker-compose deferred to section 08)
- Missing test coverage for pre_key_bundles, roles, guild_member_roles, sender_id non-cascade, and files ON DELETE SET NULL deferred to section 09

## Test Summary

- **Integration tests (16):** migrations apply (1), users CRUD + constraints (4), guilds FK + triggers (2), channels cascade + unique (2), messages CHECK constraints (4), guild/DM member PKs (2), files nullable FK (1)

## File Summary

| File Path | Action |
|-----------|--------|
| `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000001_users.sql` | Create |
| `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000002_pre_key_bundles.sql` | Create |
| `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000003_guilds.sql` | Create |
| `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000004_channels.sql` | Create |
| `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000005_roles.sql` | Create |
| `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000006_guild_members.sql` | Create |
| `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000007_dm_channels.sql` | Create |
| `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000008_messages.sql` | Create |
| `/Users/nisar/personal/projects/openconv/apps/server/migrations/20240101000009_files.sql` | Create |
| `/Users/nisar/personal/projects/openconv/apps/server/tests/migrations.rs` | Create |