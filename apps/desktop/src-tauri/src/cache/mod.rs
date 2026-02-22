pub mod messages;
pub mod migrations;
pub mod queue;
pub mod read_positions;
pub mod search;
pub mod sync;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use crate::auth_service::AppError;
use rusqlite::Connection;

const CACHE_DB_KEY_INFO: &[u8] = b"openconv-cache-db-v1";

pub struct CacheDb {
    conn: Mutex<Connection>,
}

impl CacheDb {
    pub fn open(app_data_dir: &Path) -> Result<Self, AppError> {
        let db_path = app_data_dir.join("openconv_secure.db");

        let master_key = openconv_crypto::master_key::init_master_key_from_keychain()
            .map_err(|e| AppError::new(e.to_string()))?;
        let db_key = openconv_crypto::master_key::derive_db_encryption_key_with_info(
            &master_key,
            CACHE_DB_KEY_INFO,
        )
        .map_err(|e| AppError::new(e.to_string()))?;

        let conn = Connection::open(&db_path)
            .map_err(|e| AppError::new(e.to_string()))?;
        openconv_crypto::master_key::apply_encryption_key(&conn, &db_key)
            .map_err(|e| AppError::new(e.to_string()))?;

        conn.pragma_update(None, "busy_timeout", 5000)
            .map_err(|e| AppError::new(e.to_string()))?;

        migrations::run_cache_migrations(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, Connection>, AppError> {
        self.conn
            .lock()
            .map_err(|e| AppError::new(format!("cache db mutex poisoned: {e}")))
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";",
        )
        .unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn.pragma_update(None, "busy_timeout", 5000).unwrap();
        migrations::run_cache_migrations(&conn).unwrap();
        Self {
            conn: Mutex::new(conn),
        }
    }
}
