//! Crypto storage layer — SQLite-backed store for all crypto state.

pub mod migrations;
pub mod identity_store;
pub mod pre_key_store;
pub mod signed_pre_key_store;
pub mod session_store;
pub mod sender_key_store;

use crate::error::CryptoError;
use rusqlite::Connection;

/// Central storage coordinator for all crypto state.
/// Wraps a borrowed SQLite connection and exposes libsignal store
/// traits plus convenience methods.
pub struct CryptoStore<'a> {
    conn: &'a Connection,
}

impl<'a> CryptoStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn run_migrations(&self) -> Result<(), CryptoError> {
        migrations::run_crypto_migrations(self.conn)
    }

    pub fn store_identity_keypair(
        &self,
        public_key: &[u8],
        private_key: &[u8],
    ) -> Result<(), CryptoError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_secs() as i64;

        self.conn.execute(
            "INSERT OR REPLACE INTO crypto_identity_keys (id, public_key, private_key, created_at)
             VALUES (1, ?1, ?2, ?3)",
            rusqlite::params![public_key, private_key, now],
        )?;
        Ok(())
    }

    pub fn get_identity_keypair(&self) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
        self.conn
            .query_row(
                "SELECT public_key, private_key FROM crypto_identity_keys WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| CryptoError::IdentityNotInitialized)
    }

    pub fn count_available_pre_keys(&self) -> Result<u32, CryptoError> {
        let count: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM crypto_pre_keys",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn prune_skipped_message_keys(&self, max_age_seconds: u64) -> Result<u32, CryptoError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_secs();
        let cutoff = now.saturating_sub(max_age_seconds) as i64;

        let deleted = self.conn.execute(
            "DELETE FROM crypto_skipped_message_keys WHERE created_at < ?1",
            [cutoff],
        )?;
        Ok(deleted as u32)
    }

    pub fn store_config(&self, key: &str, value: &[u8]) -> Result<(), CryptoError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO crypto_config (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn get_config(&self, key: &str) -> Result<Option<Vec<u8>>, CryptoError> {
        let result = self.conn.query_row(
            "SELECT value FROM crypto_config WHERE key = ?1",
            [key],
            |row| row.get(0),
        );
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CryptoError::from(e)),
        }
    }
}

/// Execute a closure within a SQLite transaction.
/// Commits on Ok, rolls back on Err.
pub fn with_transaction<F, T>(conn: &Connection, f: F) -> Result<T, CryptoError>
where
    F: FnOnce(&CryptoStore) -> Result<T, CryptoError>,
{
    let tx = conn.unchecked_transaction()?;
    let store = CryptoStore::new(conn);
    match f(&store) {
        Ok(value) => {
            tx.commit()?;
            Ok(value)
        }
        Err(e) => {
            // Transaction drops without commit = implicit rollback
            drop(tx);
            Err(e)
        }
    }
}

/// Create an in-memory SQLCipher database with migrations applied.
/// For use in tests only.
#[cfg(test)]
pub fn init_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    // SQLCipher requires PRAGMA key even for in-memory DBs
    conn.execute_batch("PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";")
        .unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn.pragma_update(None, "busy_timeout", 5000).unwrap();
    migrations::run_crypto_migrations(&conn).unwrap();
    conn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_available_pre_keys_returns_0_on_empty_db() {
        let conn = init_test_db();
        let store = CryptoStore::new(&conn);
        assert_eq!(store.count_available_pre_keys().unwrap(), 0);
    }

    #[test]
    fn count_available_pre_keys_counts_correctly() {
        let conn = init_test_db();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        for i in 1..=5 {
            conn.execute(
                "INSERT INTO crypto_pre_keys (key_id, record, uploaded, created_at) VALUES (?1, X'AA', 0, ?2)",
                rusqlite::params![i, now],
            )
            .unwrap();
        }

        let store = CryptoStore::new(&conn);
        assert_eq!(store.count_available_pre_keys().unwrap(), 5);
    }

    #[test]
    fn store_config_get_config_round_trips() {
        let conn = init_test_db();
        let store = CryptoStore::new(&conn);
        store.store_config("test_key", b"test_value").unwrap();

        let value = store.get_config("test_key").unwrap();
        assert_eq!(value, Some(b"test_value".to_vec()));
    }

    #[test]
    fn get_config_returns_none_for_missing_key() {
        let conn = init_test_db();
        let store = CryptoStore::new(&conn);
        let value = store.get_config("nonexistent").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn prune_skipped_message_keys_deletes_old_keeps_recent() {
        let conn = init_test_db();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let two_weeks_ago = now - (14 * 24 * 3600);

        // Old entry
        conn.execute(
            "INSERT INTO crypto_skipped_message_keys (session_address, session_device_id, ratchet_key, message_number, message_key, created_at)
             VALUES ('addr', 1, X'AA', 1, X'BB', ?1)",
            [two_weeks_ago],
        )
        .unwrap();

        // Recent entry
        conn.execute(
            "INSERT INTO crypto_skipped_message_keys (session_address, session_device_id, ratchet_key, message_number, message_key, created_at)
             VALUES ('addr', 1, X'CC', 2, X'DD', ?1)",
            [now],
        )
        .unwrap();

        let store = CryptoStore::new(&conn);
        let deleted = store.prune_skipped_message_keys(7 * 24 * 3600).unwrap();
        assert_eq!(deleted, 1);

        let remaining: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM crypto_skipped_message_keys",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 1);
    }

    #[test]
    fn with_transaction_commits_on_success() {
        let conn = init_test_db();
        with_transaction(&conn, |store| {
            store.store_config("tx_key", b"tx_value")?;
            Ok(())
        })
        .unwrap();

        let store = CryptoStore::new(&conn);
        let value = store.get_config("tx_key").unwrap();
        assert_eq!(value, Some(b"tx_value".to_vec()));
    }

    #[test]
    fn with_transaction_rolls_back_on_error() {
        let conn = init_test_db();
        let result: Result<(), _> = with_transaction(&conn, |store| {
            store.store_config("rollback_key", b"should_not_persist")?;
            Err(CryptoError::StorageError("intentional error".into()))
        });
        assert!(result.is_err());

        let store = CryptoStore::new(&conn);
        let value = store.get_config("rollback_key").unwrap();
        assert!(value.is_none());
    }
}
