use std::path::Path;

use crate::auth_service::AppError;
use rusqlite::Connection;

const MIGRATIONS: &[(i32, &str)] = &[
    (1, MIGRATION_001),
    (2, MIGRATION_002),
    (3, MIGRATION_003),
    (4, MIGRATION_004),
    (5, MIGRATION_005),
];

const MIGRATION_001: &str = "
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    channel_id TEXT,
    dm_channel_id TEXT,
    sender_id TEXT NOT NULL,
    plaintext TEXT,
    ciphertext BLOB,
    message_type TEXT,
    created_at INTEGER NOT NULL,
    edited_at INTEGER,
    decrypted_at INTEGER,
    status TEXT NOT NULL DEFAULT 'delivered'
);

CREATE INDEX idx_messages_channel_created ON messages (channel_id, created_at);
CREATE INDEX idx_messages_dm_channel_created ON messages (dm_channel_id, created_at);
CREATE INDEX idx_messages_status ON messages (status);

CREATE VIRTUAL TABLE messages_fts USING fts5(
    message_id,
    plaintext,
    tokenize='unicode61 remove_diacritics 2',
    prefix='2 3'
);

CREATE TABLE outgoing_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL UNIQUE,
    channel_id TEXT,
    dm_channel_id TEXT,
    plaintext TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    retry_count INTEGER DEFAULT 0,
    last_retry_at INTEGER,
    status TEXT DEFAULT 'pending'
);

CREATE TABLE sync_state (
    channel_id TEXT PRIMARY KEY,
    last_sequence INTEGER NOT NULL,
    last_sync_at INTEGER NOT NULL
);

CREATE TABLE user_cache (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    updated_at INTEGER NOT NULL
);

