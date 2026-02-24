use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

use openconv_shared::api::ws::ClientMessage;
use openconv_shared::ids::{ChannelId, MessageId};
use tauri::{AppHandle, Manager, State};

use crate::auth_service::AppError;
use crate::cache::messages::{self, CachedMessage};
use crate::cache::queue;
use crate::cache::search;
use crate::cache::CacheDb;
use crate::crypto_service::CryptoState;
use crate::ws::handlers::{MSG_STATUS_DELIVERED, MSG_STATUS_PENDING, MSG_STATUS_QUEUED};
use crate::ws::WsState;

/// Simple sliding-window rate limiter for outgoing messages.
pub struct MessageRateLimiter {
    recent_sends: VecDeque<Instant>,
    max_per_second: usize,
}

impl MessageRateLimiter {
    pub fn new(max_per_second: usize) -> Self {
        Self {
            recent_sends: VecDeque::with_capacity(max_per_second),
            max_per_second,
        }
    }

    /// Check if a send is allowed; if so, record it. Returns Err if rate-limited.
    pub fn check_and_record(&mut self) -> Result<(), AppError> {
        let now = Instant::now();
        // Remove entries older than 1 second
        while let Some(&oldest) = self.recent_sends.front() {
            if now.duration_since(oldest).as_secs_f64() >= 1.0 {
                self.recent_sends.pop_front();
            } else {
                break;
            }
        }
        if self.recent_sends.len() >= self.max_per_second {
            return Err(AppError::new("rate limited: max 5 messages per second"));
        }
        self.recent_sends.push_back(now);
        Ok(())
    }
}

/// Send an encrypted message to a channel.
///
/// Inserts an optimistic local message with status="pending", then sends via WebSocket.
/// Encryption per-device will be wired in once the member/device directory is available.
#[tauri::command]
#[specta::specta]
pub async fn send_message(
    _app: AppHandle,
    channel_id: String,
    plaintext: String,
    ws_state: State<'_, WsState>,
    cache_db: State<'_, CacheDb>,
    rate_limiter: State<'_, Mutex<MessageRateLimiter>>,
) -> Result<String, AppError> {
    // 1. Rate limit check
    {
        let mut limiter = rate_limiter
            .lock()
            .map_err(|_| AppError::new("rate limiter lock poisoned"))?;
        limiter.check_and_record()?;
    }

    // 2. Get current user ID
    let sender_id = {
        let uid = ws_state.current_user_id.read().await;
        uid.map(|id| id.to_string())
            .unwrap_or_default()
    };

    // 3. Generate message ID and client nonce
    let message_id = MessageId::new();
    let msg_id_str = message_id.to_string();
    let client_nonce = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let now_ms = now.timestamp_millis();

    // 4. Optimistic insert into local cache
    let cached = CachedMessage {
        id: msg_id_str.clone(),
        channel_id: Some(channel_id.clone()),
        dm_channel_id: None,
        sender_id,
        plaintext: Some(plaintext.clone()),
        ciphertext: None,
        message_type: None,
        created_at: now_ms,
        edited_at: None,
        decrypted_at: Some(now_ms),
        status: MSG_STATUS_PENDING.into(),
    };

    {
        let conn = cache_db.lock()?;
        messages::insert_message(&conn, &cached)?;

        // Index in FTS immediately (the sender's own message)
        search::index_message(&conn, &msg_id_str, &plaintext)?;
    }

    // 5. Store nonce for dedup matching when server echoes back
    {
        let mut nonces = ws_state.pending_nonces.write().await;
        nonces.insert(client_nonce.clone(), msg_id_str.clone());
    }

    // 6. Encrypt for channel members
    // TODO: Fetch channel members and their device lists, establish sessions,
    // encrypt plaintext per-device via CryptoService::encrypt_for_channel.
    // For now, recipients is empty -- the server will reject this until
    // the member/device directory and encryption pipeline are wired in.
    let recipients = vec![];

    // 7. Send via WebSocket
    let channel_id_typed: ChannelId = channel_id
        .parse()
        .map_err(|_| AppError::new("invalid channel_id"))?;

    let tx_guard = ws_state.outgoing_tx.read().await;
    if let Some(tx) = tx_guard.as_ref() {
        tx.send(ClientMessage::SendMessage {
            channel_id: Some(channel_id_typed),
            dm_channel_id: None,
            recipients,
            client_nonce: Some(client_nonce),
        })
        .map_err(|_| AppError::new("failed to send message: WebSocket channel closed"))?;
    } else {
        // Not connected -- enqueue for offline delivery
        let conn = cache_db.lock()?;
        messages::update_message_status(&conn, &msg_id_str, MSG_STATUS_QUEUED)?;
        queue::enqueue_message(&conn, &msg_id_str, Some(&channel_id), None, &plaintext, now_ms)?;
    }

    Ok(msg_id_str)
}

