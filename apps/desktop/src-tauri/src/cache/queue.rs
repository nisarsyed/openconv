#![allow(dead_code)]

use crate::auth_service::AppError;
use rusqlite::Connection;

pub struct QueuedMessage {
    pub id: i64,
    pub message_id: String,
    pub channel_id: Option<String>,
    pub dm_channel_id: Option<String>,
    pub plaintext: String,
    pub created_at: i64,
    pub retry_count: i32,
    pub last_retry_at: Option<i64>,
    pub status: String,
}

pub fn enqueue_message(
    conn: &Connection,
    message_id: &str,
    channel_id: Option<&str>,
    dm_channel_id: Option<&str>,
    plaintext: &str,
    created_at: i64,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO outgoing_queue (message_id, channel_id, dm_channel_id, plaintext, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![message_id, channel_id, dm_channel_id, plaintext, created_at],
    )?;
    Ok(())
}

pub fn dequeue_pending(conn: &Connection, limit: u32) -> Result<Vec<QueuedMessage>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, message_id, channel_id, dm_channel_id, plaintext, created_at, retry_count, last_retry_at, status
         FROM outgoing_queue WHERE status = 'pending'
         ORDER BY id ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |row| {
        Ok(QueuedMessage {
            id: row.get(0)?,
            message_id: row.get(1)?,
            channel_id: row.get(2)?,
            dm_channel_id: row.get(3)?,
            plaintext: row.get(4)?,
            created_at: row.get(5)?,
            retry_count: row.get(6)?,
            last_retry_at: row.get(7)?,
            status: row.get(8)?,
        })
    })?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row?);
    }
    Ok(messages)
}

pub fn mark_sent(conn: &Connection, message_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM outgoing_queue WHERE message_id = ?1",
        [message_id],
    )?;
    Ok(())
}

pub fn increment_retry(conn: &Connection, message_id: &str) -> Result<u32, AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::new(e.to_string()))?
        .as_secs() as i64;

    conn.execute(
        "UPDATE outgoing_queue SET retry_count = retry_count + 1, last_retry_at = ?1 WHERE message_id = ?2",
        rusqlite::params![now, message_id],
    )?;

    let count: i32 = conn.query_row(
        "SELECT retry_count FROM outgoing_queue WHERE message_id = ?1",
        [message_id],
        |row| row.get(0),
    )?;
    Ok(count as u32)
}

pub fn mark_failed(conn: &Connection, message_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE outgoing_queue SET status = 'failed' WHERE message_id = ?1",
        [message_id],
    )?;
    Ok(())
}

pub fn pending_count(conn: &Connection) -> Result<u32, AppError> {
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM outgoing_queue WHERE status = 'pending'",
        [],
        |row| row.get(0),
    )?;
    Ok(count as u32)
}

/// Retrieve pending messages for a specific channel, ordered by created_at (FIFO).
pub fn get_pending_for_channel(
    conn: &Connection,
    channel_id: &str,
) -> Result<Vec<QueuedMessage>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, message_id, channel_id, dm_channel_id, plaintext, created_at, retry_count, last_retry_at, status
         FROM outgoing_queue WHERE status = 'pending' AND channel_id = ?1
         ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([channel_id], |row| {
        Ok(QueuedMessage {
            id: row.get(0)?,
            message_id: row.get(1)?,
            channel_id: row.get(2)?,
            dm_channel_id: row.get(3)?,
            plaintext: row.get(4)?,
            created_at: row.get(5)?,
            retry_count: row.get(6)?,
            last_retry_at: row.get(7)?,
            status: row.get(8)?,
        })
    })?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row?);
    }
    Ok(messages)
}

/// Mark a message as currently being sent (status="sending").
pub fn mark_sending(conn: &Connection, message_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE outgoing_queue SET status = 'sending' WHERE message_id = ?1",
        [message_id],
    )?;
    Ok(())
}

