use std::sync::Mutex;

use openconv_shared::api::ws::ClientMessage;
use openconv_shared::ids::{DmChannelId, MessageId};
use tauri::{AppHandle, State};

use crate::auth_service::{self, AppError};
use crate::cache::dm_channels::{self, CachedDmChannel};
use crate::cache::messages::{self, CachedMessage};
use crate::cache::queue;
use crate::cache::search;
use crate::cache::CacheDb;
use crate::commands::device_directory;
use crate::ws::handlers::{MSG_STATUS_PENDING, MSG_STATUS_QUEUED};
use crate::ws::WsState;

use super::messaging::MessageRateLimiter;

/// DM channel info returned to the frontend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DmChannelInfo {
    pub id: String,
    pub participant_ids: Vec<String>,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<String>,
    pub unread_count: u32,
}

/// Start or retrieve a DM conversation with a user.
///
/// 1. Check local cache for existing DM channel with this user
/// 2. If found locally, return its DmChannelId
/// 3. If not found locally, call the server to create/find the DM
/// 4. Cache the DM channel locally
/// 5. Return the DmChannelId
#[tauri::command]
#[specta::specta]
pub async fn start_dm(user_id: String, cache_db: State<'_, CacheDb>) -> Result<String, AppError> {
    // 1. Check local cache for existing DM with this user
    {
        let conn = cache_db.lock()?;
        if let Some(dm) = dm_channels::get_dm_channel_by_participant(&conn, &user_id)? {
            return Ok(dm.id);
        }
    }

    // 2. Not found locally -- call server to create or retrieve the DM channel
    let access_token = tokio::task::spawn_blocking(auth_service::get_access_token)
        .await
        .map_err(|e| AppError::new(format!("internal error: {e}")))?
        .map_err(|_| AppError::new("not authenticated"))?;

    let api_base_url =
        std::env::var("OPENCONV_API_URL").unwrap_or_else(|_| "http://localhost:3000".into());

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{api_base_url}/api/dm-channels"))
        .bearer_auth(&access_token)
        .json(&openconv_shared::api::dm_channel::CreateDmChannelRequest {
            user_ids: vec![user_id
                .parse()
                .map_err(|_| AppError::new("invalid user_id"))?],
            name: None,
        })
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::new(format!("server returned {status}: {body}")));
    }

    let dm_resp: openconv_shared::api::dm_channel::DmChannelResponse = resp.json().await?;
    let dm_channel_id = dm_resp.id.to_string();

    // 3. Cache the DM channel locally
    let now_ms = chrono::Utc::now().timestamp_millis();
    {
        let conn = cache_db.lock()?;
        dm_channels::upsert_dm_channel(
            &conn,
            &CachedDmChannel {
                id: dm_channel_id.clone(),
                participant_ids: dm_resp.members.iter().map(|m| m.to_string()).collect(),
                last_message_preview: None,
                last_message_at: None,
                created_at: now_ms,
            },
        )?;
    }

    Ok(dm_channel_id)
}