/// Edit a previously sent message.
///
/// Updates local cache and FTS, then sends the edit via WebSocket.
#[tauri::command]
#[specta::specta]
pub async fn edit_message(
    channel_id: String,
    message_id: String,
    new_plaintext: String,
    ws_state: State<'_, WsState>,
    cache_db: State<'_, CacheDb>,
) -> Result<(), AppError> {
    let now_ms = chrono::Utc::now().timestamp_millis();

    // Update local cache
    {
        let conn = cache_db.lock()?;
        messages::update_message_content(&conn, &message_id, &new_plaintext, now_ms)?;
        search::reindex_message(&conn, &message_id, &new_plaintext)?;
    }

    // TODO: Re-encrypt new plaintext per-device with current ratchet state.
    let recipients = vec![];

    // Send edit via WebSocket
    let channel_id_typed: ChannelId = channel_id
        .parse()
        .map_err(|_| AppError::new("invalid channel_id"))?;
    let message_id_typed: MessageId = message_id
        .parse()
        .map_err(|_| AppError::new("invalid message_id"))?;

    let tx_guard = ws_state.outgoing_tx.read().await;
    if let Some(tx) = tx_guard.as_ref() {
        tx.send(ClientMessage::EditMessage {
            channel_id: channel_id_typed,
            message_id: message_id_typed,
            recipients,
        })
        .map_err(|_| AppError::new("failed to send edit: WebSocket channel closed"))?;
    } else {
        return Err(AppError::new("cannot edit message: not connected"));
    }

    Ok(())
}

/// Delete a message (soft-delete locally, notify server).
#[tauri::command]
#[specta::specta]
pub async fn delete_message(
    channel_id: String,
    message_id: String,
    ws_state: State<'_, WsState>,
    cache_db: State<'_, CacheDb>,
) -> Result<(), AppError> {
    // Check connection first - don't modify local state if we can't notify server
    let tx_guard = ws_state.outgoing_tx.read().await;
    let tx = tx_guard
        .as_ref()
        .ok_or_else(|| AppError::new("cannot delete message: not connected"))?;

    // Soft-delete locally and remove from FTS
    {
        let conn = cache_db.lock()?;
        messages::soft_delete_message(&conn, &message_id)?;
        search::deindex_message(&conn, &message_id)?;
    }

    // Notify server
    let channel_id_typed: ChannelId = channel_id
        .parse()
        .map_err(|_| AppError::new("invalid channel_id"))?;
    let message_id_typed: MessageId = message_id
        .parse()
        .map_err(|_| AppError::new("invalid message_id"))?;

    tx.send(ClientMessage::DeleteMessage {
        channel_id: channel_id_typed,
        message_id: message_id_typed,
    })
    .map_err(|_| AppError::new("failed to send delete: WebSocket channel closed"))?;

    Ok(())
}