/// Retrieve pending messages for a specific DM channel, ordered by created_at (FIFO).
pub fn get_pending_for_dm_channel(
    conn: &Connection,
    dm_channel_id: &str,
) -> Result<Vec<QueuedMessage>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, message_id, channel_id, dm_channel_id, plaintext, created_at, retry_count, last_retry_at, status
         FROM outgoing_queue WHERE status = 'pending' AND dm_channel_id = ?1
         ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([dm_channel_id], |row| {
        Ok(QueuedMessage {
            id: row.get(0)?,
            message_id: row.get(1)?,
            channel_id: row.get(2)?,
            dm_channel_id: row.get(3)?,
            plaintext: row.get(4)?,
            created_at: row.get(5)?,
            retry_count: row.get(6)?,
            last_retry_at: row.get(7)?,
            status: row.get(8)?,
        })
    })?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row?);
    }
    Ok(messages)
}

/// Reset a failed message back to pending state with retry_count=0.
/// Returns true if the message was found and reset, false if not found or not in failed state.
pub fn reset_to_pending(conn: &Connection, message_id: &str) -> Result<bool, AppError> {
    let rows = conn.execute(
        "UPDATE outgoing_queue SET status = 'pending', retry_count = 0 WHERE message_id = ?1 AND status = 'failed'",
        [message_id],
    )?;
    Ok(rows > 0)
}

