#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, State};

use crate::auth_service::{self, AppError};
use crate::cache::dm_channels;
use crate::cache::messages::{self, DEFAULT_PLAINTEXT_TTL_SECS};
use crate::cache::queue;
use crate::cache::search;
use crate::cache::sync;
use crate::cache::CacheDb;
use crate::commands::device_directory;
use crate::crypto_service::CryptoState;
use crate::ws::handlers::MSG_STATUS_DELIVERED;
use crate::ws::WsState;

/// Maximum messages to send per second from the queue.
const QUEUE_RATE_LIMIT_PER_SEC: u64 = 5;

/// Maximum retry attempts before marking a message as failed.
const MAX_RETRY_COUNT: u32 = 5;

/// Manually trigger sync for all inactive channels.
/// Fetches events via REST for channels not currently subscribed via WebSocket.
#[tauri::command]
#[specta::specta]
pub async fn sync_inactive_channels(
    _app: AppHandle,
    cache_db: State<'_, CacheDb>,
    ws_state: State<'_, WsState>,
) -> Result<u32, String> {
    let api_base_url = ws_state.api_base_url.read().await.clone();

    // Get auth token
    let access_token = tokio::task::spawn_blocking(auth_service::get_access_token)
        .await
        .map_err(|e| format!("internal error: {e}"))?
        .map_err(|_| "not authenticated".to_string())?;

    // Get all channels with sync state
    let all_channels = {
        let conn = cache_db.lock().map_err(|e| e.to_string())?;
        sync::get_all_sync_channels(&conn).map_err(|e| e.to_string())?
    };

    // Get currently subscribed channels
    let subscribed = ws_state.subscribed_channels.read().await;
    let subscribed_set: std::collections::HashSet<String> =
        subscribed.iter().map(|id| id.to_string()).collect();
    drop(subscribed);

    // Filter to inactive channels
    let inactive: Vec<String> = all_channels
        .into_iter()
        .filter(|ch| !subscribed_set.contains(ch))
        .collect();

    let mut total_events = 0u32;

    for channel_id in &inactive {
        let mut last_seq = {
            let conn = cache_db.lock().map_err(|e| e.to_string())?;
            sync::get_last_sequence(&conn, channel_id).map_err(|e| e.to_string())?
        };

        // Paginated fetch loop
        loop {
            let url = format!(
                "{}/api/sync?after_sequence={}&channel_ids={}&limit=100",
                api_base_url, last_seq, channel_id
            );

            let response = ws_state
                .http_client
                .get(&url)
                .bearer_auth(&access_token)
                .send()
                .await
                .map_err(|e| format!("sync request failed: {e}"))?;

            if !response.status().is_success() {
                tracing::warn!(
                    "sync request for channel {} returned status {}",
                    channel_id,
                    response.status()
                );
                break;
            }

            let sync_response: sync::SyncResponse = response
                .json()
                .await
                .map_err(|e| format!("failed to parse sync response: {e}"))?;

            let mut max_sequence = last_seq;
            for event in &sync_response.events {
                // TODO: For message_created and message_updated, fetch full message
                // content from server (GET /api/channels/{channel_id}/messages/{message_id})
                // and decrypt via CryptoService before applying.
                let conn = cache_db.lock().map_err(|e| e.to_string())?;
                match sync::apply_sync_event(&conn, event, None, None) {
                    Ok(applied) => {
                        if applied {
                            total_events += 1;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "failed to apply sync event {} for message {}: {e}",
                            event.event_type,
                            event.message_id
                        );
                        // Don't advance sequence past failed events
                        continue;
                    }
                }
                if event.sequence > max_sequence {
                    max_sequence = event.sequence;
                }
            }

            // Update sync state with highest successfully processed sequence
            if max_sequence > last_seq {
                let conn = cache_db.lock().map_err(|e| e.to_string())?;
                sync::update_last_sequence(&conn, channel_id, max_sequence)
                    .map_err(|e| e.to_string())?;
            }

            // Continue if there are more pages
            if sync_response.has_more {
                last_seq = sync_response.next_sequence;
            } else {
                break;
            }
        }
    }

    Ok(total_events)
}