/// Retry decryption of a previously failed message.
///
/// For SessionNotFound/SessionCorrupted: re-fetch pre-key bundle,
/// establish new session, then attempt decryption with retained ciphertext.
#[tauri::command]
#[specta::specta]
pub async fn retry_decrypt(
    app: AppHandle,
    message_id: String,
    cache_db: State<'_, CacheDb>,
) -> Result<(), AppError> {
    // Load the failed message from cache
    let (sender_id, ciphertext, message_type) = {
        let conn = cache_db.lock()?;
        let msg = messages::get_message(&conn, &message_id)?
            .ok_or_else(|| AppError::new("message not found"))?;

        if msg.status != crate::ws::handlers::MSG_STATUS_DECRYPT_FAILED {
            return Err(AppError::new("message is not in decrypt_failed state"));
        }

        let ct = msg
            .ciphertext
            .ok_or_else(|| AppError::new("no ciphertext retained for retry"))?;
        let mt = msg
            .message_type
            .unwrap_or_else(|| "signal".to_string());

        (msg.sender_id, ct, mt)
    };

    // TODO: Re-fetch pre-key bundle from server and establish new session
    // via CryptoService::ensure_session before retrying decrypt.

    // Attempt decryption
    let msg_id = message_id.clone();
    let app_clone = app.clone();
    let decrypt_result = tokio::task::spawn_blocking(move || {
        let crypto_state = app_clone.state::<CryptoState>();
        crypto_state.crypto_service.decrypt_message(
            &sender_id,
            1, // TODO: device_id
            &ciphertext,
            &message_type,
        )
    })
    .await
    .map_err(|e| AppError::new(format!("internal error: {e}")))?;

    match decrypt_result {
        Ok(plaintext_bytes) => {
            let plaintext = String::from_utf8_lossy(&plaintext_bytes).to_string();
            let now_ms = chrono::Utc::now().timestamp_millis();
            let conn = cache_db.lock()?;
            messages::update_message_plaintext(&conn, &msg_id, &plaintext, now_ms)?;
            messages::update_message_status(&conn, &msg_id, MSG_STATUS_DELIVERED)?;
            search::index_message(&conn, &msg_id, &plaintext)?;
            Ok(())
        }
        Err(e) => Err(AppError::new(format!("decryption retry failed: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- MessageRateLimiter tests ---

    #[test]
    fn rate_limiter_allows_up_to_max() {
        let mut limiter = MessageRateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.check_and_record().is_ok());
        }
    }

    #[test]
    fn rate_limiter_rejects_over_max() {
        let mut limiter = MessageRateLimiter::new(5);
        for _ in 0..5 {
            limiter.check_and_record().unwrap();
        }
        assert!(limiter.check_and_record().is_err());
    }

    #[test]
    fn rate_limiter_allows_after_window_expires() {
        let mut limiter = MessageRateLimiter::new(5);
        // Fill up the window with old entries
        let old = Instant::now() - std::time::Duration::from_secs(2);
        for _ in 0..5 {
            limiter.recent_sends.push_back(old);
        }
        // Should be allowed since entries are >1s old
        assert!(limiter.check_and_record().is_ok());
    }

    #[test]
    fn rate_limiter_new_starts_empty() {
        let limiter = MessageRateLimiter::new(5);
        assert!(limiter.recent_sends.is_empty());
        assert_eq!(limiter.max_per_second, 5);
    }

    // --- Cache operation tests using in-memory DB ---

    #[test]
    fn insert_and_retrieve_message_from_cache() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let msg = CachedMessage {
            id: "msg-001".into(),
            channel_id: Some("ch-001".into()),
            dm_channel_id: None,
            sender_id: "user-001".into(),
            plaintext: Some("hello world".into()),
            ciphertext: None,
            message_type: Some("signal".into()),
            created_at: 1700000000000,
            edited_at: None,
            decrypted_at: Some(1700000000001),
            status: "delivered".into(),
        };

        messages::insert_message(&conn, &msg).unwrap();
        let retrieved = messages::get_message(&conn, "msg-001").unwrap().unwrap();
        assert_eq!(retrieved.plaintext.as_deref(), Some("hello world"));
        assert_eq!(retrieved.status, "delivered");
    }

    #[test]
    fn optimistic_insert_then_update_status() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let msg = CachedMessage {
            id: "msg-002".into(),
            channel_id: Some("ch-001".into()),
            dm_channel_id: None,
            sender_id: "user-001".into(),
            plaintext: Some("pending message".into()),
            ciphertext: None,
            message_type: None,
            created_at: 1700000000000,
            edited_at: None,
            decrypted_at: None,
            status: "pending".into(),
        };

        messages::insert_message(&conn, &msg).unwrap();
        messages::update_message_status(&conn, "msg-002", "delivered").unwrap();

        let retrieved = messages::get_message(&conn, "msg-002").unwrap().unwrap();
        assert_eq!(retrieved.status, "delivered");
    }

    #[test]
    fn soft_delete_sets_status_deleted() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let msg = CachedMessage {
            id: "msg-003".into(),
            channel_id: Some("ch-001".into()),
            dm_channel_id: None,
            sender_id: "user-001".into(),
            plaintext: Some("to be deleted".into()),
            ciphertext: None,
            message_type: None,
            created_at: 1700000000000,
            edited_at: None,
            decrypted_at: None,
            status: "delivered".into(),
        };

        messages::insert_message(&conn, &msg).unwrap();
        messages::soft_delete_message(&conn, "msg-003").unwrap();

        let retrieved = messages::get_message(&conn, "msg-003").unwrap().unwrap();
        assert_eq!(retrieved.status, "deleted");
    }

    /// Helper to insert a message into both the messages table and FTS index.
    fn insert_with_fts(conn: &rusqlite::Connection, id: &str, plaintext: &str) {
        let msg = CachedMessage {
            id: id.into(),
            channel_id: Some("ch-001".into()),
            dm_channel_id: None,
            sender_id: "user-001".into(),
            plaintext: Some(plaintext.into()),
            ciphertext: None,
            message_type: None,
            created_at: 1700000000000,
            edited_at: None,
            decrypted_at: None,
            status: "delivered".into(),
        };
        messages::insert_message(conn, &msg).unwrap();
        search::index_message(conn, id, plaintext).unwrap();
    }

    #[test]
    fn fts_index_and_deindex() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        insert_with_fts(&conn, "msg-004", "searchable content here");

        let results =
            search::search_messages(&conn, "searchable", &search::SearchScope::AllMessages, 10)
                .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].message_id, "msg-004");

        search::deindex_message(&conn, "msg-004").unwrap();
        let results =
            search::search_messages(&conn, "searchable", &search::SearchScope::AllMessages, 10)
                .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn fts_reindex_updates_content() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        insert_with_fts(&conn, "msg-005", "original content");
        search::reindex_message(&conn, "msg-005", "updated content").unwrap();

        // Old content should not be found
        let results =
            search::search_messages(&conn, "original", &search::SearchScope::AllMessages, 10)
                .unwrap();
        assert!(results.is_empty());

        // New content should be found
        let results =
            search::search_messages(&conn, "updated", &search::SearchScope::AllMessages, 10)
                .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn edit_message_updates_content_and_edited_at() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let msg = CachedMessage {
            id: "msg-006".into(),
            channel_id: Some("ch-001".into()),
            dm_channel_id: None,
            sender_id: "user-001".into(),
            plaintext: Some("original".into()),
            ciphertext: None,
            message_type: None,
            created_at: 1700000000000,
            edited_at: None,
            decrypted_at: None,
            status: "delivered".into(),
        };

        messages::insert_message(&conn, &msg).unwrap();
        messages::update_message_content(&conn, "msg-006", "edited content", 1700000001000)
            .unwrap();

        let retrieved = messages::get_message(&conn, "msg-006").unwrap().unwrap();
        assert_eq!(retrieved.plaintext.as_deref(), Some("edited content"));
        assert_eq!(retrieved.edited_at, Some(1700000001000));
    }

    #[test]
    fn decrypt_failed_message_retains_ciphertext() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let msg = CachedMessage {
            id: "msg-007".into(),
            channel_id: Some("ch-001".into()),
            dm_channel_id: None,
            sender_id: "user-001".into(),
            plaintext: None,
            ciphertext: Some(vec![0xDE, 0xAD, 0xBE, 0xEF]),
            message_type: Some("prekey".into()),
            created_at: 1700000000000,
            edited_at: None,
            decrypted_at: None,
            status: "decrypt_failed".into(),
        };

        messages::insert_message(&conn, &msg).unwrap();
        let retrieved = messages::get_message(&conn, "msg-007").unwrap().unwrap();
        assert_eq!(retrieved.status, "decrypt_failed");
        assert_eq!(retrieved.ciphertext, Some(vec![0xDE, 0xAD, 0xBE, 0xEF]));
        assert!(retrieved.plaintext.is_none());
    }
}
