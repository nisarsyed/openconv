use crate::auth_service::AppError;
use rusqlite::Connection;

pub struct CachedMessage {
    pub id: String,
    pub channel_id: Option<String>,
    pub dm_channel_id: Option<String>,
    pub sender_id: String,
    pub plaintext: Option<String>,
    pub ciphertext: Option<Vec<u8>>,
    pub message_type: Option<String>,
    pub created_at: i64,
    pub edited_at: Option<i64>,
    pub decrypted_at: Option<i64>,
    pub status: String,
}

pub fn insert_message(conn: &Connection, msg: &CachedMessage) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO messages (id, channel_id, dm_channel_id, sender_id, plaintext, ciphertext, message_type, created_at, edited_at, decrypted_at, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            msg.id,
            msg.channel_id,
            msg.dm_channel_id,
            msg.sender_id,
            msg.plaintext,
            msg.ciphertext,
            msg.message_type,
            msg.created_at,
            msg.edited_at,
            msg.decrypted_at,
            msg.status,
        ],
    )?;
    Ok(())
}

pub fn get_message(conn: &Connection, id: &str) -> Result<Option<CachedMessage>, AppError> {
    let result = conn.query_row(
        "SELECT id, channel_id, dm_channel_id, sender_id, plaintext, ciphertext, message_type, created_at, edited_at, decrypted_at, status
         FROM messages WHERE id = ?1",
        [id],
        |row| {
            Ok(CachedMessage {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                dm_channel_id: row.get(2)?,
                sender_id: row.get(3)?,
                plaintext: row.get(4)?,
                ciphertext: row.get(5)?,
                message_type: row.get(6)?,
                created_at: row.get(7)?,
                edited_at: row.get(8)?,
                decrypted_at: row.get(9)?,
                status: row.get(10)?,
            })
        },
    );
    match result {
        Ok(msg) => Ok(Some(msg)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_messages_for_channel(
    conn: &Connection,
    channel_id: &str,
    before: Option<i64>,
    limit: u32,
) -> Result<Vec<CachedMessage>, AppError> {
    let mut messages = Vec::new();
    if let Some(before_ts) = before {
        let mut stmt = conn.prepare(
            "SELECT id, channel_id, dm_channel_id, sender_id, plaintext, ciphertext, message_type, created_at, edited_at, decrypted_at, status
             FROM messages WHERE channel_id = ?1 AND created_at < ?2
             ORDER BY created_at ASC LIMIT ?3",
        )?;
        let rows = stmt.query_map(rusqlite::params![channel_id, before_ts, limit], |row| {
            Ok(CachedMessage {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                dm_channel_id: row.get(2)?,
                sender_id: row.get(3)?,
                plaintext: row.get(4)?,
                ciphertext: row.get(5)?,
                message_type: row.get(6)?,
                created_at: row.get(7)?,
                edited_at: row.get(8)?,
                decrypted_at: row.get(9)?,
                status: row.get(10)?,
            })
        })?;
        for row in rows {
            messages.push(row?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, channel_id, dm_channel_id, sender_id, plaintext, ciphertext, message_type, created_at, edited_at, decrypted_at, status
             FROM messages WHERE channel_id = ?1
             ORDER BY created_at ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![channel_id, limit], |row| {
            Ok(CachedMessage {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                dm_channel_id: row.get(2)?,
                sender_id: row.get(3)?,
                plaintext: row.get(4)?,
                ciphertext: row.get(5)?,
                message_type: row.get(6)?,
                created_at: row.get(7)?,
                edited_at: row.get(8)?,
                decrypted_at: row.get(9)?,
                status: row.get(10)?,
            })
        })?;
        for row in rows {
            messages.push(row?);
        }
    }
    Ok(messages)
}

pub fn get_messages_for_dm_channel(
    conn: &Connection,
    dm_channel_id: &str,
    before: Option<i64>,
    limit: u32,
) -> Result<Vec<CachedMessage>, AppError> {
    let mut messages = Vec::new();
    if let Some(before_ts) = before {
        let mut stmt = conn.prepare(
            "SELECT id, channel_id, dm_channel_id, sender_id, plaintext, ciphertext, message_type, created_at, edited_at, decrypted_at, status
             FROM messages WHERE dm_channel_id = ?1 AND created_at < ?2
             ORDER BY created_at ASC LIMIT ?3",
        )?;
        let rows = stmt.query_map(rusqlite::params![dm_channel_id, before_ts, limit], |row| {
            Ok(CachedMessage {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                dm_channel_id: row.get(2)?,
                sender_id: row.get(3)?,
                plaintext: row.get(4)?,
                ciphertext: row.get(5)?,
                message_type: row.get(6)?,
                created_at: row.get(7)?,
                edited_at: row.get(8)?,
                decrypted_at: row.get(9)?,
                status: row.get(10)?,
            })
        })?;
        for row in rows {
            messages.push(row?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, channel_id, dm_channel_id, sender_id, plaintext, ciphertext, message_type, created_at, edited_at, decrypted_at, status
             FROM messages WHERE dm_channel_id = ?1
             ORDER BY created_at ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![dm_channel_id, limit], |row| {
            Ok(CachedMessage {
                id: row.get(0)?,
                channel_id: row.get(1)?,
                dm_channel_id: row.get(2)?,
                sender_id: row.get(3)?,
                plaintext: row.get(4)?,
                ciphertext: row.get(5)?,
                message_type: row.get(6)?,
                created_at: row.get(7)?,
                edited_at: row.get(8)?,
                decrypted_at: row.get(9)?,
                status: row.get(10)?,
            })
        })?;
        for row in rows {
            messages.push(row?);
        }
    }
    Ok(messages)
}

pub fn update_message_status(conn: &Connection, id: &str, status: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE messages SET status = ?1 WHERE id = ?2",
        rusqlite::params![status, id],
    )?;
    Ok(())
}

pub fn update_message_plaintext(
    conn: &Connection,
    id: &str,
    plaintext: &str,
    decrypted_at: i64,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE messages SET plaintext = ?1, decrypted_at = ?2 WHERE id = ?3",
        rusqlite::params![plaintext, decrypted_at, id],
    )?;
    Ok(())
}

pub fn soft_delete_message(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE messages SET status = 'deleted', plaintext = NULL, ciphertext = NULL WHERE id = ?1",
        [id],
    )?;
    Ok(())
}

pub fn clear_expired_plaintext(conn: &Connection, ttl_seconds: i64) -> Result<u32, AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::new(e.to_string()))?
        .as_secs() as i64;

    let count = conn.execute(
        "UPDATE messages SET plaintext = NULL WHERE decrypted_at IS NOT NULL AND decrypted_at + ?1 < ?2 AND plaintext IS NOT NULL",
        rusqlite::params![ttl_seconds, now],
    )?;
    Ok(count as u32)
}

