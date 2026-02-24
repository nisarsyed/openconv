use crate::auth_service::AppError;
use rusqlite::Connection;
use serde::Deserialize;

/// A single sync event from the server's /api/sync endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncEvent {
    pub sequence: i64,
    pub channel_id: String,
    pub event_type: String, // "message_created", "message_updated", "message_deleted"
    pub message_id: String,
    pub sender_id: Option<String>,
    pub created_at: String,
    pub edited_at: Option<String>,
}

/// Response from the server's /api/sync endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncResponse {
    pub events: Vec<SyncEvent>,
    pub next_sequence: i64,
    pub has_more: bool,
}

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

/// Get all channel IDs that have sync state entries.
pub fn get_all_sync_channels(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT channel_id FROM sync_state")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut channels = Vec::new();
    for row in rows {
        channels.push(row?);
    }
    Ok(channels)
}

/// Apply a sync event to the local cache.
///
/// - `message_created`: Skip if message already exists (dedup by ID).
/// - `message_updated`: Apply LWW (only if server edited_at is newer).
/// - `message_deleted`: Always soft-delete (delete-wins).
///
/// For `message_created` and `message_updated`, the caller must provide
/// the decrypted plaintext. This function handles the database operations only.
pub fn apply_sync_event(
    conn: &Connection,
    event: &SyncEvent,
    plaintext: Option<&str>,
    decrypted_at: Option<i64>,
) -> Result<bool, AppError> {
    match event.event_type.as_str() {
        "message_created" => {
            // Dedup: skip if message already exists
            if crate::cache::messages::get_message(conn, &event.message_id)?.is_some() {
                return Ok(false);
            }
            // Insert the new message (caller provides plaintext from decryption)
            let msg = crate::cache::messages::CachedMessage {
                id: event.message_id.clone(),
                channel_id: Some(event.channel_id.clone()),
                dm_channel_id: None,
                sender_id: event.sender_id.clone().unwrap_or_default(),
                plaintext: plaintext.map(|s| s.to_string()),
                ciphertext: None,
                message_type: None,
                created_at: chrono::DateTime::parse_from_rfc3339(&event.created_at)
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or(0),
                edited_at: None,
                decrypted_at,
                status: if plaintext.is_some() {
                    "delivered".to_string()
                } else {
                    "decrypt_failed".to_string()
                },
            };
            crate::cache::messages::insert_message(conn, &msg)?;
            Ok(true)
        }
        "message_updated" => {
            // Check if message is deleted (delete-wins: ignore edits on deleted messages)
            if crate::cache::messages::is_deleted(conn, &event.message_id)? {
                return Ok(false);
            }
            if let Some(pt) = plaintext {
                // Prefer edited_at if available, fall back to created_at
                let edited_at_str = event.edited_at.as_deref().unwrap_or(&event.created_at);
                let edited_at = chrono::DateTime::parse_from_rfc3339(edited_at_str)
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or(0);
                crate::cache::messages::update_message_content_lww(
                    conn,
                    &event.message_id,
                    pt,
                    edited_at,
                    decrypted_at.unwrap_or(0),
                )?;
                Ok(true)
            } else {
                // No plaintext available - can't apply update
                Ok(false)
            }
        }
        "message_deleted" => {
            // Delete-wins: always soft-delete regardless of local state
            crate::cache::messages::soft_delete_message(conn, &event.message_id)?;
            if let Err(e) = crate::cache::search::deindex_message(conn, &event.message_id) {
                tracing::warn!(
                    "failed to deindex deleted message {} from FTS: {e}",
                    event.message_id
                );
            }
            Ok(true)
        }
        _ => {
            tracing::warn!("unknown sync event type: {}", event.event_type);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::messages::{self, CachedMessage};

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

    fn make_msg(id: &str, channel_id: &str) -> CachedMessage {
        CachedMessage {
            id: id.to_string(),
            channel_id: Some(channel_id.to_string()),
            dm_channel_id: None,
            sender_id: "u1".to_string(),
            plaintext: Some("hello".to_string()),
            ciphertext: None,
            message_type: None,
            created_at: 1000,
            edited_at: None,
            decrypted_at: Some(1000),
            status: "delivered".to_string(),
        }
    }

    fn make_event(
        seq: i64,
        channel_id: &str,
        event_type: &str,
        message_id: &str,
    ) -> SyncEvent {
        SyncEvent {
            sequence: seq,
            channel_id: channel_id.to_string(),
            event_type: event_type.to_string(),
            message_id: message_id.to_string(),
            sender_id: Some("u1".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            edited_at: None,
        }
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

    #[test]
    fn test_get_all_sync_channels() {
        let conn = test_conn();
        update_last_sequence(&conn, "ch1", 10).unwrap();
        update_last_sequence(&conn, "ch2", 20).unwrap();
        update_last_sequence(&conn, "ch3", 30).unwrap();

        let channels = get_all_sync_channels(&conn).unwrap();
        assert_eq!(channels.len(), 3);
        assert!(channels.contains(&"ch1".to_string()));
        assert!(channels.contains(&"ch2".to_string()));
        assert!(channels.contains(&"ch3".to_string()));
    }

    #[test]
    fn test_replay_deduplicates_by_message_id() {
        let conn = test_conn();
        messages::insert_message(&conn, &make_msg("m1", "ch1")).unwrap();

        let event = make_event(1, "ch1", "message_created", "m1");
        let applied = apply_sync_event(&conn, &event, Some("new content"), Some(2000)).unwrap();
        assert!(!applied, "duplicate message should be skipped");

        let msg = messages::get_message(&conn, "m1").unwrap().unwrap();
        assert_eq!(msg.plaintext.as_deref(), Some("hello"));
    }

    #[test]
    fn test_gap_fill_applies_created_events() {
        let conn = test_conn();

        let event = make_event(1, "ch1", "message_created", "m_new");
        let applied =
            apply_sync_event(&conn, &event, Some("synced message"), Some(2000)).unwrap();
        assert!(applied, "new message should be inserted");

        let msg = messages::get_message(&conn, "m_new").unwrap().unwrap();
        assert_eq!(msg.plaintext.as_deref(), Some("synced message"));
        assert_eq!(msg.status, "delivered");
        assert_eq!(msg.sender_id, "u1");
    }

    #[test]
    fn test_gap_fill_created_without_plaintext() {
        let conn = test_conn();

        let event = make_event(1, "ch1", "message_created", "m_no_pt");
        let applied = apply_sync_event(&conn, &event, None, None).unwrap();
        assert!(applied, "message without plaintext should still be inserted");

        let msg = messages::get_message(&conn, "m_no_pt").unwrap().unwrap();
        assert!(msg.plaintext.is_none());
        assert_eq!(msg.status, "decrypt_failed");
    }

    #[test]
    fn test_gap_fill_applies_deleted_events() {
        let conn = test_conn();
        messages::insert_message(&conn, &make_msg("m1", "ch1")).unwrap();

        let event = make_event(2, "ch1", "message_deleted", "m1");
        let applied = apply_sync_event(&conn, &event, None, None).unwrap();
        assert!(applied);

        let msg = messages::get_message(&conn, "m1").unwrap().unwrap();
        assert_eq!(msg.status, "deleted");
    }

    #[test]
    fn test_gap_fill_updates_sync_state() {
        let conn = test_conn();
        update_last_sequence(&conn, "ch1", 42).unwrap();
        update_last_sequence(&conn, "ch1", 100).unwrap();
        let seq = get_last_sequence(&conn, "ch1").unwrap();
        assert_eq!(seq, 100, "sync_state should reflect highest sequence");
    }

    #[test]
    fn test_edits_use_last_write_wins() {
        let conn = test_conn();
        let mut msg = make_msg("m1", "ch1");
        msg.edited_at = Some(100);
        messages::insert_message(&conn, &msg).unwrap();

        // Apply newer edit
        let mut event_newer = make_event(1, "ch1", "message_updated", "m1");
        event_newer.edited_at = Some("2024-06-01T00:00:00Z".to_string());
        apply_sync_event(&conn, &event_newer, Some("newer content"), Some(2000)).unwrap();

        let m = messages::get_message(&conn, "m1").unwrap().unwrap();
        assert_eq!(m.plaintext.as_deref(), Some("newer content"));

        // Apply older edit (should be rejected by LWW)
        let mut event_older = make_event(2, "ch1", "message_updated", "m1");
        event_older.edited_at = Some("2023-01-01T00:00:00Z".to_string());
        apply_sync_event(&conn, &event_older, Some("old content"), Some(500)).unwrap();

        let m = messages::get_message(&conn, "m1").unwrap().unwrap();
        assert_eq!(
            m.plaintext.as_deref(),
            Some("newer content"),
            "LWW should keep newer content"
        );
    }

    #[test]
    fn test_message_updated_without_plaintext_returns_false() {
        let conn = test_conn();
        messages::insert_message(&conn, &make_msg("m1", "ch1")).unwrap();

        let event = make_event(1, "ch1", "message_updated", "m1");
        let applied = apply_sync_event(&conn, &event, None, None).unwrap();
        assert!(!applied, "update without plaintext should return false");
    }

    #[test]
    fn test_deletions_always_win() {
        let conn = test_conn();
        messages::insert_message(&conn, &make_msg("m1", "ch1")).unwrap();

        let delete_event = make_event(1, "ch1", "message_deleted", "m1");
        apply_sync_event(&conn, &delete_event, None, None).unwrap();

        let m = messages::get_message(&conn, "m1").unwrap().unwrap();
        assert_eq!(m.status, "deleted");

        // Edit on deleted message should be ignored
        let mut edit_event = make_event(2, "ch1", "message_updated", "m1");
        edit_event.edited_at = Some("2024-06-01T00:00:00Z".to_string());
        let applied =
            apply_sync_event(&conn, &edit_event, Some("edited after delete"), Some(3000)).unwrap();
        assert!(!applied, "edit on deleted message should be ignored");

        let m = messages::get_message(&conn, "m1").unwrap().unwrap();
        assert_eq!(m.status, "deleted", "message should remain deleted");
    }
}
