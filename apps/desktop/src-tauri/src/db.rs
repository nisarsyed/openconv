use rusqlite::{Connection, Result};

fn configure_connection(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;",
    )
}

const MIGRATIONS: &[(i32, &str)] = &[(1, MIGRATION_001)];

const MIGRATION_001: &str = "
CREATE TABLE local_user (
    id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL,
    email TEXT NOT NULL,
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    token TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE cached_users (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE cached_guilds (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    icon_url TEXT,
    joined_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE cached_channels (
    id TEXT PRIMARY KEY,
    guild_id TEXT NOT NULL,
    name TEXT NOT NULL,
    channel_type TEXT NOT NULL DEFAULT 'text',
    position INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

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

CREATE TABLE cached_files (
    id TEXT PRIMARY KEY,
    message_id TEXT,
    file_name TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    mime_type TEXT,
    local_path TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE sync_state (
    channel_id TEXT PRIMARY KEY,
    last_message_id TEXT,
    last_sync_at TEXT NOT NULL DEFAULT (datetime('now'))
);
";

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let current_version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            [],
            |row| row.get(0),
        )?;

    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO _migrations (version) VALUES (?1)",
                [version],
            )?;
            tx.commit()?;
        }
    }

    Ok(())
}

pub fn init_db(path: &std::path::Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    configure_connection(&conn)?;
    run_migrations(&conn)?;
    Ok(conn)
}