pub fn update_message_content(
    conn: &Connection,
    id: &str,
    new_plaintext: &str,
    edited_at: i64,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE messages SET plaintext = ?1, edited_at = ?2 WHERE id = ?3",
        rusqlite::params![new_plaintext, edited_at, id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheDb;

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

    fn make_msg(id: &str, channel_id: &str, sender_id: &str, created_at: i64) -> CachedMessage {
        CachedMessage {
            id: id.to_string(),
            channel_id: Some(channel_id.to_string()),
            dm_channel_id: None,
            sender_id: sender_id.to_string(),
            plaintext: Some("hello".to_string()),
            ciphertext: None,
            message_type: None,
            created_at,
            edited_at: None,
            decrypted_at: None,
            status: "delivered".to_string(),
        }
    }

    #[test]
    fn test_insert_message_roundtrip() {
        let conn = test_conn();
        let msg = CachedMessage {
            id: "m1".to_string(),
            channel_id: Some("ch1".to_string()),
            dm_channel_id: None,
            sender_id: "u1".to_string(),
            plaintext: Some("hello world".to_string()),
            ciphertext: Some(vec![1, 2, 3]),
            message_type: Some("text".to_string()),
            created_at: 1000,
            edited_at: Some(1001),
            decrypted_at: Some(1000),
            status: "delivered".to_string(),
        };
        insert_message(&conn, &msg).unwrap();

        let retrieved = get_message(&conn, "m1").unwrap().unwrap();
        assert_eq!(retrieved.id, "m1");
        assert_eq!(retrieved.channel_id.as_deref(), Some("ch1"));
        assert_eq!(retrieved.dm_channel_id, None);
        assert_eq!(retrieved.sender_id, "u1");
        assert_eq!(retrieved.plaintext.as_deref(), Some("hello world"));
        assert_eq!(retrieved.ciphertext, Some(vec![1, 2, 3]));
        assert_eq!(retrieved.message_type.as_deref(), Some("text"));
        assert_eq!(retrieved.created_at, 1000);
        assert_eq!(retrieved.edited_at, Some(1001));
        assert_eq!(retrieved.decrypted_at, Some(1000));
        assert_eq!(retrieved.status, "delivered");
    }

    #[test]
    fn test_get_message_not_found() {
        let conn = test_conn();
        let result = get_message(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_messages_ordered() {
        let conn = test_conn();
        insert_message(&conn, &make_msg("m3", "ch1", "u1", 3000)).unwrap();
        insert_message(&conn, &make_msg("m1", "ch1", "u1", 1000)).unwrap();
        insert_message(&conn, &make_msg("m2", "ch1", "u1", 2000)).unwrap();

        let msgs = get_messages_for_channel(&conn, "ch1", None, 10).unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].id, "m1");
        assert_eq!(msgs[1].id, "m2");
        assert_eq!(msgs[2].id, "m3");
    }

    #[test]
    fn test_update_message_status() {
        let conn = test_conn();
        insert_message(&conn, &make_msg("m1", "ch1", "u1", 1000)).unwrap();
        update_message_status(&conn, "m1", "failed").unwrap();
        let msg = get_message(&conn, "m1").unwrap().unwrap();
        assert_eq!(msg.status, "failed");
    }

    #[test]
    fn test_update_message_plaintext() {
        let conn = test_conn();
        let mut msg = make_msg("m1", "ch1", "u1", 1000);
        msg.plaintext = None;
        insert_message(&conn, &msg).unwrap();

        update_message_plaintext(&conn, "m1", "decrypted text", 2000).unwrap();
        let retrieved = get_message(&conn, "m1").unwrap().unwrap();
        assert_eq!(retrieved.plaintext.as_deref(), Some("decrypted text"));
        assert_eq!(retrieved.decrypted_at, Some(2000));
    }

    #[test]
    fn test_soft_delete_message() {
        let conn = test_conn();
        insert_message(&conn, &make_msg("m1", "ch1", "u1", 1000)).unwrap();
        soft_delete_message(&conn, "m1").unwrap();

        let msg = get_message(&conn, "m1").unwrap().unwrap();
        assert_eq!(msg.status, "deleted");
        assert!(msg.plaintext.is_none());
    }

    #[test]
    fn test_clear_expired_plaintext() {
        let conn = test_conn();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Old message (decrypted long ago)
        let mut old_msg = make_msg("m1", "ch1", "u1", 1000);
        old_msg.plaintext = Some("old text".to_string());
        old_msg.decrypted_at = Some(now - 7200); // 2 hours ago
        insert_message(&conn, &old_msg).unwrap();

        // Recent message
        let mut new_msg = make_msg("m2", "ch1", "u1", 2000);
        new_msg.plaintext = Some("new text".to_string());
        new_msg.decrypted_at = Some(now - 100); // 100 seconds ago
        insert_message(&conn, &new_msg).unwrap();

        let cleared = clear_expired_plaintext(&conn, 3600).unwrap(); // 1 hour TTL
        assert_eq!(cleared, 1);

        let m1 = get_message(&conn, "m1").unwrap().unwrap();
        assert!(m1.plaintext.is_none(), "expired plaintext should be cleared");

        let m2 = get_message(&conn, "m2").unwrap().unwrap();
        assert!(m2.plaintext.is_some(), "recent plaintext should remain");
    }

    #[test]
    fn test_clear_expired_plaintext_preserves_fts() {
        let conn = test_conn();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut msg = make_msg("m1", "ch1", "u1", 1000);
        msg.plaintext = Some("searchable content".to_string());
        msg.decrypted_at = Some(now - 7200);
        insert_message(&conn, &msg).unwrap();

        // Index in FTS
        crate::cache::search::index_message(&conn, "m1", "searchable content").unwrap();

        clear_expired_plaintext(&conn, 3600).unwrap();

        // FTS should still find it
        let results = crate::cache::search::search_messages(
            &conn,
            "searchable",
            &crate::cache::search::SearchScope::AllMessages,
            10,
        )
        .unwrap();
        assert!(!results.is_empty(), "FTS should still find the message");
    }
}
