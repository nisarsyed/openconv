#![allow(dead_code)]

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

/// Serializable read position returned to the frontend via Tauri commands.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReadPositionInfo {
    pub channel_id: String,
    pub last_read_message_id: String,
    pub last_read_created_at: i64,
    pub unread_count: i32,
}

impl From<ReadPosition> for ReadPositionInfo {
    fn from(rp: ReadPosition) -> Self {
        Self {
            channel_id: rp.channel_id,
            last_read_message_id: rp.last_read_message_id,
            last_read_created_at: rp.last_read_created_at,
            unread_count: rp.unread_count,
        }
    }
}

/// Server-side read position returned from GET /api/users/me/read-state.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ServerReadPosition {
    pub channel_id: String,
    pub last_read_message_id: String,
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

    conn.execute(
        "INSERT INTO read_positions (channel_id, last_read_message_id, last_read_created_at, unread_count, synced_to_server, updated_at)
         VALUES (?1, '', 0, 1, 0, ?2)
         ON CONFLICT(channel_id) DO UPDATE SET unread_count = unread_count + 1, updated_at = ?2",
        rusqlite::params![channel_id, now],
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

pub fn mark_positions_synced(conn: &Connection, channel_ids: &[&str]) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    for channel_id in channel_ids {
        tx.execute(
            "UPDATE read_positions SET synced_to_server = 1 WHERE channel_id = ?1",
            [channel_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn merge_read_positions(
    conn: &Connection,
    server_positions: &[ServerReadPosition],
) -> Result<(), AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::new(e.to_string()))?
        .as_secs() as i64;

    let tx = conn.unchecked_transaction()?;
    for sp in server_positions {
        let local = get_read_position(&tx, &sp.channel_id)?;
        match local {
            Some(lp) if lp.last_read_created_at >= sp.updated_at => {
                // Local is more recent or equal -- keep local, it will sync on next batch
            }
            _ => {
                // Server is more recent or no local position -- update local
                tx.execute(
                    "INSERT INTO read_positions (channel_id, last_read_message_id, last_read_created_at, unread_count, synced_to_server, updated_at)
                     VALUES (?1, ?2, ?3, 0, 1, ?4)
                     ON CONFLICT(channel_id) DO UPDATE SET last_read_message_id = ?2, last_read_created_at = ?3, unread_count = 0, synced_to_server = 1, updated_at = ?4",
                    rusqlite::params![sp.channel_id, sp.last_read_message_id, sp.updated_at, now],
                )?;
            }
        }
    }
    tx.commit()?;
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

    #[test]
    fn test_increment_unread_creates_row_if_missing() {
        let conn = test_conn();
        // No prior set_read_position call
        increment_unread(&conn, "c-new").unwrap();
        let pos = get_read_position(&conn, "c-new").unwrap().unwrap();
        assert_eq!(pos.unread_count, 1);
        assert_eq!(pos.last_read_message_id, "");
    }

    #[test]
    fn test_get_all_read_positions() {
        let conn = test_conn();
        set_read_position(&conn, "c1", "m1", 1000).unwrap();
        set_read_position(&conn, "c2", "m2", 2000).unwrap();
        set_read_position(&conn, "c3", "m3", 3000).unwrap();
        let all = get_all_read_positions(&conn).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_mark_positions_synced_batch() {
        let conn = test_conn();
        set_read_position(&conn, "c1", "m1", 1000).unwrap();
        set_read_position(&conn, "c2", "m2", 2000).unwrap();
        set_read_position(&conn, "c3", "m3", 3000).unwrap();

        mark_positions_synced(&conn, &["c1", "c2"]).unwrap();

        let unsynced = get_unsynced(&conn).unwrap();
        assert_eq!(unsynced.len(), 1);
        assert_eq!(unsynced[0].channel_id, "c3");
    }

    #[test]
    fn test_merge_takes_server_when_more_recent() {
        let conn = test_conn();
        set_read_position(&conn, "c1", "m1", 1000).unwrap();

        let server = vec![ServerReadPosition {
            channel_id: "c1".into(),
            last_read_message_id: "m5".into(),
            updated_at: 2000,
        }];
        merge_read_positions(&conn, &server).unwrap();

        let pos = get_read_position(&conn, "c1").unwrap().unwrap();
        assert_eq!(pos.last_read_message_id, "m5");
        assert_eq!(pos.last_read_created_at, 2000);
        assert!(pos.synced_to_server);
    }

    #[test]
    fn test_merge_keeps_local_when_more_recent() {
        let conn = test_conn();
        set_read_position(&conn, "c1", "m5", 2000).unwrap();

        let server = vec![ServerReadPosition {
            channel_id: "c1".into(),
            last_read_message_id: "m1".into(),
            updated_at: 1000,
        }];
        merge_read_positions(&conn, &server).unwrap();

        let pos = get_read_position(&conn, "c1").unwrap().unwrap();
        assert_eq!(pos.last_read_message_id, "m5");
        assert_eq!(pos.last_read_created_at, 2000);
    }

    #[test]
    fn test_merge_creates_new_position_for_unknown_channel() {
        let conn = test_conn();

        let server = vec![ServerReadPosition {
            channel_id: "c-new".into(),
            last_read_message_id: "m1".into(),
            updated_at: 1000,
        }];
        merge_read_positions(&conn, &server).unwrap();

        let pos = get_read_position(&conn, "c-new").unwrap().unwrap();
        assert_eq!(pos.last_read_message_id, "m1");
        assert!(pos.synced_to_server);
    }
}