CREATE TABLE guild_cache (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    icon_url TEXT,
    joined_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE channel_cache (
    id TEXT PRIMARY KEY,
    guild_id TEXT NOT NULL,
    name TEXT NOT NULL,
    channel_type TEXT NOT NULL DEFAULT 'text',
    position INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE read_positions (
    channel_id TEXT PRIMARY KEY,
    last_read_message_id TEXT NOT NULL,
    last_read_created_at INTEGER NOT NULL,
    unread_count INTEGER DEFAULT 0,
    synced_to_server INTEGER DEFAULT 0,
    updated_at INTEGER NOT NULL
);

CREATE TABLE local_user (
    id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL,
    email TEXT NOT NULL,
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    token TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE local_device (
    id TEXT PRIMARY KEY,
    device_name TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE cached_files (
    id TEXT PRIMARY KEY,
    message_id TEXT,
    file_name TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    mime_type TEXT,
    local_path TEXT,
    created_at INTEGER NOT NULL
);
";

const MIGRATION_002: &str = "
CREATE TABLE dm_channel_cache (
    id TEXT PRIMARY KEY,
    participant_ids TEXT NOT NULL,
    last_message_preview TEXT,
    last_message_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_dm_channel_last_message ON dm_channel_cache (last_message_at DESC);
";

const MIGRATION_003: &str = "
ALTER TABLE cached_files ADD COLUMN thumbnail_path TEXT;
";

const MIGRATION_004: &str = "
ALTER TABLE messages ADD COLUMN sender_device_id TEXT;
";

// The Signal device id of the sender, as assigned by the server. Needed to
// rebuild the decrypting ProtocolAddress when retrying a failed decrypt; the
// sender_device_id UUID above cannot serve that role.
const MIGRATION_005: &str = "
ALTER TABLE messages ADD COLUMN sender_signal_device_id INTEGER;
";

pub fn run_cache_migrations(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _cache_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let current_version: i32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM _cache_migrations",
        [],
        |row| row.get(0),
    )?;

    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO _cache_migrations (version) VALUES (?1)",
                [version],
            )?;
            tx.commit()?;
        }
    }

    Ok(())
}

pub fn migrate_from_old_db(new_conn: &Connection, old_db_path: &Path) -> Result<(), AppError> {
    let old_conn = Connection::open(old_db_path)?;
    let tx = new_conn.unchecked_transaction()?;

    // Copy cached_users -> user_cache
    {
        let mut stmt = old_conn.prepare(
            "SELECT id, display_name, avatar_url, strftime('%s', updated_at) FROM cached_users",
        )?;
        let mut insert = new_conn.prepare(
            "INSERT OR IGNORE INTO user_cache (id, display_name, avatar_url, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let display_name: String = row.get(1)?;
            let avatar_url: Option<String> = row.get(2)?;
            let updated_at: i64 = row.get::<_, String>(3)?.parse().unwrap_or(0);
            insert.execute(rusqlite::params![id, display_name, avatar_url, updated_at])?;
        }
    }

    // Copy cached_guilds -> guild_cache
    {
        let mut stmt = old_conn.prepare(
            "SELECT id, name, owner_id, icon_url, strftime('%s', joined_at), strftime('%s', updated_at) FROM cached_guilds",
        )?;
        let mut insert = new_conn.prepare(
            "INSERT OR IGNORE INTO guild_cache (id, name, owner_id, icon_url, joined_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let owner_id: String = row.get(2)?;
            let icon_url: Option<String> = row.get(3)?;
            let joined_at: i64 = row.get::<_, String>(4)?.parse().unwrap_or(0);
            let updated_at: i64 = row.get::<_, String>(5)?.parse().unwrap_or(0);
            insert.execute(rusqlite::params![
                id, name, owner_id, icon_url, joined_at, updated_at
            ])?;
        }
    }

    // Copy cached_channels -> channel_cache
    {
        let mut stmt = old_conn.prepare(
            "SELECT id, guild_id, name, channel_type, position, strftime('%s', updated_at) FROM cached_channels",
        )?;
        let mut insert = new_conn.prepare(
            "INSERT OR IGNORE INTO channel_cache (id, guild_id, name, channel_type, position, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let guild_id: String = row.get(1)?;
            let name: String = row.get(2)?;
            let channel_type: String = row.get(3)?;
            let position: i64 = row.get(4)?;
            let updated_at: i64 = row.get::<_, String>(5)?.parse().unwrap_or(0);
            insert.execute(rusqlite::params![
                id,
                guild_id,
                name,
                channel_type,
                position,
                updated_at
            ])?;
        }
    }

    // Copy local_user
    {
        let mut stmt = old_conn.prepare(
            "SELECT id, public_key, email, display_name, avatar_url, token, strftime('%s', created_at) FROM local_user",
        )?;
        let mut insert = new_conn.prepare(
            "INSERT OR IGNORE INTO local_user (id, public_key, email, display_name, avatar_url, token, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let public_key: String = row.get(1)?;
            let email: String = row.get(2)?;
            let display_name: String = row.get(3)?;
            let avatar_url: Option<String> = row.get(4)?;
            let token: String = row.get(5)?;
            let created_at: i64 = row.get::<_, String>(6)?.parse().unwrap_or(0);
            insert.execute(rusqlite::params![
                id,
                public_key,
                email,
                display_name,
                avatar_url,
                token,
                created_at
            ])?;
        }
    }

    // Copy local_device
    {
        let mut stmt = old_conn
            .prepare("SELECT id, device_name, strftime('%s', created_at) FROM local_device")?;
        let mut insert = new_conn.prepare(
            "INSERT OR IGNORE INTO local_device (id, device_name, created_at) VALUES (?1, ?2, ?3)",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let device_name: String = row.get(1)?;
            let created_at: i64 = row.get::<_, String>(2)?.parse().unwrap_or(0);
            insert.execute(rusqlite::params![id, device_name, created_at])?;
        }
    }

    // Copy cached_files
    {
        let mut stmt = old_conn.prepare(
            "SELECT id, message_id, file_name, file_size, mime_type, local_path, strftime('%s', created_at) FROM cached_files",
        )?;
        let mut insert = new_conn.prepare(
            "INSERT OR IGNORE INTO cached_files (id, message_id, file_name, file_size, mime_type, local_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let message_id: Option<String> = row.get(1)?;
            let file_name: String = row.get(2)?;
            let file_size: i64 = row.get(3)?;
            let mime_type: Option<String> = row.get(4)?;
            let local_path: Option<String> = row.get(5)?;
            let created_at: i64 = row.get::<_, String>(6)?.parse().unwrap_or(0);
            insert.execute(rusqlite::params![
                id, message_id, file_name, file_size, mime_type, local_path, created_at
            ])?;
        }
    }

    // Do NOT copy cached_messages (plaintext security concern)

    tx.commit()?;
    drop(old_conn);
    std::fs::remove_file(old_db_path)
        .map_err(|e| AppError::new(format!("failed to delete old database: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";",
        )
        .unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        run_cache_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_schema_creates_all_tables() {
        let conn = test_conn();
        let expected = [
            "messages",
            "outgoing_queue",
            "sync_state",
            "user_cache",
            "guild_cache",
            "channel_cache",
            "settings",
            "read_positions",
            "messages_fts",
            "local_user",
            "local_device",
            "cached_files",
        ];
        for table in &expected {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap_or_else(|e| panic!("should query for table {table}: {e}"));
            assert!(exists, "table {table} should exist");
        }
    }

    #[test]
    fn test_messages_table_channel_id_xor_dm_channel_id() {
        let conn = test_conn();
        // channel_id set, dm_channel_id null
        conn.execute(
            "INSERT INTO messages (id, channel_id, dm_channel_id, sender_id, created_at, status)
             VALUES ('m1', 'ch1', NULL, 'u1', 1000, 'delivered')",
            [],
        )
        .expect("should insert with channel_id");

        // channel_id null, dm_channel_id set
        conn.execute(
            "INSERT INTO messages (id, channel_id, dm_channel_id, sender_id, created_at, status)
             VALUES ('m2', NULL, 'dm1', 'u1', 1001, 'delivered')",
            [],
        )
        .expect("should insert with dm_channel_id");
    }

    #[test]
    fn test_messages_table_status_values() {
        let conn = test_conn();
        for (i, status) in ["pending", "delivered", "failed", "decrypt_failed"]
            .iter()
            .enumerate()
        {
            conn.execute(
                "INSERT INTO messages (id, channel_id, sender_id, created_at, status)
                 VALUES (?1, 'ch1', 'u1', ?2, ?3)",
                rusqlite::params![format!("m{i}"), 1000 + i as i64, status],
            )
            .unwrap_or_else(|e| panic!("should insert with status {status}: {e}"));
        }
    }

    #[test]
    fn test_settings_key_value_roundtrip() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('theme', 'dark')",
            [],
        )
        .unwrap();
        let value: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'theme'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, "dark");
    }

    #[test]
    fn test_fts5_table_created_and_queryable() {
        let conn = test_conn();
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='messages_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "messages_fts table should exist");

        conn.execute(
            "INSERT INTO messages_fts (message_id, plaintext) VALUES ('m1', 'hello world')",
            [],
        )
        .unwrap();
        let found: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM messages_fts WHERE messages_fts MATCH 'hello'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(found, "FTS5 should find match");
    }

    #[test]
    fn test_fts5_indexes_text() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO messages_fts (message_id, plaintext) VALUES ('m1', 'the quick brown fox')",
            [],
        )
        .unwrap();
        let msg_id: String = conn
            .query_row(
                "SELECT message_id FROM messages_fts WHERE messages_fts MATCH 'quick'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(msg_id, "m1");
    }

    #[test]
    fn test_fts5_delete_removes_entry() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO messages_fts (message_id, plaintext) VALUES ('m1', 'delete me later')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM messages_fts WHERE message_id = 'm1'", [])
            .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'delete'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_fts5_independent_of_messages_plaintext() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO messages (id, channel_id, sender_id, plaintext, created_at, status)
             VALUES ('m1', 'ch1', 'u1', 'searchable text here', 1000, 'delivered')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages_fts (message_id, plaintext) VALUES ('m1', 'searchable text here')",
            [],
        )
        .unwrap();

        // Clear plaintext from messages table (simulating TTL)
        conn.execute("UPDATE messages SET plaintext = NULL WHERE id = 'm1'", [])
            .unwrap();

        // FTS should still find it
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'searchable'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_outgoing_queue_autoincrement() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO outgoing_queue (message_id, plaintext, created_at) VALUES ('m1', 'hello', 1000)",
            [],
        )
        .unwrap();
        let id1: i64 = conn
            .query_row(
                "SELECT id FROM outgoing_queue WHERE message_id = 'm1'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        conn.execute(
            "INSERT INTO outgoing_queue (message_id, plaintext, created_at) VALUES ('m2', 'world', 1001)",
            [],
        )
        .unwrap();
        let id2: i64 = conn
            .query_row(
                "SELECT id FROM outgoing_queue WHERE message_id = 'm2'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(id2 > id1, "second id should be greater than first");
    }

    #[test]
    fn test_outgoing_queue_message_id_unique() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO outgoing_queue (message_id, plaintext, created_at) VALUES ('m1', 'hello', 1000)",
            [],
        )
        .unwrap();
        let result = conn.execute(
            "INSERT INTO outgoing_queue (message_id, plaintext, created_at) VALUES ('m1', 'dupe', 1001)",
            [],
        );
        assert!(result.is_err(), "duplicate message_id should fail");
    }

    #[test]
    fn test_migrations_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";",
        )
        .unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();

        run_cache_migrations(&conn).unwrap();
        let count1: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        run_cache_migrations(&conn).unwrap();
        let count2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(count1 > 0, "tables should exist after first run");
        assert_eq!(
            count1, count2,
            "table count should be unchanged after second run"
        );
    }

    #[test]
    fn test_read_positions_primary_key() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO read_positions (channel_id, last_read_message_id, last_read_created_at, updated_at)
             VALUES ('ch1', 'm1', 1000, 1000)",
            [],
        )
        .unwrap();
        let result = conn.execute(
            "INSERT INTO read_positions (channel_id, last_read_message_id, last_read_created_at, updated_at)
             VALUES ('ch1', 'm2', 1001, 1001)",
            [],
        );
        assert!(result.is_err(), "duplicate channel_id should fail");
    }

    #[test]
    fn test_sync_state_primary_key() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO sync_state (channel_id, last_sequence, last_sync_at) VALUES ('ch1', 1, 1000)",
            [],
        )
        .unwrap();
        let result = conn.execute(
            "INSERT INTO sync_state (channel_id, last_sequence, last_sync_at) VALUES ('ch1', 2, 1001)",
            [],
        );
        assert!(result.is_err(), "duplicate channel_id should fail");
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    fn setup_old_db(conn: &Connection) {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;",
        )
        .unwrap();
        crate::db::run_migrations(conn).unwrap();
    }

    #[test]
    fn test_migration_copies_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("openconv.db");

        // Create old DB with data
        {
            let old_conn = Connection::open(&old_path).unwrap();
            setup_old_db(&old_conn);
            old_conn
                .execute(
                    "INSERT INTO cached_users (id, display_name, avatar_url) VALUES ('u1', 'Alice', 'https://img/a.png')",
                    [],
                )
                .unwrap();
            old_conn
                .execute(
                    "INSERT INTO cached_guilds (id, name, owner_id, icon_url, joined_at) VALUES ('g1', 'Guild', 'u1', NULL, '2024-01-01T00:00:00')",
                    [],
                )
                .unwrap();
            old_conn
                .execute(
                    "INSERT INTO cached_channels (id, guild_id, name, channel_type, position) VALUES ('c1', 'g1', 'general', 'text', 0)",
                    [],
                )
                .unwrap();
        }

        // Create new encrypted DB and migrate
        let new_conn = Connection::open_in_memory().unwrap();
        new_conn
            .execute_batch(
                "PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";",
            )
            .unwrap();
        new_conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        new_conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        run_cache_migrations(&new_conn).unwrap();
        migrate_from_old_db(&new_conn, &old_path).unwrap();

        // Verify data
        let name: String = new_conn
            .query_row(
                "SELECT display_name FROM user_cache WHERE id = 'u1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "Alice");

        let guild_name: String = new_conn
            .query_row("SELECT name FROM guild_cache WHERE id = 'g1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(guild_name, "Guild");

        let channel_name: String = new_conn
            .query_row(
                "SELECT name FROM channel_cache WHERE id = 'c1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(channel_name, "general");

        // Old DB should be deleted
        assert!(!old_path.exists(), "old db should be deleted");
    }

    #[test]
    fn test_migration_discards_plaintext_messages() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("openconv.db");

        {
            let old_conn = Connection::open(&old_path).unwrap();
            setup_old_db(&old_conn);
            old_conn
                .execute(
                    "INSERT INTO cached_messages (id, channel_id, sender_id, content, created_at)
                     VALUES ('m1', 'c1', 'u1', 'secret message', '2024-01-01T00:00:00')",
                    [],
                )
                .unwrap();
        }

        let new_conn = Connection::open_in_memory().unwrap();
        new_conn
            .execute_batch(
                "PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";",
            )
            .unwrap();
        new_conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        new_conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        run_cache_migrations(&new_conn).unwrap();
        migrate_from_old_db(&new_conn, &old_path).unwrap();

        let count: i64 = new_conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            count, 0,
            "messages should be empty (plaintext not migrated)"
        );
    }

    #[test]
    fn test_migration_copies_local_user_and_device() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("openconv.db");

        {
            let old_conn = Connection::open(&old_path).unwrap();
            setup_old_db(&old_conn);
            old_conn
                .execute(
                    "INSERT INTO local_user (id, public_key, email, display_name, token)
                     VALUES ('u1', 'pk1', 'a@b.com', 'Alice', 'tok1')",
                    [],
                )
                .unwrap();
            old_conn
                .execute(
                    "INSERT INTO local_device (id, device_name)
                     VALUES ('d1', 'MacBook')",
                    [],
                )
                .unwrap();
        }

        let new_conn = Connection::open_in_memory().unwrap();
        new_conn
            .execute_batch(
                "PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";",
            )
            .unwrap();
        new_conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        new_conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        run_cache_migrations(&new_conn).unwrap();
        migrate_from_old_db(&new_conn, &old_path).unwrap();

        let email: String = new_conn
            .query_row("SELECT email FROM local_user WHERE id = 'u1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(email, "a@b.com");

        let device_name: String = new_conn
            .query_row(
                "SELECT device_name FROM local_device WHERE id = 'd1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(device_name, "MacBook");
    }
}