/// List all DM conversations for the current user.
///
/// Returns DM channels sorted by most recent message, fetched from local cache.
/// On first load or cache miss, fetches from server and populates cache.
#[tauri::command]
#[specta::specta]
pub async fn list_dms(cache_db: State<'_, CacheDb>) -> Result<Vec<DmChannelInfo>, AppError> {
    let cached = {
        let conn = cache_db.lock()?;
        dm_channels::list_dm_channels(&conn)?
    };

    // If cache is empty, try fetching from server
    if cached.is_empty() {
        let access_token = tokio::task::spawn_blocking(auth_service::get_access_token)
            .await
            .map_err(|e| AppError::new(format!("internal error: {e}")))?;

        // If not authenticated, return empty list
        let access_token = match access_token {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        let api_base_url =
            std::env::var("OPENCONV_API_URL").unwrap_or_else(|_| "http://localhost:3000".into());

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{api_base_url}/api/dm-channels"))
            .bearer_auth(&access_token)
            .send()
            .await?;

        if resp.status().is_success() {
            let dm_list: Vec<openconv_shared::api::dm_channel::DmChannelResponse> =
                resp.json().await?;

            let conn = cache_db.lock()?;
            let mut result = Vec::new();

            for dm_resp in dm_list {
                let dm_id = dm_resp.id.to_string();
                let participants: Vec<String> =
                    dm_resp.members.iter().map(|m| m.to_string()).collect();
                let created = dm_resp.created_at.timestamp_millis();

                dm_channels::upsert_dm_channel(
                    &conn,
                    &CachedDmChannel {
                        id: dm_id.clone(),
                        participant_ids: participants.clone(),
                        last_message_preview: None,
                        last_message_at: None,
                        created_at: created,
                    },
                )?;

                result.push(DmChannelInfo {
                    id: dm_id,
                    participant_ids: participants,
                    last_message_preview: None,
                    last_message_at: None,
                    unread_count: 0,
                });
            }

            return Ok(result);
        }
    }

    // Return from cache
    Ok(cached
        .into_iter()
        .map(|dm| DmChannelInfo {
            id: dm.id,
            participant_ids: dm.participant_ids,
            last_message_preview: dm.last_message_preview,
            last_message_at: dm.last_message_at.map(|ts| {
                chrono::DateTime::from_timestamp_millis(ts)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            }),
            unread_count: 0, // Will be populated by unread tracking (section-13)
        })
        .collect())
}

/// Send a message in a DM channel.
///
/// Inserts an optimistic local message with status="pending", then sends via WebSocket.
#[tauri::command]
#[specta::specta]
pub async fn send_dm_message(
    app: AppHandle,
    dm_channel_id: String,
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
        uid.map(|id| id.to_string()).unwrap_or_default()
    };
    // The sending device is excluded from its own recipient list;
    // every other device of ours still needs a copy.
    let sender_device_id = *ws_state.current_device_id.read().await;

    // 3. Generate message ID and client nonce
    let message_id = MessageId::new();
    let msg_id_str = message_id.to_string();
    let client_nonce = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let now_ms = now.timestamp_millis();

    // 4. Get participant IDs from local DM channel cache (needed for encryption)
    let participant_ids = {
        let conn = cache_db.lock()?;
        let dm = dm_channels::get_dm_channel(&conn, &dm_channel_id)?
            .ok_or_else(|| AppError::new("DM channel not found in local cache"))?;
        dm.participant_ids
    };

    // 5. Optimistic insert into local cache
    let cached = CachedMessage {
        id: msg_id_str.clone(),
        channel_id: None,
        dm_channel_id: Some(dm_channel_id.clone()),
        sender_id: sender_id.clone(),
        sender_device_id: None,
        sender_signal_device_id: None,
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
        search::index_message(&conn, &msg_id_str, &plaintext)?;

        // Update DM channel's last message preview (UTF-8 safe truncation)
        let preview = if plaintext.chars().count() > 100 {
            let truncated: String = plaintext.chars().take(97).collect();
            format!("{truncated}...")
        } else {
            plaintext.clone()
        };
        dm_channels::update_dm_last_message(&conn, &dm_channel_id, &preview, now_ms)?;
    }

    // 6. Store nonce for dedup matching
    {
        let mut nonces = ws_state.pending_nonces.write().await;
        nonces.insert(client_nonce.clone(), msg_id_str.clone());
    }

    // 7. Check connectivity — only encrypt if we can send
    let dm_channel_id_typed: DmChannelId = dm_channel_id
        .parse()
        .map_err(|_| AppError::new("invalid dm_channel_id"))?;

    let tx_guard = ws_state.outgoing_tx.read().await;
    if let Some(tx) = tx_guard.as_ref() {
        // 8. Encrypt for DM recipient devices
        let recipients = device_directory::encrypt_for_dm(
            &app,
            &participant_ids,
            &sender_id,
            sender_device_id.as_ref(),
            plaintext.as_bytes(),
        )
        .await?;

        tx.send(ClientMessage::SendMessage {
            channel_id: None,
            dm_channel_id: Some(dm_channel_id_typed),
            recipients,
            client_nonce: Some(client_nonce),
        })
        .map_err(|_| AppError::new("failed to send DM: WebSocket channel closed"))?;
    } else {
        // Not connected -- enqueue plaintext for offline delivery.
        // Encryption will happen on retry when connectivity is restored.
        let conn = cache_db.lock()?;
        messages::update_message_status(&conn, &msg_id_str, MSG_STATUS_QUEUED)?;
        queue::enqueue_message(
            &conn,
            &msg_id_str,
            None,
            Some(&dm_channel_id),
            &plaintext,
            now_ms,
        )?;
    }

    Ok(msg_id_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::dm_channels::{self, CachedDmChannel};
    use crate::cache::messages::{self, CachedMessage};
    use crate::cache::CacheDb;

    // --- DM Cache operation tests ---

    #[test]
    fn upsert_and_retrieve_dm_channel() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let dm = CachedDmChannel {
            id: "dm-001".into(),
            participant_ids: vec!["user-a".into(), "user-b".into()],
            last_message_preview: Some("Hey there!".into()),
            last_message_at: Some(1700000000000),
            created_at: 1700000000000,
        };

        dm_channels::upsert_dm_channel(&conn, &dm).unwrap();
        let retrieved = dm_channels::get_dm_channel(&conn, "dm-001")
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.id, "dm-001");
        assert_eq!(retrieved.participant_ids, vec!["user-a", "user-b"]);
        assert_eq!(
            retrieved.last_message_preview.as_deref(),
            Some("Hey there!")
        );
    }

    #[test]
    fn get_dm_channel_not_found() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let result = dm_channels::get_dm_channel(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn find_dm_channel_by_participant() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let dm = CachedDmChannel {
            id: "dm-002".into(),
            participant_ids: vec!["user-a".into(), "user-c".into()],
            last_message_preview: None,
            last_message_at: None,
            created_at: 1700000000000,
        };

        dm_channels::upsert_dm_channel(&conn, &dm).unwrap();
        let found = dm_channels::get_dm_channel_by_participant(&conn, "user-c")
            .unwrap()
            .unwrap();
        assert_eq!(found.id, "dm-002");
    }

    #[test]
    fn list_dm_channels_sorted_by_recent() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let dm1 = CachedDmChannel {
            id: "dm-old".into(),
            participant_ids: vec!["user-a".into(), "user-b".into()],
            last_message_preview: Some("old".into()),
            last_message_at: Some(1700000000000),
            created_at: 1700000000000,
        };
        let dm2 = CachedDmChannel {
            id: "dm-new".into(),
            participant_ids: vec!["user-a".into(), "user-c".into()],
            last_message_preview: Some("new".into()),
            last_message_at: Some(1700000001000),
            created_at: 1700000001000,
        };
        let dm3 = CachedDmChannel {
            id: "dm-mid".into(),
            participant_ids: vec!["user-a".into(), "user-d".into()],
            last_message_preview: Some("mid".into()),
            last_message_at: Some(1700000000500),
            created_at: 1700000000500,
        };

        dm_channels::upsert_dm_channel(&conn, &dm1).unwrap();
        dm_channels::upsert_dm_channel(&conn, &dm2).unwrap();
        dm_channels::upsert_dm_channel(&conn, &dm3).unwrap();

        let list = dm_channels::list_dm_channels(&conn).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].id, "dm-new");
        assert_eq!(list[1].id, "dm-mid");
        assert_eq!(list[2].id, "dm-old");
    }

    #[test]
    fn update_dm_last_message() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let dm = CachedDmChannel {
            id: "dm-003".into(),
            participant_ids: vec!["user-a".into(), "user-b".into()],
            last_message_preview: None,
            last_message_at: None,
            created_at: 1700000000000,
        };

        dm_channels::upsert_dm_channel(&conn, &dm).unwrap();
        dm_channels::update_dm_last_message(&conn, "dm-003", "New message!", 1700000001000)
            .unwrap();

        let retrieved = dm_channels::get_dm_channel(&conn, "dm-003")
            .unwrap()
            .unwrap();
        assert_eq!(
            retrieved.last_message_preview.as_deref(),
            Some("New message!")
        );
        assert_eq!(retrieved.last_message_at, Some(1700000001000));
    }

    #[test]
    fn upsert_dm_channel_updates_existing() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let dm = CachedDmChannel {
            id: "dm-004".into(),
            participant_ids: vec!["user-a".into(), "user-b".into()],
            last_message_preview: Some("first".into()),
            last_message_at: Some(1700000000000),
            created_at: 1700000000000,
        };
        dm_channels::upsert_dm_channel(&conn, &dm).unwrap();

        let updated = CachedDmChannel {
            id: "dm-004".into(),
            participant_ids: vec!["user-a".into(), "user-b".into()],
            last_message_preview: Some("second".into()),
            last_message_at: Some(1700000001000),
            created_at: 1700000000000,
        };
        dm_channels::upsert_dm_channel(&conn, &updated).unwrap();

        let list = dm_channels::list_dm_channels(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].last_message_preview.as_deref(), Some("second"));
    }

    #[test]
    fn dm_messages_use_dm_channel_id_in_cache() {
        let db = CacheDb::open_in_memory();
        let conn = db.lock().unwrap();

        let msg = CachedMessage {
            id: "msg-dm-001".into(),
            channel_id: None,
            dm_channel_id: Some("dm-001".into()),
            sender_id: "user-a".into(),
            sender_device_id: None,
            sender_signal_device_id: None,
            plaintext: Some("DM hello".into()),
            ciphertext: None,
            message_type: None,
            created_at: 1700000000000,
            edited_at: None,
            decrypted_at: Some(1700000000000),
            status: "delivered".into(),
        };

        messages::insert_message(&conn, &msg).unwrap();
        let msgs = messages::get_messages_for_dm_channel(&conn, "dm-001", None, 10).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, "msg-dm-001");
        assert!(msgs[0].channel_id.is_none());
        assert_eq!(msgs[0].dm_channel_id.as_deref(), Some("dm-001"));
    }

    #[test]
    fn dm_channel_info_serializes_correctly() {
        let info = DmChannelInfo {
            id: "dm-001".into(),
            participant_ids: vec!["user-a".into(), "user-b".into()],
            last_message_preview: Some("Hello!".into()),
            last_message_at: Some("2024-01-01T00:00:00Z".into()),
            unread_count: 3,
        };
        let json = serde_json::to_string(&info).unwrap();
        // Verify camelCase keys in JSON output
        assert!(
            json.contains("\"participantIds\""),
            "should use camelCase keys"
        );
        assert!(
            json.contains("\"lastMessagePreview\""),
            "should use camelCase keys"
        );
        assert!(
            json.contains("\"unreadCount\""),
            "should use camelCase keys"
        );
        let back: DmChannelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "dm-001");
        assert_eq!(back.participant_ids.len(), 2);
        assert_eq!(back.unread_count, 3);
    }
}
