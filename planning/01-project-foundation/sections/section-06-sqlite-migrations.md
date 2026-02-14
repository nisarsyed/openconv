The project structure doesn't exist yet -- this is a planning document. I now have all the context I need. Let me generate the section content.

# Section 06: SQLite Client Migrations

## Overview

This section implements the client-side SQLite migration system for the Tauri desktop app. The migration runner is embedded in Rust code (not external SQL files) because `rusqlite` does not include a built-in migration runner like SQLx. The runner uses a `_migrations` tracking table to apply migrations idempotently, and creates all client-side cache tables: `local_user`, `cached_users`, `cached_guilds`, `cached_channels`, `cached_messages`, `cached_files`, and `sync_state`.

**File created/modified:** `apps/desktop/src-tauri/src/db.rs`

**Dependencies:** This section depends on **section-05-tauri-scaffold** having created the Tauri crate at `apps/desktop/src-tauri/` with `rusqlite` (0.32, features: bundled) as a dependency. The `db.rs` module is referenced from `lib.rs` (set up in section 05), but the migration logic itself is self-contained.

**No FTS5:** Full-text search is deferred to the messaging split. The `cached_messages` table uses `id TEXT PK` which is incompatible with FTS5's INTEGER rowid requirement. FTS5 virtual tables and sync triggers will be added later.

**No encryption yet:** The `cached_messages` table stores decrypted plaintext for offline access. SQLite encryption (SQLCipher or application-level) is a crypto split concern. This section establishes the table structure only.

---

## Tests (Write First)

All tests live in `apps/desktop/src-tauri/src/db.rs` (as a `#[cfg(test)] mod tests` block) or in a dedicated test file at `apps/desktop/src-tauri/tests/db_tests.rs`. Tests use **in-memory SQLite databases** (`rusqlite::Connection::open_in_memory()`) so no filesystem or cleanup is needed.

```rust
// File: apps/desktop/src-tauri/src/db.rs (bottom of file, in #[cfg(test)] mod tests)

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    // Test: run_migrations creates _migrations table on fresh in-memory database
    #[test]
    fn test_run_migrations_creates_migrations_table() {
        // Open in-memory connection, call run_migrations, then query
        // sqlite_master for a table named '_migrations'. Assert it exists.
    }

    // Test: run_migrations creates all expected tables
    #[test]
    fn test_run_migrations_creates_all_tables() {
        // After run_migrations, query sqlite_master for each table name:
        // local_user, cached_users, cached_guilds, cached_channels,
        // cached_messages, cached_files, sync_state.
        // Assert all 7 tables exist.
    }

    // Test: run_migrations is idempotent (running twice doesn't error)
    #[test]
    fn test_run_migrations_idempotent() {
        // Call run_migrations twice on the same connection.
        // Assert the second call does not return an error.
        // Assert table count is unchanged.
    }

    // Test: _migrations table records applied migrations with version numbers
    #[test]
    fn test_migrations_table_records_versions() {
        // After run_migrations, SELECT * FROM _migrations.
        // Assert rows exist and each has a version (integer) and
        // applied_at (text/timestamp). Assert versions are sequential
        // starting from 1.
    }

    // Test: local_user table has expected columns (insert and query)
    #[test]
    fn test_local_user_table_columns() {
        // INSERT a row into local_user with all columns:
        // id, public_key, email, display_name, avatar_url, token, created_at
        // SELECT it back and assert all values match.
    }

    // Test: cached_users table has expected columns (insert and query)
    #[test]
    fn test_cached_users_table_columns() {
        // INSERT a row into cached_users with columns:
        // id, display_name, avatar_url, updated_at
        // SELECT it back and assert values match.
    }

    // Test: cached_guilds table has expected columns
    #[test]
    fn test_cached_guilds_table_columns() {
        // INSERT a row into cached_guilds with columns:
        // id, name, owner_id, icon_url, joined_at, updated_at
        // SELECT it back and assert values match.
    }

    // Test: cached_channels table has expected columns
    #[test]
    fn test_cached_channels_table_columns() {
        // INSERT a row into cached_channels with columns:
        // id, guild_id, name, channel_type, position, updated_at
        // SELECT it back and assert values match.
    }

    // Test: cached_messages table has expected columns and index on (channel_id, created_at)
    #[test]
    fn test_cached_messages_table_columns_and_index() {
        // INSERT a row into cached_messages with columns:
        // id, channel_id, sender_id, content, nonce, created_at
        // SELECT it back and assert values match.
        // Query sqlite_master for an index on cached_messages with
        // columns (channel_id, created_at). Assert the index exists.
    }

    // Test: cached_files table has expected columns
    #[test]
    fn test_cached_files_table_columns() {
        // INSERT a row into cached_files with columns:
        // id, message_id, file_name, file_size, mime_type, local_path, created_at
        // SELECT it back and assert values match.
    }

    // Test: sync_state table has channel_id as primary key
    #[test]
    fn test_sync_state_primary_key() {
        // INSERT a row into sync_state with channel_id as PK.
        // Attempt to INSERT a second row with the same channel_id.
        // Assert the second insert fails with a constraint violation.
    }
}
```