/// Re-decrypt a single message on demand (e.g., when scrolled into view).
/// Returns the decrypted plaintext on success, or an error on failure.
#[tauri::command]
#[specta::specta]
pub async fn redecrypt_message(
    app: AppHandle,
    message_id: String,
    cache_db: State<'_, CacheDb>,
) -> Result<Option<String>, String> {
    // Load the message from cache
    let (sender_id, sender_device_id_opt, ciphertext, message_type) = {
        let conn = cache_db.lock().map_err(|e| e.to_string())?;
        let msg = messages::get_message(&conn, &message_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "message not found".to_string())?;

        if msg.plaintext.is_some() {
            return Ok(msg.plaintext);
        }

        let ct = msg
            .ciphertext
            .ok_or_else(|| "no ciphertext available for re-decryption".to_string())?;
        let mt = msg.message_type.unwrap_or_else(|| "signal".to_string());
        (msg.sender_id, msg.sender_device_id, ct, mt)
    };

    // Attempt decryption
    let signal_device_id =
        crate::crypto_service::resolve_signal_device_id(sender_device_id_opt.as_deref());
    let msg_id = message_id.clone();
    let app_clone = app.clone();
    let decrypt_result = tokio::task::spawn_blocking(move || {
        let crypto_state = app_clone.state::<CryptoState>();
        crypto_state.crypto_service.decrypt_message(
            &sender_id,
            signal_device_id,
            &ciphertext,
            &message_type,
        )
    })
    .await
    .map_err(|e| format!("internal error: {e}"))?;

    match decrypt_result {
        Ok(plaintext_bytes) => {
            let plaintext = String::from_utf8_lossy(&plaintext_bytes).to_string();
            let now_ms = chrono::Utc::now().timestamp_millis();
            let conn = cache_db.lock().map_err(|e| e.to_string())?;
            messages::update_message_plaintext(&conn, &msg_id, &plaintext, now_ms)
                .map_err(|e| e.to_string())?;
            messages::update_message_status(&conn, &msg_id, MSG_STATUS_DELIVERED)
                .map_err(|e| e.to_string())?;
            search::index_message(&conn, &msg_id, &plaintext).map_err(|e| e.to_string())?;
            Ok(Some(plaintext))
        }
        Err(e) => Err(format!("re-decryption failed: {e}")),
    }
}

