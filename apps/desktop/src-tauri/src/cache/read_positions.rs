use crate::auth_service::AppError;
use rusqlite::Connection;

pub struct ReadPosition {
    pub channel_id: String,
    pub last_read_message_id: String,
    pub last_read_created_at: i64,
    pub unread_count: i32,
    pub synced_to_server: bool,
    pub updated_at: i64,
}

pub fn set_read_position(
    conn: &Connection,
    channel_id: &str,
    last_read_message_id: &str,
    last_read_created_at: i64,
) -> Result<(), AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::new(e.to_string()))?
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO read_positions (channel_id, last_read_message_id, last_read_created_at, unread_count, synced_to_server, updated_at)
         VALUES (?1, ?2, ?3, 0, 0, ?4)
         ON CONFLICT(channel_id) DO UPDATE SET last_read_message_id = ?2, last_read_created_at = ?3, synced_to_server = 0, updated_at = ?4",
        rusqlite::params![channel_id, last_read_message_id, last_read_created_at, now],
    )?;
    Ok(())
}

pub fn increment_unread(conn: &Connection, channel_id: &str) -> Result<(), AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::new(e.to_string()))?
        .as_secs() as i64;

    // Only increment if position exists
    conn.execute(
        "UPDATE read_positions SET unread_count = unread_count + 1, updated_at = ?1 WHERE channel_id = ?2",
        rusqlite::params![now, channel_id],
    )?;
    Ok(())
}

pub fn clear_unread(
    conn: &Connection,
    channel_id: &str,
    last_read_message_id: &str,
    last_read_created_at: i64,
) -> Result<(), AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::new(e.to_string()))?
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO read_positions (channel_id, last_read_message_id, last_read_created_at, unread_count, synced_to_server, updated_at)
         VALUES (?1, ?2, ?3, 0, 0, ?4)
         ON CONFLICT(channel_id) DO UPDATE SET last_read_message_id = ?2, last_read_created_at = ?3, unread_count = 0, synced_to_server = 0, updated_at = ?4",
        rusqlite::params![channel_id, last_read_message_id, last_read_created_at, now],
    )?;
    Ok(())
}

pub fn get_unsynced(conn: &Connection) -> Result<Vec<ReadPosition>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT channel_id, last_read_message_id, last_read_created_at, unread_count, synced_to_server, updated_at
         FROM read_positions WHERE synced_to_server = 0",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ReadPosition {
            channel_id: row.get(0)?,
            last_read_message_id: row.get(1)?,
            last_read_created_at: row.get(2)?,
            unread_count: row.get(3)?,
            synced_to_server: row.get::<_, i32>(4)? != 0,
            updated_at: row.get(5)?,
        })
    })?;
    let mut positions = Vec::new();
    for row in rows {
        positions.push(row?);
    }
    Ok(positions)
}

pub fn mark_synced(conn: &Connection, channel_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE read_positions SET synced_to_server = 1 WHERE channel_id = ?1",
        [channel_id],
    )?;
    Ok(())
}

pub fn get_read_position(
    conn: &Connection,
    channel_id: &str,
) -> Result<Option<ReadPosition>, AppError> {
    let result = conn.query_row(
        "SELECT channel_id, last_read_message_id, last_read_created_at, unread_count, synced_to_server, updated_at
         FROM read_positions WHERE channel_id = ?1",
        [channel_id],
        |row| {
            Ok(ReadPosition {
                channel_id: row.get(0)?,
                last_read_message_id: row.get(1)?,
                last_read_created_at: row.get(2)?,
                unread_count: row.get(3)?,
                synced_to_server: row.get::<_, i32>(4)? != 0,
                updated_at: row.get(5)?,
            })
        },
    );
    match result {
        Ok(pos) => Ok(Some(pos)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_all_read_positions(conn: &Connection) -> Result<Vec<ReadPosition>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT channel_id, last_read_message_id, last_read_created_at, unread_count, synced_to_server, updated_at
         FROM read_positions",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ReadPosition {
            channel_id: row.get(0)?,
            last_read_message_id: row.get(1)?,
            last_read_created_at: row.get(2)?,
            unread_count: row.get(3)?,
            synced_to_server: row.get::<_, i32>(4)? != 0,
            updated_at: row.get(5)?,
        })
    })?;
    let mut positions = Vec::new();
    for row in rows {
        positions.push(row?);
    }
    Ok(positions)
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
    fn test_set_read_position() {
        let conn = test_conn();
        set_read_position(&conn, "ch1", "m1", 1000).unwrap();
        let pos = get_read_position(&conn, "ch1").unwrap().unwrap();
        assert_eq!(pos.channel_id, "ch1");
        assert_eq!(pos.last_read_message_id, "m1");
        assert_eq!(pos.last_read_created_at, 1000);
        assert_eq!(pos.unread_count, 0);
    }

    #[test]
    fn test_set_read_position_upsert() {
        let conn = test_conn();
        set_read_position(&conn, "ch1", "m1", 1000).unwrap();
        set_read_position(&conn, "ch1", "m5", 5000).unwrap();
        let pos = get_read_position(&conn, "ch1").unwrap().unwrap();
        assert_eq!(pos.last_read_message_id, "m5");
        assert_eq!(pos.last_read_created_at, 5000);
    }

    #[test]
    fn test_increment_unread() {
        let conn = test_conn();
        set_read_position(&conn, "ch1", "m1", 1000).unwrap();
        increment_unread(&conn, "ch1").unwrap();
        increment_unread(&conn, "ch1").unwrap();
        let pos = get_read_position(&conn, "ch1").unwrap().unwrap();
        assert_eq!(pos.unread_count, 2);
    }

    #[test]
    fn test_clear_unread() {
        let conn = test_conn();
        set_read_position(&conn, "ch1", "m1", 1000).unwrap();
        increment_unread(&conn, "ch1").unwrap();
        increment_unread(&conn, "ch1").unwrap();

        clear_unread(&conn, "ch1", "m3", 3000).unwrap();
        let pos = get_read_position(&conn, "ch1").unwrap().unwrap();
        assert_eq!(pos.unread_count, 0);
        assert_eq!(pos.last_read_message_id, "m3");
    }

    #[test]
    fn test_get_unsynced() {
        let conn = test_conn();
        set_read_position(&conn, "ch1", "m1", 1000).unwrap();
        set_read_position(&conn, "ch2", "m2", 2000).unwrap();
        mark_synced(&conn, "ch1").unwrap();

        let unsynced = get_unsynced(&conn).unwrap();
        assert_eq!(unsynced.len(), 1);
        assert_eq!(unsynced[0].channel_id, "ch2");
    }

    #[test]
    fn test_mark_synced() {
        let conn = test_conn();
        set_read_position(&conn, "ch1", "m1", 1000).unwrap();
        mark_synced(&conn, "ch1").unwrap();
        let pos = get_read_position(&conn, "ch1").unwrap().unwrap();
        assert!(pos.synced_to_server);
    }

    #[test]
    fn test_stores_last_read_created_at() {
        let conn = test_conn();
        set_read_position(&conn, "ch1", "m1", 1704067200).unwrap();
        let pos = get_read_position(&conn, "ch1").unwrap().unwrap();
        assert_eq!(pos.last_read_created_at, 1704067200);
    }
}