pub fn init_db_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure_connection(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_db_in_memory() {
        let conn = init_db_in_memory().expect("should create in-memory db");
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("should query journal_mode");
        assert!(
            mode == "wal" || mode == "memory",
            "unexpected journal_mode: {mode}"
        );
    }

    #[test]
    fn test_init_db_connection_is_functional() {
        let conn = init_db_in_memory().expect("should create in-memory db");
        let result: i64 = conn
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("should execute query");
        assert_eq!(result, 1);

        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("should query foreign_keys");
        assert_eq!(fk, 1, "foreign_keys should be enabled");
    }

    fn migrated_conn() -> Connection {
        let conn = init_db_in_memory().expect("should create in-memory db");
        run_migrations(&conn).expect("should run migrations");
        conn
    }

    #[test]
    fn test_run_migrations_creates_migrations_table() {
        let conn = migrated_conn();
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_migrations'",
                [],
                |row| row.get(0),
            )
            .expect("should query sqlite_master");
        assert!(exists, "_migrations table should exist");
    }

    #[test]
    fn test_run_migrations_creates_all_tables() {
        let conn = migrated_conn();
        let expected = [
            "local_user",
            "cached_users",
            "cached_guilds",
            "cached_channels",
            "cached_messages",
            "cached_files",
            "sync_state",
        ];
        for table in &expected {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .expect("should query sqlite_master");
            assert!(exists, "table {table} should exist");
        }
    }

    #[test]
    fn test_run_migrations_idempotent() {
        let conn = init_db_in_memory().expect("should create db");
        run_migrations(&conn).expect("first run should succeed");

        let count_after_first: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .expect("should count tables");

        run_migrations(&conn).expect("second run should succeed");

        let count_after_second: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .expect("should count tables");

        assert!(count_after_first > 0, "tables should exist after first run");
        assert_eq!(
            count_after_first, count_after_second,
            "table count should be unchanged after second run"
        );
    }

    #[test]
    fn test_migrations_table_records_versions() {
        let conn = migrated_conn();
        let mut stmt = conn
            .prepare("SELECT version, applied_at FROM _migrations ORDER BY version")
            .expect("should prepare query");
        let rows: Vec<(i32, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("should query")
            .collect::<Result<Vec<_>>>()
            .expect("should collect");

        assert!(!rows.is_empty(), "should have migration records");
        assert_eq!(rows[0].0, 1, "first version should be 1");
        assert!(!rows[0].1.is_empty(), "applied_at should not be empty");
    }

    #[test]
    fn test_local_user_table_columns() {
        let conn = migrated_conn();
        conn.execute(
            "INSERT INTO local_user (id, public_key, email, display_name, avatar_url, token)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            ["user1", "pubkey123", "test@example.com", "Test User", "https://img.test/a.png", "token123"],
        )
        .expect("should insert");

        let (id, pk, email, name, avatar, token): (String, String, String, String, Option<String>, String) = conn
            .query_row("SELECT id, public_key, email, display_name, avatar_url, token FROM local_user", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
            })
            .expect("should query");

        assert_eq!(id, "user1");
        assert_eq!(pk, "pubkey123");
        assert_eq!(email, "test@example.com");
        assert_eq!(name, "Test User");
        assert_eq!(avatar.as_deref(), Some("https://img.test/a.png"));
        assert_eq!(token, "token123");
    }

    #[test]
    fn test_cached_users_table_columns() {
        let conn = migrated_conn();
        conn.execute(
            "INSERT INTO cached_users (id, display_name, avatar_url) VALUES (?1, ?2, ?3)",
            ["u1", "Alice", "https://img.test/alice.png"],
        )
        .expect("should insert");

        let (id, name, avatar): (String, String, Option<String>) = conn
            .query_row("SELECT id, display_name, avatar_url FROM cached_users", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .expect("should query");

        assert_eq!(id, "u1");
        assert_eq!(name, "Alice");
        assert_eq!(avatar.as_deref(), Some("https://img.test/alice.png"));
    }

    #[test]
    fn test_cached_guilds_table_columns() {
        let conn = migrated_conn();
        conn.execute(
            "INSERT INTO cached_guilds (id, name, owner_id, icon_url, joined_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            ["g1", "Test Guild", "owner1", "https://img.test/icon.png", "2024-01-01T00:00:00"],
        )
        .expect("should insert");

        let (id, name, owner, icon): (String, String, String, Option<String>) = conn
            .query_row("SELECT id, name, owner_id, icon_url FROM cached_guilds", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .expect("should query");

        assert_eq!(id, "g1");
        assert_eq!(name, "Test Guild");
        assert_eq!(owner, "owner1");
        assert_eq!(icon.as_deref(), Some("https://img.test/icon.png"));
    }

    #[test]
    fn test_cached_channels_table_columns() {
        let conn = migrated_conn();
        conn.execute(
            "INSERT INTO cached_channels (id, guild_id, name, channel_type, position)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["c1", "g1", "general", "text", 0i64],
        )
        .expect("should insert");

        let (id, guild_id, name, ch_type, pos): (String, String, String, String, i64) = conn
            .query_row(
                "SELECT id, guild_id, name, channel_type, position FROM cached_channels",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .expect("should query");

        assert_eq!(id, "c1");
        assert_eq!(guild_id, "g1");
        assert_eq!(name, "general");
        assert_eq!(ch_type, "text");
        assert_eq!(pos, 0);
    }

    #[test]
    fn test_cached_messages_table_columns_and_index() {
        let conn = migrated_conn();
        conn.execute(
            "INSERT INTO cached_messages (id, channel_id, sender_id, content, nonce, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            ["m1", "c1", "u1", "hello world", "nonce123", "2024-01-01T00:00:00"],
        )
        .expect("should insert");

        let (id, ch, sender, content, nonce): (String, String, String, String, Option<String>) = conn
            .query_row(
                "SELECT id, channel_id, sender_id, content, nonce FROM cached_messages",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .expect("should query");

        assert_eq!(id, "m1");
        assert_eq!(ch, "c1");
        assert_eq!(sender, "u1");
        assert_eq!(content, "hello world");
        assert_eq!(nonce.as_deref(), Some("nonce123"));

        // Verify index exists
        let idx_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='index' AND name='idx_cached_messages_channel_created'",
                [],
                |row| row.get(0),
            )
            .expect("should query for index");
        assert!(idx_exists, "index idx_cached_messages_channel_created should exist");
    }

    #[test]
    fn test_cached_files_table_columns() {
        let conn = migrated_conn();
        conn.execute(
            "INSERT INTO cached_files (id, message_id, file_name, file_size, mime_type, local_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            ["f1", "m1", "photo.jpg", "1024", "image/jpeg", "/tmp/photo.jpg"],
        )
        .expect("should insert");

        let (id, msg_id, fname, fsize, mime, path): (String, Option<String>, String, i64, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT id, message_id, file_name, file_size, mime_type, local_path FROM cached_files",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .expect("should query");

        assert_eq!(id, "f1");
        assert_eq!(msg_id.as_deref(), Some("m1"));
        assert_eq!(fname, "photo.jpg");
        assert_eq!(fsize, 1024);
        assert_eq!(mime.as_deref(), Some("image/jpeg"));
        assert_eq!(path.as_deref(), Some("/tmp/photo.jpg"));
    }

    #[test]
    fn test_sync_state_primary_key() {
        let conn = migrated_conn();
        conn.execute(
            "INSERT INTO sync_state (channel_id, last_message_id) VALUES (?1, ?2)",
            ["c1", "m100"],
        )
        .expect("should insert first row");

        let result = conn.execute(
            "INSERT INTO sync_state (channel_id, last_message_id) VALUES (?1, ?2)",
            ["c1", "m200"],
        );
        assert!(result.is_err(), "duplicate channel_id should fail");
    }
}