/// Retry sending a failed queued message.
/// Resets the message status back to pending so the queue processor will pick it up.
#[tauri::command]
#[specta::specta]
pub async fn retry_queued_message(
    message_id: String,
    cache_db: State<'_, CacheDb>,
) -> Result<(), String> {
    let conn = cache_db.lock().map_err(|e| e.to_string())?;

    let reset = queue::reset_to_pending(&conn, &message_id).map_err(|e| e.to_string())?;
    if !reset {
        return Err("message not found in queue or not in failed state".to_string());
    }

    messages::update_message_status(&conn, &message_id, "pending").map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a failed queued message (user chose to discard it).
/// Removes from queue and marks the message as failed in the messages table.
#[tauri::command]
#[specta::specta]
pub async fn discard_queued_message(
    message_id: String,
    cache_db: State<'_, CacheDb>,
) -> Result<(), String> {
    let conn = cache_db.lock().map_err(|e| e.to_string())?;

    if !queue::exists_in_queue(&conn, &message_id).map_err(|e| e.to_string())? {
        return Err("message not found in outgoing queue".to_string());
    }

    queue::remove_from_queue(&conn, &message_id).map_err(|e| e.to_string())?;
    messages::update_message_status(&conn, &message_id, "failed").map_err(|e| e.to_string())?;
    Ok(())
}

/// Process the outgoing queue after reconnection.
/// Re-encrypts each message and sends via WebSocket, FIFO per-channel.
/// Rate-limited to 5 msg/s. Stops if connection drops.
pub async fn process_queue(app: AppHandle) {
    let cache_db = app.state::<CacheDb>();
    let ws_state = app.state::<WsState>();

    let pending = {
        let conn = match cache_db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("process_queue: failed to lock cache db: {e}");
                return;
            }
        };
        match queue::dequeue_pending(&conn, 500) {
            Ok(msgs) => msgs,
            Err(e) => {
                tracing::error!("process_queue: failed to dequeue: {e}");
                return;
            }
        }
    };

    if pending.is_empty() {
        tracing::debug!("process_queue: no pending messages");
        return;
    }

    tracing::info!(
        "process_queue: processing {} queued messages",
        pending.len()
    );
    let rate_delay = Duration::from_millis(1000 / QUEUE_RATE_LIMIT_PER_SEC);

    for queued_msg in &pending {
        // Check if still connected
        {
            use crate::ws::state::WsConnectionState;
            let state = ws_state.connection_state.read().await;
            if !matches!(*state, WsConnectionState::Authenticated) {
                tracing::warn!("process_queue: connection lost, stopping");
                return;
            }
        }

        // Mark as sending
        {
            let conn = match cache_db.lock() {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Err(e) = queue::mark_sending(&conn, &queued_msg.message_id) {
                tracing::warn!(
                    "process_queue: failed to mark sending {}: {e}",
                    queued_msg.message_id
                );
                continue;
            }
        }

        // Get sender ID for encryption
        let sender_id = {
            let uid = ws_state.current_user_id.read().await;
            match uid.map(|id| id.to_string()) {
                Some(id) if !id.is_empty() => id,
                _ => {
                    tracing::warn!("process_queue: no current user_id, skipping encryption");
                    continue;
                }
            }
        };

        // Encrypt plaintext per-device for the appropriate recipient set
        let encrypt_result = if let Some(ref ch_id) = queued_msg.channel_id {
            // Channel message: look up guild_id from channel_cache
            let guild_id = {
                let conn = match cache_db.lock() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                conn.query_row(
                    "SELECT guild_id FROM channel_cache WHERE id = ?1",
                    [ch_id.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .ok()
            };
            match guild_id {
                Some(gid) => {
                    device_directory::encrypt_for_channel(
                        &app,
                        &gid,
                        &sender_id,
                        queued_msg.plaintext.as_bytes(),
                    )
                    .await
                }
                None => {
                    tracing::warn!(
                        "process_queue: no guild_id for channel {} — skipping {}",
                        ch_id,
                        queued_msg.message_id
                    );
                    continue;
                }
            }
        } else if let Some(ref dm_id) = queued_msg.dm_channel_id {
            // DM message: look up participant_ids from dm_channel_cache
            let participant_ids = {
                let conn = match cache_db.lock() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                dm_channels::get_dm_channel(&conn, dm_id)
                    .ok()
                    .flatten()
                    .map(|dm| dm.participant_ids)
            };
            match participant_ids {
                Some(pids) => {
                    device_directory::encrypt_for_dm(
                        &app,
                        &pids,
                        &sender_id,
                        queued_msg.plaintext.as_bytes(),
                    )
                    .await
                }
                None => {
                    tracing::warn!(
                        "process_queue: no participants for DM {} — skipping {}",
                        dm_id,
                        queued_msg.message_id
                    );
                    continue;
                }
            }
        } else {
            tracing::warn!(
                "process_queue: message {} has no channel_id or dm_channel_id",
                queued_msg.message_id
            );
            continue;
        };

        let recipients = match encrypt_result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    "process_queue: encryption failed for {}: {e}",
                    queued_msg.message_id
                );
                // Increment retry — encryption failure is recoverable (session may establish later)
                let conn = match cache_db.lock() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let retry_count = queue::increment_retry(&conn, &queued_msg.message_id)
                    .unwrap_or(MAX_RETRY_COUNT);
                if retry_count >= MAX_RETRY_COUNT {
                    let _ = queue::mark_failed(&conn, &queued_msg.message_id);
                    let _ =
                        messages::update_message_status(&conn, &queued_msg.message_id, "failed");
                }
                continue;
            }
        };

        // Build and send the message via WebSocket
        let sent = {
            let tx_guard = ws_state.outgoing_tx.read().await;
            if let Some(tx) = tx_guard.as_ref() {
                use openconv_shared::api::ws::ClientMessage;
                let msg = if let Some(ref ch_id) = queued_msg.channel_id {
                    ClientMessage::SendMessage {
                        channel_id: ch_id.parse().ok(),
                        dm_channel_id: None,
                        recipients,
                        client_nonce: Some(queued_msg.message_id.clone()),
                    }
                } else if let Some(ref dm_id) = queued_msg.dm_channel_id {
                    ClientMessage::SendMessage {
                        channel_id: None,
                        dm_channel_id: dm_id.parse().ok(),
                        recipients,
                        client_nonce: Some(queued_msg.message_id.clone()),
                    }
                } else {
                    continue;
                };
                tx.send(msg).is_ok()
            } else {
                false
            }
        };

        if sent {
            // Mark as sent (remove from queue) and update message status
            let conn = match cache_db.lock() {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Err(e) = queue::mark_sent(&conn, &queued_msg.message_id) {
                tracing::warn!(
                    "process_queue: failed to mark sent {}: {e}",
                    queued_msg.message_id
                );
            }
            let _ = messages::update_message_status(
                &conn,
                &queued_msg.message_id,
                MSG_STATUS_DELIVERED,
            );
        } else {
            // Send failed - increment retry
            let conn = match cache_db.lock() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let retry_count =
                queue::increment_retry(&conn, &queued_msg.message_id).unwrap_or(MAX_RETRY_COUNT);
            if retry_count >= MAX_RETRY_COUNT {
                let _ = queue::mark_failed(&conn, &queued_msg.message_id);
                let _ = messages::update_message_status(&conn, &queued_msg.message_id, "failed");
                tracing::warn!(
                    "process_queue: message {} failed after {} retries",
                    queued_msg.message_id,
                    retry_count
                );
            }
        }

        // Rate limit
        tokio::time::sleep(rate_delay).await;
    }

    tracing::info!("process_queue: finished processing");
}