---

## Implementation Details

### Migration Runner Architecture

The migration runner is a pure function that takes a `&rusqlite::Connection` and applies all pending migrations. There is no framework dependency -- just raw SQL executed through rusqlite.

#### The `_migrations` Tracking Table

```sql
CREATE TABLE IF NOT EXISTS _migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

This table tracks which migrations have already been applied. Each migration has an integer version number starting at 1. The `applied_at` column records when the migration was applied using SQLite's `datetime('now')` function.

#### Migration Runner Function

The `run_migrations` function in `apps/desktop/src-tauri/src/db.rs` should follow this algorithm:

1. Create the `_migrations` table if it does not exist (using `CREATE TABLE IF NOT EXISTS`).
2. Query `SELECT MAX(version) FROM _migrations` to determine the current schema version (0 if no rows).
3. Define an ordered list of migration functions or SQL strings, each associated with a version number.
4. For each migration whose version is greater than the current version:
   a. Execute the migration SQL within a transaction.
   b. Insert a row into `_migrations` recording the version.
5. Return `Ok(())` or a `rusqlite::Result<()>`.

Each migration should be wrapped in a transaction so that a partially-applied migration leaves the database in a known state.

#### Function Signatures

```rust
// File: apps/desktop/src-tauri/src/db.rs

use rusqlite::{Connection, Result};
use std::path::Path;

/// Opens or creates a SQLite database at the given path, runs all pending
/// migrations, and returns the connection.
pub fn init_db(path: &Path) -> Result<Connection> {
    // Open connection, enable WAL mode, run migrations
}