/// Check if a message exists in the outgoing queue.
pub fn exists_in_queue(conn: &Connection, message_id: &str) -> Result<bool, AppError> {
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM outgoing_queue WHERE message_id = ?1",
        [message_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Delete a message from the queue (user-initiated removal of failed messages).
pub fn remove_from_queue(conn: &Connection, message_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM outgoing_queue WHERE message_id = ?1",
        [message_id],
    )?;
    Ok(())
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
    fn test_enqueue_message() {
        let conn = test_conn();
        enqueue_message(&conn, "m1", Some("ch1"), None, "hello", 1000).unwrap();

        let count = pending_count(&conn).unwrap();
        assert_eq!(count, 1);

        let msgs = dequeue_pending(&conn, 10).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message_id, "m1");
        assert_eq!(msgs[0].plaintext, "hello");
        assert_eq!(msgs[0].status, "pending");
    }

    #[test]
    fn test_dequeue_pending_fifo() {
        let conn = test_conn();
        enqueue_message(&conn, "m1", Some("ch1"), None, "first", 1000).unwrap();
        enqueue_message(&conn, "m2", Some("ch1"), None, "second", 1001).unwrap();
        enqueue_message(&conn, "m3", Some("ch1"), None, "third", 1002).unwrap();

        let msgs = dequeue_pending(&conn, 10).unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].message_id, "m1");
        assert_eq!(msgs[1].message_id, "m2");
        assert_eq!(msgs[2].message_id, "m3");
    }

    #[test]
    fn test_mark_sent_removes() {
        let conn = test_conn();
        enqueue_message(&conn, "m1", Some("ch1"), None, "hello", 1000).unwrap();
        mark_sent(&conn, "m1").unwrap();
        let count = pending_count(&conn).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_increment_retry() {
        let conn = test_conn();
        enqueue_message(&conn, "m1", Some("ch1"), None, "hello", 1000).unwrap();

        let count = increment_retry(&conn, "m1").unwrap();
        assert_eq!(count, 1);

        let count = increment_retry(&conn, "m1").unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_mark_failed() {
        let conn = test_conn();
        enqueue_message(&conn, "m1", Some("ch1"), None, "hello", 1000).unwrap();
        mark_failed(&conn, "m1").unwrap();

        let pending = pending_count(&conn).unwrap();
        assert_eq!(pending, 0, "failed messages should not count as pending");

        let msgs = dequeue_pending(&conn, 10).unwrap();
        assert!(msgs.is_empty(), "failed messages should not be dequeued");
    }

    #[test]
    fn test_queue_message_id_is_idempotent() {
        let conn = test_conn();
        enqueue_message(&conn, "m1", Some("ch1"), None, "hello", 1000).unwrap();
        let result = enqueue_message(&conn, "m1", Some("ch1"), None, "hello again", 1001);
        assert!(
            result.is_err(),
            "duplicate message_id should be rejected by UNIQUE constraint"
        );
    }

    #[test]
    fn test_max_retries_marks_failed() {
        let conn = test_conn();
        enqueue_message(&conn, "m1", Some("ch1"), None, "hello", 1000).unwrap();

        // Increment retry 4 times (count goes to 4)
        for _ in 0..4 {
            increment_retry(&conn, "m1").unwrap();
        }

        // 5th retry should bring count to 5
        let count = increment_retry(&conn, "m1").unwrap();
        assert_eq!(count, 5);

        // After 5 retries, mark as failed
        mark_failed(&conn, "m1").unwrap();

        let pending = pending_count(&conn).unwrap();
        assert_eq!(pending, 0, "failed message should not be pending");
    }

    #[test]
    fn test_mark_sending() {
        let conn = test_conn();
        enqueue_message(&conn, "m1", Some("ch1"), None, "hello", 1000).unwrap();

        mark_sending(&conn, "m1").unwrap();

        // Should no longer appear in pending
        let pending = pending_count(&conn).unwrap();
        assert_eq!(pending, 0, "sending messages should not count as pending");

        // Verify status is "sending"
        let status: String = conn
            .query_row(
                "SELECT status FROM outgoing_queue WHERE message_id = 'm1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "sending");
    }

    #[test]
    fn test_get_pending_for_channel() {
        let conn = test_conn();
        enqueue_message(&conn, "m1", Some("ch1"), None, "first", 1000).unwrap();
        enqueue_message(&conn, "m2", Some("ch2"), None, "second", 1001).unwrap();
        enqueue_message(&conn, "m3", Some("ch1"), None, "third", 1002).unwrap();

        let ch1_msgs = get_pending_for_channel(&conn, "ch1").unwrap();
        assert_eq!(ch1_msgs.len(), 2);
        assert_eq!(ch1_msgs[0].message_id, "m1");
        assert_eq!(ch1_msgs[1].message_id, "m3");

        let ch2_msgs = get_pending_for_channel(&conn, "ch2").unwrap();
        assert_eq!(ch2_msgs.len(), 1);
        assert_eq!(ch2_msgs[0].message_id, "m2");
    }

    #[test]
    fn test_remove_from_queue() {
        let conn = test_conn();
        enqueue_message(&conn, "m1", Some("ch1"), None, "hello", 1000).unwrap();
        mark_failed(&conn, "m1").unwrap();

        remove_from_queue(&conn, "m1").unwrap();

        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM outgoing_queue", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "message should be removed from queue entirely");
    }

    #[test]
    fn test_successful_send_removes_and_updates_status() {
        let conn = test_conn();

        // Insert optimistic message in messages table
        crate::cache::messages::insert_message(
            &conn,
            &crate::cache::messages::CachedMessage {
                id: "m1".into(),
                channel_id: Some("ch1".into()),
                dm_channel_id: None,
                sender_id: "u1".into(),
                sender_device_id: None,
                plaintext: Some("hello".into()),
                ciphertext: None,
                message_type: None,
                created_at: 1000,
                edited_at: None,
                decrypted_at: Some(1000),
                status: "pending".into(),
            },
        )
        .unwrap();

        // Enqueue for offline delivery
        enqueue_message(&conn, "m1", Some("ch1"), None, "hello", 1000).unwrap();

        // Simulate successful send
        mark_sent(&conn, "m1").unwrap();
        crate::cache::messages::update_message_status(&conn, "m1", "delivered").unwrap();

        // Queue should be empty
        let count = pending_count(&conn).unwrap();
        assert_eq!(count, 0);

        // Message should be delivered
        let msg = crate::cache::messages::get_message(&conn, "m1")
            .unwrap()
            .unwrap();
        assert_eq!(msg.status, "delivered");
    }
}