/// Spawn a periodic task that cleans up expired plaintext every hour.
/// Returns a JoinHandle that can be used to cancel the task.
pub fn spawn_ttl_cleanup_task(
    cache_db: Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_secs(3600); // 1 hour
        loop {
            tokio::time::sleep(interval).await;

            let ttl = DEFAULT_PLAINTEXT_TTL_SECS;
            match cache_db.lock() {
                Ok(conn) => match messages::clear_expired_plaintext(&conn, ttl) {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!("ttl_cleanup: cleared plaintext from {count} messages");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("ttl_cleanup: failed to clear expired plaintext: {e}");
                    }
                },
                Err(e) => {
                    tracing::warn!("ttl_cleanup: failed to lock cache db: {e}");
                }
            }
        }
    })
}

/// Re-decrypt messages from the last session window that have NULL plaintext.
/// Processes messages in reverse chronological order (most recent first).
/// Returns the number of messages successfully re-decrypted.
pub async fn proactive_redecrypt(app: &AppHandle, window_secs: i64) -> Result<usize, AppError> {
    let cache_db = app.state::<CacheDb>();

    // Get messages needing re-decryption
    let messages_to_decrypt = {
        let conn = cache_db.lock()?;
        messages::get_messages_needing_redecrypt(&conn, window_secs)?
    };

    if messages_to_decrypt.is_empty() {
        return Ok(0);
    }

    tracing::info!(
        "proactive_redecrypt: {} messages need re-decryption",
        messages_to_decrypt.len()
    );

    let mut success_count = 0usize;

    for msg in &messages_to_decrypt {
        let ciphertext = match &msg.ciphertext {
            Some(ct) => ct.clone(),
            None => continue,
        };
        let sender_id = msg.sender_id.clone();
        let sender_device_id_opt = msg.sender_device_id.clone();
        let message_type = msg
            .message_type
            .clone()
            .unwrap_or_else(|| "signal".to_string());
        let msg_id = msg.id.clone();

        let signal_device_id =
            crate::crypto_service::resolve_signal_device_id(sender_device_id_opt.as_deref());
        let app_clone = app.clone();
        let decrypt_result = tokio::task::spawn_blocking(move || {
            let crypto_state = app_clone.state::<CryptoState>();
            crypto_state.crypto_service.decrypt_message(
                &sender_id,
                signal_device_id,
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
                if let Err(e) = search::index_message(&conn, &msg_id, &plaintext) {
                    tracing::warn!("proactive_redecrypt: failed to index {msg_id} in FTS: {e}");
                }
                success_count += 1;
            }
            Err(e) => {
                tracing::debug!(
                    "proactive_redecrypt: failed to decrypt {msg_id}: {e} (will try lazy)"
                );
            }
        }
    }

    tracing::info!(
        "proactive_redecrypt: re-decrypted {success_count}/{} messages",
        messages_to_decrypt.len()
    );
    Ok(success_count)
}