/// Applies all pending migrations to the given connection.
/// Creates the _migrations tracking table if it does not exist.
/// Safe to call multiple times (idempotent).
pub fn run_migrations(conn: &Connection) -> Result<()> {
    // 1. CREATE TABLE IF NOT EXISTS _migrations
    // 2. Get current version
    // 3. Apply pending migrations in order
}
```

### SQLite Pragmas

Before running migrations, the `init_db` function should set these pragmas for performance and reliability:

- `PRAGMA journal_mode=WAL;` -- Write-Ahead Logging for better concurrent read/write performance
- `PRAGMA foreign_keys=ON;` -- Enforce foreign key constraints (off by default in SQLite)

### Migration 1: All Client Tables

A single migration (version 1) creates all seven client-side tables. Since this is the initial schema, there is no need to split into multiple migrations yet.

#### `local_user` Table

Stores the currently logged-in user's profile and auth token. Only one row should exist at a time.

```sql
CREATE TABLE local_user (
    id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL,
    email TEXT NOT NULL,
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    token TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

- `id` -- UUID string of the user (matches server-side `UserId`)
- `public_key` -- The user's public key (hex or base64 encoded string)
- `email` -- The user's email address
- `display_name` -- Display name for the user
- `avatar_url` -- Optional avatar URL
- `token` -- Auth token received from server after login
- `created_at` -- When this row was created locally

#### `cached_users` Table

Caches display names and avatars for other users the client has interacted with. Essential for offline message rendering -- without this table, the UI cannot resolve `sender_id` to a display name when offline.

```sql
CREATE TABLE cached_users (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

#### `cached_guilds` Table

Caches guild (server) metadata for offline access.

```sql
CREATE TABLE cached_guilds (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    icon_url TEXT,
    joined_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

#### `cached_channels` Table

Caches channel metadata within guilds.

```sql
CREATE TABLE cached_channels (
    id TEXT PRIMARY KEY,
    guild_id TEXT NOT NULL,
    name TEXT NOT NULL,
    channel_type TEXT NOT NULL DEFAULT 'text',
    position INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

- `guild_id` -- References a cached guild (not enforced as FK since the guild may not be cached yet)
- `channel_type` -- String value, e.g. "text" or "voice"
- `position` -- Display ordering position within the guild

#### `cached_messages` Table

Caches decrypted message content for offline viewing. Note: this stores **plaintext** content after client-side decryption. Encryption of the SQLite file itself is deferred to the crypto split.

```sql
CREATE TABLE cached_messages (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    content TEXT NOT NULL,
    nonce TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_cached_messages_channel_created
    ON cached_messages (channel_id, created_at);
```

- `channel_id` -- The channel (guild or DM) this message belongs to
- `sender_id` -- The user who sent this message (can be resolved via `cached_users`)
- `content` -- Decrypted plaintext message content
- `nonce` -- Encryption nonce used for this message (stored for re-encryption if needed)
- The composite index on `(channel_id, created_at)` enables efficient chronological message loading per channel

#### `cached_files` Table

Caches file attachment metadata and optional local file paths.

```sql
CREATE TABLE cached_files (
    id TEXT PRIMARY KEY,
    message_id TEXT,
    file_name TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    mime_type TEXT,
    local_path TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

- `message_id` -- The message this file is attached to (nullable, file may be uploaded before message is sent)
- `local_path` -- Path to the downloaded file on the local filesystem (nullable, populated when file is downloaded)

#### `sync_state` Table

Tracks synchronization state per channel, enabling efficient incremental sync with the server.

```sql
CREATE TABLE sync_state (
    channel_id TEXT PRIMARY KEY,
    last_message_id TEXT,
    last_sync_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

- `channel_id` -- The channel being tracked (PK, one row per channel)
- `last_message_id` -- The ID of the most recent message synced for this channel
- `last_sync_at` -- Timestamp of the last successful sync for this channel

### How Migrations are Stored in Code

Define migrations as an array of `(version, sql)` tuples or a slice of string constants. This keeps the migration system simple and avoids external file dependencies.

```rust
const MIGRATIONS: &[(i32, &str)] = &[
    (1, include_str!("../migrations/001_initial_schema.sql")),
    // Future migrations would be added here as (2, "..."), (3, "..."), etc.
];
```

Alternatively, the SQL can be defined inline as string constants if no separate migration files are desired. Either approach is valid -- the key requirement is that migrations are embedded in the binary at compile time and are ordered by version number.

If using a separate SQL file, create it at `apps/desktop/src-tauri/migrations/001_initial_schema.sql` containing all the CREATE TABLE and CREATE INDEX statements from above, concatenated into a single file.

### Integration with Tauri App (from Section 05)

The `init_db` function is called during Tauri app initialization in `lib.rs`. Section 05 sets up the call site; this section provides the implementation. The connection is stored in Tauri's managed state for access by IPC command handlers.

```rust
// In lib.rs (set up by section 05, calls into db.rs from this section)
let app_data_dir = app.path().app_data_dir().expect("failed to get app data dir");
std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
let db_path = app_data_dir.join("openconv.db");
let conn = db::init_db(&db_path).expect("failed to initialize database");
```

---

## Files Summary

| File | Action | Purpose |
|------|--------|---------|
| `apps/desktop/src-tauri/src/db.rs` | Modified | Migration runner (`run_migrations`, `init_db`) and all migration SQL inline |

## Implementation Deviations

- **Inline SQL**: Used inline `const MIGRATION_001: &str` instead of `include_str!` with separate SQL file. Simpler, fewer files.
- **init_db calls run_migrations**: `init_db(path)` now calls `run_migrations` after configuring pragmas. `init_db_in_memory()` does NOT call migrations (test helper for section-05 tests that don't need migrations).
- **Test helper `migrated_conn()`**: Calls `init_db_in_memory()` + `run_migrations()` explicitly for migration tests.
- **unchecked_transaction**: Used `conn.unchecked_transaction()` for migration transactions (avoids nested transaction checks, appropriate since migrations run at startup).
- **Tests**: 15 passing total (4 from section-05 + 11 new migration tests)