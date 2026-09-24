use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AppHealth {
    pub version: String,
    pub db_status: String,
}

/// Inner logic for health check, testable without Tauri state.
pub fn health_check_inner(conn: &rusqlite::Connection) -> AppHealth {
    let db_status = match conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0)) {
        Ok(_) => "ok".to_string(),
        Err(e) => e.to_string(),
    };

    AppHealth {
        version: env!("CARGO_PKG_VERSION").to_string(),
        db_status,
    }
}

#[tauri::command]
#[specta::specta]
pub fn health_check(
    cache_db: tauri::State<'_, crate::cache::CacheDb>,
) -> Result<AppHealth, String> {
    let conn = cache_db.lock().map_err(|e| e.to_string())?;
    Ok(health_check_inner(&conn))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check_returns_app_health() {
        let cache = crate::cache::CacheDb::open_in_memory();
        let conn = cache.lock().expect("should lock cache db");
        let health = health_check_inner(&conn);
        assert_eq!(health.db_status, "ok");
    }

    #[test]
    fn test_health_check_includes_version() {
        let cache = crate::cache::CacheDb::open_in_memory();
        let conn = cache.lock().expect("should lock cache db");
        let health = health_check_inner(&conn);
        assert!(!health.version.is_empty(), "version should not be empty");
    }
}
