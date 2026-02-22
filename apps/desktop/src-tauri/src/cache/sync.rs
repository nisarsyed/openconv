use crate::auth_service::AppError;
use rusqlite::Connection;

pub fn get_last_sequence(conn: &Connection, channel_id: &str) -> Result<i64, AppError> {
    let result = conn.query_row(
        "SELECT last_sequence FROM sync_state WHERE channel_id = ?1",
        [channel_id],
        |row| row.get(0),
    );
    match result {
        Ok(seq) => Ok(seq),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(e) => Err(e.into()),
    }
}

pub fn update_last_sequence(
    conn: &Connection,
    channel_id: &str,
    sequence: i64,
) -> Result<(), AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::new(e.to_string()))?
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO sync_state (channel_id, last_sequence, last_sync_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(channel_id) DO UPDATE SET last_sequence = ?2, last_sync_at = ?3",
        rusqlite::params![channel_id, sequence, now],
    )?;
    Ok(())
}

pub fn get_last_sync_at(conn: &Connection, channel_id: &str) -> Result<Option<i64>, AppError> {
    let result = conn.query_row(
        "SELECT last_sync_at FROM sync_state WHERE channel_id = ?1",
        [channel_id],
        |row| row.get(0),
    );
    match result {
        Ok(ts) => Ok(Some(ts)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA key = \"x'0000000000000000000000000000000000000000000000000000000000000000'\";",
        )
        .unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::cache::migrations::run_cache_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_get_last_sequence_unknown() {
        let conn = test_conn();
        let seq = get_last_sequence(&conn, "nonexistent").unwrap();
        assert_eq!(seq, 0);
    }

    #[test]
    fn test_update_last_sequence() {
        let conn = test_conn();
        update_last_sequence(&conn, "ch1", 42).unwrap();
        let seq = get_last_sequence(&conn, "ch1").unwrap();
        assert_eq!(seq, 42);
    }

    #[test]
    fn test_update_last_sequence_upsert() {
        let conn = test_conn();
        update_last_sequence(&conn, "ch1", 10).unwrap();
        update_last_sequence(&conn, "ch1", 20).unwrap();
        let seq = get_last_sequence(&conn, "ch1").unwrap();
        assert_eq!(seq, 20);
    }
}
