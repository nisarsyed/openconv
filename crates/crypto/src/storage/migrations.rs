//! Crypto migration runner — separate from the desktop app's `_migrations` table.

use crate::error::CryptoError;
use rusqlite::Connection;

const MIGRATIONS: &[(i32, &str)] = &[(1, MIGRATION_001), (2, MIGRATION_002)];

const MIGRATION_001: &str = "
CREATE TABLE IF NOT EXISTS crypto_identity_keys (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    public_key  BLOB NOT NULL,
    private_key BLOB NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS crypto_trusted_identities (
    address       TEXT NOT NULL,
    device_id     INTEGER NOT NULL DEFAULT 1,
    identity_key  BLOB NOT NULL,
    first_seen_at INTEGER NOT NULL,
    verified_at   INTEGER,
    PRIMARY KEY (address, device_id)
);

CREATE TABLE IF NOT EXISTS crypto_pre_keys (
    key_id     INTEGER PRIMARY KEY,
    record     BLOB NOT NULL,
    uploaded   INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS crypto_signed_pre_keys (
    key_id     INTEGER PRIMARY KEY,
    record     BLOB NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS crypto_sessions (
    address      TEXT NOT NULL,
    device_id    INTEGER NOT NULL DEFAULT 1,
    session_data BLOB NOT NULL,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL,
    PRIMARY KEY (address, device_id)
);

CREATE TABLE IF NOT EXISTS crypto_skipped_message_keys (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    session_address    TEXT NOT NULL,
    session_device_id  INTEGER NOT NULL DEFAULT 1,
    ratchet_key        BLOB NOT NULL,
    message_number     INTEGER NOT NULL,
    message_key        BLOB NOT NULL,
    created_at         INTEGER NOT NULL,
    UNIQUE (session_address, session_device_id, ratchet_key, message_number)
);

CREATE TABLE IF NOT EXISTS crypto_config (
    key   TEXT PRIMARY KEY,
    value BLOB NOT NULL
);
";

const MIGRATION_002: &str = "
CREATE TABLE IF NOT EXISTS crypto_kyber_pre_keys (
    key_id     INTEGER PRIMARY KEY,
    record     BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
";

pub fn run_crypto_migrations(conn: &Connection) -> Result<(), CryptoError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _crypto_migrations (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )?;

    let current_version: i32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM _crypto_migrations",
        [],
        |row| row.get(0),
    )?;

    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO _crypto_migrations (version) VALUES (?1)",
                [version],
            )?;
            tx.commit()?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::init_test_db;

    #[test]
    fn run_migrations_creates_all_crypto_tables() {
        let conn = init_test_db();
        let expected = [
            "crypto_identity_keys",
            "crypto_trusted_identities",
            "crypto_pre_keys",
            "crypto_signed_pre_keys",
            "crypto_sessions",
            "crypto_skipped_message_keys",
            "crypto_config",
            "crypto_kyber_pre_keys",
        ];
        for table in &expected {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "table {table} should exist");
        }
    }

    #[test]
    fn run_migrations_is_idempotent() {
        let conn = init_test_db();
        let count_before: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        run_crypto_migrations(&conn).unwrap();

        let count_after: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count_before, count_after);
    }

    #[test]
    fn crypto_migrations_table_tracks_version() {
        let conn = init_test_db();
        let version: i32 = conn
            .query_row(
                "SELECT version FROM _crypto_migrations WHERE version = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 1);

        let applied_at: String = conn
            .query_row(
                "SELECT applied_at FROM _crypto_migrations WHERE version = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!applied_at.is_empty());
    }

    #[test]
    fn crypto_migrations_does_not_conflict_with_app_migrations() {
        let conn = init_test_db();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .unwrap();
        conn.execute("INSERT INTO _migrations (version) VALUES (1)", [])
            .unwrap();

        // Run crypto migrations again — should not affect _migrations
        run_crypto_migrations(&conn).unwrap();

        let app_count: i32 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(app_count, 1);

        let crypto_count: i32 = conn
            .query_row("SELECT COUNT(*) FROM _crypto_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(crypto_count >= 1);
    }

    #[test]
    fn identity_keys_check_constraint_prevents_id_not_1() {
        let conn = init_test_db();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        conn.execute(
            "INSERT INTO crypto_identity_keys (id, public_key, private_key, created_at) VALUES (1, X'AA', X'BB', ?1)",
            [now],
        )
        .unwrap();

        let result = conn.execute(
            "INSERT INTO crypto_identity_keys (id, public_key, private_key, created_at) VALUES (2, X'CC', X'DD', ?1)",
            [now],
        );
        assert!(result.is_err());
    }
}
